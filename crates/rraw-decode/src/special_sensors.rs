#![forbid(unsafe_code)]

//! Specialized pure-Rust sensor pipelines for non-standard CFA geometries:
//! - Fujifilm SuperCCD 45° octagonal sensors (`fuji_rotation_width`).
//! - 4-color 2x2 CFA sensors (e.g., CYGM / GMYC / RGBE).

use rraw_core::{BayerOrigin, CfaPattern, CoreError};

use crate::{DecodeError, LinearRgbBuffer};

/// Standard D65 linear sRGB to CIE XYZ 3x3 conversion matrix (row-major).
pub const SRGB_TO_XYZ_D65: [f32; 9] = [
    0.412_456_4,
    0.357_576_1,
    0.180_437_5,
    0.212_672_9,
    0.715_152_2,
    0.072_175_0,
    0.019_333_9,
    0.119_192,
    0.950_304_1,
];

/// Standard D65 CIE XYZ to linear sRGB 3x3 conversion matrix (row-major).
///
/// Passing this as `DecodedRaw::color_matrix` (`XYZ -> Camera`) alongside neutral `[1.0; 4]`
/// WB multipliers causes downstream `XYZ_TO_SRGB * inv(color_matrix)` calibration to collapse
/// to the exact 3x3 identity matrix for buffers already calibrated in linear sRGB.
pub const XYZ_TO_SRGB_D65: [f32; 9] = [
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

/// Returns `true` when the sensor uses a 2x2 repeating 4-color CFA (e.g., CYGM / GMYC / RGBE).
#[must_use]
pub fn is_four_color_2x2_cfa(cfa: &rawler::cfa::CFA) -> bool {
    cfa.width == 2 && cfa.height == 2 && cfa.unique_colors() == 4
}

/// Computes the inscribed upright dimensions after rotating a SuperCCD sensor image by 45° CW.
#[must_use]
pub fn fuji_rotated_dimensions(src_width: usize, fuji_rotation_width: usize) -> (usize, usize) {
    let t = fuji_rotation_width as f64;
    let s = src_width as f64;
    let crop_w = (t * std::f64::consts::SQRT_2).floor() as usize;
    let crop_h = ((s - t).max(0.0) * std::f64::consts::SQRT_2).floor() as usize;
    if crop_w >= crop_h {
        (crop_w, crop_h)
    } else {
        (crop_h, crop_w)
    }
}

/// Demosaics a 45°-tilted Fujifilm SuperCCD Bayer sensor and rotates it upright into a
/// linear camera-RGB buffer (`LinearRgbBuffer`).
pub fn demosaic_and_rotate_fuji_superccd(
    raw_image: &rawler::RawImage,
    cfa_pattern: &CfaPattern,
    fuji_rotation_width: usize,
) -> Result<LinearRgbBuffer, DecodeError> {
    let origin = match cfa_pattern {
        CfaPattern::Bayer(origin) => *origin,
        CfaPattern::XTrans(_) => {
            return Err(DecodeError::UnsupportedFormat(
                "SuperCCD 45-degree rotation requires a Bayer CFA layout".to_owned(),
            ));
        }
    };

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

    let black_levels = raw_image.blacklevel.as_bayer_array();
    let white_levels = raw_image.whitelevel.as_bayer_array();
    let mut inv_ranges = [0.0_f32; 4];
    for ch in 0..4 {
        let range = white_levels[ch] - black_levels[ch];
        if !range.is_finite() || range <= 0.0 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid SuperCCD black/white level range on channel {ch}: black={}, white={}",
                black_levels[ch], white_levels[ch]
            )));
        }
        inv_ranges[ch] = 1.0 / range;
    }

    let mut normalized = vec![0.0_f32; expected];
    match &raw_image.data {
        rawler::RawImageData::Integer(samples) => {
            if samples.len() != expected {
                return Err(CoreError::BufferLengthMismatch {
                    expected,
                    actual: samples.len(),
                }
                .into());
            }
            for y in 0..src_h {
                let row = y * src_w;
                let y_bit = (y & 1) * 2;
                for x in 0..src_w {
                    let ch = y_bit + (x & 1);
                    let raw = f32::from(samples[row + x]);
                    normalized[row + x] = (raw - black_levels[ch]) * inv_ranges[ch];
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
            for y in 0..src_h {
                let row = y * src_w;
                let y_bit = (y & 1) * 2;
                for x in 0..src_w {
                    let ch = y_bit + (x & 1);
                    let raw = samples[row + x];
                    let norm = (raw - black_levels[ch]) * inv_ranges[ch];
                    normalized[row + x] = if norm.is_finite() { norm } else { 0.0 };
                }
            }
        }
    }

    let mut rgb = demosaic_bayer_bilinear(&normalized, src_w, src_h, origin);
    let mut cur_w = src_w;
    let mut cur_h = src_h;

    if raw_image.camera.find_hint("fuji_rotate_90cw") {
        rgb = rotate_rgb_90cw(&rgb, cur_w, cur_h);
        std::mem::swap(&mut cur_w, &mut cur_h);
    }

    let (rot_w, rot_h, rotated) = rotate_rgb_45cw(&rgb, cur_w, cur_h, fuji_rotation_width)?;

    if let Some(rect) = raw_image.crop_area {
        if rect.d.w > 0
            && rect.d.h > 0
            && rect.p.x.saturating_add(rect.d.w) <= rot_w
            && rect.p.y.saturating_add(rect.d.h) <= rot_h
        {
            let mut cropped = Vec::with_capacity(rect.d.w * rect.d.h);
            for row in 0..rect.d.h {
                let start = (rect.p.y + row) * rot_w + rect.p.x;
                cropped.extend_from_slice(&rotated[start..start + rect.d.w]);
            }
            return LinearRgbBuffer::new(rect.d.w, rect.d.h, cropped);
        }
    }

    LinearRgbBuffer::new(rot_w, rot_h, rotated)
}

/// Demosaics a 2x2 4-color CFA sensor (e.g., CYGM / GMYC / RGBE) and calibrates it into
/// linear D65 sRGB (`LinearRgbBuffer`).
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
fn bayer_color_index(origin: BayerOrigin, x: usize, y: usize) -> usize {
    match (origin, y & 1, x & 1) {
        (BayerOrigin::Rggb, 0, 0)
        | (BayerOrigin::Bggr, 1, 1)
        | (BayerOrigin::Grbg, 0, 1)
        | (BayerOrigin::Gbrg, 1, 0) => 0,
        (BayerOrigin::Rggb, 1, 1)
        | (BayerOrigin::Bggr, 0, 0)
        | (BayerOrigin::Grbg, 1, 0)
        | (BayerOrigin::Gbrg, 0, 1) => 2,
        _ => 1,
    }
}

fn demosaic_bayer_bilinear(
    plane: &[f32],
    width: usize,
    height: usize,
    origin: BayerOrigin,
) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0_f32; 3]; width * height];
    let max_x = width - 1;
    let max_y = height - 1;

    for y in 0..height {
        let ym1 = y.saturating_sub(1);
        let yp1 = (y + 1).min(max_y);
        let row = y * width;
        let row_up = ym1 * width;
        let row_dn = yp1 * width;

        for x in 0..width {
            let xm1 = x.saturating_sub(1);
            let xp1 = (x + 1).min(max_x);
            let center = plane[row + x];

            let h_avg = 0.5 * (plane[row + xm1] + plane[row + xp1]);
            let v_avg = 0.5 * (plane[row_up + x] + plane[row_dn + x]);
            let cross_avg = 0.25
                * (plane[row + xm1] + plane[row + xp1] + plane[row_up + x] + plane[row_dn + x]);
            let diag_avg = 0.25
                * (plane[row_up + xm1]
                    + plane[row_up + xp1]
                    + plane[row_dn + xm1]
                    + plane[row_dn + xp1]);

            let rgb = match bayer_color_index(origin, x, y) {
                0 => [center, cross_avg, diag_avg],
                2 => [diag_avg, cross_avg, center],
                _ => {
                    let h_is_red = bayer_color_index(origin, x ^ 1, y) == 0;
                    if h_is_red {
                        [h_avg, center, v_avg]
                    } else {
                        [v_avg, center, h_avg]
                    }
                }
            };
            out[row + x] = rgb;
        }
    }

    out
}

