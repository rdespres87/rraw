#![forbid(unsafe_code)]

//! Top-level TIFF/DNG IFD selection, normalization, and metadata assembly.

use std::path::Path;

use rraw_core::{BayerOrigin, CfaPattern, ColorLabel, PhotoMetadata, PickStatus};

use super::ifd::{parse_ifd, Endian};
use super::payloads::{
    decode_dng_jpeg_cfa, decode_dng_uncompressed_cfa, decode_dng_vc5_cfa,
    decode_uncompressed_rgb_tiff,
};
use crate::noise_profile::estimate_noise_profile_wavelet_mad;
use crate::{
    align_crop_to_cfa_phase, exif, normalize_cfa_signed, sensor, DecodeError, DecodedRaw,
    SensorCropRect, SensorDimensions,
};

pub(super) fn cfa_pattern_from_bytes(pat: [u8; 4]) -> Option<CfaPattern> {
    match pat {
        [0, 1, 1, 2] => Some(CfaPattern::Bayer(BayerOrigin::Rggb)),
        [2, 1, 1, 0] => Some(CfaPattern::Bayer(BayerOrigin::Bggr)),
        [1, 0, 2, 1] => Some(CfaPattern::Bayer(BayerOrigin::Grbg)),
        [1, 2, 0, 1] => Some(CfaPattern::Bayer(BayerOrigin::Gbrg)),
        _ => None,
    }
}

