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

fn crop_rgb_plane<'a, T: Copy>(
    samples: &'a [T],
    raw_image: &rawler::RawImage,
) -> Result<(usize, usize, Cow<'a, [T]>), DecodeError> {
    let expected_len = raw_image
        .width
        .checked_mul(raw_image.height)
        .and_then(|px| px.checked_mul(3))
        .ok_or_else(|| {
            DecodeError::MalformedMetadata(format!(
                "linear RGB dimensions overflow: {}x{}x3",
                raw_image.width, raw_image.height
            ))
        })?;
    if samples.len() != expected_len {
        return Err(DecodeError::MalformedMetadata(format!(
            "linear RGB plane length mismatch: expected {expected_len}, got {}",
            samples.len()
        )));
    }

    if let Some(crop) = raw_image.crop_area.or(raw_image.active_area) {
        let (crop_x, crop_y, crop_w, crop_h) = (crop.x(), crop.y(), crop.width(), crop.height());
        if crop_x.saturating_add(crop_w) <= raw_image.width
            && crop_y.saturating_add(crop_h) <= raw_image.height
            && crop_w > 0
            && crop_h > 0
            && (crop_x > 0 || crop_y > 0 || crop_w < raw_image.width || crop_h < raw_image.height)
        {
            let row_stride = raw_image.width * 3;
            let copy_len = crop_w * 3;
            let mut cropped = Vec::with_capacity(crop_h * copy_len);
            for row in crop_y..(crop_y + crop_h) {
                let start = row * row_stride + crop_x * 3;
                let end = start + copy_len;
                cropped.extend_from_slice(&samples[start..end]);
            }
            return Ok((crop_w, crop_h, Cow::Owned(cropped)));
        }
    }

    Ok((raw_image.width, raw_image.height, Cow::Borrowed(samples)))
}

/// Extracts cropped interleaved 3-channel linear camera-RGB samples (`cpp == 3`,
/// e.g., Apple ProRAW, Canon sRAW, Adobe Linear DNG, Sony Pixel-Shift ARW).
pub fn extract_cropped_rgb_samples<'a>(
    raw_image: &'a rawler::RawImage,
) -> Result<(usize, usize, RawSamples<'a>), DecodeError> {
    if raw_image.cpp != 3 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "expected 3-channel linear RGB RAW (cpp=3), got cpp={}",
            raw_image.cpp
        )));
    }

    match &raw_image.data {
        rawler::RawImageData::Integer(samples) => {
            let (w, h, plane) = crop_rgb_plane(samples.as_slice(), raw_image)?;
            Ok((w, h, RawSamples::U16(plane)))
        }
        rawler::RawImageData::Float(samples) => {
            let (w, h, plane) = crop_rgb_plane(samples.as_slice(), raw_image)?;
            Ok((w, h, RawSamples::F32(plane)))
        }
    }
}

/// Extracts per-channel `[R, G, B]` black and white levels for 3-channel linear RGB RAWs.
///
/// Canon CR2 sRAW/mRAW decoding in `rawler` converts YCbCr to 0-referenced 16-bit RGB
/// (already subtracting the sensor Y offset and clamping at 0), while still reporting
/// the 14-bit Bayer black pedestal in `raw_image.blacklevel`. Subtracting that pedestal
/// a second time crushes dark blue shadows into a yellow cast, so Canon sRAW uses `[0.0; 3]`.
#[must_use]
pub fn extract_rgb_black_white_levels(raw_image: &rawler::RawImage) -> ([f32; 3], [f32; 3]) {
    let is_canon_sraw = raw_image.clean_make.eq_ignore_ascii_case("Canon")
        && (raw_image.camera.mode.to_ascii_lowercase().contains("sraw")
            || raw_image.camera.find_hint("sraw_40d")
            || raw_image.camera.find_hint("sraw_new"));

    let bl = &raw_image.blacklevel.levels;
    let black = if is_canon_sraw {
        [0.0; 3]
    } else if bl.len() >= 3 {
        [bl[0].as_f32(), bl[1].as_f32(), bl[2].as_f32()]
    } else if let Some(b0) = bl.first() {
        [b0.as_f32(); 3]
    } else {
        [0.0; 3]
    };

    let wl = &raw_image.whitelevel.0;
    let white = if wl.len() >= 3 {
        [wl[0] as f32, wl[1] as f32, wl[2] as f32]
    } else if let Some(&w0) = wl.first() {
        [w0 as f32; 3]
    } else {
        [65_535.0; 3]
    };

    (black, white)
}

