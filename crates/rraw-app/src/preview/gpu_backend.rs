#![forbid(unsafe_code)]

//! Live WGPU interactive preview backend (`Stage5` → `Stage6` → `Stage7`) with automatic CPU/Rayon fallback.

use egui::Color32;
use rayon::prelude::*;
use rraw_color::DisplayTransform;
use rraw_core::{ExecutionBackend, PipelineStageId, PipelineTiming, StageTimer};
use rraw_gpu::{
    GpuContext, GpuError, SemanticMaskPass, Stage5Params, Stage5Pipeline, Stage6Params,
    Stage6Pipeline, Stage7Params, Stage7Pipeline,
};

use super::pipeline::{render_preview_from_base_profiled, PreparedGradingFrame};
use super::types::BaseRgbPreview;
use crate::state::{DarkroomGradingState, HistogramData, LocalMaskKind};

/// Execution backend that produced the active interactive preview frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewBackendKind {
    /// Executed on live WGPU compute pipelines (Metal/Vulkan/DX12).
    GpuWgpu,
    /// Executed on CPU via Rayon parallel iterators (automatic fallback or user toggle).
    #[default]
    CpuRayon,
}

impl PreviewBackendKind {
    /// Returns `true` if this frame was rendered through the live WGPU backend.
    #[inline]
    #[must_use]
    pub const fn is_gpu(self) -> bool {
        matches!(self, Self::GpuWgpu)
    }
}

/// Evaluates why the active frame cannot use the GPU Stages 5→7 fast path, returning `None`
/// when it is eligible for live WGPU execution.
#[must_use]
pub fn gpu_fallback_reason(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> Option<&'static str> {
    if base.width == 0
        || base.height == 0
        || base.samples_rgb.len() != base.width.saturating_mul(base.height)
        || u32::try_from(base.width).is_err()
        || u32::try_from(base.height).is_err()
    {
        return Some("invalid preview dimensions");
    }

    if grading.color.display_transform != DisplayTransform::AgX {
        return Some("OpenDRT display transform");
    }

    if grading.soft_proof.enabled || grading.soft_proof.show_gamut_warning {
        return Some("ICC soft proofing");
    }

    if grading.denoise_sharpen.ane_neural_denoise
        && grading.denoise_sharpen.ane_denoise_strength > 1e-4
    {
        return Some("ANE neural denoise");
    }

    if (grading.denoise_sharpen.capture_sharpening_amount - 0.5).max(0.0) > 1e-4 {
        return Some("Richardson-Lucy sharpening");
    }

    if grading.optics.defringe_purple_amount > 1e-4
        || grading.optics.defringe_green_amount > 1e-4
        || grading.optics.defringe_preview_mask
    {
        return Some("optical defringe");
    }

    if grading.masks.show_semantic_mask_overlay.is_some()
        || grading.masks.show_local_mask_overlay.is_some()
    {
        return Some("mask overlay visualization");
    }

    let local_masks_compatible = grading
        .masks
        .local_masks
        .iter()
        .filter(|m| m.enabled)
        .all(|m| {
            matches!(
                m.kind,
                LocalMaskKind::LinearGradient { .. } | LocalMaskKind::RadialGradient { .. }
            ) && !m.invert
                && m.contrast.abs() <= 1e-4
                && m.saturation.abs() <= 1e-4
                && m.temperature_shift.abs() <= 1e-4
        });
    if !local_masks_compatible {
        return Some("advanced local mask");
    }

    let semantic_masks_compatible = grading
        .masks
        .semantic_masks
        .iter()
        .filter(|m| m.enabled)
        .all(|m| {
            m.contrast.abs() <= 1e-4
                && m.saturation.abs() <= 1e-4
                && m.temperature_shift.abs() <= 1e-4
        });
    if !semantic_masks_compatible {
        return Some("semantic color/contrast mask");
    }

    None
}