pub(super) fn decode_dng_tiff(path: &Path, data: &[u8]) -> Result<DecodedRaw, DecodeError> {
    if data.len() < 8 {
        return Err(DecodeError::UnsupportedFormat(
            "file too small to be a TIFF/DNG container".to_owned(),
        ));
    }
    let endian = match &data[0..2] {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => {
            return Err(DecodeError::UnsupportedFormat(
                "unrecognized TIFF/DNG byte order".to_owned(),
            ));
        }
    };
    if endian.u16(data, 2) != Some(42) {
        return Err(DecodeError::UnsupportedFormat(
            "invalid TIFF/DNG magic number".to_owned(),
        ));
    }
    let ifd0_off = endian
        .u32(data, 4)
        .ok_or_else(|| DecodeError::MalformedMetadata("missing TIFF IFD0 offset".to_owned()))?
        as usize;
    let ifd0 = parse_ifd(data, ifd0_off, endian)
        .ok_or_else(|| DecodeError::MalformedMetadata("failed to parse TIFF IFD0".to_owned()))?;

    let mut candidates = Vec::new();
    candidates.push(ifd0.clone());
    for &sub_off in &ifd0.sub_ifds {
        if let Some(sub) = parse_ifd(data, sub_off, endian) {
            candidates.push(sub);
        }
    }

    // Select primary full-resolution CFA or LinearRaw IFD
    let raw_ifd = candidates
        .iter()
        .filter(|c| {
            c.geometry.width > 0
                && c.geometry.height > 0
                && matches!(c.geometry.photometric, 32803 | 34892)
        })
        .max_by_key(|c| {
            (
                c.new_subfile_type == 0,
                c.geometry.width.saturating_mul(c.geometry.height),
            )
        })
        .cloned();

    let Some(raw_ifd) = raw_ifd else {
        // Fallback for uncompressed 8-bit / 16-bit interleaved RGB TIFF/NEF scanners
        // (e.g., Nikon COOLSCAN IV/V ED).
        if let Some(rgb_ifd) = candidates
            .into_iter()
            .filter(|c| {
                c.geometry.width > 0
                    && c.geometry.height > 0
                    && c.geometry.photometric == 2
                    && c.geometry.spp == 3
                    && c.geometry.compression == 1
                    && matches!(c.geometry.bps, 8 | 16)
                    && !c.storage.strip_offsets.is_empty()
            })
            .max_by_key(|c| c.geometry.width.saturating_mul(c.geometry.height))
        {
            return decode_uncompressed_rgb_tiff(path, data, &ifd0, &rgb_ifd, endian);
        }
        return Err(DecodeError::UnsupportedFormat(
            "no CFA, LinearRaw, or uncompressed RGB IFD found in DNG/TIFF".to_owned(),
        ));
    };

    let cfa_pattern = raw_ifd
        .geometry
        .cfa_pattern
        .or(ifd0.geometry.cfa_pattern)
        .and_then(cfa_pattern_from_bytes)
        .ok_or_else(|| {
            DecodeError::UnsupportedFormat(
                "missing or unsupported 2x2 Bayer CFAPattern in DNG fallback".to_owned(),
            )
        })?;

    if raw_ifd.geometry.spp != 1 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "unsupported DNG fallback SamplesPerPixel={}",
            raw_ifd.geometry.spp
        )));
    }

    let (full_w, full_h, mut samples_u16) = match raw_ifd.geometry.compression {
        7 | 34892 => decode_dng_jpeg_cfa(data, &raw_ifd)?,
        9 => decode_dng_vc5_cfa(data, &ifd0, &raw_ifd, &cfa_pattern)?,
        1 => decode_dng_uncompressed_cfa(data, &raw_ifd, endian)?,
        comp => {
            return Err(DecodeError::UnsupportedFormat(format!(
                "unsupported DNG fallback compression={comp}"
            )));
        }
    };

    // Apply DNG LinearizationTable if present
    if let Some(lut) = raw_ifd
        .color
        .linearization_table
        .as_ref()
        .or(ifd0.color.linearization_table.as_ref())
    {
        if !lut.is_empty() {
            let max_idx = lut.len() - 1;
            for s in &mut samples_u16 {
                *s = lut[usize::from(*s).min(max_idx)];
            }
        }
    }

    // Apply ActiveArea crop aligned to Bayer 2x2 phase
    let (width, height, cropped_samples) =
        if let Some([top, left, bottom, right]) = raw_ifd.geometry.active_area {
            if bottom > top && right > left && bottom <= full_h && right <= full_w {
                let aligned = align_crop_to_cfa_phase(
                    &cfa_pattern,
                    SensorCropRect {
                        x: left,
                        y: top,
                        width: right - left,
                        height: bottom - top,
                    },
                    SensorDimensions {
                        width: full_w,
                        height: full_h,
                    },
                );
                if aligned.width > 0 && aligned.height > 0 {
                    let mut cropped = Vec::with_capacity(aligned.width * aligned.height);
                    for y in 0..aligned.height {
                        let start = (aligned.y + y) * full_w + aligned.x;
                        cropped.extend_from_slice(&samples_u16[start..start + aligned.width]);
                    }
                    (aligned.width, aligned.height, cropped)
                } else {
                    (full_w, full_h, samples_u16)
                }
            } else {
                (full_w, full_h, samples_u16)
            }
        } else {
            (full_w, full_h, samples_u16)
        };

    let black_levels = raw_ifd
        .color
        .black_level
        .or(ifd0.color.black_level)
        .unwrap_or([0.0; 4]);
    let default_white = if raw_ifd.color.linearization_table.is_some() {
        65535.0
    } else if raw_ifd.geometry.bps > 0 && raw_ifd.geometry.bps <= 16 {
        ((1_u32 << raw_ifd.geometry.bps) - 1) as f32
    } else {
        65535.0
    };
    let white_levels = raw_ifd
        .color
        .white_level
        .or(ifd0.color.white_level)
        .unwrap_or([default_white; 4]);

    if black_levels[0] >= white_levels[0] {
        return Err(DecodeError::MalformedMetadata(format!(
            "invalid DNG fallback black/white level range: black={}, white={}",
            black_levels[0], white_levels[0]
        )));
    }

    let color_matrix = if ifd0.color.calib_illuminant2 == Some(21) {
        ifd0.color
            .color_matrix2
            .or(raw_ifd.color.color_matrix2)
            .or(ifd0.color.color_matrix1)
            .or(raw_ifd.color.color_matrix1)
    } else {
        ifd0.color
            .color_matrix1
            .or(raw_ifd.color.color_matrix1)
            .or(ifd0.color.color_matrix2)
            .or(raw_ifd.color.color_matrix2)
    };

    let raw_wb =
        if let Some([nr, ng, nb]) = ifd0.color.as_shot_neutral.or(raw_ifd.color.as_shot_neutral) {
            [1.0 / nr, 1.0 / ng, 1.0 / nb, 1.0]
        } else {
            [f32::NAN; 4]
        };
    let wb_coeffs = sensor::sanitize_wb_coeffs_with_matrix(raw_wb, color_matrix.as_ref());

    let camera_make = ifd0
        .camera
        .make
        .clone()
        .or_else(|| raw_ifd.camera.make.clone())
        .unwrap_or_default();
    let camera_model = ifd0
        .camera
        .model
        .clone()
        .or_else(|| raw_ifd.camera.model.clone())
        .unwrap_or_else(|| "DNG".to_owned());

    let mut metadata = PhotoMetadata {
        camera_make,
        camera_model,
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: None,
        iso: None,
        width,
        height,
        black_levels,
        white_levels,
        cfa_pattern,
        noise_profile: rraw_core::NoiseProfile::new(
            [0.001; 3],
            [0.0001; 3],
            rraw_core::NoiseProfileSource::WaveletMadFallback,
        )?,
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        rating: 0,
        pick: PickStatus::Unflagged,
        label: ColorLabel::None,
        keywords: Vec::new(),
    };

    let mut orientation = ifd0
        .camera
        .orientation
        .or(raw_ifd.camera.orientation)
        .filter(|c| (1..=8).contains(c))
        .unwrap_or(1);

    let mut exif_tags = Vec::with_capacity(32);
    exif_tags.push(("Camera Make".to_owned(), metadata.camera_make.clone()));
    exif_tags.push(("Camera Model".to_owned(), metadata.camera_model.clone()));
    exif_tags.push((
        "Dimensions".to_owned(),
        format!(
            "{width} x {height} ({:.1} MP)",
            (width * height) as f32 / 1_000_000.0
        ),
    ));
    exif_tags.push((
        "Sensor Format".to_owned(),
        format!(
            "DNG Fallback (Compression={})",
            raw_ifd.geometry.compression
        ),
    ));

    exif::extract_dng_and_exif_metadata(path, &mut metadata, &mut orientation, &mut exif_tags);

    let cfa_signed = normalize_cfa_signed(&cropped_samples, width, height, &metadata)?;
    metadata.noise_profile = estimate_noise_profile_wavelet_mad(&cfa_signed)?;
    exif_tags.push((
        "Orientation".to_owned(),
        exif::orientation_label(orientation).to_owned(),
    ));

    Ok(DecodedRaw {
        metadata,
        cfa_signed,
        linear_rgb: None,
        wb_coeffs,
        color_matrix,
        exif_tags,
        orientation,
    })
}
