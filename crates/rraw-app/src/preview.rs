#![forbid(unsafe_code)]

//! Image decoding, preview caching, and background worker payloads.

use std::path::{Path, PathBuf};

use egui::Color32;
use rayon::prelude::*;
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
    /// Pre-inferred 4-class CoreML AI semantic segmentation probability maps.
    pub semantic_masks: std::sync::Arc<rraw_ai::SemanticMaskSet>,
    /// Applied EXIF TIFF orientation code (`1..=8`).
    pub exif_orientation: u16,
}

/// Returns upright `(width, height)` after applying EXIF orientation (`5..=8` transpose dimensions).
#[must_use]
pub const fn oriented_dimensions(width: u32, height: u32, orientation: u16) -> (u32, u32) {
    if matches!(orientation, 5..=8) {
        (height, width)
    } else {
        (width, height)
    }
}

/// Transforms a row-major RGB buffer according to standard EXIF Orientation (`1..=8`).
#[must_use]
pub fn apply_exif_orientation_rgb(
    samples: &[[f32; 3]],
    width: usize,
    height: usize,
    orientation: u16,
) -> (usize, usize, Vec<[f32; 3]>) {
    if width == 0 || height == 0 || samples.len() < width.saturating_mul(height) {
        return (width, height, samples.to_vec());
    }
    match orientation {
        2 => {
            // Mirror Horizontal
            let mut out = vec![[0.0_f32; 3]; width * height];
            for y in 0..height {
                for x in 0..width {
                    out[y * width + x] = samples[y * width + (width - 1 - x)];
                }
            }
            (width, height, out)
        }
        3 => {
            // Rotate 180°
            let mut out = vec![[0.0_f32; 3]; width * height];
            for y in 0..height {
                for x in 0..width {
                    out[y * width + x] = samples[(height - 1 - y) * width + (width - 1 - x)];
                }
            }
            (width, height, out)
        }
        4 => {
            // Mirror Vertical
            let mut out = vec![[0.0_f32; 3]; width * height];
            for y in 0..height {
                for x in 0..width {
                    out[y * width + x] = samples[(height - 1 - y) * width + x];
                }
            }
            (width, height, out)
        }
        5 => {
            // Transpose: Mirror Horizontal + Rotate 270° CW -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    out[dy * out_w + dx] = samples[dx * width + dy];
                }
            }
            (out_w, out_h, out)
        }
        6 => {
            // Rotate 90° CW (Standard Portrait) -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    let sx = dy;
                    let sy = height - 1 - dx;
                    out[dy * out_w + dx] = samples[sy * width + sx];
                }
            }
            (out_w, out_h, out)
        }
        7 => {
            // Transverse: Mirror Horizontal + Rotate 90° CW -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    let sx = width - 1 - dy;
                    let sy = height - 1 - dx;
                    out[dy * out_w + dx] = samples[sy * width + sx];
                }
            }
            (out_w, out_h, out)
        }
        8 => {
            // Rotate 270° CW / 90° CCW (Portrait) -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    let sx = width - 1 - dy;
                    let sy = dx;
                    out[dy * out_w + dx] = samples[sy * width + sx];
                }
            }
            (out_w, out_h, out)
        }
        _ => (width, height, samples.to_vec()),
    }
}

/// Applies EXIF Orientation (`1..=8`) to a `DynamicImage`.
#[must_use]
pub fn apply_exif_orientation_dyn_image(
    img: image::DynamicImage,
    orientation: u16,
) -> image::DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate270().fliph(),
        6 => img.rotate90(),
        7 => img.rotate90().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn parse_tiff_ifd0_orientation(tiff: &[u8]) -> Option<u16> {
    if tiff.len() < 8 {
        return None;
    }
    let is_le = match &tiff[0..4] {
        [b'I', b'I', 42, 0] => true,
        [b'M', b'M', 0, 42] => false,
        _ => return None,
    };
    let read_u16 = |off: usize| -> Option<u16> {
        let slice = tiff.get(off..off + 2)?;
        Some(if is_le {
            u16::from_le_bytes([slice[0], slice[1]])
        } else {
            u16::from_be_bytes([slice[0], slice[1]])
        })
    };
    let read_u32 = |off: usize| -> Option<u32> {
        let slice = tiff.get(off..off + 4)?;
        Some(if is_le {
            u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
        } else {
            u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
        })
    };

    let ifd_offset = read_u32(4)? as usize;
    let entry_count = read_u16(ifd_offset)? as usize;
    let entries_base = ifd_offset.checked_add(2)?;

    for i in 0..entry_count.min(128) {
        let entry_off = entries_base.checked_add(i * 12)?;
        let tag = read_u16(entry_off)?;
        if tag == 0x0112 {
            let val = read_u16(entry_off + 8)?;
            if (1..=8).contains(&val) {
                return Some(val);
            }
        }
    }
    None
}

