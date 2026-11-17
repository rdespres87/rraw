#![forbid(unsafe_code)]

//! Pure-Rust RAW decoding contracts, signed normalization, and DNG metadata extraction.

use std::fmt;
use std::path::Path;

use rraw_core::{CfaPattern, CoreError, PhotoMetadata, SignedCfaBuffer};

/// Typed errors produced by `rraw-decode`.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    Io(String),
    UnsupportedFormat(String),
    MalformedMetadata(String),
    Core(CoreError),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "RAW I/O error: {msg}"),
            Self::UnsupportedFormat(msg) => write!(f, "unsupported RAW format: {msg}"),
            Self::MalformedMetadata(msg) => write!(f, "malformed RAW metadata: {msg}"),
            Self::Core(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<CoreError> for DecodeError {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}

/// Output produced by a `RawDecoder` implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedRaw {
    pub metadata: PhotoMetadata,
    pub cfa_signed: SignedCfaBuffer,
}

/// Pure-Rust contract for read-only RAW file decoding.
pub trait RawDecoder: Send + Sync {
    /// Decodes a RAW file at `path` into canonical metadata and signed normalized CFA data.
    fn decode_file(&self, path: &Path) -> Result<DecodedRaw, DecodeError>;
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
    if raw_samples.len() != expected {
        return Err(CoreError::BufferLengthMismatch {
            expected,
            actual: raw_samples.len(),
        }
        .into());
    }

    let active_channels = match metadata.cfa_pattern {
        CfaPattern::Bayer(_) => 4,
        CfaPattern::XTrans(_) => 3,
    };
    for idx in 0..active_channels {
        let black = metadata.black_levels[idx];
        let white = metadata.white_levels[idx];
        let span = white - black;
        if !black.is_finite() || !white.is_finite() || !span.is_finite() || span <= 0.0 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid white-black levels at channel index {idx}: black={black}, white={white}"
            )));
        }
    }

    let mut out = Vec::with_capacity(expected);
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let level_idx = match metadata.cfa_pattern {
                CfaPattern::Bayer(_) => ((y % 2) * 2) + (x % 2),
                CfaPattern::XTrans(_) => metadata.cfa_pattern.color_at(x, y).channel_index(),
            };
            let black = metadata.black_levels[level_idx];
            let white = metadata.white_levels[level_idx];
            let span = white - black;
            let normalized = (f32::from(raw_samples[idx]) - black) / span;
            out.push(normalized);
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
