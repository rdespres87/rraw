#![forbid(unsafe_code)]

//! Pure-Rust RAW decoding contracts, signed normalization, and DNG metadata extraction.

use std::path::Path;

use rraw_core::{CfaPattern, ColorLabel, CoreError, PhotoMetadata, PickStatus, SignedCfaBuffer};
use thiserror::Error;

pub mod camera_aliases;
pub mod cfa;
pub mod dng_fallback;
pub mod exif;
pub mod jpeg12;
pub mod makernotes;
pub mod naked_fallback;
pub mod noise_profile;
pub mod opcodes;
pub mod sensor;
pub mod special_sensors;
pub mod synthetic;
pub mod vc5;
mod vc5_table17;
pub mod x3f;

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

/// Pre-demosaiced signed normalized linear camera-RGB buffer (`cpp == 3`,
/// e.g., Apple ProRAW, Canon sRAW, Adobe Linear DNG, Sony Pixel-Shift ARW).
#[derive(Debug, Clone, PartialEq)]
pub struct LinearRgbBuffer {
    width: usize,
    height: usize,
    samples: std::sync::Arc<[[f32; 3]]>,
}

impl LinearRgbBuffer {
    /// Creates a validated linear RGB buffer with `width * height` pixels.
    pub fn new(width: usize, height: usize, samples: Vec<[f32; 3]>) -> Result<Self, DecodeError> {
        let Some(expected) = width.checked_mul(height) else {
            return Err(CoreError::InvalidDimensions { width, height }.into());
        };
        if expected == 0 {
            return Err(CoreError::InvalidDimensions { width, height }.into());
        }
        if samples.len() != expected {
            return Err(CoreError::BufferLengthMismatch {
                expected,
                actual: samples.len(),
            }
            .into());
        }
        Ok(Self {
            width,
            height,
            samples: std::sync::Arc::from(samples),
        })
    }

    /// Image width in pixels.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Image height in pixels.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Interleaved linear camera-RGB pixel slice (`width * height`).
    #[must_use]
    pub fn samples(&self) -> &[[f32; 3]] {
        &self.samples
    }

    /// Shared zero-copy reference to the linear camera-RGB pixel buffer.
    #[must_use]
    pub fn samples_arc(&self) -> std::sync::Arc<[[f32; 3]]> {
        std::sync::Arc::clone(&self.samples)
    }
}

/// Output produced by a `RawDecoder` implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedRaw {
    pub metadata: PhotoMetadata,
    pub cfa_signed: SignedCfaBuffer,
    /// Present for pre-demosaiced 3-channel linear RGB RAWs (`cpp == 3`).
    /// When `Some`, `cfa_signed` is empty (`SignedCfaBuffer::default()`).
    pub linear_rgb: Option<LinearRgbBuffer>,
    pub wb_coeffs: [f32; 4],
    pub color_matrix: Option<[f32; 9]>,
    pub exif_tags: Vec<(String, String)>,
    /// Standard EXIF TIFF Orientation code (`1..=8`, where `6` = 90° CW Portrait, `8` = 270° CW Portrait).
    pub orientation: u16,
}

impl DecodedRaw {
    /// Returns `true` when this decoded RAW contains pre-demosaiced linear RGB (`cpp == 3`).
    #[must_use]
    pub const fn is_linear_rgb(&self) -> bool {
        self.linear_rgb.is_some()
    }
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
        let (raw_image, original_identity) = match rawler::decode_file(path) {
            Ok(img) => (img, None),
            Err(primary_err) => {
                if let Some(aliased) = camera_aliases::try_decode_with_alias(path) {
                    (
                        aliased.raw_image,
                        Some((aliased.original_make, aliased.original_model)),
                    )
                } else if let Ok(fallback) = dng_fallback::try_decode_fallback(path) {
                    return Ok(fallback);
                } else if let Ok(x3f_raw) = x3f::try_decode_x3f(path) {
                    return Ok(x3f_raw);
                } else if let Ok(pro70) = special_sensors::try_decode_canon_pro70_ciff(path) {
                    return Ok(pro70);
                } else if let Ok(naked) = naked_fallback::try_decode_naked(path) {
                    return Ok(naked);
                } else {
                    return Err(DecodeError::Io(format!(
                        "failed to decode RAW file {}: {primary_err}",
                        path.display()
                    )));
                }
            }
        };

