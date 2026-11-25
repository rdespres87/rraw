#![forbid(unsafe_code)]

//! Pure-Rust RAW decoding contracts, signed normalization, and DNG metadata extraction.

use std::path::Path;

use rraw_core::{CfaPattern, ColorLabel, CoreError, PhotoMetadata, PickStatus, SignedCfaBuffer};
use thiserror::Error;

pub mod cfa;
pub mod exif;
pub mod makernotes;
pub mod noise_profile;
pub mod opcodes;
pub mod sensor;
pub mod synthetic;

pub use cfa::*;
use noise_profile::estimate_noise_profile_wavelet_mad;

/// Typed errors produced by `rraw-decode`.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DecodeError {
    #[error("RAW I/O error: {0}")]
    Io(String),
    #[error("unsupported RAW format: {0}")]
    UnsupportedFormat(String),
    #[error("malformed RAW metadata: {0}")]
    MalformedMetadata(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}

/// Output produced by a `RawDecoder` implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedRaw {
    pub metadata: PhotoMetadata,
    pub cfa_signed: SignedCfaBuffer,
    pub wb_coeffs: [f32; 4],
    pub color_matrix: Option<[f32; 9]>,
    pub exif_tags: Vec<(String, String)>,
    /// Standard EXIF TIFF Orientation code (`1..=8`, where `6` = 90° CW Portrait, `8` = 270° CW Portrait).
    pub orientation: u16,
}

/// Pure-Rust contract for read-only RAW file decoding.
pub trait RawDecoder: Send + Sync {
    /// Decodes a RAW file at `path` into canonical metadata and signed normalized CFA data.
    fn decode_file(&self, path: &Path) -> Result<DecodedRaw, DecodeError>;
}

/// Concrete pure-safe-Rust RAW decoder wrapping `rawler` and DNG parsers.
#[derive(Debug, Default, Clone, Copy)]
pub struct PureRustRawDecoder;

impl PureRustRawDecoder {
    /// Creates a new pure-Rust decoder instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RawDecoder for PureRustRawDecoder {
    fn decode_file(&self, path: &Path) -> Result<DecodedRaw, DecodeError> {
        let raw_image = rawler::decode_file(path).map_err(|e| {
            DecodeError::Io(format!("failed to decode RAW file {}: {e}", path.display()))
        })?;

        let cfa_pattern =
            try_resolve_cfa_pattern_from_rawler(&raw_image.camera.cfa).ok_or_else(|| {
                DecodeError::UnsupportedFormat(format!(
                    "unsupported CFA pattern '{}' ({}x{})",
                    raw_image.camera.cfa.name,
                    raw_image.camera.cfa.width,
                    raw_image.camera.cfa.height
                ))
            })?;
        let (width, height, raw_samples) =
            sensor::extract_cropped_samples(&raw_image, &cfa_pattern)?;

        let black_levels = raw_image.blacklevel.as_bayer_array();
        let white_levels = raw_image.whitelevel.as_bayer_array();

        let mut metadata = PhotoMetadata {
            camera_make: raw_image.clean_make.clone(),
            camera_model: raw_image.clean_model.clone(),
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

        let wb_coeffs = sensor::sanitize_wb_coeffs(raw_image.wb_coeffs);
        let color_matrix = sensor::extract_color_matrix(&raw_image);

        let mut exif_tags = Vec::with_capacity(64);
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
            "As-Shot WB Multipliers".to_owned(),
            format!(
                "R={:.3}, G={:.3}, B={:.3}",
                wb_coeffs[0], wb_coeffs[1], wb_coeffs[2]
            ),
        ));

        let mut orientation = match raw_image.orientation.to_u16() {
            code @ 1..=8 => code,
            _ => 1,
        };

        exif::extract_dng_and_exif_metadata(path, &mut metadata, &mut orientation, &mut exif_tags);

        exif_tags.push((
            "CFA Pattern".to_owned(),
            format!("{:?}", metadata.cfa_pattern),
        ));
        exif_tags.push((
            "Sensor Black Level".to_owned(),
            format!(
                "{:.1}, {:.1}, {:.1}, {:.1}",
                black_levels[0], black_levels[1], black_levels[2], black_levels[3]
            ),
        ));
        exif_tags.push((
            "Sensor White Level".to_owned(),
            format!(
                "{:.1}, {:.1}, {:.1}, {:.1}",
                white_levels[0], white_levels[1], white_levels[2], white_levels[3]
            ),
        ));