/// Evaluates whether the active `BaseRgbPreview` and `DarkroomGradingState` can execute
/// through the live WGPU Stages 5→7 fast path while maintaining strict colorimetric parity
/// ($\Delta E_{00} < 1.0$) with the CPU reference pipeline.
#[must_use]
pub fn can_use_gpu_fast_path(base: &BaseRgbPreview, grading: &DarkroomGradingState) -> bool {
    gpu_fallback_reason(base, grading).is_none()
}

struct CachedGpuPipelines {
    ctx: GpuContext,
    adapter_name: String,
    stage5: Stage5Pipeline,
    stage6: Stage6Pipeline,
    stage7: Stage7Pipeline,
}

enum GpuEngineState {
    Uninitialized,
    Ready(Box<CachedGpuPipelines>),
    Unavailable,
}

/// Lazily initialized WGPU interactive preview engine caching compiled Stages 5–7 compute pipelines.
pub struct GpuPreviewEngine {
    state: GpuEngineState,
    last_fallback_reason: Option<&'static str>,
}

impl Default for GpuPreviewEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuPreviewEngine {
    /// Creates an uninitialized engine; GPU adapter and pipelines are compiled lazily on first GPU frame.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: GpuEngineState::Uninitialized,
            last_fallback_reason: None,
        }
    }

    /// Returns the active GPU adapter name once initialized, or `None` if unavailable/uninitialized.
    #[must_use]
    pub fn adapter_name(&self) -> Option<&str> {
        match &self.state {
            GpuEngineState::Ready(pipes) => Some(pipes.adapter_name.as_str()),
            GpuEngineState::Uninitialized | GpuEngineState::Unavailable => None,
        }
    }

    /// Returns the human-readable reason why the last frame fell back to CPU, if any.
    #[must_use]
    pub const fn last_fallback_reason(&self) -> Option<&'static str> {
        self.last_fallback_reason
    }

    /// Attempts lazy initialization and returns whether a live WGPU backend is available.
    pub fn ensure_initialized(&mut self) -> bool {
        if matches!(self.state, GpuEngineState::Uninitialized) {
            self.state = match init_cached_pipelines() {
                Ok(pipes) => GpuEngineState::Ready(Box::new(pipes)),
                Err(_) => GpuEngineState::Unavailable,
            };
        }
        matches!(self.state, GpuEngineState::Ready(_))
    }

    /// Renders the interactive preview and histogram, using the live WGPU backend when
    /// `gpu_enabled` is true, the grading qualifies for the GPU fast path, and an adapter is available.
    /// Automatically falls back to the CPU/Rayon pipeline otherwise.
    #[must_use]
    pub fn render_preview_profiled(
        &mut self,
        base: &BaseRgbPreview,
        grading: &DarkroomGradingState,
        gpu_enabled: bool,
    ) -> (
        egui::ColorImage,
        HistogramData,
        PipelineTiming,
        PreviewBackendKind,
    ) {
        if !gpu_enabled {
            self.last_fallback_reason = Some("disabled by user");
            let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
            return (img, hist, timing, PreviewBackendKind::CpuRayon);
        }

        if let Some(reason) = gpu_fallback_reason(base, grading) {
            self.last_fallback_reason = Some(reason);
            let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
            return (img, hist, timing, PreviewBackendKind::CpuRayon);
        }

        if !self.ensure_initialized() {
            self.last_fallback_reason = Some("no WGPU adapter available");
            let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
            return (img, hist, timing, PreviewBackendKind::CpuRayon);
        }

        if let GpuEngineState::Ready(pipes) = &self.state {
            if let Ok((img, hist, timing)) = execute_gpu_preview_stages(pipes, base, grading) {
                self.last_fallback_reason = None;
                return (img, hist, timing, PreviewBackendKind::GpuWgpu);
            }
        }

        self.last_fallback_reason = Some("GPU stage execution error");
        let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
        (img, hist, timing, PreviewBackendKind::CpuRayon)
    }
}