/// Reads the EXIF `Orientation` tag (`0x0112`, values `1..=8`) from a JPEG or TIFF/RAW file header.
#[must_use]
pub fn read_exif_orientation(path: &Path) -> u16 {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return 1;
    };
    let mut buf = vec![0_u8; 65536];
    let Ok(n) = file.read(&mut buf) else {
        return 1;
    };
    let data = &buf[..n];
    if data.len() < 4 {
        return 1;
    }

    // 1. Direct TIFF / DNG / RAW container
    if let Some(orient) = parse_tiff_ifd0_orientation(data) {
        return orient;
    }

    // 2. JPEG container (0xFFD8 followed by APP markers)
    if data[0] == 0xFF && data[1] == 0xD8 {
        let mut pos = 2_usize;
        while pos + 4 <= data.len() && data[pos] == 0xFF {
            let marker = data[pos + 1];
            if marker == 0xDA || marker == 0xD9 {
                break;
            }
            let seg_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
            if seg_len < 2 || pos + 2 + seg_len > data.len() {
                break;
            }
            if marker == 0xE1 && seg_len >= 8 {
                let payload = &data[pos + 4..pos + 2 + seg_len];
                if payload.starts_with(b"Exif\0\0") {
                    if let Some(orient) = parse_tiff_ifd0_orientation(&payload[6..]) {
                        return orient;
                    }
                }
            }
            pos += 2 + seg_len;
        }
    }

    1
}

fn infer_preview_semantic_masks(
    samples_rgb: &[[f32; 3]],
    width: usize,
    height: usize,
    as_shot_wb: [f32; 3],
    cam_to_srgb: [f32; 9],
) -> std::sync::Arc<rraw_ai::SemanticMaskSet> {
    use rraw_ai::{CoreMlSemanticSegmenter, SemanticMaskSet, SemanticSegmenter};

    if width == 0 || height == 0 || samples_rgb.len() < width.saturating_mul(height) {
        return std::sync::Arc::new(SemanticMaskSet {
            width: 1,
            height: 1,
            subject: vec![0.0],
            sky: vec![0.0],
            background: vec![1.0],
            portrait: vec![0.0],
        });
    }

    let step = (width / 160).max(1);
    let sem_w = (width / step).max(1);
    let sem_h = (height / step).max(1);
    let mut cal_rgb = Vec::with_capacity(sem_w * sem_h);

    for sy in 0..sem_h {
        let py = (sy * step).min(height - 1);
        for sx in 0..sem_w {
            let px = (sx * step).min(width - 1);
            let [mr, mg, mb] = samples_rgb[py * width + px];
            let r_wb = mr * as_shot_wb[0];
            let g_wb = mg * as_shot_wb[1];
            let b_wb = mb * as_shot_wb[2];
            let r =
                (cam_to_srgb[0] * r_wb + cam_to_srgb[1] * g_wb + cam_to_srgb[2] * b_wb).max(0.0);
            let g =
                (cam_to_srgb[3] * r_wb + cam_to_srgb[4] * g_wb + cam_to_srgb[5] * b_wb).max(0.0);
            let b =
                (cam_to_srgb[6] * r_wb + cam_to_srgb[7] * g_wb + cam_to_srgb[8] * b_wb).max(0.0);
            cal_rgb.push([r, g, b]);
        }
    }

    let segmenter = CoreMlSemanticSegmenter::new();
    let masks = segmenter
        .segment_rgb(&cal_rgb, sem_w, sem_h)
        .unwrap_or_else(|_| SemanticMaskSet {
            width: 1,
            height: 1,
            subject: vec![0.0],
            sky: vec![0.0],
            background: vec![1.0],
            portrait: vec![0.0],
        });
    std::sync::Arc::new(masks)
}