        if black_levels[0] >= white_levels[0] {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid black/white level range: black={}, white={}",
                black_levels[0], white_levels[0]
            )));
        }

        let cfa_signed = match &raw_samples {
            sensor::RawSamples::U16(samples) => {
                normalize_cfa_signed(samples.as_ref(), width, height, &metadata)?
            }
            sensor::RawSamples::F32(samples) => {
                normalize_cfa_signed_f32(samples.as_ref(), width, height, &metadata)?
            }
        };
        metadata.noise_profile = estimate_noise_profile_wavelet_mad(&cfa_signed)?;
        exif_tags.push((
            "Orientation".to_owned(),
            exif::orientation_label(orientation).to_owned(),
        ));

        Ok(DecodedRaw {
            metadata,
            cfa_signed,
            wb_coeffs,
            color_matrix,
            exif_tags,
            orientation,
        })
    }
}

fn validate_and_compute_inv_spans(
    sample_len: usize,
    width: usize,
    height: usize,
    metadata: &PhotoMetadata,
) -> Result<(usize, [f32; 4]), DecodeError> {
    if width != metadata.width || height != metadata.height {
        return Err(DecodeError::MalformedMetadata(format!(
            "dimension mismatch: buffer {width}x{height} != metadata {}x{}",
            metadata.width, metadata.height
        )));
    }
    let Some(expected) = width.checked_mul(height) else {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    };
    if expected == 0 {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    }
    if sample_len != expected {
        return Err(CoreError::BufferLengthMismatch {
            expected,
            actual: sample_len,
        }
        .into());
    }

    let active_channels = match metadata.cfa_pattern {
        CfaPattern::Bayer(_) => 4,
        CfaPattern::XTrans(_) => 3,
    };
    let mut inv_spans = [0.0_f32; 4];
    for (idx, inv_span) in inv_spans.iter_mut().enumerate().take(active_channels) {
        let black = metadata.black_levels[idx];
        let white = metadata.white_levels[idx];
        let span = white - black;
        if !black.is_finite() || !white.is_finite() || !span.is_finite() || span <= 0.0 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid white-black levels at channel index {idx}: black={black}, white={white}"
            )));
        }
        *inv_span = 1.0 / span;
    }

    Ok((expected, inv_spans))
}

/// Performs Stage 1 signed black/white normalization `(raw - B_c) / (W_c - B_c)`
/// without clamping sub-black read-noise values to `0.0`.
///
/// Validates buffer vs `metadata` dimensions, rejects zero/overflow dimensions and
/// sample length mismatches, and verifies all active CFA channel spans upfront
/// (`0..4` for Bayer, `0..3` for X-Trans) before entering the pixel loop.
pub fn normalize_cfa_signed(
    raw_samples: &[u16],
    width: usize,
    height: usize,
    metadata: &PhotoMetadata,
) -> Result<SignedCfaBuffer, DecodeError> {
    let (expected, inv_spans) =
        validate_and_compute_inv_spans(raw_samples.len(), width, height, metadata)?;

    let mut out = Vec::with_capacity(expected);
    match metadata.cfa_pattern {
        CfaPattern::Bayer(_) => {
            let b0 = metadata.black_levels[0];
            let b1 = metadata.black_levels[1];
            let b2 = metadata.black_levels[2];
            let b3 = metadata.black_levels[3];
            let inv0 = inv_spans[0];
            let inv1 = inv_spans[1];
            let inv2 = inv_spans[2];
            let inv3 = inv_spans[3];

            for y in 0..height {
                let (black_even, inv_even, black_odd, inv_odd) = if y.is_multiple_of(2) {
                    (b0, inv0, b1, inv1)
                } else {
                    (b2, inv2, b3, inv3)
                };

                let row_start = y * width;
                let row_end = row_start + width;
                let row_samples = &raw_samples[row_start..row_end];

                let (chunks, remainder) = row_samples.as_chunks::<2>();
                for chunk in chunks {
                    out.push((f32::from(chunk[0]) - black_even) * inv_even);
                    out.push((f32::from(chunk[1]) - black_odd) * inv_odd);
                }
                if let Some(&last) = remainder.first() {
                    out.push((f32::from(last) - black_even) * inv_even);
                }
            }
        }
        CfaPattern::XTrans(_) => {
            for y in 0..height {
                let row_start = y * width;
                for x in 0..width {
                    let level_idx = metadata.cfa_pattern.color_at(x, y).channel_index();
                    let black = metadata.black_levels[level_idx];
                    let inv = inv_spans[level_idx];
                    out.push((f32::from(raw_samples[row_start + x]) - black) * inv);
                }
            }
        }
    }

    Ok(SignedCfaBuffer::new(
        width,
        height,
        metadata.cfa_pattern,
        out,
    )?)
}

