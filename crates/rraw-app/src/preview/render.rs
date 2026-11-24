#![forbid(unsafe_code)]

//! High-level CPU/Rayon interactive preview rendering, histogram aggregation, and export rasterization.

use egui::Color32;
use rayon::prelude::*;
use rraw_core::{ExecutionBackend, PipelineStageId, PipelineTiming, SignedCfaBuffer, StageTimer};

use super::pipeline::PreparedGradingFrame;
use super::types::BaseRgbPreview;
use crate::state::*;

/// Single-allocation output raster buffer for photographic exports (either 8-bit or 16-bit RGB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportRasterBuffer {
    /// 8-bit interleaved RGB bytes (`width * height * 3`).
    U8(Vec<u8>),
    /// 16-bit interleaved RGB samples (`width * height * 3`).
    U16(Vec<u16>),
}

impl ExportRasterBuffer {
    /// Returns the 8-bit RGB slice if this buffer is `ExportRasterBuffer::U8`.
    #[inline]
    #[must_use]
    pub fn as_u8_slice(&self) -> Option<&[u8]> {
        match self {
            Self::U8(b) => Some(b.as_slice()),
            Self::U16(_) => None,
        }
    }

    /// Returns the 16-bit RGB slice if this buffer is `ExportRasterBuffer::U16`.
    #[inline]
    #[must_use]
    pub fn as_u16_slice(&self) -> Option<&[u16]> {
        match self {
            Self::U8(_) => None,
            Self::U16(s) => Some(s.as_slice()),
        }
    }
}

/// Evaluates darkroom grading, optical distortion, vignetting, and histogram from `BaseRgbPreview`.
///
/// Executes in < 1 ms over contiguous RAM without re-sampling the raw sensor matrix.
#[must_use]
pub fn render_preview_from_base(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let (img, hist, _) = render_preview_from_base_profiled(base, grading);
    (img, hist)
}