/// Evaluates accumulated local semantic mask offsets `(ev, contrast, saturation, temp_shift)` at `(u, v)`.
#[inline]
fn eval_semantic_mask_adjustments(
    masks_set: &rraw_ai::SemanticMaskSet,
    layers: &[SemanticMaskLayer],
    u: f32,
    v: f32,
) -> (f32, f32, f32, f32) {
    let mut ev = 0.0_f32;
    let mut contrast = 0.0_f32;
    let mut sat = 0.0_f32;
    let mut temp = 0.0_f32;

    for layer in layers {
        if layer.is_neutral() {
            continue;
        }
        let w = masks_set.sample_bilinear(layer.category, u, v, layer.invert, layer.feather)
            * layer.opacity.clamp(0.0, 1.0);
        if w > 1e-4 {
            ev += w * layer.exposure_ev;
            contrast += w * layer.contrast;
            sat += w * layer.saturation;
            temp += w * layer.temperature_shift;
        }
    }
    (ev, contrast, sat, temp)
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
            let semantic_masks =
                infer_preview_semantic_masks(&[[0.0, 0.0, 0.0]], 1, 1, as_shot_wb, cam_to_srgb);
            return Self {
                width: 1,
                height: 1,
                samples_rgb: vec![[0.0, 0.0, 0.0]],
                as_shot_wb,
                cam_to_srgb,
                semantic_masks,
                exif_orientation: 1,
            };
        }

        let samples = cfa.samples();
        let pattern = cfa.pattern();

        if target_width >= width {
            // 1:1 Full-Resolution CFA Bilinear Demosaicing (preserves 100% native sensor pixels & sharpness)
            let preview_w = width;
            let preview_h = height;
            let mut samples_rgb = vec![[0.0_f32; 3]; preview_w * preview_h];

            // Precompute 6x6 periodic CFA color LUT (covers both 2x2 Bayer and 6x6 X-Trans without modulo per neighbor)
            let mut cfa_lut = [[CfaColor::Green; 6]; 6];
            for (ly, row_lut) in cfa_lut.iter_mut().enumerate() {
                for (lx, cell) in row_lut.iter_mut().enumerate() {
                    *cell = pattern.color_at(lx, ly);
                }
            }

            samples_rgb
                .par_chunks_mut(preview_w)
                .enumerate()
                .for_each(|(y, out_row)| {
                    let y0 = y.saturating_sub(1);
                    let y1 = (y + 1).min(height - 1);
                    let lut_y = y % 6;

                    for (x, out_px) in out_row.iter_mut().enumerate() {
                        let center_color = cfa_lut[lut_y][x % 6];
                        let center_val = samples[y * width + x];

                        let x0 = x.saturating_sub(1);
                        let x1 = (x + 1).min(width - 1);

                        let mut r_sum = 0.0_f32;
                        let mut r_cnt = 0_u32;
                        let mut g_sum = 0.0_f32;
                        let mut g_cnt = 0_u32;
                        let mut b_sum = 0.0_f32;
                        let mut b_cnt = 0_u32;

                        for ny in y0..=y1 {
                            let n_lut_y = ny % 6;
                            let row_off = ny * width;
                            for nx in x0..=x1 {
                                let val = samples[row_off + nx];
                                match cfa_lut[n_lut_y][nx % 6] {
                                    CfaColor::Red => {
                                        r_sum += val;
                                        r_cnt += 1;
                                    }
                                    CfaColor::Green => {
                                        g_sum += val;
                                        g_cnt += 1;
                                    }
                                    CfaColor::Blue => {
                                        b_sum += val;
                                        b_cnt += 1;
                                    }
                                }
                            }
                        }

                        let r = if center_color == CfaColor::Red {
                            center_val
                        } else if r_cnt > 0 {
                            r_sum / (r_cnt as f32)
                        } else {
                            center_val
                        };
                        let g = if center_color == CfaColor::Green {
                            center_val
                        } else if g_cnt > 0 {
                            g_sum / (g_cnt as f32)
                        } else {
                            center_val
                        };
                        let b = if center_color == CfaColor::Blue {
                            center_val
                        } else if b_cnt > 0 {
                            b_sum / (b_cnt as f32)
                        } else {
                            center_val
                        };

                        *out_px = [r, g, b];
                    }
                });

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
                samples_rgb,
                as_shot_wb,
                cam_to_srgb,
                semantic_masks,
                exif_orientation: 1,
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
            // Align to even 2x2 Bayer super-pixel origin while matching exact target_width / preview_h dimensions
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
            samples_rgb,
            as_shot_wb,
            cam_to_srgb,
            semantic_masks,
            exif_orientation: 1,
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
                // Convert sRGB 8-bit non-linear gamma to linear float
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
            samples_rgb,
            as_shot_wb,
            cam_to_srgb,
            semantic_masks,
            exif_orientation: 1,
        }
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
            self.samples_rgb = new_samples;
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

