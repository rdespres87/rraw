#![forbid(unsafe_code)]

//! 4-color 2x2 CFA (CYGM / GMYC / RGBE) and Canon Pro70 CIFF decoding.

use rraw_core::{BayerOrigin, CfaPattern, CoreError};

use super::{SRGB_TO_XYZ_D65, XYZ_TO_SRGB_D65};
use crate::{DecodeError, LinearRgbBuffer};

/// Returns `true` when the sensor uses a 2x2 repeating 4-color CFA (e.g., CYGM / GMYC / RGBE).
#[must_use]
pub fn is_four_color_2x2_cfa(cfa: &rawler::cfa::CFA) -> bool {
    cfa.width == 2 && cfa.height == 2 && cfa.unique_colors() == 4
}

pub fn demosaic_four_color_cfa_to_linear_srgb(
    raw_image: &rawler::RawImage,
    raw_cfa: &rawler::cfa::CFA,
) -> Result<LinearRgbBuffer, DecodeError> {
    if !is_four_color_2x2_cfa(raw_cfa) {
        return Err(DecodeError::UnsupportedFormat(format!(
            "expected 2x2 4-color CFA, got '{}' ({}x{})",
            raw_cfa.name, raw_cfa.width, raw_cfa.height
        )));
    }

    let xyz2cam = crate::sensor::extract_color_matrix_4x3(raw_image).ok_or_else(|| {
        DecodeError::UnsupportedFormat("missing 4x3 color matrix for 4-color CFA sensor".to_owned())
    })?;

    let src_w = raw_image.width;
    let src_h = raw_image.height;
    let Some(expected) = src_w.checked_mul(src_h) else {
        return Err(CoreError::InvalidDimensions {
            width: src_w,
            height: src_h,
        }
        .into());
    };
    if src_w < 2 || src_h < 2 {
        return Err(CoreError::InvalidDimensions {
            width: src_w,
            height: src_h,
        }
        .into());
    }

    let (crop_x, crop_y, crop_w, crop_h) =
        if let Some(rect) = raw_image.crop_area.or(raw_image.active_area) {
            let ax = (rect.p.x / 2) * 2;
            let ay = (rect.p.y / 2) * 2;
            let max_w = src_w.saturating_sub(ax);
            let max_h = src_h.saturating_sub(ay);
            let aw = ((rect.d.w.min(max_w)) / 2) * 2;
            let ah = ((rect.d.h.min(max_h)) / 2) * 2;
            if aw >= 2 && ah >= 2 {
                (ax, ay, aw, ah)
            } else {
                (0, 0, (src_w / 2) * 2, (src_h / 2) * 2)
            }
        } else {
            (0, 0, (src_w / 2) * 2, (src_h / 2) * 2)
        };

    if crop_w < 2 || crop_h < 2 {
        return Err(CoreError::InvalidDimensions {
            width: crop_w,
            height: crop_h,
        }
        .into());
    }

    let black_levels = raw_image.blacklevel.as_bayer_array();
    let white_levels = raw_image.whitelevel.as_bayer_array();
    let mut inv_ranges = [0.0_f32; 4];
    for ch in 0..4 {
        let range = white_levels[ch] - black_levels[ch];
        if !range.is_finite() || range <= 0.0 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid 4-color black/white level range on channel {ch}: black={}, white={}",
                black_levels[ch], white_levels[ch]
            )));
        }
        inv_ranges[ch] = 1.0 / range;
    }

    let mut plane = vec![0.0_f32; crop_w * crop_h];
    match &raw_image.data {
        rawler::RawImageData::Integer(samples) => {
            if samples.len() != expected {
                return Err(CoreError::BufferLengthMismatch {
                    expected,
                    actual: samples.len(),
                }
                .into());
            }
            for y in 0..crop_h {
                let src_row = (crop_y + y) * src_w + crop_x;
                let dst_row = y * crop_w;
                let y_bit = (y & 1) * 2;
                for x in 0..crop_w {
                    let ch = y_bit + (x & 1);
                    let raw = f32::from(samples[src_row + x]);
                    plane[dst_row + x] = (raw - black_levels[ch]) * inv_ranges[ch];
                }
            }
        }
        rawler::RawImageData::Float(samples) => {
            if samples.len() != expected {
                return Err(CoreError::BufferLengthMismatch {
                    expected,
                    actual: samples.len(),
                }
                .into());
            }
            for y in 0..crop_h {
                let src_row = (crop_y + y) * src_w + crop_x;
                let dst_row = y * crop_w;
                let y_bit = (y & 1) * 2;
                for x in 0..crop_w {
                    let ch = y_bit + (x & 1);
                    let raw = samples[src_row + x];
                    let norm = (raw - black_levels[ch]) * inv_ranges[ch];
                    plane[dst_row + x] = if norm.is_finite() { norm } else { 0.0 };
                }
            }
        }
    }

    let plane_map = match &raw_image.photometric {
        rawler::rawimage::RawPhotometricInterpretation::Cfa(cfg) => {
            Some(cfg.colors.plane_lookup_table())
        }
        _ => None,
    };
    let resolve_plane = |row: usize, col: usize| -> Result<usize, DecodeError> {
        let color_id = raw_cfa.cfa_color_at(row, col) as usize;
        if let Some(map) = &plane_map {
            let plane = map[color_id];
            if plane < 4 {
                return Ok(plane);
            }
        }
        if color_id < 4 {
            Ok(color_id)
        } else {
            Err(DecodeError::UnsupportedFormat(format!(
                "unmapped 4-color CFA channel id {color_id} in '{}'",
                raw_cfa.name
            )))
        }
    };
    let color_lut = [
        [resolve_plane(0, 0)?, resolve_plane(0, 1)?],
        [resolve_plane(1, 0)?, resolve_plane(1, 1)?],
    ];

    let demosaiced = demosaic_four_color_bilinear(&plane, crop_w, crop_h, color_lut);
    let cam2rgb = compute_cam4_to_srgb_matrix(&xyz2cam).ok_or_else(|| {
        DecodeError::MalformedMetadata("singular 4x3 camera color matrix".to_owned())
    })?;
    let wb = sanitize_four_channel_wb_with_matrix(raw_image.wb_coeffs, &xyz2cam);

    let mut out = Vec::with_capacity(crop_w * crop_h);
    for pix in &demosaiced {
        let c0 = pix[0] * wb[0];
        let c1 = pix[1] * wb[1];
        let c2 = pix[2] * wb[2];
        let c3 = pix[3] * wb[3];

        let r = cam2rgb[0][0] * c0 + cam2rgb[0][1] * c1 + cam2rgb[0][2] * c2 + cam2rgb[0][3] * c3;
        let g = cam2rgb[1][0] * c0 + cam2rgb[1][1] * c1 + cam2rgb[1][2] * c2 + cam2rgb[1][3] * c3;
        let b = cam2rgb[2][0] * c0 + cam2rgb[2][1] * c1 + cam2rgb[2][2] * c2 + cam2rgb[2][3] * c3;

        out.push([
            if r.is_finite() { r.max(0.0) } else { 0.0 },
            if g.is_finite() { g.max(0.0) } else { 0.0 },
            if b.is_finite() { b.max(0.0) } else { 0.0 },
        ]);
    }

    LinearRgbBuffer::new(crop_w, crop_h, out)
}