/// Evaluates darkroom grading, optical distortion, vignetting, and histogram from `BaseRgbPreview`
/// with granular stage timing instrumentation across Stages 5, 6, and 7.
#[must_use]
pub fn render_preview_from_base_profiled(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData, PipelineTiming) {
    let mut timing = PipelineTiming::new();
    let preview_w = base.width;
    let preview_h = base.height;
    let num_pixels = preview_w * preview_h;

    let t_optics = StageTimer::start(
        PipelineStageId::Stage5Optics,
        ExecutionBackend::CpuScalar,
        num_pixels,
    );

    let frame = PreparedGradingFrame::prepare(base, grading, true);
    let is_identity_geom = frame.is_identity_geom_base();

    struct RowHistogram {
        hist_r: [u32; 256],
        hist_g: [u32; 256],
        hist_b: [u32; 256],
        hist_lum: [u32; 256],
        shadow_clipped: u32,
        highlight_clipped: u32,
        out_of_gamut_count: u32,
    }

    t_optics.finish_and_record(&mut timing);
    let t_tonal = StageTimer::start(
        PipelineStageId::Stage6TonalGrading,
        ExecutionBackend::CpuRayon,
        num_pixels,
    );

    let mut color_pixels = vec![Color32::TRANSPARENT; preview_w * preview_h];

    let row_histograms: Vec<RowHistogram> = color_pixels
        .par_chunks_exact_mut(preview_w)
        .enumerate()
        .map(|(py, row_pixels)| {
            let mut row_r = [0_u32; 256];
            let mut row_g = [0_u32; 256];
            let mut row_b = [0_u32; 256];
            let mut row_lum = [0_u32; 256];
            let mut row_sc = 0_u32;
            let mut row_hc = 0_u32;
            let mut row_oog = 0_u32;

            for (px, out_pixel) in row_pixels.iter_mut().enumerate() {
                let identity_xy = if is_identity_geom {
                    Some((px, py))
                } else {
                    None
                };
                let (rgb_post, sp_res, u_norm, v_norm) =
                    frame.evaluate_pixel(px as f32, py as f32, identity_xy);

                if sp_res.is_out_of_gamut {
                    row_oog += 1;
                }

                let [mut u8_r, mut u8_g, mut u8_b] = frame.quantize_display_u8(rgb_post);

                if grading.soft_proof.show_gamut_warning && sp_res.is_out_of_gamut {
                    let w = (0.55 + sp_res.gamut_excursion * 1.6).clamp(0.55, 0.88);
                    let is_stripe = ((px + py) & 2) == 0;
                    let (alert_r, alert_g, alert_b) = if is_stripe {
                        (255.0_f32, 0.0_f32, 220.0_f32)
                    } else {
                        (0.0_f32, 245.0_f32, 255.0_f32)
                    };
                    u8_r = ((u8_r as f32) * (1.0 - w) + alert_r * w).clamp(0.0, 255.0) as u8;
                    u8_g = ((u8_g as f32) * (1.0 - w) + alert_g * w).clamp(0.0, 255.0) as u8;
                    u8_b = ((u8_b as f32) * (1.0 - w) + alert_b * w).clamp(0.0, 255.0) as u8;
                }

                if let Some(overlay_cat) = grading.masks.show_semantic_mask_overlay {
                    let (inv, fth) = grading
                        .masks
                        .semantic_masks
                        .iter()
                        .find(|m| m.category == overlay_cat)
                        .map_or((false, 0.35), |m| (m.invert, m.feather));
                    let mask_w =
                        base.semantic_masks
                            .sample_bilinear(overlay_cat, u_norm, v_norm, inv, fth)
                            * 0.65;
                    if mask_w > 1e-3 {
                        u8_r = ((u8_r as f32) * (1.0 - mask_w) + 255.0 * mask_w).clamp(0.0, 255.0)
                            as u8;
                        u8_g = ((u8_g as f32) * (1.0 - mask_w) + 40.0 * mask_w).clamp(0.0, 255.0)
                            as u8;
                        u8_b = ((u8_b as f32) * (1.0 - mask_w) + 115.0 * mask_w).clamp(0.0, 255.0)
                            as u8;
                    }
                }

                if let Some(mask_idx) = grading.masks.show_local_mask_overlay {
                    if let Some(m) = grading.masks.local_masks.get(mask_idx) {
                        let raw_mw = if let Some(Some(grid)) = frame.brush_grids().get(mask_idx) {
                            grid.sample(u_norm, v_norm)
                        } else {
                            m.evaluate_weight(u_norm, v_norm)
                        };
                        let mask_w = raw_mw * 0.65;
                        if mask_w > 1e-3 {
                            u8_r = ((u8_r as f32) * (1.0 - mask_w) + 255.0 * mask_w)
                                .clamp(0.0, 255.0) as u8;
                            u8_g = ((u8_g as f32) * (1.0 - mask_w) + 40.0 * mask_w)
                                .clamp(0.0, 255.0) as u8;
                            u8_b = ((u8_b as f32) * (1.0 - mask_w) + 115.0 * mask_w)
                                .clamp(0.0, 255.0) as u8;
                        }
                    }
                }

                row_r[u8_r as usize] += 1;
                row_g[u8_g as usize] += 1;
                row_b[u8_b as usize] += 1;

                let lum = (((u8_r as u32 * 54) + (u8_g as u32 * 183) + (u8_b as u32 * 19)) >> 8)
                    .min(255) as usize;
                row_lum[lum] += 1;

                if lum <= 1 {
                    row_sc += 1;
                }
                if lum >= 250 || u8_r >= 253 || u8_g >= 253 || u8_b >= 253 {
                    row_hc += 1;
                }

                *out_pixel = Color32::from_rgb(u8_r, u8_g, u8_b);
            }

            RowHistogram {
                hist_r: row_r,
                hist_g: row_g,
                hist_b: row_b,
                hist_lum: row_lum,
                shadow_clipped: row_sc,
                highlight_clipped: row_hc,
                out_of_gamut_count: row_oog,
            }
        })
        .collect();

    t_tonal.finish_and_record(&mut timing);
    let t_edr = StageTimer::start(
        PipelineStageId::Stage7EdrDisplay,
        ExecutionBackend::CpuScalar,
        num_pixels,
    );

    let mut hist_r = [0_u32; 256];
    let mut hist_g = [0_u32; 256];
    let mut hist_b = [0_u32; 256];
    let mut hist_lum = [0_u32; 256];
    let mut shadow_clipped = 0_u32;
    let mut highlight_clipped = 0_u32;
    let mut out_of_gamut_count = 0_u32;

    for row in row_histograms {
        for i in 0..256 {
            hist_r[i] += row.hist_r[i];
            hist_g[i] += row.hist_g[i];
            hist_b[i] += row.hist_b[i];
            hist_lum[i] += row.hist_lum[i];
        }
        shadow_clipped += row.shadow_clipped;
        highlight_clipped += row.highlight_clipped;
        out_of_gamut_count += row.out_of_gamut_count;
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
        out_of_gamut_count,
    };

    t_edr.finish_and_record(&mut timing);

    (
        egui::ColorImage::new([preview_w, preview_h], color_pixels),
        hist_data,
        timing,
    )
}

/// Renders the fully developed photographic raster for export, applying all darkroom grades,
/// cropping boundaries, geometric corrections, color transform, and scaling across all CPU cores.
#[must_use]
pub fn render_export_image(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
    scale: f32,
) -> (usize, usize, Vec<u8>, Vec<u16>) {
    render_export_image_internal(base, grading, scale, None)
}

/// Renders the developed photographic raster for export using Rayon parallelization and allocating
/// ONLY the requested bit-depth buffer (`ExportBitDepth::Bits8` or `ExportBitDepth::Bits16`).
#[must_use]
pub fn render_export_image_with_depth(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
    scale: f32,
    bit_depth: ExportBitDepth,
) -> (usize, usize, ExportRasterBuffer) {
    let (out_w, out_h, u8_bytes, u16_samples) =
        render_export_image_internal(base, grading, scale, Some(bit_depth));
    let buf = match bit_depth {
        ExportBitDepth::Bits8 => ExportRasterBuffer::U8(u8_bytes),
        ExportBitDepth::Bits16 => ExportRasterBuffer::U16(u16_samples),
    };
    (out_w, out_h, buf)
}