/// Generates a fast ~140px RGB thumbnail image from a RAW or raster file applying its sidecar grading if available.
#[must_use]
pub fn generate_fast_thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let ext_lower = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let header_orient = read_exif_orientation(path);

    if matches!(ext_lower.as_str(), "jpg" | "jpeg" | "png" | "tif" | "tiff") {
        if let Ok(dyn_img) = image::open(path) {
            let oriented = apply_exif_orientation_dyn_image(dyn_img, header_orient);
            let thumb = oriented.thumbnail(140, 100).to_rgb8();
            let (w, h) = thumb.dimensions();
            return Some(egui::ColorImage::from_rgb(
                [w as usize, h as usize],
                &thumb.into_raw(),
            ));
        }
    }

    let decoder = PureRustRawDecoder::new();
    let decoded = decoder.decode_file(path).ok()?;
    let orient = if decoded.orientation > 1 {
        decoded.orientation
    } else {
        header_orient
    };
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
    )
    .with_exif_orientation(orient);

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

    let curves_lut = rraw_color::curves::generate_1d_lut_1024(&grading.tone_curves);
    let has_curves = !grading.tone_curves.is_identity();

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

    let is_identity_geom = rot_deg.abs() < 1e-4
        && k_dist.abs() < 1e-5
        && keystone_v.abs() < 1e-5
        && keystone_h.abs() < 1e-5
        && flip_x > 0.0
        && flip_y > 0.0
        && (auto_zoom - 1.0).abs() < 1e-4;

    let has_tonal_adjustment = grading.shadows.abs() > 0.01
        || grading.highlights.abs() > 0.01
        || grading.blacks.abs() > 0.01;

    let combined_mat = [
        mat[0] * eff_wb_r * exposure_gain,
        mat[1] * eff_wb_g * exposure_gain,
        mat[2] * eff_wb_b * exposure_gain,
        mat[3] * eff_wb_r * exposure_gain,
        mat[4] * eff_wb_g * exposure_gain,
        mat[5] * eff_wb_b * exposure_gain,
        mat[6] * eff_wb_r * exposure_gain,
        mat[7] * eff_wb_g * exposure_gain,
        mat[8] * eff_wb_b * exposure_gain,
    ];

    let factor = 1.0 + (grading.contrast / 100.0).clamp(-0.8, 2.0);
    let base_vibrance = 0.10_f32;
    let user_vibrance = grading.vibrance / 100.0;
    let user_sat = grading.saturation / 100.0;

    let to_u8_fast = |x: f32| -> u8 {
        let clamped = x.clamp(0.0, 1.0);
        let toe = if clamped < 0.35 {
            clamped.powf(1.08)
        } else {
            clamped
        };
        let mapped = toe.powf(1.0 / 2.2);
        (mapped * 255.0).clamp(0.0, 255.0) as u8
    };

    struct RowHistogram {
        hist_r: [u32; 256],
        hist_g: [u32; 256],
        hist_b: [u32; 256],
        hist_lum: [u32; 256],
        shadow_clipped: u32,
        highlight_clipped: u32,
        out_of_gamut_count: u32,
    }

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

            let raw_dy = (py as f32 - cy) * flip_y;
            let row_identity_offset = py * preview_w;

            for (px, out_pixel) in row_pixels.iter_mut().enumerate() {
                let (sample_idx, v_mult) = if is_identity_geom {
                    let s_idx =
                        (row_identity_offset + px).min(base.samples_rgb.len().saturating_sub(1));
                    let v = if v_gain > 1e-4 {
                        let kx = px as f32 - cx;
                        let ky = py as f32 - cy;
                        let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                        1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
                    } else {
                        1.0
                    };
                    (s_idx, v)
                } else {
                    let raw_dx = (px as f32 - cx) * flip_x;
                    let rx = raw_dx * cos_rot + raw_dy * sin_rot;
                    let ry = -raw_dx * sin_rot + raw_dy * cos_rot;
                    let v_scale = (1.0 + keystone_v * (ry / cy)).max(0.1);
                    let kx = (rx / v_scale) / auto_zoom;
                    let h_scale = (1.0 + keystone_h * (kx / cx)).max(0.1);
                    let ky = (ry / h_scale) / auto_zoom;

                    let s_idx = if k_dist.abs() > 1e-5 {
                        let r2 = (kx * kx + ky * ky) / max_radius_sq;
                        let warp = 1.0 + k_dist * r2;
                        let warped_x = ((cx + kx * warp).round() as isize)
                            .clamp(0, preview_w as isize - 1)
                            as usize;
                        let warped_y = ((cy + ky * warp).round() as isize)
                            .clamp(0, preview_h as isize - 1)
                            as usize;
                        warped_y * preview_w + warped_x
                    } else {
                        let sx =
                            ((cx + kx).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                        let sy =
                            ((cy + ky).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                        sy * preview_w + sx
                    };

                    let v = if v_gain > 1e-4 {
                        let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                        1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
                    } else {
                        1.0
                    };
                    (s_idx.min(base.samples_rgb.len().saturating_sub(1)), v)
                };

                let [mean_r, mean_g, mean_b] = base.samples_rgb[sample_idx];

                let r_scaled = (combined_mat[0] * mean_r
                    + combined_mat[1] * mean_g
                    + combined_mat[2] * mean_b)
                    .max(0.0)
                    * v_mult;
                let g_scaled = (combined_mat[3] * mean_r
                    + combined_mat[4] * mean_g
                    + combined_mat[5] * mean_b)
                    .max(0.0)
                    * v_mult;
                let b_scaled = (combined_mat[6] * mean_r
                    + combined_mat[7] * mean_g
                    + combined_mat[8] * mean_b)
                    .max(0.0)
                    * v_mult;

                let (r_toned, g_toned, b_toned) = if has_tonal_adjustment {
                    let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
                    let y_norm = lum / (lum + 0.18);
                    let shadow_weight = (1.0 - y_norm).powi(2);
                    let highlight_weight = y_norm.powi(2);
                    let shadow_shift = (grading.shadows / 100.0) * 2.5 * shadow_weight;
                    let highlight_shift = (grading.highlights / 100.0) * 2.5 * highlight_weight;
                    let blacks_weight = (1.0 - y_norm).powi(4);
                    let blacks_shift = (grading.blacks / 100.0) * 1.5 * blacks_weight;
                    let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);
                    (
                        (r_scaled * tone_gain).max(0.0),
                        (g_scaled * tone_gain).max(0.0),
                        (b_scaled * tone_gain).max(0.0),
                    )
                } else {
                    (r_scaled, g_scaled, b_scaled)
                };

                let (r_curved, g_curved, b_curved) = if has_curves {
                    let c = rraw_color::curves::apply_tone_curves_lut(
                        glam::Vec3::new(r_toned, g_toned, b_toned),
                        &curves_lut,
                    );
                    (c.x, c.y, c.z)
                } else {
                    (r_toned, g_toned, b_toned)
                };

                let (r_lut, g_lut, b_lut) = if let Some(clut) = &grading.custom_clut {
                    let out = clut
                        .lut
                        .sample_tetrahedral(glam::Vec3::new(r_curved, g_curved, b_curved));
                    let w = clut.strength.clamp(0.0, 1.0);
                    (
                        r_curved * (1.0 - w) + out.x * w,
                        g_curved * (1.0 - w) + out.y * w,
                        b_curved * (1.0 - w) + out.z * w,
                    )
                } else {
                    (r_curved, g_curved, b_curved)
                };

                let u_norm = (px as f32 + 0.5) / (preview_w.max(1) as f32);
                let v_norm = (py as f32 + 0.5) / (preview_h.max(1) as f32);

                let (sem_ev, sem_contrast, sem_sat, sem_temp) =
                    if !grading.semantic_masks.is_empty() {
                        eval_semantic_mask_adjustments(
                            &base.semantic_masks,
                            &grading.semantic_masks,
                            u_norm,
                            v_norm,
                        )
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    };

                let (r_sem, g_sem, b_sem) = if sem_ev.abs() > 1e-4 || sem_temp.abs() > 1e-3 {
                    let g_ev = 2.0_f32.powf(sem_ev);
                    let r_t = (1.0 + sem_temp * 0.004).max(0.1);
                    let b_t = (1.0 - sem_temp * 0.004).max(0.1);
                    (
                        (r_lut * g_ev * r_t).max(0.0),
                        (g_lut * g_ev).max(0.0),
                        (b_lut * g_ev * b_t).max(0.0),
                    )
                } else {
                    (r_lut, g_lut, b_lut)
                };

                let (r_drt, g_drt, b_drt) = match grading.display_transform {
                    DisplayTransform::AgX => (
                        apply_agx_edr(r_sem, 1.0),
                        apply_agx_edr(g_sem, 1.0),
                        apply_agx_edr(b_sem, 1.0),
                    ),
                    DisplayTransform::OpenDrt => (
                        apply_opendrt(r_sem, 1.0),
                        apply_opendrt(g_sem, 1.0),
                        apply_opendrt(b_sem, 1.0),
                    ),
                };

                let eff_factor =
                    (factor * (1.0 + (sem_contrast / 100.0).clamp(-0.8, 2.0))).max(0.05);
                let r_c = ((r_drt - 0.18) * eff_factor + 0.18).max(0.0);
                let g_c = ((g_drt - 0.18) * eff_factor + 0.18).max(0.0);
                let b_c = ((b_drt - 0.18) * eff_factor + 0.18).max(0.0);

                let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
                let max_val = r_c.max(g_c).max(b_c);
                let min_val = r_c.min(g_c).min(b_c);
                let current_sat = if max_val > 1e-4 {
                    (max_val - min_val) / max_val
                } else {
                    0.0
                };
                let vib_weight = (1.0 - current_sat).clamp(0.0, 1.0);
                let eff_sat = ((1.0 + user_sat + (sem_sat / 100.0))
                    * (1.0 + (base_vibrance + user_vibrance) * vib_weight))
                    .max(0.0);

                let r_s = lum_c + (r_c - lum_c) * eff_sat;
                let g_s = lum_c + (g_c - lum_c) * eff_sat;
                let b_s = lum_c + (b_c - lum_c) * eff_sat;

                let sp_res = rraw_color::evaluate_soft_proof(
                    glam::Vec3::new(r_s, g_s, b_s),
                    &grading.soft_proof,
                );
                if sp_res.is_out_of_gamut {
                    row_oog += 1;
                }

                let (r_post, g_post, b_post) = if grading.soft_proof.enabled {
                    (
                        sp_res.proofed_rgb.x,
                        sp_res.proofed_rgb.y,
                        sp_res.proofed_rgb.z,
                    )
                } else {
                    (r_s.max(0.0), g_s.max(0.0), b_s.max(0.0))
                };

                let (mut u8_r, mut u8_g, mut u8_b) = if grading.is_monochrome {
                    let lum_val = ((to_u8_fast(r_post) as u32 * 54
                        + to_u8_fast(g_post) as u32 * 183
                        + to_u8_fast(b_post) as u32 * 19)
                        >> 8) as u8;
                    (lum_val, lum_val, lum_val)
                } else {
                    (to_u8_fast(r_post), to_u8_fast(g_post), to_u8_fast(b_post))
                };

                if grading.soft_proof.show_gamut_warning && sp_res.is_out_of_gamut {
                    let w = (0.55 + sp_res.gamut_excursion * 1.6).clamp(0.55, 0.88);
                    // High-contrast electric magenta/cyan zebra warning
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

                if let Some(overlay_cat) = grading.show_semantic_mask_overlay {
                    let (inv, fth) = grading
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

                row_r[u8_r as usize] += 1;
                row_g[u8_g as usize] += 1;
                row_b[u8_b as usize] += 1;

                let lum = (((u8_r as u32 * 54) + (u8_g as u32 * 183) + (u8_b as u32 * 19)) >> 8)
                    .min(255) as usize;
                row_lum[lum] += 1;

                if lum == 0 {
                    row_sc += 1;
                }
                if lum == 255 {
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

    let curves_lut = rraw_color::curves::generate_1d_lut_1024(&grading.tone_curves);
    let has_curves = !grading.tone_curves.is_identity();

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

            let (r_toned, g_toned, b_toned) = if has_curves {
                let c = rraw_color::curves::apply_tone_curves_lut(
                    glam::Vec3::new(r_toned, g_toned, b_toned),
                    &curves_lut,
                );
                (c.x, c.y, c.z)
            } else {
                (r_toned, g_toned, b_toned)
            };

            let (r_lut, g_lut, b_lut) = if let Some(clut) = &grading.custom_clut {
                let out = clut
                    .lut
                    .sample_tetrahedral(glam::Vec3::new(r_toned, g_toned, b_toned));
                let w = clut.strength.clamp(0.0, 1.0);
                (
                    r_toned * (1.0 - w) + out.x * w,
                    g_toned * (1.0 - w) + out.y * w,
                    b_toned * (1.0 - w) + out.z * w,
                )
            } else {
                (r_toned, g_toned, b_toned)
            };

            let u_norm = (orig_x / (preview_w.max(1) as f32)).clamp(0.0, 1.0);
            let v_norm = (orig_y / (preview_h.max(1) as f32)).clamp(0.0, 1.0);
            let (sem_ev, sem_contrast, sem_sat, sem_temp) = if !grading.semantic_masks.is_empty() {
                eval_semantic_mask_adjustments(
                    &base.semantic_masks,
                    &grading.semantic_masks,
                    u_norm,
                    v_norm,
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };

            let (r_sem, g_sem, b_sem) = if sem_ev.abs() > 1e-4 || sem_temp.abs() > 1e-3 {
                let g_ev = 2.0_f32.powf(sem_ev);
                let r_t = (1.0 + sem_temp * 0.004).max(0.1);
                let b_t = (1.0 - sem_temp * 0.004).max(0.1);
                (
                    (r_lut * g_ev * r_t).max(0.0),
                    (g_lut * g_ev).max(0.0),
                    (b_lut * g_ev * b_t).max(0.0),
                )
            } else {
                (r_lut, g_lut, b_lut)
            };

            let (r_drt, g_drt, b_drt) = match grading.display_transform {
                DisplayTransform::AgX => (
                    apply_agx_edr(r_sem, 1.0),
                    apply_agx_edr(g_sem, 1.0),
                    apply_agx_edr(b_sem, 1.0),
                ),
                DisplayTransform::OpenDrt => (
                    apply_opendrt(r_sem, 1.0),
                    apply_opendrt(g_sem, 1.0),
                    apply_opendrt(b_sem, 1.0),
                ),
            };

            let factor = (1.0 + (grading.contrast / 100.0).clamp(-0.8, 2.0))
                * (1.0 + (sem_contrast / 100.0).clamp(-0.8, 2.0));
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
            let eff_sat = ((1.0 + user_sat + (sem_sat / 100.0))
                * (1.0 + (base_vibrance + user_vibrance) * vib_weight))
                .max(0.0);

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

/// Re-scans `dir` and updates `current_entries` in-place if files were added or removed.
/// Returns `true` when the list of images changed.
pub fn sync_directory_entries(dir: &Path, current_entries: &mut Vec<PathBuf>) -> bool {
    let latest = scan_directory_for_images(dir);
    if latest != *current_entries {
        *current_entries = latest;
        true
    } else {
        false
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
    let ext_lower = target_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let header_orient = read_exif_orientation(target_path);

    if matches!(ext_lower.as_str(), "jpg" | "jpeg" | "png" | "tif" | "tiff") {
        if let Ok(dyn_img) = image::open(target_path) {
            let rgb = dyn_img.to_rgb8();
            let (raw_w, raw_h) = rgb.dimensions();
            let (orig_w, orig_h) = oriented_dimensions(raw_w, raw_h, header_orient);
            let base =
                BaseRgbPreview::from_rgb_image(&rgb, 1600).with_exif_orientation(header_orient);
            let dummy_cfa = SignedCfaBuffer::default();

            let file_name = target_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image")
                .to_string();

            let photo_view = LoadedPhotoView {
                path: target_path.to_path_buf(),
                camera_make: "Raster Image".to_string(),
                camera_model: format!("{} ({})", ext_lower.to_ascii_uppercase(), file_name),
                lens_model: String::new(),
                dimensions: (orig_w, orig_h),
                iso: 100,
                shutter_str: "-".to_string(),
                aperture_str: "-".to_string(),
                focal_length_str: String::new(),
                date_time: String::new(),
                exposure_bias: String::new(),
                rating: 0,
                exif_tags: vec![
                    ("Format".to_string(), ext_lower.to_ascii_uppercase()),
                    ("Dimensions".to_string(), format!("{orig_w}x{orig_h}")),
                    ("Orientation".to_string(), format!("{header_orient}")),
                ],
            };

            return Some(BackgroundDecodePayload {
                path: target_path.to_path_buf(),
                photo_view,
                base_preview: base,
                cfa: dummy_cfa,
                is_prefetch,
            });
        }
    }

    let decoder = PureRustRawDecoder::new();
    let decoded = decoder.decode_file(target_path).ok()?;
    let orient = if decoded.orientation > 1 {
        decoded.orientation
    } else {
        header_orient
    };
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

    let upright_dims = oriented_dimensions(meta.width as u32, meta.height as u32, orient);

    let photo_view = LoadedPhotoView {
        path: target_path.to_path_buf(),
        camera_make: make,
        camera_model: camera,
        lens_model: lens_str,
        dimensions: upright_dims,
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
    )
    .with_exif_orientation(orient);

    Some(BackgroundDecodePayload {
        path: target_path.to_path_buf(),
        photo_view,
        base_preview: base,
        cfa: decoded.cfa_signed,
        is_prefetch,
    })
}

/// Computes per-frame grading states for a timelapse sequence, applying optional Holy Grail
/// keyframe ramping (between first and last frame) and temporal exposure deflicker (`ΔEV`).
#[must_use]
pub fn prepare_timelapse_frame_gradings(
    frames: &[(&BaseRgbPreview, DarkroomGradingState)],
    keyframe_ramping_enabled: bool,
    deflicker_config: &rraw_export::TimelapseDeflickerConfig,
) -> (Vec<DarkroomGradingState>, Vec<f32>) {
    let n = frames.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }

    let mut gradings: Vec<DarkroomGradingState> = frames.iter().map(|(_, g)| g.clone()).collect();

    if keyframe_ramping_enabled && n >= 2 {
        let g_first = &gradings[0];
        let g_last = &gradings[n - 1];
        let kfs = [
            rraw_export::TimelapseKeyframe {
                frame_index: 0,
                exposure_ev: g_first.exposure_ev,
                kelvin: g_first.kelvin,
                tint: g_first.tint,
                contrast: g_first.contrast,
                saturation: g_first.saturation,
            },
            rraw_export::TimelapseKeyframe {
                frame_index: n - 1,
                exposure_ev: g_last.exposure_ev,
                kelvin: g_last.kelvin,
                tint: g_last.tint,
                contrast: g_last.contrast,
                saturation: g_last.saturation,
            },
        ];

        let interpolated = rraw_export::interpolate_keyframes(n, &kfs);
        for (g, kf) in gradings.iter_mut().zip(interpolated.iter()) {
            g.exposure_ev = kf.exposure_ev;
            g.kelvin = kf.kelvin;
            g.tint = kf.tint;
            g.contrast = kf.contrast;
            g.saturation = kf.saturation;
        }
    }

    let mut measured_log_lumas = Vec::with_capacity(n);
    for ((base, _), g) in frames.iter().zip(gradings.iter()) {
        let flat_rgb: Vec<f32> = base
            .samples_rgb
            .iter()
            .flat_map(|px| [px[0], px[1], px[2]])
            .collect();
        let sensor_log_y = rraw_export::compute_frame_log_luminance(&flat_rgb);
        measured_log_lumas.push(sensor_log_y + g.exposure_ev);
    }

    let offsets_ev =
        rraw_export::compute_deflicker_ev_offsets(&measured_log_lumas, deflicker_config);
    for (g, &delta_ev) in gradings.iter_mut().zip(offsets_ev.iter()) {
        g.exposure_ev = (g.exposure_ev + delta_ev).clamp(-5.0, 5.0);
    }

    (gradings, offsets_ev)
}