#[inline]
pub(super) fn demosaic_four_color_bilinear(
    plane: &[f32],
    width: usize,
    height: usize,
    color_lut: [[usize; 2]; 2],
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0_f32; 4]; width * height];

    for y in 0..height {
        let row = y * width;
        let has_up = y > 0;
        let has_dn = y + 1 < height;
        let y_par = y & 1;
        let y_opp = y_par ^ 1;

        for x in 0..width {
            let has_lt = x > 0;
            let has_rt = x + 1 < width;
            let x_par = x & 1;
            let x_opp = x_par ^ 1;

            let mut pix = [0.0_f32; 4];

            // Exact color at (x, y)
            let c_self = color_lut[y_par][x_par];
            pix[c_self] = plane[row + x];

            // Horizontal neighbor color
            let c_h = color_lut[y_par][x_opp];
            let mut sum_h = 0.0_f32;
            let mut cnt_h = 0.0_f32;
            if has_lt {
                sum_h += plane[row + x - 1];
                cnt_h += 1.0;
            }
            if has_rt {
                sum_h += plane[row + x + 1];
                cnt_h += 1.0;
            }
            pix[c_h] = if cnt_h > 0.0 { sum_h / cnt_h } else { 0.0 };

            // Vertical neighbor color
            let c_v = color_lut[y_opp][x_par];
            let mut sum_v = 0.0_f32;
            let mut cnt_v = 0.0_f32;
            if has_up {
                sum_v += plane[(y - 1) * width + x];
                cnt_v += 1.0;
            }
            if has_dn {
                sum_v += plane[(y + 1) * width + x];
                cnt_v += 1.0;
            }
            pix[c_v] = if cnt_v > 0.0 { sum_v / cnt_v } else { 0.0 };

            // Diagonal neighbor color
            let c_d = color_lut[y_opp][x_opp];
            let mut sum_d = 0.0_f32;
            let mut cnt_d = 0.0_f32;
            if has_up && has_lt {
                sum_d += plane[(y - 1) * width + x - 1];
                cnt_d += 1.0;
            }
            if has_up && has_rt {
                sum_d += plane[(y - 1) * width + x + 1];
                cnt_d += 1.0;
            }
            if has_dn && has_lt {
                sum_d += plane[(y + 1) * width + x - 1];
                cnt_d += 1.0;
            }
            if has_dn && has_rt {
                sum_d += plane[(y + 1) * width + x + 1];
                cnt_d += 1.0;
            }
            pix[c_d] = if cnt_d > 0.0 { sum_d / cnt_d } else { 0.0 };

            out[row + x] = pix;
        }
    }

    out
}