fn init_cached_pipelines() -> Result<CachedGpuPipelines, GpuError> {
    let ctx = GpuContext::new_headless()?;
    let adapter_name = ctx.adapter().get_info().name;
    let stage5 = Stage5Pipeline::new(&ctx)?;
    let stage6 = Stage6Pipeline::new(&ctx)?;
    let stage7 = Stage7Pipeline::new(&ctx)?;
    Ok(CachedGpuPipelines {
        ctx,
        adapter_name,
        stage5,
        stage6,
        stage7,
    })
}

fn execute_gpu_preview_stages(
    pipes: &CachedGpuPipelines,
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> Result<(egui::ColorImage, HistogramData, PipelineTiming), GpuError> {
    let preview_w = base.width;
    let preview_h = base.height;
    let width_u32 = u32::try_from(preview_w).map_err(|_| GpuError::BufferLengthMismatch {
        expected: usize::MAX,
        actual: preview_w,
    })?;
    let height_u32 = u32::try_from(preview_h).map_err(|_| GpuError::BufferLengthMismatch {
        expected: usize::MAX,
        actual: preview_h,
    })?;
    let num_pixels = preview_w.saturating_mul(preview_h);
    let mut timing = PipelineTiming::new();

    // Pre-Stage-5: camera matrix + WB + exposure + shadows/highlights/blacks into flat linear RGB.
    let t_s5 = StageTimer::start(
        PipelineStageId::Stage5Optics,
        ExecutionBackend::GpuMetal,
        num_pixels,
    );
    let linear_rgb = build_pre_stage5_linear_rgb(base, grading);

    let has_curves = !grading.tone_curves.is_identity();
    let has_clut = grading.custom_clut.is_some();
    let stage5_rgb = if has_curves || has_clut {
        let mut s5_params = Stage5Params::new(width_u32, height_u32, 0, 0, 0.0);
        if has_curves {
            let lut = rraw_color::curves::generate_1d_lut_1024(&grading.tone_curves);
            s5_params = s5_params.with_tone_curves(lut);
        }
        if let Some(clut) = &grading.custom_clut {
            s5_params = s5_params.with_clut_3d((*clut.lut).clone(), clut.strength);
        }
        pipes.stage5.execute(&pipes.ctx, &linear_rgb, &s5_params)?
    } else {
        linear_rgb
    };
    t_s5.finish_and_record(&mut timing);

    // Stage 6: GPU local geometric & semantic EV masks.
    let t_s6 = StageTimer::start(
        PipelineStageId::Stage6TonalGrading,
        ExecutionBackend::GpuMetal,
        num_pixels,
    );
    let stage6_masks = grading.to_stage6_masks_params();
    let stage6_rgb = if !stage6_masks.masks.is_empty() {
        let s6_params = Stage6Params {
            width: width_u32,
            height: height_u32,
            masks: stage6_masks,
        };
        let sem_rgba = build_interleaved_semantic_rgba(&base.semantic_masks);
        let sem_pass = sem_rgba.as_ref().map(|(buf, sw, sh)| SemanticMaskPass {
            rgba_probabilities: buf.as_slice(),
            width: *sw,
            height: *sh,
        });
        pipes
            .stage6
            .execute_with_semantics(&pipes.ctx, &stage5_rgb, &s6_params, sem_pass)?
    } else {
        stage5_rgb
    };
    t_s6.finish_and_record(&mut timing);

    // Stage 7: GPU AgX DRT + scope pass.
    let t_s7 = StageTimer::start(
        PipelineStageId::Stage7EdrDisplay,
        ExecutionBackend::GpuMetal,
        num_pixels,
    );
    let s7_params = Stage7Params {
        width: width_u32,
        height: height_u32,
        headroom: 1.0,
    };
    let (drt_rgb, _gpu_hist, _clip) = pipes.stage7.execute(&pipes.ctx, &stage6_rgb, &s7_params)?;
    let (color_image, hist_data) =
        finalize_post_drt_display(&drt_rgb, preview_w, preview_h, grading);
    t_s7.finish_and_record(&mut timing);

    Ok((color_image, hist_data, timing))
}

fn build_pre_stage5_linear_rgb(base: &BaseRgbPreview, grading: &DarkroomGradingState) -> Vec<f32> {
    let frame = PreparedGradingFrame::prepare(base, grading, false);
    let preview_w = frame.preview_w();
    let preview_h = frame.preview_h();
    let mut flat = vec![0.0_f32; preview_w.saturating_mul(preview_h).saturating_mul(3)];
    flat.par_chunks_exact_mut(preview_w.saturating_mul(3))
        .enumerate()
        .for_each(|(py, row_dst)| {
            for (px, dst) in row_dst.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                *dst = frame.evaluate_pre_stage5_linear(px, py);
            }
        });
    flat
}