/// Replaces non-finite or non-positive white-balance multipliers with safe daylight fallbacks
/// and normalizes channels relative to Green (`wb[1] = 1.0`).
#[must_use]
pub fn sanitize_wb_coeffs(wb_coeffs: [f32; 4]) -> [f32; 4] {
    sanitize_wb_coeffs_with_matrix(wb_coeffs, None)
}

/// Sanitizes white-balance multipliers, deriving exact D65 daylight multipliers from the
/// `XYZ -> Camera` `color_matrix` whenever camera as-shot WB is missing (`NaN` or `<= 0`).
#[must_use]
pub fn sanitize_wb_coeffs_with_matrix(
    mut wb_coeffs: [f32; 4],
    color_matrix: Option<&[f32; 9]>,
) -> [f32; 4] {
    let valid_rgb = wb_coeffs[0].is_finite()
        && wb_coeffs[0] > 0.0
        && wb_coeffs[1].is_finite()
        && wb_coeffs[1] > 0.0
        && wb_coeffs[2].is_finite()
        && wb_coeffs[2] > 0.0;

    if valid_rgb {
        let g = wb_coeffs[1];
        wb_coeffs[0] /= g;
        wb_coeffs[1] = 1.0;
        wb_coeffs[2] /= g;
        if wb_coeffs[3].is_finite() && wb_coeffs[3] > 0.0 {
            wb_coeffs[3] /= g;
        }
        return wb_coeffs;
    }

    // Derive D65 daylight neutral multipliers from XYZ_D65 -> Camera matrix when available
    if let Some(cm) = color_matrix {
        const D65_XYZ: [f32; 3] = [0.950_47, 1.0, 1.088_83];
        let cam_r = cm[0] * D65_XYZ[0] + cm[1] * D65_XYZ[1] + cm[2] * D65_XYZ[2];
        let cam_g = cm[3] * D65_XYZ[0] + cm[4] * D65_XYZ[1] + cm[5] * D65_XYZ[2];
        let cam_b = cm[6] * D65_XYZ[0] + cm[7] * D65_XYZ[1] + cm[8] * D65_XYZ[2];
        if cam_r.is_finite()
            && cam_g.is_finite()
            && cam_b.is_finite()
            && cam_r > 1e-4
            && cam_g > 1e-4
            && cam_b > 1e-4
        {
            return [cam_g / cam_r, 1.0, cam_g / cam_b, 1.0];
        }
    }

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
    use super::{sanitize_wb_coeffs, sanitize_wb_coeffs_with_matrix};

    #[test]
    fn sanitize_wb_replaces_invalid_channels_and_preserves_valid_ones() {
        let out = sanitize_wb_coeffs([f32::NAN, 0.0, -1.0, 1.0]);
        assert_eq!(out, [2.0, 1.0, 1.5, 1.0]);

        let valid = sanitize_wb_coeffs([2.15, 1.0, 1.62, 1.0]);
        assert_eq!(valid, [2.15, 1.0, 1.62, 1.0]);

        // Unnormalized WB (e.g., G = 256) must normalize relative to Green
        let unnorm = sanitize_wb_coeffs([512.0, 256.0, 384.0, 256.0]);
        assert!((unnorm[0] - 2.0).abs() < 1e-5);
        assert!((unnorm[1] - 1.0).abs() < 1e-5);
        assert!((unnorm[2] - 1.5).abs() < 1e-5);
    }

    #[test]
    fn sanitize_wb_derives_d65_from_color_matrix_when_camera_wb_missing() {
        // Mamiya ZD XYZ->Cam matrix where rawler reports NaN WB
        let mamiya_cm: [f32; 9] = [
            0.7645, 0.2579, -0.1363, -0.8689, 1.6717, 0.2014, -0.3712, 0.5941, 0.5961,
        ];
        let wb = sanitize_wb_coeffs_with_matrix([f32::NAN; 4], Some(&mamiya_cm));
        assert!(
            (wb[0] - 1.274).abs() < 0.02,
            "expected ~1.27, got {}",
            wb[0]
        );
        assert!((wb[1] - 1.0).abs() < 1e-5);
        assert!(
            (wb[2] - 1.196).abs() < 0.02,
            "expected ~1.20, got {}",
            wb[2]
        );
    }
}