/// Performs Stage 1 signed black/white normalization on floating-point RAW samples
/// (e.g., HDR DNG fp16/fp24/fp32), sanitizing non-finite inputs to `0.0` before scaling.
pub fn normalize_cfa_signed_f32(
    raw_samples: &[f32],
    width: usize,
    height: usize,
    metadata: &PhotoMetadata,
) -> Result<SignedCfaBuffer, DecodeError> {
    let (expected, inv_spans) =
        validate_and_compute_inv_spans(raw_samples.len(), width, height, metadata)?;

    let sanitize = |v: f32| if v.is_finite() { v } else { 0.0 };
    let mut out = Vec::with_capacity(expected);
    match metadata.cfa_pattern {
        CfaPattern::Bayer(_) => {
            let b0 = metadata.black_levels[0];
            let b1 = metadata.black_levels[1];
            let b2 = metadata.black_levels[2];
            let b3 = metadata.black_levels[3];
            let inv0 = inv_spans[0];
            let inv1 = inv_spans[1];
            let inv2 = inv_spans[2];
            let inv3 = inv_spans[3];

            for y in 0..height {
                let (black_even, inv_even, black_odd, inv_odd) = if y.is_multiple_of(2) {
                    (b0, inv0, b1, inv1)
                } else {
                    (b2, inv2, b3, inv3)
                };

                let row_start = y * width;
                let row_end = row_start + width;
                let row_samples = &raw_samples[row_start..row_end];

                let (chunks, remainder) = row_samples.as_chunks::<2>();
                for chunk in chunks {
                    out.push((sanitize(chunk[0]) - black_even) * inv_even);
                    out.push((sanitize(chunk[1]) - black_odd) * inv_odd);
                }
                if let Some(&last) = remainder.first() {
                    out.push((sanitize(last) - black_even) * inv_even);
                }
            }
        }
        CfaPattern::XTrans(_) => {
            for y in 0..height {
                let row_start = y * width;
                for x in 0..width {
                    let level_idx = metadata.cfa_pattern.color_at(x, y).channel_index();
                    let black = metadata.black_levels[level_idx];
                    let inv = inv_spans[level_idx];
                    out.push((sanitize(raw_samples[row_start + x]) - black) * inv);
                }
            }
        }
    }

    Ok(SignedCfaBuffer::new(
        width,
        height,
        metadata.cfa_pattern,
        out,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_core::{
        BayerOrigin, CfaPattern, ColorLabel, NoiseProfile, NoiseProfileSource, PickStatus,
    };

    fn synthetic_metadata(width: usize, height: usize) -> Result<PhotoMetadata, CoreError> {
        Ok(PhotoMetadata {
            camera_make: "Synthetic".to_owned(),
            camera_model: "TestCam".to_owned(),
            lens_model: None,
            focal_length_mm: None,
            aperture_f_number: None,
            iso: Some(100),
            width,
            height,
            black_levels: [512.0; 4],
            white_levels: [16383.0; 4],
            cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
            noise_profile: NoiseProfile::new(
                [0.001; 3],
                [0.0001; 3],
                NoiseProfileSource::DngTag0xc761,
            )?,
            vignette_handled_by_gainmap: false,
            dng_warp_rectilinear: None,
            rating: 0,
            pick: PickStatus::Unflagged,
            label: ColorLabel::None,
            keywords: Vec::new(),
        })
    }

    #[test]
    fn signed_normalization_preserves_sub_black_samples() -> Result<(), DecodeError> {
        let meta = synthetic_metadata(2, 2)?;
        // Value 500 is below black level 512 and must remain strictly negative.
        let raw = [500_u16, 512_u16, 524_u16, 16383_u16];
        let signed = normalize_cfa_signed(&raw, 2, 2, &meta)?;
        assert!(signed.samples()[0] < 0.0);
        assert!((signed.samples()[1] - 0.0).abs() < 1e-7);
        assert!(signed.samples()[2] > 0.0);
        assert!((signed.samples()[3] - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn signed_normalization_handles_xtrans_and_rejects_degenerate_span() -> Result<(), DecodeError>
    {
        let mut meta = synthetic_metadata(6, 1)?;
        meta.cfa_pattern = CfaPattern::fujifilm_xtrans();
        meta.black_levels = [100.0, 200.0, 300.0, 0.0];
        meta.white_levels = [1100.0, 1200.0, 1300.0, 0.0];
        // Row 0 of Fujifilm X-Trans: [G, B, G, G, R, G]
        let raw = [200_u16, 250_u16, 300_u16, 200_u16, 100_u16, 200_u16];
        let signed = normalize_cfa_signed(&raw, 6, 1, &meta)?;
        assert!((signed.samples()[0] - 0.0).abs() < 1e-6); // G at black=200
        assert!(signed.samples()[1] < 0.0); // B=250 < black=300
        assert!((signed.samples()[4] - 0.0).abs() < 1e-6); // R at black=100

        meta.white_levels[0] = meta.black_levels[0];
        assert!(matches!(
            normalize_cfa_signed(&raw, 6, 1, &meta),
            Err(DecodeError::MalformedMetadata(_))
        ));
        Ok(())
    }

    #[test]
    fn signed_normalization_validates_dimensions_length_and_unvisited_channels(
    ) -> Result<(), DecodeError> {
        let meta_2x2 = synthetic_metadata(2, 2)?;

        // Buffer length mismatch
        assert_eq!(
            normalize_cfa_signed(&[512_u16; 3], 2, 2, &meta_2x2),
            Err(DecodeError::Core(CoreError::BufferLengthMismatch {
                expected: 4,
                actual: 3
            }))
        );

        // Dimension mismatch against metadata
        assert!(matches!(
            normalize_cfa_signed(&[512_u16; 4], 4, 1, &meta_2x2),
            Err(DecodeError::MalformedMetadata(_))
        ));

        // Zero dimensions
        let meta_zero = synthetic_metadata(0, 2)?;
        assert_eq!(
            normalize_cfa_signed(&[], 0, 2, &meta_zero),
            Err(DecodeError::Core(CoreError::InvalidDimensions {
                width: 0,
                height: 2
            }))
        );

        // Multiplication overflow
        let meta_overflow = synthetic_metadata(usize::MAX, 2)?;
        assert_eq!(
            normalize_cfa_signed(&[], usize::MAX, 2, &meta_overflow),
            Err(DecodeError::Core(CoreError::InvalidDimensions {
                width: usize::MAX,
                height: 2
            }))
        );

        // 1x1 Bayer image only visits channel 0, yet invalid channel 3 must be rejected upfront
        let mut meta_1x1 = synthetic_metadata(1, 1)?;
        meta_1x1.white_levels[3] = meta_1x1.black_levels[3];
        assert!(matches!(
            normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1),
            Err(DecodeError::MalformedMetadata(_))
        ));

        // 1x1 Bayer image with NaN in unvisited channel 2 must also be rejected upfront
        let mut meta_1x1_nan = synthetic_metadata(1, 1)?;
        meta_1x1_nan.black_levels[2] = f32::NAN;
        assert!(matches!(
            normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1_nan),
            Err(DecodeError::MalformedMetadata(_))
        ));

        // 1x1 X-Trans image visits only Green, yet invalid channel 2 (Blue) must be rejected
        let mut meta_1x1_xtrans = synthetic_metadata(1, 1)?;
        meta_1x1_xtrans.cfa_pattern = CfaPattern::fujifilm_xtrans();
        meta_1x1_xtrans.white_levels[2] = meta_1x1_xtrans.black_levels[2];
        assert!(matches!(
            normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1_xtrans),
            Err(DecodeError::MalformedMetadata(_))
        ));

        Ok(())
    }
}
