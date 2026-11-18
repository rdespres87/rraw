#![forbid(unsafe_code)]

//! Image decoding, preview caching, and background worker payloads.

use std::path::{Path, PathBuf};

use egui::Color32;
use rraw_color::{apply_agx_edr, apply_opendrt, DisplayTransform};
use rraw_core::{CfaColor, SignedCfaBuffer};
use rraw_decode::{PureRustRawDecoder, RawDecoder};
use serde::{Deserialize, Serialize};

use crate::state::*;

/// Metadata and state for an opened or inspected RAW photograph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedPhotoView {
    pub path: PathBuf,
    pub camera_make: String,
    pub camera_model: String,
    pub lens_model: String,
    pub dimensions: (u32, u32),
    pub iso: u32,
    pub shutter_str: String,
    pub aperture_str: String,
    pub focal_length_str: String,
    pub date_time: String,
    pub exposure_bias: String,
    pub rating: u8,
    pub exif_tags: Vec<(String, String)>,
}

impl LoadedPhotoView {
    /// Serializes all EXIF metadata tags into a formatted JSON string for export.
    #[must_use]
    pub fn export_exif_json(&self) -> String {
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "file_path".to_string(),
            self.path.to_string_lossy().to_string(),
        );
        map.insert("camera_make".to_string(), self.camera_make.clone());
        map.insert("camera_model".to_string(), self.camera_model.clone());
        if !self.lens_model.is_empty() {
            map.insert("lens_model".to_string(), self.lens_model.clone());
        }
        map.insert(
            "dimensions".to_string(),
            format!("{}x{}", self.dimensions.0, self.dimensions.1),
        );
        map.insert("iso".to_string(), self.iso.to_string());
        map.insert("shutter_speed".to_string(), self.shutter_str.clone());
        map.insert("aperture".to_string(), self.aperture_str.clone());
        map.insert("focal_length".to_string(), self.focal_length_str.clone());
        if !self.date_time.is_empty() {
            map.insert("date_time_original".to_string(), self.date_time.clone());
        }
        if !self.exposure_bias.is_empty() {
            map.insert("exposure_bias".to_string(), self.exposure_bias.clone());
        }
        for (k, v) in &self.exif_tags {
            map.insert(k.clone(), v.clone());
        }
        serde_json::to_string_pretty(&map).unwrap_or_default()
    }
}

/// Computes a 3x3 camera-to-sRGB calibration matrix from DNG ColorMatrix and white balance.
#[must_use]
pub fn compute_cam_to_srgb_matrix(cm: Option<&[f32; 9]>, wb: &[f32; 3]) -> [f32; 9] {
    if let Some(cm) = cm {
        let a = cm[0];
        let b = cm[1];
        let c = cm[2];
        let d = cm[3];
        let e = cm[4];
        let f = cm[5];
        let g = cm[6];
        let h = cm[7];
        let i = cm[8];
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if det.abs() > 1e-6 {
            let invdet = 1.0 / det;
            let inv_cm = [
                (e * i - f * h) * invdet,
                (c * h - b * i) * invdet,
                (b * f - c * e) * invdet,
                (f * g - d * i) * invdet,
                (a * i - c * g) * invdet,
                (c * d - a * f) * invdet,
                (d * h - e * g) * invdet,
                (g * b - a * h) * invdet,
                (a * e - b * d) * invdet,
            ];

            let w_r = if wb[0] > 0.0 { 1.0 / wb[0] } else { 1.0 };
            let w_g = if wb[1] > 0.0 { 1.0 / wb[1] } else { 1.0 };
            let w_b = if wb[2] > 0.0 { 1.0 / wb[2] } else { 1.0 };

            let cam_wb_to_xyz = [
                inv_cm[0] * w_r,
                inv_cm[1] * w_g,
                inv_cm[2] * w_b,
                inv_cm[3] * w_r,
                inv_cm[4] * w_g,
                inv_cm[5] * w_b,
                inv_cm[6] * w_r,
                inv_cm[7] * w_g,
                inv_cm[8] * w_b,
            ];

            const XYZ_TO_SRGB: [f32; 9] = [
                3.240_454_2,
                -1.537_138_5,
                -0.498_531_4,
                -0.969_266,
                1.876_010_8,
                0.041_556,
                0.055_643_4,
                -0.204_025_9,
                1.057_225_2,
            ];

            let mut out = [0.0_f32; 9];
            for r in 0..3 {
                for col in 0..3 {
                    let mut sum = 0.0;
                    for k in 0..3 {
                        sum += XYZ_TO_SRGB[r * 3 + k] * cam_wb_to_xyz[k * 3 + col];
                    }
                    out[r * 3 + col] = sum;
                }
            }

            for r in 0..3 {
                let row_sum = out[r * 3] + out[r * 3 + 1] + out[r * 3 + 2];
                if row_sum.abs() > 1e-4 {
                    out[r * 3] /= row_sum;
                    out[r * 3 + 1] /= row_sum;
                    out[r * 3 + 2] /= row_sum;
                }
            }

            return out;
        }
    }

    // Default saturated sRGB primary matrix preserving row neutral sum 1.0
    [1.45, -0.35, -0.10, -0.15, 1.35, -0.20, -0.05, -0.30, 1.35]
}