fn build_interleaved_semantic_rgba(sem: &rraw_ai::SemanticMaskSet) -> Option<(Vec<f32>, u32, u32)> {
    let sw = u32::try_from(sem.width).ok()?;
    let sh = u32::try_from(sem.height).ok()?;
    let n = sem.width.checked_mul(sem.height)?;
    if n == 0
        || sem.subject.len() != n
        || sem.sky.len() != n
        || sem.background.len() != n
        || sem.portrait.len() != n
    {
        return None;
    }
    let mut rgba = Vec::with_capacity(n.saturating_mul(4));
    for i in 0..n {
        rgba.push(sem.subject[i]);
        rgba.push(sem.sky[i]);
        rgba.push(sem.background[i]);
        rgba.push(sem.portrait[i]);
    }
    Some((rgba, sw, sh))
}

fn finalize_post_drt_display(
    drt_rgb: &[f32],
    preview_w: usize,
    preview_h: usize,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let contrast_factor = (1.0 + (grading.tone.contrast / 100.0).clamp(-0.8, 2.0)).max(0.05);
    let base_vibrance = 0.10_f32;
    let user_vibrance = grading.tone.vibrance / 100.0;
    let user_sat = grading.tone.saturation / 100.0;
    let is_mono = grading.color.is_monochrome;

    let has_cv = grading.effects.creative_vignette_amount.abs() > 0.1;
    let cv_amt = grading.effects.creative_vignette_amount / 100.0;
    let cv_mid = (grading.effects.creative_vignette_midpoint / 100.0).clamp(0.05, 0.95);
    let cv_feather = (grading.effects.creative_vignette_feather / 100.0).clamp(0.05, 0.95);
    let cv_round = (grading.effects.creative_vignette_roundness / 100.0).clamp(-1.0, 1.0);
    let cx = preview_w as f32 * 0.5;
    let cy = preview_h as f32 * 0.5;

    struct RowHist {
        r: [u32; 256],
        g: [u32; 256],
        b: [u32; 256],
        lum: [u32; 256],
        sc: u32,
        hc: u32,
    }

    let mut color_pixels = vec![Color32::TRANSPARENT; preview_w.saturating_mul(preview_h)];
    let row_hists: Vec<RowHist> = color_pixels
        .par_chunks_exact_mut(preview_w)
        .zip(drt_rgb.par_chunks_exact(preview_w.saturating_mul(3)))
        .enumerate()
        .map(|(py, (row_out, row_in))| {
            let mut hr = [0_u32; 256];
            let mut hg = [0_u32; 256];
            let mut hb = [0_u32; 256];
            let mut hl = [0_u32; 256];
            let mut sc = 0_u32;
            let mut hc = 0_u32;

            for (px, (out_px, in_rgb)) in row_out
                .iter_mut()
                .zip(row_in.as_chunks::<3>().0)
                .enumerate()
            {
                let r_c = ((in_rgb[0] - 0.18) * contrast_factor + 0.18).max(0.0);
                let g_c = ((in_rgb[1] - 0.18) * contrast_factor + 0.18).max(0.0);
                let b_c = ((in_rgb[2] - 0.18) * contrast_factor + 0.18).max(0.0);

                let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
                let max_v = r_c.max(g_c).max(b_c);
                let min_v = r_c.min(g_c).min(b_c);
                let cur_sat = if max_v > 1e-4 {
                    (max_v - min_v) / max_v
                } else {
                    0.0
                };
                let vib_w = (1.0 - cur_sat).clamp(0.0, 1.0);
                let eff_sat =
                    ((1.0 + user_sat) * (1.0 + (base_vibrance + user_vibrance) * vib_w)).max(0.0);

                let mut r_s = lum_c + (r_c - lum_c) * eff_sat;
                let mut g_s = lum_c + (g_c - lum_c) * eff_sat;
                let mut b_s = lum_c + (b_c - lum_c) * eff_sat;

                if has_cv {
                    let fx = px as f32;
                    let fy = py as f32;
                    let u_rel = (fx - cx) / cx.max(1.0);
                    let v_rel = (fy - cy) / cy.max(1.0);
                    let r_ellip = (u_rel * u_rel + v_rel * v_rel).sqrt();
                    let r_circ =
                        ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() / cx.min(cy).max(1.0);
                    let r_rect = u_rel.abs().max(v_rel.abs());
                    let r_dist = if cv_round > 0.0 {
                        r_ellip * (1.0 - cv_round) + r_circ * cv_round
                    } else {
                        r_ellip * (1.0 + cv_round) + r_rect * (-cv_round)
                    };
                    let t = ((r_dist - cv_mid) / (cv_feather.max(0.05) * 1.2)).clamp(0.0, 1.0);
                    let smooth_t = t * t * (3.0 - 2.0 * t);
                    let cv_mult = (1.0 + cv_amt * smooth_t).max(0.0);
                    r_s *= cv_mult;
                    g_s *= cv_mult;
                    b_s *= cv_mult;
                }

                let ur = PreparedGradingFrame::to_u8_fast(r_s.max(0.0));
                let ug = PreparedGradingFrame::to_u8_fast(g_s.max(0.0));
                let ub = PreparedGradingFrame::to_u8_fast(b_s.max(0.0));
                let [u8_r, u8_g, u8_b] = if is_mono {
                    let lv = ((ur as u32 * 54 + ug as u32 * 183 + ub as u32 * 19) >> 8) as u8;
                    [lv, lv, lv]
                } else {
                    [ur, ug, ub]
                };

                hr[u8_r as usize] += 1;
                hg[u8_g as usize] += 1;
                hb[u8_b as usize] += 1;
                let lum = (((u8_r as u32 * 54) + (u8_g as u32 * 183) + (u8_b as u32 * 19)) >> 8)
                    .min(255) as usize;
                hl[lum] += 1;
                if lum <= 1 {
                    sc += 1;
                }
                if lum >= 250 || u8_r >= 253 || u8_g >= 253 || u8_b >= 253 {
                    hc += 1;
                }
                *out_px = Color32::from_rgb(u8_r, u8_g, u8_b);
            }

            RowHist {
                r: hr,
                g: hg,
                b: hb,
                lum: hl,
                sc,
                hc,
            }
        })
        .collect();

    let mut hist_r = [0_u32; 256];
    let mut hist_g = [0_u32; 256];
    let mut hist_b = [0_u32; 256];
    let mut hist_lum = [0_u32; 256];
    let mut shadow_clipped = 0_u32;
    let mut highlight_clipped = 0_u32;
    for row in row_hists {
        for i in 0..256 {
            hist_r[i] += row.r[i];
            hist_g[i] += row.g[i];
            hist_b[i] += row.b[i];
            hist_lum[i] += row.lum[i];
        }
        shadow_clipped += row.sc;
        highlight_clipped += row.hc;
    }

    let max_count = hist_lum[1..255].iter().copied().max().unwrap_or(1).max(1);
    let hist_data = HistogramData {
        r: hist_r,
        g: hist_g,
        b: hist_b,
        lum: hist_lum,
        max_count,
        shadow_clipped,
        highlight_clipped,
        out_of_gamut_count: 0,
    };

    (
        egui::ColorImage::new([preview_w, preview_h], color_pixels),
        hist_data,
    )
}
