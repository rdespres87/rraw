#![forbid(unsafe_code)]

//! Live WGPU interactive preview backend (`Stage5` → `Stage6` → `Stage7`) with automatic CPU/Rayon fallback.

use std::sync::Arc;

use egui::Color32;
use rayon::prelude::*;
use rraw_color::DisplayTransform;
use rraw_core::{ExecutionBackend, PipelineStageId, PipelineTiming, StageTimer};
use rraw_gpu::{
    execute_stages5_to_7_fused, FusedPreviewRequest, GpuContext, GpuError, SemanticMaskPass,
    Stage5Params, Stage5Pipeline, Stage6Params, Stage6Pipeline, Stage7Params, Stage7Pipeline,
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

    let local_masks_compatible = grading
        .masks
        .local_masks
        .iter()
        .filter(|m| m.enabled)
        .all(|m| {
            matches!(
                m.kind,
                LocalMaskKind::LinearGradient { .. } | LocalMaskKind::RadialGradient { .. }
            )
        });
    if !local_masks_compatible {
        return Some("freehand brush mask");
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
    last_gpu_clipping: bool,
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
            last_gpu_clipping: false,
        }
    }

    /// Returns whether the last GPU Stage 7 pass detected highlight clipping above display headroom.
    #[must_use]
    pub const fn last_gpu_clipping(&self) -> bool {
        self.last_gpu_clipping
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

    /// Renders the interactive preview and histogram with explicit display headroom (`1.0` for SDR,
    /// `> 1.0` for EDR), using the fused live WGPU backend when eligible and available.
    #[must_use]
    pub fn render_preview_profiled_with_headroom(
        &mut self,
        base: &BaseRgbPreview,
        grading: &DarkroomGradingState,
        gpu_enabled: bool,
        headroom: f32,
    ) -> (
        egui::ColorImage,
        HistogramData,
        PipelineTiming,
        PreviewBackendKind,
    ) {
        if !gpu_enabled {
            self.last_fallback_reason = Some("disabled by user");
            self.last_gpu_clipping = false;
            let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
            return (img, hist, timing, PreviewBackendKind::CpuRayon);
        }

        if let Some(reason) = gpu_fallback_reason(base, grading) {
            self.last_fallback_reason = Some(reason);
            self.last_gpu_clipping = false;
            let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
            return (img, hist, timing, PreviewBackendKind::CpuRayon);
        }

        if !self.ensure_initialized() {
            self.last_fallback_reason = Some("no WGPU adapter available");
            self.last_gpu_clipping = false;
            let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
            return (img, hist, timing, PreviewBackendKind::CpuRayon);
        }

        if let GpuEngineState::Ready(pipes) = &self.state {
            if let Ok((img, hist, timing, has_clipping)) =
                execute_gpu_preview_stages(pipes, base, grading, headroom)
            {
                self.last_fallback_reason = None;
                self.last_gpu_clipping = has_clipping;
                return (img, hist, timing, PreviewBackendKind::GpuWgpu);
            }
        }

        self.last_fallback_reason = Some("GPU stage execution error");
        self.last_gpu_clipping = false;
        let (img, hist, timing) = render_preview_from_base_profiled(base, grading);
        (img, hist, timing, PreviewBackendKind::CpuRayon)
    }

    /// Renders the interactive preview and histogram in SDR headroom (`1.0`), using the live WGPU
    /// backend when `gpu_enabled` is true, the grading qualifies for the GPU fast path, and an
    /// adapter is available. Automatically falls back to the CPU/Rayon pipeline otherwise.
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
        self.render_preview_profiled_with_headroom(base, grading, gpu_enabled, 1.0)
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
    headroom: f32,
) -> Result<(egui::ColorImage, HistogramData, PipelineTiming, bool), GpuError> {
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
    let s5_params = if has_curves || has_clut {
        let mut params = Stage5Params::new(width_u32, height_u32, 0, 0, 0.0);
        if has_curves {
            let lut = rraw_color::curves::generate_1d_lut_1024(&grading.tone_curves);
            params = params.with_tone_curves(lut);
        }
        if let Some(clut) = &grading.custom_clut {
            params = params.with_clut_3d_arc(Arc::clone(&clut.lut), clut.strength);
        }
        Some(params)
    } else {
        None
    };
    t_s5.finish_and_record(&mut timing);

    let t_s6 = StageTimer::start(
        PipelineStageId::Stage6TonalGrading,
        ExecutionBackend::GpuMetal,
        num_pixels,
    );
    let stage6_masks = grading.to_stage6_masks_params();
    let s6_params = if !stage6_masks.masks.is_empty() {
        Some(Stage6Params {
            width: width_u32,
            height: height_u32,
            masks: stage6_masks,
        })
    } else {
        None
    };
    let sem_rgba = if s6_params.is_some() {
        build_interleaved_semantic_rgba(&base.semantic_masks)
    } else {
        None
    };
    let sem_pass = sem_rgba.as_ref().map(|(buf, sw, sh)| SemanticMaskPass {
        rgba_probabilities: buf.as_slice(),
        width: *sw,
        height: *sh,
    });
    t_s6.finish_and_record(&mut timing);

    // Fused single-submit GPU execution across active Stages 5 → 6 → 7 (zero intermediate readbacks).
    let t_s7 = StageTimer::start(
        PipelineStageId::Stage7EdrDisplay,
        ExecutionBackend::GpuMetal,
        num_pixels,
    );
    let safe_headroom = if headroom.is_finite() {
        headroom.clamp(1.0, 16.0)
    } else {
        1.0
    };
    let s7_params = Stage7Params {
        width: width_u32,
        height: height_u32,
        headroom: safe_headroom,
    };
    let fused_req = FusedPreviewRequest {
        linear_rgb: &linear_rgb,
        stage5: s5_params.as_ref(),
        stage6: s6_params.as_ref().map(|p| (p, sem_pass)),
        stage7: s7_params,
    };
    let fused_out = execute_stages5_to_7_fused(
        &pipes.ctx,
        &pipes.stage5,
        &pipes.stage6,
        &pipes.stage7,
        &fused_req,
    )?;
    let (color_image, hist_data) =
        finalize_post_drt_display(base, &fused_out.drt_rgb, preview_w, preview_h, grading);
    t_s7.finish_and_record(&mut timing);

    Ok((color_image, hist_data, timing, fused_out.has_clipping))
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

struct RowHist {
    r: [u32; 256],
    g: [u32; 256],
    b: [u32; 256],
    lum: [u32; 256],
    sc: u32,
    hc: u32,
}

/// Applies creative vignette attenuation/boost to a linear post-DRT pixel.
#[allow(clippy::too_many_arguments)]
fn apply_creative_vignette(
    rgb: &mut [f32; 3],
    px: usize,
    py: usize,
    cx: f32,
    cy: f32,
    cv_amt: f32,
    cv_mid: f32,
    cv_feather: f32,
    cv_round: f32,
) {
    let fx = px as f32;
    let fy = py as f32;
    let u_rel = (fx - cx) / cx.max(1.0);
    let v_rel = (fy - cy) / cy.max(1.0);
    let r_ellip = (u_rel * u_rel + v_rel * v_rel).sqrt();
    let r_circ = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() / cx.min(cy).max(1.0);
    let r_rect = u_rel.abs().max(v_rel.abs());
    let r_dist = if cv_round > 0.0 {
        r_ellip * (1.0 - cv_round) + r_circ * cv_round
    } else {
        r_ellip * (1.0 + cv_round) + r_rect * (-cv_round)
    };
    let t = ((r_dist - cv_mid) / (cv_feather.max(0.05) * 1.2)).clamp(0.0, 1.0);
    let smooth_t = t * t * (3.0 - 2.0 * t);
    let cv_mult = (1.0 + cv_amt * smooth_t).max(0.0);
    rgb[0] *= cv_mult;
    rgb[1] *= cv_mult;
    rgb[2] *= cv_mult;
}

/// Blends semantic and local mask visualization overlays onto an 8-bit RGB pixel.
fn apply_mask_overlays(
    rgb: &mut [u8; 3],
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
    u_norm: f32,
    v_norm: f32,
) {
    let blend_overlay = |px: &mut [u8; 3], mask_w: f32| {
        if mask_w > 1e-3 {
            px[0] = ((px[0] as f32) * (1.0 - mask_w) + 255.0 * mask_w).clamp(0.0, 255.0) as u8;
            px[1] = ((px[1] as f32) * (1.0 - mask_w) + 40.0 * mask_w).clamp(0.0, 255.0) as u8;
            px[2] = ((px[2] as f32) * (1.0 - mask_w) + 115.0 * mask_w).clamp(0.0, 255.0) as u8;
        }
    };

    if let Some(overlay_cat) = grading.masks.show_semantic_mask_overlay {
        let (inv, fth) = grading
            .masks
            .semantic_masks
            .iter()
            .find(|m| m.category == overlay_cat)
            .map_or((false, 0.35), |m| (m.invert, m.feather));
        let mask_w = base
            .semantic_masks
            .sample_bilinear(overlay_cat, u_norm, v_norm, inv, fth)
            * 0.65;
        blend_overlay(rgb, mask_w);
    }

    if let Some(mask_idx) = grading.masks.show_local_mask_overlay {
        if let Some(m) = grading.masks.local_masks.get(mask_idx) {
            let mask_w = m.evaluate_weight(u_norm, v_norm) * 0.65;
            blend_overlay(rgb, mask_w);
        }
    }
}

/// Reduces per-row histogram buckets into a single frame `HistogramData`.
fn merge_row_histograms(row_hists: Vec<RowHist>) -> HistogramData {
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
    HistogramData {
        r: hist_r,
        g: hist_g,
        b: hist_b,
        lum: hist_lum,
        max_count,
        shadow_clipped,
        highlight_clipped,
        out_of_gamut_count: 0,
    }
}

fn finalize_post_drt_display(
    base: &BaseRgbPreview,
    drt_rgb: &[f32],
    preview_w: usize,
    preview_h: usize,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let base_contrast_factor = (1.0 + (grading.tone.contrast / 100.0).clamp(-0.8, 2.0)).max(0.05);
    let base_vibrance = 0.10_f32;
    let user_vibrance = grading.tone.vibrance / 100.0;
    let user_sat = grading.tone.saturation / 100.0;
    let is_mono = grading.color.is_monochrome;

    let has_post_drt_mask_adj = grading
        .masks
        .local_masks
        .iter()
        .any(|m| m.enabled && (m.contrast.abs() > 1e-4 || m.saturation.abs() > 1e-4))
        || grading
            .masks
            .semantic_masks
            .iter()
            .any(|m| m.enabled && (m.contrast.abs() > 1e-4 || m.saturation.abs() > 1e-4));
    let has_overlay = grading.masks.show_semantic_mask_overlay.is_some()
        || grading.masks.show_local_mask_overlay.is_some();
    let inv_w = 1.0 / preview_w.max(1) as f32;
    let inv_h = 1.0 / preview_h.max(1) as f32;

    let has_cv = grading.effects.creative_vignette_amount.abs() > 0.1;
    let cv_amt = grading.effects.creative_vignette_amount / 100.0;
    let cv_mid = (grading.effects.creative_vignette_midpoint / 100.0).clamp(0.05, 0.95);
    let cv_feather = (grading.effects.creative_vignette_feather / 100.0).clamp(0.05, 0.95);
    let cv_round = (grading.effects.creative_vignette_roundness / 100.0).clamp(-1.0, 1.0);
    let cx = preview_w as f32 * 0.5;
    let cy = preview_h as f32 * 0.5;

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
            let v_norm = ((py as f32 + 0.5) * inv_h).clamp(0.0, 1.0);

            for (px, (out_px, in_rgb)) in row_out
                .iter_mut()
                .zip(row_in.as_chunks::<3>().0)
                .enumerate()
            {
                let u_norm = if has_post_drt_mask_adj || has_overlay {
                    ((px as f32 + 0.5) * inv_w).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (sem_contrast, sem_sat) = if has_post_drt_mask_adj {
                    let (_, c, s, _) = crate::preview::masks::eval_all_mask_adjustments(
                        &base.semantic_masks,
                        &grading.masks.semantic_masks,
                        &grading.masks.local_masks,
                        u_norm,
                        v_norm,
                    );
                    (c, s)
                } else {
                    (0.0, 0.0)
                };

                let contrast_factor = (base_contrast_factor
                    * (1.0 + (sem_contrast / 100.0).clamp(-0.8, 2.0)))
                .max(0.05);
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
                let eff_sat = ((1.0 + user_sat + (sem_sat / 100.0))
                    * (1.0 + (base_vibrance + user_vibrance) * vib_w))
                    .max(0.0);

                let mut rgb_s = [
                    lum_c + (r_c - lum_c) * eff_sat,
                    lum_c + (g_c - lum_c) * eff_sat,
                    lum_c + (b_c - lum_c) * eff_sat,
                ];

                if has_cv {
                    apply_creative_vignette(
                        &mut rgb_s, px, py, cx, cy, cv_amt, cv_mid, cv_feather, cv_round,
                    );
                }

                let ur = PreparedGradingFrame::to_u8_fast(rgb_s[0].max(0.0));
                let ug = PreparedGradingFrame::to_u8_fast(rgb_s[1].max(0.0));
                let ub = PreparedGradingFrame::to_u8_fast(rgb_s[2].max(0.0));
                let mut u8_rgb = if is_mono {
                    let lv = ((ur as u32 * 54 + ug as u32 * 183 + ub as u32 * 19) >> 8) as u8;
                    [lv, lv, lv]
                } else {
                    [ur, ug, ub]
                };

                if has_overlay {
                    apply_mask_overlays(&mut u8_rgb, base, grading, u_norm, v_norm);
                }

                hr[u8_rgb[0] as usize] += 1;
                hg[u8_rgb[1] as usize] += 1;
                hb[u8_rgb[2] as usize] += 1;
                let lum =
                    (((u8_rgb[0] as u32 * 54) + (u8_rgb[1] as u32 * 183) + (u8_rgb[2] as u32 * 19))
                        >> 8)
                        .min(255) as usize;
                hl[lum] += 1;
                if lum <= 1 {
                    sc += 1;
                }
                if lum >= 250 || u8_rgb[0] >= 253 || u8_rgb[1] >= 253 || u8_rgb[2] >= 253 {
                    hc += 1;
                }
                *out_px = Color32::from_rgb(u8_rgb[0], u8_rgb[1], u8_rgb[2]);
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

    let hist_data = merge_row_histograms(row_hists);
    (
        egui::ColorImage::new([preview_w, preview_h], color_pixels),
        hist_data,
    )
}