        let mut decoded = match raw_image.cpp {
            1 => decode_single_plane_cfa(path, &raw_image),
            3 => decode_linear_rgb(path, &raw_image),
            cpp => Err(DecodeError::UnsupportedFormat(format!(
                "unsupported component count cpp={cpp} (expected 1 for CFA or 3 for Linear RGB)"
            ))),
        }?;

        if let Some((orig_make, orig_model)) = original_identity {
            if !orig_make.is_empty() {
                decoded.metadata.camera_make = orig_make.clone();
                for (k, v) in &mut decoded.exif_tags {
                    if k == "Camera Make" {
                        *v = orig_make.clone();
                    }
                }
            }
            if !orig_model.is_empty() {
                decoded.metadata.camera_model = orig_model.clone();
                for (k, v) in &mut decoded.exif_tags {
                    if k == "Camera Model" {
                        *v = orig_model.clone();
                    }
                }
            }
        }

        Ok(decoded)
    }
}

fn decode_single_plane_cfa(
    path: &Path,
    raw_image: &rawler::RawImage,
) -> Result<DecodedRaw, DecodeError> {
    let raw_cfa = match &raw_image.photometric {
        rawler::rawimage::RawPhotometricInterpretation::Cfa(cfg) => &cfg.cfa,
        _ => &raw_image.camera.cfa,
    };

    let resolved_cfa = try_resolve_cfa_pattern_from_rawler(raw_cfa)
        .or_else(|| try_resolve_cfa_pattern_from_rawler(&raw_image.camera.cfa));

    // Fujifilm SuperCCD 45° octagonal sensors: demosaic + rotate upright into LinearRgbBuffer
    if let (Some(cfa_pattern), Some(fuji_rot_width)) =
        (resolved_cfa.as_ref(), raw_image.fuji_rotation_width)
    {
        let linear_rgb = special_sensors::demosaic_and_rotate_fuji_superccd(
            raw_image,
            cfa_pattern,
            fuji_rot_width,
        )?;
        let color_matrix = sensor::extract_color_matrix(raw_image);
        let wb_coeffs =
            sensor::sanitize_wb_coeffs_with_matrix(raw_image.wb_coeffs, color_matrix.as_ref());
        return build_special_linear_rgb_decoded_raw(
            path,
            raw_image,
            linear_rgb,
            wb_coeffs,
            color_matrix,
            "SuperCCD 45-deg Linear RGB",
        );
    }

    // 4-color 2x2 CFA sensors (e.g., CYGM / GMYC / RGBE): demosaic + calibrate to linear sRGB
    if resolved_cfa.is_none() && special_sensors::is_four_color_2x2_cfa(raw_cfa) {
        let linear_rgb =
            special_sensors::demosaic_four_color_cfa_to_linear_srgb(raw_image, raw_cfa)?;
        return build_special_linear_rgb_decoded_raw(
            path,
            raw_image,
            linear_rgb,
            [1.0; 4],
            Some(special_sensors::XYZ_TO_SRGB_D65),
            &format!("4-Color {} (Linear sRGB)", raw_cfa.name),
        );
    }

    // True monochrome / CFA-less sensors (e.g., Leica M Monochrom, Pentax K-3 III Monochrome)
    if resolved_cfa.is_none()
        && (raw_cfa.width == 0
            || raw_cfa.height == 0
            || matches!(
                raw_image.photometric,
                rawler::rawimage::RawPhotometricInterpretation::BlackIsZero
            ))
    {
        let linear_rgb = special_sensors::decode_monochrome_sensor_to_linear_rgb(raw_image)?;
        return build_special_linear_rgb_decoded_raw(
            path,
            raw_image,
            linear_rgb,
            [1.0; 4],
            Some(special_sensors::XYZ_TO_SRGB_D65),
            "Monochrome Sensor (Linear RGB)",
        );
    }

    let cfa_pattern = resolved_cfa.ok_or_else(|| {
        DecodeError::UnsupportedFormat(format!(
            "unsupported CFA pattern '{}' ({}x{})",
            raw_cfa.name, raw_cfa.width, raw_cfa.height
        ))
    })?;
    let (width, height, raw_samples) = sensor::extract_cropped_samples(raw_image, &cfa_pattern)?;

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

    let color_matrix = sensor::extract_color_matrix(raw_image);
    let wb_coeffs =
        sensor::sanitize_wb_coeffs_with_matrix(raw_image.wb_coeffs, color_matrix.as_ref());

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

    let mut cfa_signed = match &raw_samples {
        sensor::RawSamples::U16(samples) => {
            normalize_cfa_signed(samples.as_ref(), width, height, &metadata)?
        }
        sensor::RawSamples::F32(samples) => {
            normalize_cfa_signed_f32(samples.as_ref(), width, height, &metadata)?
        }
    };
    if matches!(metadata.cfa_pattern, CfaPattern::Bayer(_)) {
        let fixes =
            special_sensors::detect_isolated_bayer_hot_pixels(cfa_signed.samples(), width, height);
        if !fixes.is_empty() {
            let mut samples = cfa_signed.samples().to_vec();
            for (idx, val) in fixes {
                samples[idx] = val;
            }
            cfa_signed = SignedCfaBuffer::new(width, height, metadata.cfa_pattern, samples)?;
        }
    }
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

fn decode_linear_rgb(path: &Path, raw_image: &rawler::RawImage) -> Result<DecodedRaw, DecodeError> {
    let (width, height, raw_samples) = sensor::extract_cropped_rgb_samples(raw_image)?;
    let (black_rgb, white_rgb) = sensor::extract_rgb_black_white_levels(raw_image);

    let black_levels = [black_rgb[0], black_rgb[1], black_rgb[2], black_rgb[1]];
    let white_levels = [white_rgb[0], white_rgb[1], white_rgb[2], white_rgb[1]];

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
        cfa_pattern: CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
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

    let color_matrix = sensor::extract_color_matrix(raw_image);
    let wb_coeffs =
        sensor::sanitize_wb_coeffs_with_matrix(raw_image.wb_coeffs, color_matrix.as_ref());

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

    exif_tags.push(("Sensor Format".to_owned(), "Linear RGB (cpp=3)".to_owned()));
    exif_tags.push((
        "Sensor Black Level".to_owned(),
        format!(
            "{:.1}, {:.1}, {:.1}",
            black_rgb[0], black_rgb[1], black_rgb[2]
        ),
    ));
    exif_tags.push((
        "Sensor White Level".to_owned(),
        format!(
            "{:.1}, {:.1}, {:.1}",
            white_rgb[0], white_rgb[1], white_rgb[2]
        ),
    ));

    let linear_rgb = match &raw_samples {
        sensor::RawSamples::U16(samples) => {
            normalize_linear_rgb_signed(samples.as_ref(), width, height, black_rgb, white_rgb)?
        }
        sensor::RawSamples::F32(samples) => {
            normalize_linear_rgb_signed_f32(samples.as_ref(), width, height, black_rgb, white_rgb)?
        }
    };

    exif_tags.push((
        "Orientation".to_owned(),
        exif::orientation_label(orientation).to_owned(),
    ));

    Ok(DecodedRaw {
        metadata,
        cfa_signed: SignedCfaBuffer::default(),
        linear_rgb: Some(linear_rgb),
        wb_coeffs,
        color_matrix,
        exif_tags,
        orientation,
    })
}

fn build_special_linear_rgb_decoded_raw(
    path: &Path,
    raw_image: &rawler::RawImage,
    linear_rgb: LinearRgbBuffer,
    wb_coeffs: [f32; 4],
    color_matrix: Option<[f32; 9]>,
    sensor_format_label: &str,
) -> Result<DecodedRaw, DecodeError> {
    let width = linear_rgb.width();
    let height = linear_rgb.height();
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
        cfa_pattern: CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
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

    exif_tags.push(("Sensor Format".to_owned(), sensor_format_label.to_owned()));
    exif_tags.push((
        "Orientation".to_owned(),
        exif::orientation_label(orientation).to_owned(),
    ));

    Ok(DecodedRaw {
        metadata,
        cfa_signed: SignedCfaBuffer::default(),
        linear_rgb: Some(linear_rgb),
        wb_coeffs,
        color_matrix,
        exif_tags,
        orientation,
    })
}

fn validate_rgb_and_compute_inv_spans(
    sample_len: usize,
    width: usize,
    height: usize,
    black_levels: [f32; 3],
    white_levels: [f32; 3],
) -> Result<(usize, [f32; 3]), DecodeError> {
    let Some(expected_pixels) = width.checked_mul(height) else {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    };
    if expected_pixels == 0 {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    }
    let Some(expected_samples) = expected_pixels.checked_mul(3) else {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    };
    if sample_len != expected_samples {
        return Err(CoreError::BufferLengthMismatch {
            expected: expected_samples,
            actual: sample_len,
        }
        .into());
    }

    let mut inv_spans = [0.0_f32; 3];
    for idx in 0..3 {
        let black = black_levels[idx];
        let white = white_levels[idx];
        let span = white - black;
        if !black.is_finite() || !white.is_finite() || !span.is_finite() || span <= 0.0 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid linear RGB white-black levels at channel {idx}: black={black}, white={white}"
            )));
        }
        inv_spans[idx] = 1.0 / span;
    }

    Ok((expected_pixels, inv_spans))
}