pub(super) fn sanitize_four_channel_wb_with_matrix(
    wb_coeffs: [f32; 4],
    xyz2cam: &[[f32; 3]; 4],
) -> [f32; 4] {
    if wb_coeffs.iter().all(|&v| v.is_finite() && v > 0.0) {
        let min_val = wb_coeffs.iter().copied().fold(f32::INFINITY, f32::min);
        if min_val.is_finite() && min_val > 0.0 {
            return [
                wb_coeffs[0] / min_val,
                wb_coeffs[1] / min_val,
                wb_coeffs[2] / min_val,
                wb_coeffs[3] / min_val,
            ];
        }
    }

    // Derive exact D65 daylight multipliers from `xyz2cam * D65_XYZ` so a neutral D65
    // scene maps to `[1, 1, 1, 1]` before the row-normalized `cam2rgb` pseudo-inverse.
    const D65_XYZ: [f32; 3] = [0.950_47, 1.0, 1.088_83];
    let mut cam_d65 = [0.0_f32; 4];
    for (row, dst) in xyz2cam.iter().zip(cam_d65.iter_mut()) {
        *dst = row[0] * D65_XYZ[0] + row[1] * D65_XYZ[1] + row[2] * D65_XYZ[2];
    }
    if cam_d65.iter().all(|&v| v.is_finite() && v > 1e-4) {
        let inv = [
            1.0 / cam_d65[0],
            1.0 / cam_d65[1],
            1.0 / cam_d65[2],
            1.0 / cam_d65[3],
        ];
        let min_inv = inv.iter().copied().fold(f32::INFINITY, f32::min);
        if min_inv.is_finite() && min_inv > 0.0 {
            return [
                inv[0] / min_inv,
                inv[1] / min_inv,
                inv[2] / min_inv,
                inv[3] / min_inv,
            ];
        }
    }

    [1.0; 4]
}