fn rotate_rgb_90cw(src: &[[f32; 3]], width: usize, height: usize) -> Vec<[f32; 3]> {
    let mut dst = vec![[0.0_f32; 3]; width * height];
    for y in 0..height {
        let src_row = y * width;
        let dst_col = height - 1 - y;
        for x in 0..width {
            dst[x * height + dst_col] = src[src_row + x];
        }
    }
    dst
}

fn rotate_rgb_45cw(
    src: &[[f32; 3]],
    src_w: usize,
    src_h: usize,
    fuji_rotation_width: usize,
) -> Result<(usize, usize, Vec<[f32; 3]>), DecodeError> {
    let (crop_w, crop_h) = fuji_rotated_dimensions(src_w, fuji_rotation_width);
    if crop_w == 0 || crop_h == 0 {
        return Err(CoreError::InvalidDimensions {
            width: crop_w,
            height: crop_h,
        }
        .into());
    }

    let inv_sqrt2 = std::f64::consts::FRAC_1_SQRT_2;
    let src_cx = src_w as f64 * 0.5;
    let src_cy = src_h as f64 * 0.5;
    let crop_cx = crop_w as f64 * 0.5;
    let crop_cy = crop_h as f64 * 0.5;

    let mut dst = vec![[0.0_f32; 3]; crop_w * crop_h];
    let src_w_i = src_w as isize;
    let src_h_i = src_h as isize;

    for row in 0..crop_h {
        let dy = row as f64 - crop_cy;
        let dst_row = row * crop_w;
        for col in 0..crop_w {
            let dx = col as f64 - crop_cx;
            let src_x = (dx + dy) * inv_sqrt2 + src_cx;
            let src_y = (dy - dx) * inv_sqrt2 + src_cy;

            let sx = src_x.round() as isize;
            let sy = src_y.round() as isize;
            if sx < 0 || sx >= src_w_i || sy < 0 || sy >= src_h_i {
                continue;
            }

            let x0 = src_x.floor() as isize;
            let y0 = src_y.floor() as isize;
            let x1 = x0 + 1;
            let y1 = y0 + 1;

            if x0 < 0 || y0 < 0 || x1 >= src_w_i || y1 >= src_h_i {
                dst[dst_row + col] = src[sy as usize * src_w + sx as usize];
            } else {
                let fx = (src_x - x0 as f64) as f32;
                let fy = (src_y - y0 as f64) as f32;
                let w00 = (1.0 - fx) * (1.0 - fy);
                let w10 = fx * (1.0 - fy);
                let w01 = (1.0 - fx) * fy;
                let w11 = fx * fy;

                let p00 = src[y0 as usize * src_w + x0 as usize];
                let p10 = src[y0 as usize * src_w + x1 as usize];
                let p01 = src[y1 as usize * src_w + x0 as usize];
                let p11 = src[y1 as usize * src_w + x1 as usize];

                dst[dst_row + col] = [
                    p00[0] * w00 + p10[0] * w10 + p01[0] * w01 + p11[0] * w11,
                    p00[1] * w00 + p10[1] * w10 + p01[1] * w01 + p11[1] * w11,
                    p00[2] * w00 + p10[2] * w10 + p01[2] * w01 + p11[2] * w11,
                ];
            }
        }
    }

    Ok((crop_w, crop_h, dst))
}