fn render_export_image_internal(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
    scale: f32,
    bit_depth: Option<ExportBitDepth>,
) -> (usize, usize, Vec<u8>, Vec<u16>) {
    let preview_w = base.width;
    let preview_h = base.height;

    let (crop_x0, crop_y0, crop_x1, crop_y1) = if grading.geometry.crop_enabled {
        let x0 = (grading.geometry.crop_left.clamp(0.0, 0.95) * preview_w as f32).round() as usize;
        let y0 = (grading.geometry.crop_top.clamp(0.0, 0.95) * preview_h as f32).round() as usize;
        let x1 = (grading.geometry.crop_right.clamp(0.05, 1.0) * preview_w as f32)
            .round()
            .max(x0 as f32 + 1.0) as usize;
        let y1 = (grading.geometry.crop_bottom.clamp(0.05, 1.0) * preview_h as f32)
            .round()
            .max(y0 as f32 + 1.0) as usize;
        (
            x0.min(preview_w.saturating_sub(1)),
            y0.min(preview_h.saturating_sub(1)),
            x1.min(preview_w),
            y1.min(preview_h),
        )
    } else {
        (0, 0, preview_w, preview_h)
    };

    let cropped_w = (crop_x1 - crop_x0).max(1);
    let cropped_h = (crop_y1 - crop_y0).max(1);

    let out_w = ((cropped_w as f32 * scale).round() as usize).max(1);
    let out_h = ((cropped_h as f32 * scale).round() as usize).max(1);

    let frame = PreparedGradingFrame::prepare(base, grading, false);
    let is_identity_geom = crop_x0 == 0
        && crop_y0 == 0
        && out_w == preview_w
        && out_h == preview_h
        && frame.is_identity_geom_base();

    let eval_export_post = |ox: usize, oy: usize| -> [f32; 3] {
        let (orig_x, orig_y, identity_xy) = if is_identity_geom {
            (ox as f32, oy as f32, Some((ox, oy)))
        } else {
            let ox_f = crop_x0 as f32 + (ox as f32 / out_w as f32) * cropped_w as f32;
            let oy_f = crop_y0 as f32 + (oy as f32 / out_h as f32) * cropped_h as f32;
            (ox_f, oy_f, None)
        };
        let (rgb_post, _, _, _) = frame.evaluate_pixel(orig_x, orig_y, identity_xy);
        rgb_post
    };

    match bit_depth {
        Some(ExportBitDepth::Bits8) => {
            let mut u8_bytes = vec![0_u8; out_w * out_h * 3];
            u8_bytes
                .par_chunks_exact_mut(out_w * 3)
                .enumerate()
                .for_each(|(oy, row_slice)| {
                    for ox in 0..out_w {
                        let [ur, ug, ub] = frame.quantize_display_u8(eval_export_post(ox, oy));
                        let base_idx = ox * 3;
                        row_slice[base_idx] = ur;
                        row_slice[base_idx + 1] = ug;
                        row_slice[base_idx + 2] = ub;
                    }
                });
            (out_w, out_h, u8_bytes, Vec::new())
        }
        Some(ExportBitDepth::Bits16) => {
            let mut u16_samples = vec![0_u16; out_w * out_h * 3];
            u16_samples
                .par_chunks_exact_mut(out_w * 3)
                .enumerate()
                .for_each(|(oy, row_slice)| {
                    for ox in 0..out_w {
                        let [sr, sg, sb] = frame.quantize_display_u16(eval_export_post(ox, oy));
                        let base_idx = ox * 3;
                        row_slice[base_idx] = sr;
                        row_slice[base_idx + 1] = sg;
                        row_slice[base_idx + 2] = sb;
                    }
                });
            (out_w, out_h, Vec::new(), u16_samples)
        }
        None => {
            let mut u8_bytes = vec![0_u8; out_w * out_h * 3];
            let mut u16_samples = vec![0_u16; out_w * out_h * 3];
            u8_bytes
                .par_chunks_exact_mut(out_w * 3)
                .zip(u16_samples.par_chunks_exact_mut(out_w * 3))
                .enumerate()
                .for_each(|(oy, (row_u8, row_u16))| {
                    for ox in 0..out_w {
                        let rgb_post = eval_export_post(ox, oy);
                        let [ur, ug, ub] = frame.quantize_display_u8(rgb_post);
                        let [sr, sg, sb] = frame.quantize_display_u16(rgb_post);
                        let base_idx = ox * 3;
                        row_u8[base_idx] = ur;
                        row_u8[base_idx + 1] = ug;
                        row_u8[base_idx + 2] = ub;
                        row_u16[base_idx] = sr;
                        row_u16[base_idx + 1] = sg;
                        row_u16[base_idx + 2] = sb;
                    }
                });
            (out_w, out_h, u8_bytes, u16_samples)
        }
    }
}

/// Convenience helper evaluating preview and histogram directly from CFA.
#[must_use]
pub fn generate_preview_image(
    cfa: &SignedCfaBuffer,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let base = BaseRgbPreview::from_cfa(cfa);
    render_preview_from_base(&base, grading)
}
