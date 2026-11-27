#![forbid(unsafe_code)]

//! Fujifilm SuperCCD 45° octagonal sensor demosaicing and rotation.

use rraw_core::{BayerOrigin, CfaPattern, CoreError};

use crate::{DecodeError, LinearRgbBuffer};

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

pub(super) fn rotate_rgb_45cw(
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