fn demosaic_four_color_bilinear(
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

fn sanitize_four_channel_wb_with_matrix(wb_coeffs: [f32; 4], xyz2cam: &[[f32; 3]; 4]) -> [f32; 4] {
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

fn compute_cam4_to_srgb_matrix(xyz2cam: &[[f32; 3]; 4]) -> Option<[[f32; 4]; 3]> {
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
fn pseudo_inverse_4x3(m: &[[f32; 3]; 4]) -> Option<[[f32; 4]; 3]> {
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

/// Decodes a true monochrome (CFA-less, `cpp == 1`) sensor into a neutral linear RGB buffer
/// (`[luma, luma, luma]`) with signed black/white normalization and active-area cropping.
pub fn decode_monochrome_sensor_to_linear_rgb(
    raw_image: &rawler::RawImage,
) -> Result<LinearRgbBuffer, DecodeError> {
    let full_w = raw_image.width;
    let full_h = raw_image.height;
    let expected_len = full_w
        .checked_mul(full_h)
        .ok_or(CoreError::InvalidDimensions {
            width: full_w,
            height: full_h,
        })?;
    if expected_len == 0 {
        return Err(CoreError::InvalidDimensions {
            width: full_w,
            height: full_h,
        }
        .into());
    }

    let black = raw_image.blacklevel.as_bayer_array()[0];
    let white = raw_image.whitelevel.as_bayer_array()[0];
    let span = white - black;
    if !black.is_finite() || !white.is_finite() || !span.is_finite() || span <= 0.0 {
        return Err(DecodeError::MalformedMetadata(format!(
            "invalid monochrome black/white level range: black={black}, white={white}"
        )));
    }
    let inv_span = 1.0 / span;

    let (crop_x, crop_y, crop_w, crop_h) = if let Some(crop) =
        raw_image.crop_area.or(raw_image.active_area)
    {
        let (cx, cy, cw, ch) = (crop.x(), crop.y(), crop.width(), crop.height());
        if cw > 0 && ch > 0 && cx.saturating_add(cw) <= full_w && cy.saturating_add(ch) <= full_h {
            (cx, cy, cw, ch)
        } else {
            (0, 0, full_w, full_h)
        }
    } else {
        (0, 0, full_w, full_h)
    };

    let mut rgb = Vec::with_capacity(crop_w * crop_h);
    match &raw_image.data {
        rawler::RawImageData::Integer(samples) => {
            if samples.len() != expected_len {
                return Err(CoreError::BufferLengthMismatch {
                    expected: expected_len,
                    actual: samples.len(),
                }
                .into());
            }
            for y in crop_y..(crop_y + crop_h) {
                let row_start = y * full_w + crop_x;
                for &v in &samples[row_start..row_start + crop_w] {
                    let luma = (f32::from(v) - black) * inv_span;
                    rgb.push([luma, luma, luma]);
                }
            }
        }
        rawler::RawImageData::Float(samples) => {
            if samples.len() != expected_len {
                return Err(CoreError::BufferLengthMismatch {
                    expected: expected_len,
                    actual: samples.len(),
                }
                .into());
            }
            for y in crop_y..(crop_y + crop_h) {
                let row_start = y * full_w + crop_x;
                for &v in &samples[row_start..row_start + crop_w] {
                    let clean = if v.is_finite() { v } else { 0.0 };
                    let luma = (clean - black) * inv_span;
                    rgb.push([luma, luma, luma]);
                }
            }
        }
    }

    LinearRgbBuffer::new(crop_w, crop_h, rgb)
}

/// Detects isolated single-photosite hot, warm, or dead pixels on a normalized Bayer CFA plane
/// (common on unmapped medium-format CCD backs), returning `(index, neighbor_median)` pairs.
///
/// Compares each sample against all 8 same-color neighbors (`(x±2, y)`, `(x, y±2)`, and
/// diagonals `(x±2, y±2)`), so real horizontal, vertical, and diagonal edges or highlights
/// are preserved untouched. Flags:
/// - **Hot/warm impulse**: `c >= 0.04`, `c - n_max_8 >= 0.035`, and `c >= n_max_8.max(0.005) * 2.5`
/// - **Dead pixel**: `c <= 0.002` while all 8 same-color neighbors satisfy `n_min_8 >= 0.05`
///
/// Replacement uses the median of the 4 cardinal same-color neighbors. Allocates nothing
/// when no isolated impulses are present.
#[must_use]
pub fn detect_isolated_bayer_hot_pixels(
    samples: &[f32],
    width: usize,
    height: usize,
) -> Vec<(usize, f32)> {
    if width < 5 || height < 5 || samples.len() != width.saturating_mul(height) {
        return Vec::new();
    }

    let mut replacements: Vec<(usize, f32)> = Vec::new();
    for y in 2..(height - 2) {
        let row = y * width;
        let row_up = (y - 2) * width;
        let row_dn = (y + 2) * width;
        for x in 2..(width - 2) {
            let idx = row + x;
            let c = samples[idx];
            if !c.is_finite() {
                continue;
            }
            let is_warm_candidate = c >= 0.04;
            let is_dead_candidate = c <= 0.002;
            if !is_warm_candidate && !is_dead_candidate {
                continue;
            }

            let mut cardinal = [
                samples[row_up + x],
                samples[row_dn + x],
                samples[row + (x - 2)],
                samples[row + (x + 2)],
            ];
            let diag = [
                samples[row_up + (x - 2)],
                samples[row_up + (x + 2)],
                samples[row_dn + (x - 2)],
                samples[row_dn + (x + 2)],
            ];
            if !cardinal.iter().all(|v| v.is_finite()) || !diag.iter().all(|v| v.is_finite()) {
                continue;
            }

            let n_max = cardinal
                .iter()
                .chain(diag.iter())
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let n_min = cardinal
                .iter()
                .chain(diag.iter())
                .copied()
                .fold(f32::INFINITY, f32::min);

            let is_hot_or_warm =
                is_warm_candidate && c - n_max >= 0.035 && c >= n_max.max(0.005) * 2.5;
            let is_dead = is_dead_candidate && n_min >= 0.05;

            if is_hot_or_warm || is_dead {
                cardinal.sort_by(f32::total_cmp);
                let median = 0.5 * (cardinal[1] + cardinal[2]);
                replacements.push((idx, median));
            }
        }
    }

    replacements
}

/// Demosaics a 4-color 8-row × 2-column repeating CFA plane (e.g., Canon PowerShot Pro70
/// `YC/MG/CY/GM/YC/GM/CY/MG`) using weighted 3x3 neighborhood interpolation.
fn demosaic_four_color_8x2_bilinear(
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

#[cfg(test)]
mod tests {
    use super::{
        demosaic_four_color_bilinear, fuji_rotated_dimensions, pseudo_inverse_4x3, rotate_rgb_45cw,
    };

    #[test]
    fn pseudo_inverse_4x3_recovers_identity() {
        let m = [
            [0.8, 0.2, 0.0],
            [0.1, 0.7, 0.2],
            [0.0, 0.3, 0.7],
            [0.5, 0.4, 0.1],
        ];
        let pinv = pseudo_inverse_4x3(&m);
        assert!(pinv.is_some());
        if let Some(pinv) = pinv {
            for (r, pinv_row) in pinv.iter().enumerate() {
                for c in 0..3 {
                    let prod = pinv_row[0] * m[0][c]
                        + pinv_row[1] * m[1][c]
                        + pinv_row[2] * m[2][c]
                        + pinv_row[3] * m[3][c];
                    let expected = if r == c { 1.0 } else { 0.0 };
                    assert!(
                        (prod - expected).abs() < 1e-4,
                        "pinv*m[{r}][{c}] = {prod}, expected {expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn rotate_rgb_45cw_preserves_uniform_color_and_inscribed_dims() {
        let src_w = 100;
        let src_h = 100;
        let fuji_w = 60;
        let (exp_w, exp_h) = fuji_rotated_dimensions(src_w, fuji_w);
        assert!(exp_w > exp_h);
        assert_eq!(exp_w, (60.0 * std::f64::consts::SQRT_2).floor() as usize);
        assert_eq!(exp_h, (40.0 * std::f64::consts::SQRT_2).floor() as usize);

        let src = vec![[0.25_f32, 0.5, 0.75]; src_w * src_h];
        let res = rotate_rgb_45cw(&src, src_w, src_h, fuji_w);
        assert!(res.is_ok());
        if let Ok((w, h, pixels)) = res {
            assert_eq!((w, h), (exp_w, exp_h));
            let center = pixels[(h / 2) * w + (w / 2)];
            assert!((center[0] - 0.25).abs() < 1e-4);
            assert!((center[1] - 0.50).abs() < 1e-4);
            assert!((center[2] - 0.75).abs() < 1e-4);
        }
    }

    #[test]
    fn four_color_bilinear_recovers_constant_channels_everywhere() {
        let w = 6;
        let h = 6;
        let lut = [[0, 1], [2, 3]];
        let vals = [0.2_f32, 0.4, 0.6, 0.8];
        let mut plane = vec![0.0_f32; w * h];
        for y in 0..h {
            for x in 0..w {
                plane[y * w + x] = vals[lut[y & 1][x & 1]];
            }
        }

        let out = demosaic_four_color_bilinear(&plane, w, h, lut);
        for (idx, pix) in out.iter().enumerate() {
            for ch in 0..4 {
                assert!(
                    (pix[ch] - vals[ch]).abs() < 1e-5,
                    "pixel {idx} ch {ch}: got {}, expected {}",
                    pix[ch],
                    vals[ch]
                );
            }
        }
    }

    #[test]
    fn detect_isolated_bayer_hot_pixels_heals_spike_and_preserves_highlights() {
        let w = 12;
        let h = 12;
        let mut plane = vec![0.08_f32; w * h];
        // Isolated saturated hot pixel at (5, 5)
        plane[5 * w + 5] = 1.04;
        // Isolated shadow warm pixel at (9, 5)
        plane[5 * w + 9] = 0.25;
        // Isolated dead pixel at (5, 9)
        plane[9 * w + 5] = 0.0;
        // Real diagonal 2-pixel same-color highlight at (2, 2) and (4, 4) must NOT be touched
        plane[2 * w + 2] = 1.00;
        plane[4 * w + 4] = 0.98;

        let fixes = super::detect_isolated_bayer_hot_pixels(&plane, w, h);
        assert_eq!(fixes.len(), 3);
        let by_idx: std::collections::HashMap<usize, f32> = fixes.into_iter().collect();
        assert!((by_idx[&(5 * w + 5)] - 0.08).abs() < 1e-6);
        assert!((by_idx[&(5 * w + 9)] - 0.08).abs() < 1e-6);
        assert!((by_idx[&(9 * w + 5)] - 0.08).abs() < 1e-6);
    }

    #[test]
    fn canon_pro70_ciff_synthetic_and_errors() -> Result<(), Box<dyn std::error::Error>> {
        struct TempFile(std::path::PathBuf);
        impl Drop for TempFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let dir = std::env::temp_dir();
        let tf_bad = TempFile(dir.join(format!("rraw_test_bad_{}.ciff", std::process::id())));
        std::fs::write(&tf_bad.0, b"NOT_CIFF")?;
        assert!(super::try_decode_canon_pro70_ciff(&tf_bad.0).is_err());

        const SIZE: usize = 26 + 1024 * 1940 + 80;
        let mut buf = vec![0x55_u8; SIZE];
        buf[0..2].copy_from_slice(b"II");
        buf[6..14].copy_from_slice(b"HEAPCCDR");
        let tail_start = 26 + 1024 * 1940;
        buf[tail_start..tail_start + 15].copy_from_slice(b"PowerShot Pro70");
        let tf = TempFile(dir.join(format!("rraw_test_good_{}.ciff", std::process::id())));
        std::fs::write(&tf.0, &buf)?;
        let raw = super::try_decode_canon_pro70_ciff(&tf.0)?;
        assert_eq!(raw.metadata.width, 1536);
        assert_eq!(raw.metadata.height, 1024);
        assert!(raw.linear_rgb.is_some());
        Ok(())
    }

    #[test]
    fn is_four_color_2x2_cfa_checks() {
        assert!(super::is_four_color_2x2_cfa(&rawler::cfa::CFA::new("CYGM")));
        assert!(!super::is_four_color_2x2_cfa(&rawler::cfa::CFA::new(
            "RGGB"
        )));
    }

    fn make_test_raw_image(w: usize, h: usize, data: Vec<u16>, cfa_str: &str) -> rawler::RawImage {
        let cam = rawler::decoders::Camera {
            cfa: rawler::cfa::CFA::new(cfa_str),
            ..Default::default()
        };
        let pix = rawler::pixarray::PixU16::new_with(data, w, h);
        rawler::RawImage::new(
            cam,
            pix,
            1,
            [1.0; 4],
            rawler::rawimage::RawPhotometricInterpretation::BlackIsZero,
            Some(rawler::rawimage::BlackLevel::new(&[100u16; 4], 2, 2, 1)),
            Some(rawler::rawimage::WhiteLevel::new(vec![4095; 4])),
            false,
        )
    }

    #[test]
    fn test_demosaic_and_rotate_fuji_superccd_synthetic() -> Result<(), Box<dyn std::error::Error>>
    {
        let raw = make_test_raw_image(32, 32, vec![1000u16; 1024], "RGGB");
        let buf = super::demosaic_and_rotate_fuji_superccd(
            &raw,
            &rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
            20,
        )?;
        assert!(buf.width() > 0 && buf.height() > 0);
        Ok(())
    }

    #[test]
    fn test_decode_monochrome_sensor_to_linear_rgb_synthetic(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let raw = make_test_raw_image(16, 16, vec![2000u16; 256], "");
        let buf = super::decode_monochrome_sensor_to_linear_rgb(&raw)?;
        assert!(buf.width() == 16 && buf.height() == 16);

        let mut raw2 = make_test_raw_image(16, 16, vec![2000u16; 256], "");
        raw2.data = rawler::RawImageData::Float(vec![2000.0; 256]);
        let buf2 = super::decode_monochrome_sensor_to_linear_rgb(&raw2)?;
        assert!(buf2.width() == 16 && buf2.height() == 16);
        Ok(())
    }

    #[test]
    fn test_demosaic_four_color_cfa_error_on_bayer() {
        let raw = make_test_raw_image(16, 16, vec![1000u16; 256], "RGGB");
        assert!(super::demosaic_four_color_cfa_to_linear_srgb(
            &raw,
            &rawler::cfa::CFA::new("RGGB")
        )
        .is_err());
    }

    #[test]
    fn test_demosaic_four_color_cfa_to_linear_srgb_synthetic(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_test_raw_image(16, 16, vec![1000u16; 256], "RGBE");
        raw.color_matrix.insert(
            rawler::imgop::xyz::Illuminant::D65,
            vec![0.8, 0.2, 0.0, 0.1, 0.7, 0.2, 0.0, 0.3, 0.7, 0.5, 0.4, 0.1],
        );
        let buf =
            super::demosaic_four_color_cfa_to_linear_srgb(&raw, &rawler::cfa::CFA::new("RGBE"))?;
        assert_eq!((buf.width(), buf.height()), (16, 16));
        Ok(())
    }

    #[test]
    fn test_demosaic_four_color_cfa_float_synthetic() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_test_raw_image(16, 16, vec![1000u16; 256], "RGBE");
        raw.data = rawler::RawImageData::Float(vec![1000.0; 256]);
        raw.color_matrix.insert(
            rawler::imgop::xyz::Illuminant::D65,
            vec![0.8, 0.2, 0.0, 0.1, 0.7, 0.2, 0.0, 0.3, 0.7, 0.5, 0.4, 0.1],
        );
        let buf =
            super::demosaic_four_color_cfa_to_linear_srgb(&raw, &rawler::cfa::CFA::new("RGBE"))?;
        assert_eq!((buf.width(), buf.height()), (16, 16));
        Ok(())
    }

    #[test]
    fn test_fuji_superccd_float_and_xtrans_err() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = make_test_raw_image(32, 32, vec![1000u16; 1024], "RGGB");
        raw.data = rawler::RawImageData::Float(vec![1000.0; 1024]);
        let buf = super::demosaic_and_rotate_fuji_superccd(
            &raw,
            &rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
            20,
        )?;
        assert!(buf.width() > 0 && buf.height() > 0);
        assert!(super::demosaic_and_rotate_fuji_superccd(
            &raw,
            &rraw_core::CfaPattern::fujifilm_xtrans(),
            20,
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn test_monochrome_and_4color_crop_area() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw_mono = make_test_raw_image(16, 16, vec![2000u16; 256], "");
        raw_mono.crop_area = Some(rawler::imgop::Rect::new(
            rawler::imgop::Point::new(2, 2),
            rawler::imgop::Dim2::new(10, 10),
        ));
        let buf = super::decode_monochrome_sensor_to_linear_rgb(&raw_mono)?;
        assert_eq!((buf.width(), buf.height()), (10, 10));

        let mut raw4 = make_test_raw_image(16, 16, vec![1000u16; 256], "RGBE");
        raw4.crop_area = Some(rawler::imgop::Rect::new(
            rawler::imgop::Point::new(2, 2),
            rawler::imgop::Dim2::new(12, 12),
        ));
        raw4.color_matrix.insert(
            rawler::imgop::xyz::Illuminant::D65,
            vec![0.8, 0.2, 0.0, 0.1, 0.7, 0.2, 0.0, 0.3, 0.7, 0.5, 0.4, 0.1],
        );
        let buf4 =
            super::demosaic_four_color_cfa_to_linear_srgb(&raw4, &rawler::cfa::CFA::new("RGBE"))?;
        assert_eq!((buf4.width(), buf4.height()), (12, 12));

        let mut raw_fuji = make_test_raw_image(32, 32, vec![1000u16; 1024], "RGGB");
        raw_fuji.crop_area = Some(rawler::imgop::Rect::new(
            rawler::imgop::Point::new(2, 2),
            rawler::imgop::Dim2::new(28, 28),
        ));
        let buf_fuji = super::demosaic_and_rotate_fuji_superccd(
            &raw_fuji,
            &rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
            18,
        )?;
        assert!(buf_fuji.width() > 0 && buf_fuji.height() > 0);
        Ok(())
    }
}