/// Fast pre-demosaiced linear camera RGB buffer cached in RAM.
#[derive(Debug, Clone)]
pub struct BaseRgbPreview {
    pub width: usize,
    pub height: usize,
    pub samples_rgb: Vec<[f32; 3]>,
    pub as_shot_wb: [f32; 3],
    pub cam_to_srgb: [f32; 9],
}

impl BaseRgbPreview {
    /// Builds a base preview by block-averaging sensor CFA mosaic channels.
    #[must_use]
    pub fn from_cfa(cfa: &SignedCfaBuffer) -> Self {
        Self::from_cfa_with_color(cfa, [2.0, 1.0, 1.5], None)
    }

    /// Builds a base preview with full color calibration and white balance.
    #[must_use]
    pub fn from_cfa_with_color(
        cfa: &SignedCfaBuffer,
        as_shot_wb: [f32; 3],
        color_matrix: Option<&[f32; 9]>,
    ) -> Self {
        Self::from_cfa_with_color_and_target_width(cfa, as_shot_wb, color_matrix, 1600)
    }

    /// Builds a base preview scaled to a target bounding width (e.g. 1200 for preview, 140 for thumbnail).
    #[must_use]
    pub fn from_cfa_with_color_and_target_width(
        cfa: &SignedCfaBuffer,
        as_shot_wb: [f32; 3],
        color_matrix: Option<&[f32; 9]>,
        target_width: usize,
    ) -> Self {
        let width = cfa.width();
        let height = cfa.height();
        let cam_to_srgb = compute_cam_to_srgb_matrix(color_matrix, &as_shot_wb);
        if width == 0 || height == 0 {
            return Self {
                width: 1,
                height: 1,
                samples_rgb: vec![[0.0, 0.0, 0.0]],
                as_shot_wb,
                cam_to_srgb,
            };
        }

        let raw_step = (width / target_width.max(1)).max(1);
        let step = if !raw_step.is_multiple_of(2) {
            raw_step + 1
        } else {
            raw_step
        };

        let preview_w = (width / step).max(1);
        let preview_h = (height / step).max(1);
        let samples = cfa.samples();
        let pattern = cfa.pattern();

        // Precompute Bayer quad color channel mapping for sub-millisecond point-decimation sampling
        let colors = [
            pattern.color_at(0, 0),
            pattern.color_at(1, 0),
            pattern.color_at(0, 1),
            pattern.color_at(1, 1),
        ];

        let mut r_idx = 0;
        let mut g0_idx = 1;
        let mut g1_idx = 2;
        let mut b_idx = 3;
        let mut g_found = 0;

        for (idx, &col) in colors.iter().enumerate() {
            match col {
                CfaColor::Red => r_idx = idx,
                CfaColor::Blue => b_idx = idx,
                CfaColor::Green => {
                    if g_found == 0 {
                        g0_idx = idx;
                        g_found = 1;
                    } else {
                        g1_idx = idx;
                    }
                }
            }
        }

        let mut samples_rgb = Vec::with_capacity(preview_w * preview_h);

        for py in 0..preview_h {
            let sy = py * step;
            let next_sy = (sy + 1).min(height.saturating_sub(1));
            let row0 = sy * width;
            let row1 = next_sy * width;

            for px in 0..preview_w {
                let sx = px * step;
                let next_sx = (sx + 1).min(width.saturating_sub(1));

                let quad = [
                    samples[row0 + sx],
                    samples[row0 + next_sx],
                    samples[row1 + sx],
                    samples[row1 + next_sx],
                ];

                let r = quad[r_idx];
                let g = (quad[g0_idx] + quad[g1_idx]) * 0.5;
                let b = quad[b_idx];

                samples_rgb.push([r, g, b]);
            }
        }

        Self {
            width: preview_w,
            height: preview_h,
            samples_rgb,
            as_shot_wb,
            cam_to_srgb,
        }
    }
}

