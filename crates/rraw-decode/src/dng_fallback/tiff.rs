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

/// Validates the TIFF byte-order mark and magic number, returning the detected
/// endianness and the byte offset of IFD0.
fn validate_tiff_header(data: &[u8]) -> Result<(Endian, usize), DecodeError> {
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
    Ok((endian, ifd0_off))
}

/// Applies an optional DNG LinearizationTable in-place and crops the pixel buffer
/// to the Bayer-phase-aligned ActiveArea.
fn apply_linearization_and_crop(
    samples_u16: &mut Vec<u16>,
    full_w: usize,
    full_h: usize,
    cfa_pattern: &CfaPattern,
    lin_table: Option<&[u16]>,
    active_area: Option<[usize; 4]>,
) -> (usize, usize, Vec<u16>) {
    if let Some(lut) = lin_table {
        if !lut.is_empty() {
            let max_idx = lut.len() - 1;
            for s in samples_u16.iter_mut() {
                *s = lut[usize::from(*s).min(max_idx)];
            }
        }
    }

    if let Some([top, left, bottom, right]) = active_area {
        if bottom > top && right > left && bottom <= full_h && right <= full_w {
            let aligned = align_crop_to_cfa_phase(
                cfa_pattern,
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
                return (aligned.width, aligned.height, cropped);
            }
        }
    }

    (full_w, full_h, std::mem::take(samples_u16))
}

/// Validated black/white sensor levels and sanitized white-balance coefficients.
struct DngLevelsAndWb {
    black_levels: [f32; 4],
    white_levels: [f32; 4],
    wb_coeffs: [f32; 4],
}

/// Computes validated black/white levels and sanitized white-balance coefficients.
fn compute_wb_and_levels(
    black_level: Option<[f32; 4]>,
    white_level: Option<[f32; 4]>,
    has_lin_table: bool,
    bps: u16,
    color_matrix: Option<&[f32; 9]>,
    as_shot_neutral: Option<[f32; 3]>,
) -> Result<DngLevelsAndWb, DecodeError> {
    let black_levels = black_level.unwrap_or([0.0; 4]);
    let default_white = if has_lin_table {
        65535.0
    } else if bps > 0 && bps <= 16 {
        ((1_u32 << bps) - 1) as f32
    } else {
        65535.0
    };
    let white_levels = white_level.unwrap_or([default_white; 4]);

    if black_levels[0] >= white_levels[0] {
        return Err(DecodeError::MalformedMetadata(format!(
            "invalid DNG fallback black/white level range: black={}, white={}",
            black_levels[0], white_levels[0]
        )));
    }

    let raw_wb = if let Some([nr, ng, nb]) = as_shot_neutral {
        [1.0 / nr, 1.0 / ng, 1.0 / nb, 1.0]
    } else {
        [f32::NAN; 4]
    };
    let wb_coeffs = sensor::sanitize_wb_coeffs_with_matrix(raw_wb, color_matrix);
    Ok(DngLevelsAndWb {
        black_levels,
        white_levels,
        wb_coeffs,
    })
}

pub(super) fn decode_dng_tiff(path: &Path, data: &[u8]) -> Result<DecodedRaw, DecodeError> {
    let (endian, ifd0_off) = validate_tiff_header(data)?;
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

    let lin_table = raw_ifd
        .color
        .linearization_table
        .as_deref()
        .or(ifd0.color.linearization_table.as_deref());
    let (width, height, cropped_samples) = apply_linearization_and_crop(
        &mut samples_u16,
        full_w,
        full_h,
        &cfa_pattern,
        lin_table,
        raw_ifd.geometry.active_area,
    );

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

    let DngLevelsAndWb {
        black_levels,
        white_levels,
        wb_coeffs,
    } = compute_wb_and_levels(
        raw_ifd.color.black_level.or(ifd0.color.black_level),
        raw_ifd.color.white_level.or(ifd0.color.white_level),
        raw_ifd.color.linearization_table.is_some(),
        raw_ifd.geometry.bps,
        color_matrix.as_ref(),
        ifd0.color.as_shot_neutral.or(raw_ifd.color.as_shot_neutral),
    )?;

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

    let orientation = ifd0
        .camera
        .orientation
        .or(raw_ifd.camera.orientation)
        .filter(|c| (1..=8).contains(c))
        .unwrap_or(1);

    assemble_dng_raw(
        path,
        &cropped_samples,
        width,
        height,
        cfa_pattern,
        [black_levels, white_levels, wb_coeffs],
        color_matrix,
        camera_make,
        camera_model,
        raw_ifd.geometry.compression,
        orientation,
    )
}

/// Assembles `PhotoMetadata`, normalizes CFA samples, estimates noise, and returns `DecodedRaw`.
#[allow(clippy::too_many_arguments)]
fn assemble_dng_raw(
    path: &Path,
    cropped_samples: &[u16],
    width: usize,
    height: usize,
    cfa_pattern: CfaPattern,
    levels_and_wb: [[f32; 4]; 3],
    color_matrix: Option<[f32; 9]>,
    camera_make: String,
    camera_model: String,
    compression: u16,
    mut orientation: u16,
) -> Result<DecodedRaw, DecodeError> {
    let [black_levels, white_levels, wb_coeffs] = levels_and_wb;
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
        format!("DNG Fallback (Compression={compression})"),
    ));

    exif::extract_dng_and_exif_metadata(path, &mut metadata, &mut orientation, &mut exif_tags);

    let cfa_signed = normalize_cfa_signed(cropped_samples, width, height, &metadata)?;
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
