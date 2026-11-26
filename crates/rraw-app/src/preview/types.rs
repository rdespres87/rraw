#![forbid(unsafe_code)]

//! Core types, camera-to-sRGB matrix transforms, and cached photo representations.

use std::path::PathBuf;

use rraw_core::{CfaColor, SignedCfaBuffer};
use serde::{Deserialize, Serialize};

use super::masks::infer_preview_semantic_masks;
use super::orientation::apply_exif_orientation_rgb;

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

/// Maximum bounding width (in pixels) for interactive sub-sampled CFA base previews.
/// Sensors wider than this threshold use fast 2x2 Bayer quad sub-sampling during interactive
/// viewing so full-resolution CPU demosaicing is strictly reserved for explicit full-res export.
pub const PREVIEW_PROXY_MAX_WIDTH: usize = 1600;

static FULL_RES_DEMOSAIC_INVOCATIONS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Returns the cumulative count of full-resolution CPU CFA demosaicing invocations.
#[must_use]
pub fn full_res_demosaic_count() -> u64 {
    FULL_RES_DEMOSAIC_INVOCATIONS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Fast pre-demosaiced linear camera RGB buffer cached in RAM.
#[derive(Debug, Clone)]
pub struct BaseRgbPreview {
    pub width: usize,
    pub height: usize,
    pub samples_rgb: std::sync::Arc<[[f32; 3]]>,
    pub as_shot_wb: [f32; 3],
    pub cam_to_srgb: [f32; 9],
    /// Pre-inferred 4-class CoreML AI semantic segmentation probability maps.
    pub semantic_masks: std::sync::Arc<rraw_ai::SemanticMaskSet>,
    /// Applied EXIF TIFF orientation code (`1..=8`).
    pub exif_orientation: u16,
    /// Calibrated lens profile matched from EXIF metadata.
    pub matched_lens: Option<rraw_optics::MatchedLens>,
    /// Lens focal length in mm.
    pub focal_length_mm: Option<f32>,
    /// Lens aperture f-number.
    pub aperture_f_number: Option<f32>,
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
        Self::from_cfa_with_color_and_target_width(
            cfa,
            as_shot_wb,
            color_matrix,
            PREVIEW_PROXY_MAX_WIDTH,
        )
    }

    /// Returns `true` when this preview represents a sub-sampled interactive proxy for `cfa`
    /// rather than a 1:1 full-resolution CPU demosaic.
    #[must_use]
    pub fn is_interactive_proxy_for(&self, cfa: &SignedCfaBuffer) -> bool {
        cfa.width() > PREVIEW_PROXY_MAX_WIDTH && self.width <= PREVIEW_PROXY_MAX_WIDTH
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
            let semantic_masks =
                infer_preview_semantic_masks(&[[0.0, 0.0, 0.0]], 1, 1, as_shot_wb, cam_to_srgb);
            return Self {
                width: 1,
                height: 1,
                samples_rgb: std::sync::Arc::new([[0.0, 0.0, 0.0]]),
                as_shot_wb,
                cam_to_srgb,
                semantic_masks,
                exif_orientation: 1,
                matched_lens: None,
                focal_length_mm: None,
                aperture_f_number: None,
            };
        }

        let samples = cfa.samples();
        let pattern = cfa.pattern();

        if target_width >= width {
            // 1:1 Full-Resolution CFA Edge-Aware Adaptive Demosaicing
            FULL_RES_DEMOSAIC_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let preview_w = width;
            let preview_h = height;
            let samples_rgb =
                rraw_core::demosaic_cfa(cfa, rraw_core::DemosaicMethod::EdgeAwareAdaptive)
                    .unwrap_or_else(|_| vec![[0.0_f32; 3]; preview_w * preview_h]);

            let semantic_masks = infer_preview_semantic_masks(
                &samples_rgb,
                preview_w,
                preview_h,
                as_shot_wb,
                cam_to_srgb,
            );

            return Self {
                width: preview_w,
                height: preview_h,
                samples_rgb: std::sync::Arc::from(samples_rgb),
                as_shot_wb,
                cam_to_srgb,
                semantic_masks,
                exif_orientation: 1,
                matched_lens: None,
                focal_length_mm: None,
                aperture_f_number: None,
            };
        }

        // Sub-sampled fast preview / scaled export path (target_width < width)
        let preview_w = target_width.clamp(1, width);
        let preview_h = ((height as u64 * preview_w as u64) / (width as u64)).max(1) as usize;

        // Precompute Bayer quad color channel mapping for fast 2x2 quad sampling
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
        let max_even_x = width.saturating_sub(2) & !1;
        let max_even_y = height.saturating_sub(2) & !1;

        for py in 0..preview_h {
            let raw_sy = (py * height) / preview_h;
            let sy = (raw_sy & !1).min(max_even_y);
            let next_sy = (sy + 1).min(height.saturating_sub(1));
            let row0 = sy * width;
            let row1 = next_sy * width;

            for px in 0..preview_w {
                let raw_sx = (px * width) / preview_w;
                let sx = (raw_sx & !1).min(max_even_x);
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

        let semantic_masks = infer_preview_semantic_masks(
            &samples_rgb,
            preview_w,
            preview_h,
            as_shot_wb,
            cam_to_srgb,
        );

        Self {
            width: preview_w,
            height: preview_h,
            samples_rgb: std::sync::Arc::from(samples_rgb),
            as_shot_wb,
            cam_to_srgb,
            semantic_masks,
            exif_orientation: 1,
            matched_lens: None,
            focal_length_mm: None,
            aperture_f_number: None,
        }
    }

    /// Builds a base preview from pre-demosaiced linear camera-RGB samples (`cpp == 3`).
    #[must_use]
    pub fn from_linear_rgb_with_color(
        samples: &[[f32; 3]],
        width: usize,
        height: usize,
        as_shot_wb: [f32; 3],
        color_matrix: Option<&[f32; 9]>,
    ) -> Self {
        Self::from_linear_rgb_with_color_and_target_width(
            samples,
            width,
            height,
            as_shot_wb,
            color_matrix,
            PREVIEW_PROXY_MAX_WIDTH,
        )
    }

    /// Builds a base preview scaled to `target_width` from pre-demosaiced linear camera-RGB
    /// samples (`cpp == 3`, e.g., Apple ProRAW, Canon sRAW, Adobe Linear DNG).
    #[must_use]
    pub fn from_linear_rgb_with_color_and_target_width(
        samples: &[[f32; 3]],
        width: usize,
        height: usize,
        as_shot_wb: [f32; 3],
        color_matrix: Option<&[f32; 9]>,
        target_width: usize,
    ) -> Self {
        let cam_to_srgb = compute_cam_to_srgb_matrix(color_matrix, &as_shot_wb);
        if width == 0 || height == 0 || samples.len() != width.saturating_mul(height) {
            let semantic_masks =
                infer_preview_semantic_masks(&[[0.0, 0.0, 0.0]], 1, 1, as_shot_wb, cam_to_srgb);
            return Self {
                width: 1,
                height: 1,
                samples_rgb: std::sync::Arc::new([[0.0, 0.0, 0.0]]),
                as_shot_wb,
                cam_to_srgb,
                semantic_masks,
                exif_orientation: 1,
                matched_lens: None,
                focal_length_mm: None,
                aperture_f_number: None,
            };
        }

        let (preview_w, preview_h, samples_rgb): (usize, usize, Vec<[f32; 3]>) = if target_width
            >= width
        {
            (width, height, samples.to_vec())
        } else {
            let pw = target_width.clamp(1, width);
            let ph = ((height as u64 * pw as u64) / (width as u64)).max(1) as usize;
            let mut out = Vec::with_capacity(pw * ph);
            for py in 0..ph {
                let sy = ((py as u64 * height as u64) / ph as u64)
                    .min(height.saturating_sub(1) as u64) as usize;
                let row = sy * width;
                for px in 0..pw {
                    let sx = ((px as u64 * width as u64) / pw as u64)
                        .min(width.saturating_sub(1) as u64) as usize;
                    out.push(samples[row + sx]);
                }
            }
            (pw, ph, out)
        };

        let semantic_masks = infer_preview_semantic_masks(
            &samples_rgb,
            preview_w,
            preview_h,
            as_shot_wb,
            cam_to_srgb,
        );

        Self {
            width: preview_w,
            height: preview_h,
            samples_rgb: std::sync::Arc::from(samples_rgb),
            as_shot_wb,
            cam_to_srgb,
            semantic_masks,
            exif_orientation: 1,
            matched_lens: None,
            focal_length_mm: None,
            aperture_f_number: None,
        }
    }

    /// Builds a base preview from an in-memory 8-bit RGB raster image (JPEG, PNG).
    #[must_use]
    pub fn from_rgb_image(img: &image::RgbImage, target_width: usize) -> Self {
        let (w, h) = img.dimensions();
        let target_w = target_width.max(1);
        let step = (w as usize / target_w).max(1);
        let preview_w = (w as usize / step).max(1);
        let preview_h = (h as usize / step).max(1);

        let mut samples_rgb = Vec::with_capacity(preview_w * preview_h);
        for py in 0..preview_h {
            let sy = (py * step) as u32;
            for px in 0..preview_w {
                let sx = (px * step) as u32;
                let pixel = img.get_pixel(sx.min(w.saturating_sub(1)), sy.min(h.saturating_sub(1)));
                let r = ((pixel[0] as f32) / 255.0).powf(2.2);
                let g = ((pixel[1] as f32) / 255.0).powf(2.2);
                let b = ((pixel[2] as f32) / 255.0).powf(2.2);
                samples_rgb.push([r, g, b]);
            }
        }

        let as_shot_wb = [1.0, 1.0, 1.0];
        let cam_to_srgb = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let semantic_masks = infer_preview_semantic_masks(
            &samples_rgb,
            preview_w,
            preview_h,
            as_shot_wb,
            cam_to_srgb,
        );

        Self {
            width: preview_w,
            height: preview_h,
            samples_rgb: std::sync::Arc::from(samples_rgb),
            as_shot_wb,
            cam_to_srgb,
            semantic_masks,
            exif_orientation: 1,
            matched_lens: None,
            focal_length_mm: None,
            aperture_f_number: None,
        }
    }

    /// Returns the allocated memory footprint of this base preview in bytes.
    #[must_use]
    pub fn memory_footprint_bytes(&self) -> usize {
        let preview_bytes = self.samples_rgb.len() * std::mem::size_of::<[f32; 3]>();
        let mask_bytes = self.semantic_masks.subject.len() * 4 * std::mem::size_of::<f32>();
        preview_bytes + mask_bytes
    }

    /// Rotates/flips the base RGB preview according to EXIF Orientation (`1..=8`) and updates semantic masks.
    #[must_use]
    pub fn with_exif_orientation(mut self, orientation: u16) -> Self {
        self.exif_orientation = if (1..=8).contains(&orientation) {
            orientation
        } else {
            1
        };
        if matches!(self.exif_orientation, 2..=8) {
            let (new_w, new_h, new_samples) = apply_exif_orientation_rgb(
                &self.samples_rgb,
                self.width,
                self.height,
                self.exif_orientation,
            );
            self.width = new_w;
            self.height = new_h;
            self.samples_rgb = std::sync::Arc::from(new_samples);
            self.semantic_masks = infer_preview_semantic_masks(
                &self.samples_rgb,
                self.width,
                self.height,
                self.as_shot_wb,
                self.cam_to_srgb,
            );
        }
        self
    }
}

/// Cached photo entry kept in memory for instantaneous (< 0.1 ms) switching between photos.
#[derive(Debug, Clone)]
pub struct CachedPhotoEntry {
    pub path: PathBuf,
    pub photo_view: LoadedPhotoView,
    pub base_preview: BaseRgbPreview,
    pub cfa: SignedCfaBuffer,
}

impl CachedPhotoEntry {
    /// Computes the total allocated memory footprint in bytes.
    #[must_use]
    pub fn memory_footprint_bytes(&self) -> usize {
        let cfa_bytes = self.cfa.allocation_size_bytes();
        let preview_bytes = self.base_preview.memory_footprint_bytes();
        cfa_bytes + preview_bytes
    }
}

/// Payload sent from the background decoding thread to the main UI thread.
pub struct BackgroundDecodePayload {
    /// Epoch sequence counter under which this decode was requested.
    pub epoch: u64,
    pub path: PathBuf,
    pub photo_view: LoadedPhotoView,
    pub base_preview: BaseRgbPreview,
    pub cfa: SignedCfaBuffer,
    pub is_prefetch: bool,
}
