#![forbid(unsafe_code)]

//! Monochrome sensor decoding and isolated Bayer hot-pixel suppression.

use rraw_core::CoreError;

use crate::{DecodeError, LinearRgbBuffer};

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