/// Generates a fast ~140px RGB thumbnail image from a RAW file applying its sidecar grading if available.
#[must_use]
pub fn generate_fast_thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let decoder = PureRustRawDecoder::new();
    let decoded = decoder.decode_file(path).ok()?;
    let (grading_opt, _) = load_photo_sidecars(path);
    let grading = grading_opt.unwrap_or_default();

    let base = BaseRgbPreview::from_cfa_with_color_and_target_width(
        &decoded.cfa_signed,
        [
            decoded.wb_coeffs[0],
            decoded.wb_coeffs[1],
            decoded.wb_coeffs[2],
        ],
        decoded.color_matrix.as_ref(),
        140,
    );

    let (preview, _) = render_preview_from_base(&base, &grading);
    Some(preview)
}

/// Evaluates darkroom grading, optical distortion, vignetting, and histogram from `BaseRgbPreview`.
///
/// Executes in < 1 ms over contiguous RAM without re-sampling the raw sensor matrix.
#[must_use]
pub fn render_preview_from_base(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let preview_w = base.width;
    let preview_h = base.height;

    let exposure_gain = 2.0_f32.powf(grading.exposure_ev);

    let kelvin_ratio = (grading.kelvin / 5500.0).clamp(0.4, 2.5);
    let r_temp_scale = kelvin_ratio.powf(0.85);
    let b_temp_scale = (1.0 / kelvin_ratio).powf(0.85);
    let tint_scale = 1.0 - (grading.tint / 100.0);

    let eff_wb_r = base.as_shot_wb[0] * r_temp_scale;
    let eff_wb_g = base.as_shot_wb[1] * tint_scale;
    let eff_wb_b = base.as_shot_wb[2] * b_temp_scale;

    let mat = base.cam_to_srgb;

    let cx = preview_w as f32 * 0.5;
    let cy = preview_h as f32 * 0.5;
    let max_radius_sq = (cx * cx + cy * cy).max(1.0);

    let k_dist = if grading.distortion_correction {
        -0.06 + (grading.distortion_amount / 100.0) * 0.15
    } else if grading.distortion_amount.abs() > 0.001 {
        (grading.distortion_amount / 100.0) * 0.15
    } else {
        0.0
    };

    let v_gain = if grading.vignette_correction {
        0.35 + (grading.vignette_amount / 100.0) * 0.65
    } else if grading.vignette_amount > 0.01 {
        (grading.vignette_amount / 100.0) * 0.65
    } else {
        0.0
    };

    let rot_deg = grading.rotation_degrees;
    let rot_rad = rot_deg.to_radians();
    let cos_rot = rot_rad.cos();
    let sin_rot = rot_rad.sin();

    let flip_x = if grading.flip_horizontal { -1.0 } else { 1.0 };
    let flip_y = if grading.flip_vertical { -1.0 } else { 1.0 };

    let (keystone_v, keystone_h) = if grading.geometry_enabled {
        (
            grading.keystone_vertical / 100.0,
            grading.keystone_horizontal / 100.0,
        )
    } else {
        (0.0, 0.0)
    };

    let auto_zoom = if grading.geometry_enabled && grading.auto_crop_geometry {
        let abs_rot = rot_deg.abs().to_radians();
        let rot_zoom = abs_rot.cos() + abs_rot.sin() * (cy / cx.max(1.0));
        let k_zoom = 1.0
            + (grading
                .keystone_vertical
                .abs()
                .max(grading.keystone_horizontal.abs())
                / 100.0)
                * 0.5;
        (rot_zoom * k_zoom).max(1.0)
    } else {
        1.0
    };

    let mut color_pixels = Vec::with_capacity(preview_w * preview_h);
    let mut hist_r = [0_u32; 256];
    let mut hist_g = [0_u32; 256];
    let mut hist_b = [0_u32; 256];
    let mut hist_lum = [0_u32; 256];
    let mut shadow_clipped = 0_u32;
    let mut highlight_clipped = 0_u32;

    for py in 0..preview_h {
        let raw_dy = (py as f32 - cy) * flip_y;

        for px in 0..preview_w {
            let raw_dx = (px as f32 - cx) * flip_x;

            // 1. Rotation around optical center
            let rx = raw_dx * cos_rot + raw_dy * sin_rot;
            let ry = -raw_dx * sin_rot + raw_dy * cos_rot;

            // 2. Keystone perspective correction & auto-crop zoom
            let v_scale = (1.0 + keystone_v * (ry / cy)).max(0.1);
            let kx = (rx / v_scale) / auto_zoom;
            let h_scale = (1.0 + keystone_h * (kx / cx)).max(0.1);
            let ky = (ry / h_scale) / auto_zoom;

            let sample_idx = if k_dist.abs() > 1e-5 {
                let r2 = (kx * kx + ky * ky) / max_radius_sq;
                let warp = 1.0 + k_dist * r2;
                let warped_x =
                    ((cx + kx * warp).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let warped_y =
                    ((cy + ky * warp).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                warped_y * preview_w + warped_x
            } else {
                let sx = ((cx + kx).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let sy = ((cy + ky).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                sy * preview_w + sx
            };

            let [mean_r, mean_g, mean_b] = base.samples_rgb[sample_idx];

            let v_mult = if v_gain > 1e-4 {
                let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };

            let r_wb = mean_r * eff_wb_r;
            let g_wb = mean_g * eff_wb_g;
            let b_wb = mean_b * eff_wb_b;

            // Apply camera color calibration matrix to eliminate CFA spectral cross-talk
            let r_linear = (mat[0] * r_wb + mat[1] * g_wb + mat[2] * b_wb).max(0.0);
            let g_linear = (mat[3] * r_wb + mat[4] * g_wb + mat[5] * b_wb).max(0.0);
            let b_linear = (mat[6] * r_wb + mat[7] * g_wb + mat[8] * b_wb).max(0.0);

            let r_scaled = (r_linear * exposure_gain * v_mult).max(0.0);
            let g_scaled = (g_linear * exposure_gain * v_mult).max(0.0);
            let b_scaled = (b_linear * exposure_gain * v_mult).max(0.0);

            // Responsive localized Shadows and Highlights tonal adjustment
            let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
            let y_norm = lum / (lum + 0.18);
            let shadow_weight = (1.0 - y_norm).powi(2);
            let highlight_weight = y_norm.powi(2);
            let shadow_shift = (grading.shadows / 100.0) * 2.5 * shadow_weight;
            let highlight_shift = (grading.highlights / 100.0) * 2.5 * highlight_weight;
            let blacks_weight = (1.0 - y_norm).powi(4);
            let blacks_shift = (grading.blacks / 100.0) * 1.5 * blacks_weight;
            let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);

            let r_toned = (r_scaled * tone_gain).max(0.0);
            let g_toned = (g_scaled * tone_gain).max(0.0);
            let b_toned = (b_scaled * tone_gain).max(0.0);

            // Apply Display Rendering Transform (AgX or OpenDRT)
            let (r_drt, g_drt, b_drt) = match grading.display_transform {
                DisplayTransform::AgX => (
                    apply_agx_edr(r_toned, 1.0),
                    apply_agx_edr(g_toned, 1.0),
                    apply_agx_edr(b_toned, 1.0),
                ),
                DisplayTransform::OpenDrt => (
                    apply_opendrt(r_toned, 1.0),
                    apply_opendrt(g_toned, 1.0),
                    apply_opendrt(b_toned, 1.0),
                ),
            };

            let factor = 1.0 + (grading.contrast / 100.0).clamp(-0.8, 2.0);
            let r_c = ((r_drt - 0.18) * factor + 0.18).max(0.0);
            let g_c = ((g_drt - 0.18) * factor + 0.18).max(0.0);
            let b_c = ((b_drt - 0.18) * factor + 0.18).max(0.0);

            // Perceptual Vibrance & Saturation enhancement
            let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
            let max_val = r_c.max(g_c).max(b_c);
            let min_val = r_c.min(g_c).min(b_c);
            let current_sat = if max_val > 1e-4 {
                (max_val - min_val) / max_val
            } else {
                0.0
            };
            let base_vibrance = 0.10_f32;
            let user_vibrance = grading.vibrance / 100.0;
            let user_sat = grading.saturation / 100.0;
            let vib_weight = (1.0 - current_sat).clamp(0.0, 1.0);
            let eff_sat =
                ((1.0 + user_sat) * (1.0 + (base_vibrance + user_vibrance) * vib_weight)).max(0.0);

            let r_s = (lum_c + (r_c - lum_c) * eff_sat).max(0.0);
            let g_s = (lum_c + (g_c - lum_c) * eff_sat).max(0.0);
            let b_s = (lum_c + (b_c - lum_c) * eff_sat).max(0.0);

            let to_u8 = |x: f32| -> u8 {
                let clamped = x.clamp(0.0, 1.0);
                let toe = if clamped < 0.35 {
                    clamped.powf(1.08)
                } else {
                    clamped
                };
                let mapped = toe.powf(1.0 / 2.2);
                (mapped * 255.0).clamp(0.0, 255.0) as u8
            };

            let (u8_r, u8_g, u8_b) = if grading.is_monochrome {
                let lum_val =
                    ((to_u8(r_s) as u32 * 54 + to_u8(g_s) as u32 * 183 + to_u8(b_s) as u32 * 19)
                        >> 8) as u8;
                (lum_val, lum_val, lum_val)
            } else {
                (to_u8(r_s), to_u8(g_s), to_u8(b_s))
            };

            hist_r[u8_r as usize] += 1;
            hist_g[u8_g as usize] += 1;
            hist_b[u8_b as usize] += 1;

            let lum = (((u8_r as u32 * 54) + (u8_g as u32 * 183) + (u8_b as u32 * 19)) >> 8)
                .min(255) as usize;
            hist_lum[lum] += 1;

            if lum == 0 {
                shadow_clipped += 1;
            }
            if lum == 255 {
                highlight_clipped += 1;
            }

            color_pixels.push(Color32::from_rgb(u8_r, u8_g, u8_b));
        }
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
    };

    (
        egui::ColorImage::new([preview_w, preview_h], color_pixels),
        hist_data,
    )
}

/// Renders the fully developed photographic raster for export, applying all darkroom grades,
/// cropping boundaries, geometric corrections, color transform, and scaling.
#[must_use]
pub fn render_export_image(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
    scale: f32,
) -> (usize, usize, Vec<u8>, Vec<u16>) {
    let preview_w = base.width;
    let preview_h = base.height;

    let (crop_x0, crop_y0, crop_x1, crop_y1) = if grading.crop_enabled {
        let x0 = (grading.crop_left.clamp(0.0, 0.95) * preview_w as f32).round() as usize;
        let y0 = (grading.crop_top.clamp(0.0, 0.95) * preview_h as f32).round() as usize;
        let x1 = (grading.crop_right.clamp(0.05, 1.0) * preview_w as f32)
            .round()
            .max(x0 as f32 + 1.0) as usize;
        let y1 = (grading.crop_bottom.clamp(0.05, 1.0) * preview_h as f32)
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

    let exposure_gain = 2.0_f32.powf(grading.exposure_ev);

    let kelvin_ratio = (grading.kelvin / 5500.0).clamp(0.4, 2.5);
    let r_temp_scale = kelvin_ratio.powf(0.85);
    let b_temp_scale = (1.0 / kelvin_ratio).powf(0.85);
    let tint_scale = 1.0 - (grading.tint / 100.0);

    let eff_wb_r = base.as_shot_wb[0] * r_temp_scale;
    let eff_wb_g = base.as_shot_wb[1] * tint_scale;
    let eff_wb_b = base.as_shot_wb[2] * b_temp_scale;

    let mat = base.cam_to_srgb;

    let cx = preview_w as f32 * 0.5;
    let cy = preview_h as f32 * 0.5;
    let max_radius_sq = (cx * cx + cy * cy).max(1.0);

    let k_dist = if grading.distortion_correction {
        -0.06 + (grading.distortion_amount / 100.0) * 0.15
    } else if grading.distortion_amount.abs() > 0.001 {
        (grading.distortion_amount / 100.0) * 0.15
    } else {
        0.0
    };

    let v_gain = if grading.vignette_correction {
        0.35 + (grading.vignette_amount / 100.0) * 0.65
    } else if grading.vignette_amount > 0.01 {
        (grading.vignette_amount / 100.0) * 0.65
    } else {
        0.0
    };

    let rot_deg = grading.rotation_degrees;
    let rot_rad = rot_deg.to_radians();
    let cos_rot = rot_rad.cos();
    let sin_rot = rot_rad.sin();

    let flip_x = if grading.flip_horizontal { -1.0 } else { 1.0 };
    let flip_y = if grading.flip_vertical { -1.0 } else { 1.0 };

    let (keystone_v, keystone_h) = if grading.geometry_enabled {
        (
            grading.keystone_vertical / 100.0,
            grading.keystone_horizontal / 100.0,
        )
    } else {
        (0.0, 0.0)
    };

    let auto_zoom = if grading.geometry_enabled && grading.auto_crop_geometry {
        let abs_rot = rot_deg.abs().to_radians();
        let rot_zoom = abs_rot.cos() + abs_rot.sin() * (cy / cx.max(1.0));
        let k_zoom = 1.0
            + (grading
                .keystone_vertical
                .abs()
                .max(grading.keystone_horizontal.abs())
                / 100.0)
                * 0.5;
        (rot_zoom * k_zoom).max(1.0)
    } else {
        1.0
    };

    let mut u8_bytes = Vec::with_capacity(out_w * out_h * 3);
    let mut u16_samples = Vec::with_capacity(out_w * out_h * 3);

    for oy in 0..out_h {
        let orig_y = crop_y0 as f32 + (oy as f32 / out_h as f32) * cropped_h as f32;
        let raw_dy = (orig_y - cy) * flip_y;

        for ox in 0..out_w {
            let orig_x = crop_x0 as f32 + (ox as f32 / out_w as f32) * cropped_w as f32;
            let raw_dx = (orig_x - cx) * flip_x;

            let rx = raw_dx * cos_rot + raw_dy * sin_rot;
            let ry = -raw_dx * sin_rot + raw_dy * cos_rot;

            let v_scale = (1.0 + keystone_v * (ry / cy)).max(0.1);
            let kx = (rx / v_scale) / auto_zoom;
            let h_scale = (1.0 + keystone_h * (kx / cx)).max(0.1);
            let ky = (ry / h_scale) / auto_zoom;

            let sample_idx = if k_dist.abs() > 1e-5 {
                let r2 = (kx * kx + ky * ky) / max_radius_sq;
                let warp = 1.0 + k_dist * r2;
                let warped_x =
                    ((cx + kx * warp).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let warped_y =
                    ((cy + ky * warp).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                warped_y * preview_w + warped_x
            } else {
                let sx = ((cx + kx).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let sy = ((cy + ky).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                sy * preview_w + sx
            };

            let [mean_r, mean_g, mean_b] = base.samples_rgb[sample_idx];

            let v_mult = if v_gain > 1e-4 {
                let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };

            let r_wb = mean_r * eff_wb_r;
            let g_wb = mean_g * eff_wb_g;
            let b_wb = mean_b * eff_wb_b;

            let r_linear = (mat[0] * r_wb + mat[1] * g_wb + mat[2] * b_wb).max(0.0);
            let g_linear = (mat[3] * r_wb + mat[4] * g_wb + mat[5] * b_wb).max(0.0);
            let b_linear = (mat[6] * r_wb + mat[7] * g_wb + mat[8] * b_wb).max(0.0);

            let r_scaled = (r_linear * exposure_gain * v_mult).max(0.0);
            let g_scaled = (g_linear * exposure_gain * v_mult).max(0.0);
            let b_scaled = (b_linear * exposure_gain * v_mult).max(0.0);

            let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
            let y_norm = lum / (lum + 0.18);
            let shadow_weight = (1.0 - y_norm).powi(2);
            let highlight_weight = y_norm.powi(2);
            let shadow_shift = (grading.shadows / 100.0) * 2.5 * shadow_weight;
            let highlight_shift = (grading.highlights / 100.0) * 2.5 * highlight_weight;
            let blacks_weight = (1.0 - y_norm).powi(4);
            let blacks_shift = (grading.blacks / 100.0) * 1.5 * blacks_weight;
            let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);

            let r_toned = (r_scaled * tone_gain).max(0.0);
            let g_toned = (g_scaled * tone_gain).max(0.0);
            let b_toned = (b_scaled * tone_gain).max(0.0);

            let (r_drt, g_drt, b_drt) = match grading.display_transform {
                DisplayTransform::AgX => (
                    apply_agx_edr(r_toned, 1.0),
                    apply_agx_edr(g_toned, 1.0),
                    apply_agx_edr(b_toned, 1.0),
                ),
                DisplayTransform::OpenDrt => (
                    apply_opendrt(r_toned, 1.0),
                    apply_opendrt(g_toned, 1.0),
                    apply_opendrt(b_toned, 1.0),
                ),
            };

            let factor = 1.0 + (grading.contrast / 100.0).clamp(-0.8, 2.0);
            let r_c = ((r_drt - 0.18) * factor + 0.18).max(0.0);
            let g_c = ((g_drt - 0.18) * factor + 0.18).max(0.0);
            let b_c = ((b_drt - 0.18) * factor + 0.18).max(0.0);

            let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
            let max_val = r_c.max(g_c).max(b_c);
            let min_val = r_c.min(g_c).min(b_c);
            let current_sat = if max_val > 1e-4 {
                (max_val - min_val) / max_val
            } else {
                0.0
            };
            let base_vibrance = 0.10_f32;
            let user_vibrance = grading.vibrance / 100.0;
            let user_sat = grading.saturation / 100.0;
            let vib_weight = (1.0 - current_sat).clamp(0.0, 1.0);
            let eff_sat =
                ((1.0 + user_sat) * (1.0 + (base_vibrance + user_vibrance) * vib_weight)).max(0.0);

            let r_s = (lum_c + (r_c - lum_c) * eff_sat).max(0.0);
            let g_s = (lum_c + (g_c - lum_c) * eff_sat).max(0.0);
            let b_s = (lum_c + (b_c - lum_c) * eff_sat).max(0.0);

            let map_f32 = |x: f32| -> f32 {
                let clamped = x.clamp(0.0, 1.0);
                let toe = if clamped < 0.35 {
                    clamped.powf(1.08)
                } else {
                    clamped
                };
                toe.powf(1.0 / 2.2)
            };

            let (f_r, f_g, f_b) = if grading.is_monochrome {
                let lum_val = map_f32(0.2126 * r_s + 0.7152 * g_s + 0.0722 * b_s);
                (lum_val, lum_val, lum_val)
            } else {
                (map_f32(r_s), map_f32(g_s), map_f32(b_s))
            };

            u8_bytes.push((f_r * 255.0).clamp(0.0, 255.0) as u8);
            u8_bytes.push((f_g * 255.0).clamp(0.0, 255.0) as u8);
            u8_bytes.push((f_b * 255.0).clamp(0.0, 255.0) as u8);

            u16_samples.push((f_r * 65535.0).clamp(0.0, 65535.0).round() as u16);
            u16_samples.push((f_g * 65535.0).clamp(0.0, 65535.0).round() as u16);
            u16_samples.push((f_b * 65535.0).clamp(0.0, 65535.0).round() as u16);
        }
    }

    (out_w, out_h, u8_bytes, u16_samples)
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

/// Scans a filesystem directory for supported RAW and raster image files.
#[must_use]
pub fn scan_directory_for_images(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };

    const EXTENSIONS: &[&str] = &[
        "dng", "arw", "cr2", "cr3", "nef", "raf", "rw2", "orf", "pef", "jpg", "jpeg", "png", "tif",
        "tiff",
    ];

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_ascii_lowercase();
                if EXTENSIONS.contains(&ext_lower.as_str()) {
                    entries.push(path);
                }
            }
        }
    }

    entries.sort();
    entries
}

/// Cached photo entry kept in memory for instantaneous (< 0.1 ms) switching between photos.
#[derive(Debug, Clone)]
pub struct CachedPhotoEntry {
    pub path: PathBuf,
    pub photo_view: LoadedPhotoView,
    pub base_preview: BaseRgbPreview,
    pub cfa: SignedCfaBuffer,
}

/// Payload sent from the background decoding thread to the main UI thread.
pub struct BackgroundDecodePayload {
    pub path: PathBuf,
    pub photo_view: LoadedPhotoView,
    pub base_preview: BaseRgbPreview,
    pub cfa: SignedCfaBuffer,
    pub is_prefetch: bool,
}

/// Decodes a RAW image file from disk and precomputes its base RGB preview.
pub fn decode_photo_file_payload(
    target_path: &Path,
    is_prefetch: bool,
) -> Option<BackgroundDecodePayload> {
    let decoder = PureRustRawDecoder::new();
    let decoded = decoder.decode_file(target_path).ok()?;
    let meta = decoded.metadata;
    let make = meta.camera_make.clone();
    let model = meta.camera_model.clone();
    let make_model = format!("{make} {model}");
    let camera_str = make_model.trim().to_string();
    let camera = if camera_str.is_empty() {
        "Digital RAW Camera".to_string()
    } else {
        camera_str
    };

    let iso = meta.iso.unwrap_or(100);
    let aperture_str = meta
        .aperture_f_number
        .map(|f| format!("f/{f:.1}"))
        .unwrap_or_else(|| "f/2.8".to_string());
    let focal_str = meta
        .focal_length_mm
        .map(|f| format!("{f:.1} mm"))
        .unwrap_or_default();
    let lens_str = meta.lens_model.clone().unwrap_or_default();

    let shutter_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Shutter Speed")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "1/125 s".to_string());

    let date_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Date/Time Original")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let bias_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Exposure Bias")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let photo_view = LoadedPhotoView {
        path: target_path.to_path_buf(),
        camera_make: make,
        camera_model: camera,
        lens_model: lens_str,
        dimensions: (meta.width as u32, meta.height as u32),
        iso,
        shutter_str,
        aperture_str,
        focal_length_str: focal_str,
        date_time: date_str,
        exposure_bias: bias_str,
        rating: meta.rating,
        exif_tags: decoded.exif_tags,
    };

    let base = BaseRgbPreview::from_cfa_with_color(
        &decoded.cfa_signed,
        [
            decoded.wb_coeffs[0],
            decoded.wb_coeffs[1],
            decoded.wb_coeffs[2],
        ],
        decoded.color_matrix.as_ref(),
    );

    Some(BackgroundDecodePayload {
        path: target_path.to_path_buf(),
        photo_view,
        base_preview: base,
        cfa: decoded.cfa_signed,
        is_prefetch,
    })
}
