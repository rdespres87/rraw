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
}