/// Performs signed black/white normalization on interleaved 16-bit linear camera-RGB samples (`cpp == 3`).
pub fn normalize_linear_rgb_signed(
    raw_samples: &[u16],
    width: usize,
    height: usize,
    black_levels: [f32; 3],
    white_levels: [f32; 3],
) -> Result<LinearRgbBuffer, DecodeError> {
    let (expected_pixels, inv_spans) = validate_rgb_and_compute_inv_spans(
        raw_samples.len(),
        width,
        height,
        black_levels,
        white_levels,
    )?;

    let mut out = Vec::with_capacity(expected_pixels);
    for px in raw_samples.as_chunks::<3>().0 {
        out.push([
            (f32::from(px[0]) - black_levels[0]) * inv_spans[0],
            (f32::from(px[1]) - black_levels[1]) * inv_spans[1],
            (f32::from(px[2]) - black_levels[2]) * inv_spans[2],
        ]);
    }

    LinearRgbBuffer::new(width, height, out)
}

/// Performs signed black/white normalization on interleaved floating-point linear camera-RGB samples
/// (`cpp == 3`), sanitizing non-finite inputs to `0.0`.
pub fn normalize_linear_rgb_signed_f32(
    raw_samples: &[f32],
    width: usize,
    height: usize,
    black_levels: [f32; 3],
    white_levels: [f32; 3],
) -> Result<LinearRgbBuffer, DecodeError> {
    let (expected_pixels, inv_spans) = validate_rgb_and_compute_inv_spans(
        raw_samples.len(),
        width,
        height,
        black_levels,
        white_levels,
    )?;

    let sanitize = |v: f32| if v.is_finite() { v } else { 0.0 };
    let mut out = Vec::with_capacity(expected_pixels);
    for px in raw_samples.as_chunks::<3>().0 {
        out.push([
            (sanitize(px[0]) - black_levels[0]) * inv_spans[0],
            (sanitize(px[1]) - black_levels[1]) * inv_spans[1],
            (sanitize(px[2]) - black_levels[2]) * inv_spans[2],
        ]);
    }

    LinearRgbBuffer::new(width, height, out)
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
#[path = "lib_tests.rs"]
mod tests;