pub(super) fn compute_cam4_to_srgb_matrix(xyz2cam: &[[f32; 3]; 4]) -> Option<[[f32; 4]; 3]> {
    // rgb2cam = xyz2cam (4x3) * SRGB_TO_XYZ_D65 (3x3)
    let mut rgb2cam = [[0.0_f32; 3]; 4];
    for r in 0..4 {
        for c in 0..3 {
            rgb2cam[r][c] = xyz2cam[r][0] * SRGB_TO_XYZ_D65[c]
                + xyz2cam[r][1] * SRGB_TO_XYZ_D65[3 + c]
                + xyz2cam[r][2] * SRGB_TO_XYZ_D65[6 + c];
        }
        let row_sum = rgb2cam[r][0] + rgb2cam[r][1] + rgb2cam[r][2];
        if row_sum.abs() > 1e-6 {
            rgb2cam[r][0] /= row_sum;
            rgb2cam[r][1] /= row_sum;
            rgb2cam[r][2] /= row_sum;
        }
    }

    pseudo_inverse_4x3(&rgb2cam)
}

/// Computes the Moore-Penrose pseudo-inverse `(M^T M)^-1 M^T` of a 4x3 matrix.
pub(super) fn pseudo_inverse_4x3(m: &[[f32; 3]; 4]) -> Option<[[f32; 4]; 3]> {
    let mut mtm = [[0.0_f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let mut sum = 0.0_f64;
            for row in m {
                sum += f64::from(row[i]) * f64::from(row[j]);
            }
            mtm[i][j] = sum;
        }
    }

    let a = mtm[0][0];
    let b = mtm[0][1];
    let c = mtm[0][2];
    let d = mtm[1][0];
    let e = mtm[1][1];
    let f = mtm[1][2];
    let g = mtm[2][0];
    let h = mtm[2][1];
    let i = mtm[2][2];

    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() < 1e-10 {
        return None;
    }
    let inv_det = 1.0 / det;
    let inv = [
        [
            (e * i - f * h) * inv_det,
            (c * h - b * i) * inv_det,
            (b * f - c * e) * inv_det,
        ],
        [
            (f * g - d * i) * inv_det,
            (a * i - c * g) * inv_det,
            (c * d - a * f) * inv_det,
        ],
        [
            (d * h - e * g) * inv_det,
            (b * g - a * h) * inv_det,
            (a * e - b * d) * inv_det,
        ],
    ];

    let mut out = [[0.0_f32; 4]; 3];
    for r in 0..3 {
        for col in 0..4 {
            let sum = inv[r][0] * f64::from(m[col][0])
                + inv[r][1] * f64::from(m[col][1])
                + inv[r][2] * f64::from(m[col][2]);
            out[r][col] = sum as f32;
        }
    }
    Some(out)
}

/// Demosaics a 4-color 8-row × 2-column repeating CFA plane (e.g., Canon PowerShot Pro70
/// `YC/MG/CY/GM/YC/GM/CY/MG`) using weighted 3x3 neighborhood interpolation.
pub(super) fn demosaic_four_color_8x2_bilinear(
    plane: &[f32],
    width: usize,
    height: usize,
    lut_8x2: [[usize; 2]; 8],
) -> Vec<[f32; 4]> {
    let Some(total) = width.checked_mul(height) else {
        return Vec::new();
    };
    if total == 0 || plane.len() < total {
        return Vec::new();
    }
    let mut out = vec![[0.0_f32; 4]; total];
    let max_x = width.saturating_sub(1);
    let max_y = height.saturating_sub(1);

    for y in 0..height {
        let y_min = y.saturating_sub(1);
        let y_max = (y + 1).min(max_y);
        for x in 0..width {
            let x_min = x.saturating_sub(1);
            let x_max = (x + 1).min(max_x);
            let mut sums = [0.0_f32; 4];
            let mut weights = [0.0_f32; 4];

            for ny in y_min..=y_max {
                let row_off = ny * width;
                let lut_row = lut_8x2[ny & 7];
                let wy: f32 = if ny == y { 2.0 } else { 1.0 };
                for nx in x_min..=x_max {
                    let wx: f32 = if nx == x { 2.0 } else { 1.0 };
                    let w = wx * wy;
                    let ch = lut_row[nx & 1];
                    sums[ch] += plane[row_off + nx] * w;
                    weights[ch] += w;
                }
            }

            let mut px = [0.0_f32; 4];
            for ch in 0..4 {
                if weights[ch] > 0.0 {
                    px[ch] = sums[ch] / weights[ch];
                }
            }
            out[y * width + x] = px;
        }
    }

    out
}

