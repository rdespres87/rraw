#![forbid(unsafe_code)]

//! Sensor CFA sample extraction, optical-black cropping, and white-balance/color-matrix helpers.

use std::borrow::Cow;

use rraw_core::CfaPattern;

use crate::cfa::{align_crop_to_cfa_phase, SensorCropRect, SensorDimensions};
use crate::DecodeError;

/// Typed single-plane sensor samples extracted from `rawler`.
#[derive(Debug, Clone, PartialEq)]
pub enum RawSamples<'a> {
    U16(Cow<'a, [u16]>),
    F32(Cow<'a, [f32]>),
}

fn crop_plane<'a, T: Copy>(
    samples: &'a [T],
    raw_image: &rawler::RawImage,
    cfa_pattern: &CfaPattern,
) -> Result<(usize, usize, Cow<'a, [T]>), DecodeError> {
    let expected_len = raw_image
        .width
        .checked_mul(raw_image.height)
        .ok_or_else(|| {
            DecodeError::MalformedMetadata(format!(
                "sensor dimensions overflow: {}x{}",
                raw_image.width, raw_image.height
            ))
        })?;
    if samples.len() != expected_len {
        return Err(DecodeError::MalformedMetadata(format!(
            "sensor plane length mismatch: expected {expected_len}, got {}",
            samples.len()
        )));
    }

    if let Some(crop) = raw_image.crop_area.or(raw_image.active_area) {
        let aligned = align_crop_to_cfa_phase(
            cfa_pattern,
            SensorCropRect {
                x: crop.x(),
                y: crop.y(),
                width: crop.width(),
                height: crop.height(),
            },
            SensorDimensions {
                width: raw_image.width,
                height: raw_image.height,
            },
        );
        let (crop_x, crop_y, crop_w, crop_h) =
            (aligned.x, aligned.y, aligned.width, aligned.height);
        if crop_x + crop_w <= raw_image.width
            && crop_y + crop_h <= raw_image.height
            && crop_w > 0
            && crop_h > 0
        {
            let mut cropped = Vec::with_capacity(crop_w * crop_h);
            for row in crop_y..(crop_y + crop_h) {
                let start = row * raw_image.width + crop_x;
                let end = start + crop_w;
                cropped.extend_from_slice(&samples[start..end]);
            }
            return Ok((crop_w, crop_h, Cow::Owned(cropped)));
        }
    }

    Ok((raw_image.width, raw_image.height, Cow::Borrowed(samples)))
}

/// Crops optical black / sensor margins while preserving the CFA phase.
///
/// Rejects multi-channel (`cpp != 1`, e.g. Linear DNG / sRAW) and non-CFA images
/// rather than silently slicing interleaved RGB samples into the Bayer pipeline.
pub fn extract_cropped_samples<'a>(
    raw_image: &'a rawler::RawImage,
    cfa_pattern: &CfaPattern,
) -> Result<(usize, usize, RawSamples<'a>), DecodeError> {
    if raw_image.cpp != 1 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "multi-channel RAW (cpp={}) is not supported as single-plane CFA",
            raw_image.cpp
        )));
    }

    if !matches!(
        raw_image.photometric,
        rawler::rawimage::RawPhotometricInterpretation::Cfa(_)
    ) {
        return Err(DecodeError::UnsupportedFormat(format!(
            "non-CFA photometric interpretation ({:?}) is not supported",
            raw_image.photometric
        )));
    }

    match &raw_image.data {
        rawler::RawImageData::Integer(samples) => {
            let (w, h, plane) = crop_plane(samples.as_slice(), raw_image, cfa_pattern)?;
            Ok((w, h, RawSamples::U16(plane)))
        }
        rawler::RawImageData::Float(samples) => {
            let (w, h, plane) = crop_plane(samples.as_slice(), raw_image, cfa_pattern)?;
            Ok((w, h, RawSamples::F32(plane)))
        }
    }
}

/// Replaces non-finite or non-positive white-balance multipliers with safe daylight fallbacks.
#[must_use]
pub fn sanitize_wb_coeffs(mut wb_coeffs: [f32; 4]) -> [f32; 4] {
    if !wb_coeffs[0].is_finite() || wb_coeffs[0] <= 0.0 {
        wb_coeffs[0] = 2.0;
    }
    if !wb_coeffs[1].is_finite() || wb_coeffs[1] <= 0.0 {
        wb_coeffs[1] = 1.0;
    }
    if !wb_coeffs[2].is_finite() || wb_coeffs[2] <= 0.0 {
        wb_coeffs[2] = 1.5;
    }
    wb_coeffs
}

/// Extracts the first available 3x3 XYZ-to-camera color matrix (D65 -> D50 -> A preference).
#[must_use]
pub fn extract_color_matrix(raw_image: &rawler::RawImage) -> Option<[f32; 9]> {
    raw_image
        .color_matrix_find_first([
            rawler::imgop::xyz::Illuminant::D65,
            rawler::imgop::xyz::Illuminant::D50,
            rawler::imgop::xyz::Illuminant::A,
        ])
        .and_then(|(_, mat)| {
            if mat.len() >= 9 {
                let mut arr = [0.0_f32; 9];
                arr.copy_from_slice(&mat[0..9]);
                Some(arr)
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::sanitize_wb_coeffs;

    #[test]
    fn sanitize_wb_replaces_invalid_channels_and_preserves_valid_ones() {
        let out = sanitize_wb_coeffs([f32::NAN, 0.0, -1.0, 1.0]);
        assert_eq!(out, [2.0, 1.0, 1.5, 1.0]);

        let valid = sanitize_wb_coeffs([2.15, 1.0, 1.62, 1.0]);
        assert_eq!(valid, [2.15, 1.0, 1.62, 1.0]);
    }
}