/// Pure-Rust decoder for Canon PowerShot Pro70 CIFF (`.CRW`) RAW files.
///
/// The Pro70 uses uncompressed 10-bit 16-bit-word-swapped Canon packing (`1552 × 1024`, active
/// `1536 × 1024`) with a 4x4 repeating 4-color CYGM-like CFA (`EBGR/BERG/EBRG/BEGR`).
pub fn try_decode_canon_pro70_ciff(
    path: &std::path::Path,
) -> Result<crate::DecodedRaw, DecodeError> {
    let data = std::fs::read(path)
        .map_err(|e| DecodeError::Io(format!("failed to read {}: {e}", path.display())))?;

    const RAW_W: usize = 1552;
    const RAW_H: usize = 1024;
    const ACTIVE_W: usize = 1536;
    const ROW_STRIDE_BYTES: usize = RAW_W * 10 / 8; // 1940 bytes
    const RAW_DATA_OFFSET: usize = 26;
    const RAW_DATA_BYTES: usize = RAW_H * ROW_STRIDE_BYTES; // 1,986,560 bytes

    if data.len() < RAW_DATA_OFFSET + RAW_DATA_BYTES + 64
        || &data[0..2] != b"II"
        || &data[6..14] != b"HEAPCCDR"
    {
        return Err(DecodeError::UnsupportedFormat(
            "not a Canon CIFF file".to_owned(),
        ));
    }
    let tail = &data[RAW_DATA_OFFSET + RAW_DATA_BYTES..];
    if !tail
        .windows(b"PowerShot Pro70".len())
        .any(|w| w == b"PowerShot Pro70")
    {
        return Err(DecodeError::UnsupportedFormat(
            "CIFF file is not Canon PowerShot Pro70".to_owned(),
        ));
    }

    let raw_bytes = &data[RAW_DATA_OFFSET..RAW_DATA_OFFSET + RAW_DATA_BYTES];
    const BLACK: f32 = 34.0;
    const WHITE: f32 = 990.0;
    let inv_range = 1.0 / (WHITE - BLACK);

    let mut plane = Vec::with_capacity(ACTIVE_W * RAW_H);
    for y in 0..RAW_H {
        let row = &raw_bytes[y * ROW_STRIDE_BYTES..(y + 1) * ROW_STRIDE_BYTES];
        // Unpack 16-bit-swapped 10-bit big-endian (10 bytes -> 8 pixels)
        let (chunks, _) = row.as_chunks::<10>();
        let mut produced = 0usize;
        for &c in chunks {
            let swapped = [c[1], c[0], c[3], c[2], c[5], c[4], c[7], c[6], c[9], c[8]];
            let (fives, _) = swapped.as_chunks::<5>();
            for &f in fives {
                let v = (u64::from(f[0]) << 32)
                    | (u64::from(f[1]) << 24)
                    | (u64::from(f[2]) << 16)
                    | (u64::from(f[3]) << 8)
                    | u64::from(f[4]);
                for shift in [30u32, 20, 10, 0] {
                    if produced < ACTIVE_W {
                        let sample = ((v >> shift) & 0x03FF) as f32;
                        plane.push((sample - BLACK) * inv_range);
                        produced += 1;
                    }
                }
            }
            if produced >= ACTIVE_W {
                break;
            }
        }
    }

    // 8-row repeating CYGM CFA ("YC/MG/CY/GM/YC/GM/CY/MG") mapped to [G=0, M=1, C=2, Y=3]
    const PRO70_LUT: [[usize; 2]; 8] = [
        [3, 2], // y % 8 == 0: Y C
        [1, 0], // y % 8 == 1: M G
        [2, 3], // y % 8 == 2: C Y
        [0, 1], // y % 8 == 3: G M
        [3, 2], // y % 8 == 4: Y C
        [0, 1], // y % 8 == 5: G M
        [2, 3], // y % 8 == 6: C Y
        [1, 0], // y % 8 == 7: M G
    ];
    let demosaiced = demosaic_four_color_8x2_bilinear(&plane, ACTIVE_W, RAW_H, PRO70_LUT);

    // D65 XYZ -> [G, M, C, Y] calibration matrix for Canon PowerShot Pro70
    const XYZ_TO_CAM: [[f32; 3]; 4] = [
        [-0.5106, 1.0695, 0.1576],
        [0.3820, 0.0053, 0.4566],
        [-0.6497, 1.0736, 0.6701],
        [-0.3336, 1.1887, 0.1394],
    ];
    let cam2rgb = compute_cam4_to_srgb_matrix(&XYZ_TO_CAM).ok_or_else(|| {
        DecodeError::MalformedMetadata("singular 4x3 Pro70 color matrix".to_owned())
    })?;
    let wb = sanitize_four_channel_wb_with_matrix([f32::NAN; 4], &XYZ_TO_CAM);

    let mut rgb = Vec::with_capacity(ACTIVE_W * RAW_H);
    for pix in &demosaiced {
        let c0 = pix[0] * wb[0];
        let c1 = pix[1] * wb[1];
        let c2 = pix[2] * wb[2];
        let c3 = pix[3] * wb[3];
        let r = cam2rgb[0][0] * c0 + cam2rgb[0][1] * c1 + cam2rgb[0][2] * c2 + cam2rgb[0][3] * c3;
        let g = cam2rgb[1][0] * c0 + cam2rgb[1][1] * c1 + cam2rgb[1][2] * c2 + cam2rgb[1][3] * c3;
        let b = cam2rgb[2][0] * c0 + cam2rgb[2][1] * c1 + cam2rgb[2][2] * c2 + cam2rgb[2][3] * c3;
        rgb.push([
            if r.is_finite() { r.max(0.0) } else { 0.0 },
            if g.is_finite() { g.max(0.0) } else { 0.0 },
            if b.is_finite() { b.max(0.0) } else { 0.0 },
        ]);
    }
    let linear_rgb = LinearRgbBuffer::new(ACTIVE_W, RAW_H, rgb)?;

    let metadata = rraw_core::PhotoMetadata {
        camera_make: "Canon".to_owned(),
        camera_model: "PowerShot Pro70".to_owned(),
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: None,
        iso: None,
        width: ACTIVE_W,
        height: RAW_H,
        black_levels: [BLACK; 4],
        white_levels: [WHITE; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile: rraw_core::NoiseProfile::new(
            [0.001; 3],
            [0.0001; 3],
            rraw_core::NoiseProfileSource::WaveletMadFallback,
        )?,
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        rating: 0,
        pick: rraw_core::PickStatus::Unflagged,
        label: rraw_core::ColorLabel::None,
        keywords: Vec::new(),
    };

    let exif_tags = vec![
        ("Camera Make".to_owned(), "Canon".to_owned()),
        ("Camera Model".to_owned(), "PowerShot Pro70".to_owned()),
        (
            "Dimensions".to_owned(),
            format!(
                "{ACTIVE_W} x {RAW_H} ({:.1} MP)",
                (ACTIVE_W * RAW_H) as f32 / 1_000_000.0
            ),
        ),
        (
            "Sensor Format".to_owned(),
            "4-Color 4x4 CYGM (Linear sRGB)".to_owned(),
        ),
        ("Orientation".to_owned(), "Normal".to_owned()),
    ];

    Ok(crate::DecodedRaw {
        metadata,
        cfa_signed: rraw_core::SignedCfaBuffer::default(),
        linear_rgb: Some(linear_rgb),
        wb_coeffs: [1.0; 4],
        color_matrix: Some(XYZ_TO_SRGB_D65),
        exif_tags,
        orientation: 1,
    })
}
