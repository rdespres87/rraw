#![forbid(unsafe_code)]

//! Pure-safe-Rust fallback decoder for DNG/TIFF files and misnamed raster images
//! rejected by `rawler` (e.g., 12-bit Lossy CinemaDNG `SOF1` tiles, 8-bit Lossy DNG
//! compression `34892`, uncompressed DNGs with mismatched strip width, and JPEG/PNG
//! files saved with a `.dng` extension).

mod ifd;
mod payloads;
#[cfg(test)]
mod tests;
mod tiff;

use tiff::decode_dng_tiff;

use std::path::Path;

use rraw_core::{BayerOrigin, CfaPattern, ColorLabel, PhotoMetadata, PickStatus, SignedCfaBuffer};

use crate::special_sensors::XYZ_TO_SRGB_D65;
use crate::{exif, DecodeError, DecodedRaw, LinearRgbBuffer};

/// Attempts to decode a RAW file using the pure-Rust DNG/TIFF/raster fallback decoder.
pub fn try_decode_fallback(path: &Path) -> Result<DecodedRaw, DecodeError> {
    let data = std::fs::read(path).map_err(|e| {
        DecodeError::Io(format!(
            "failed to read RAW file {} for fallback decode: {e}",
            path.display()
        ))
    })?;

    try_decode_fallback_bytes(path, &data)
}

/// Attempts to decode an in-memory DNG/TIFF/raster byte slice using the pure-Rust fallback decoder.
pub fn try_decode_fallback_bytes(path: &Path, data: &[u8]) -> Result<DecodedRaw, DecodeError> {
    if is_jpeg_or_png_magic(data) {
        return decode_embedded_raster(path, data);
    }

    decode_dng_tiff(path, data)
}

#[inline]
fn is_jpeg_or_png_magic(data: &[u8]) -> bool {
    (data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF)
        || (data.len() >= 8 && data[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
}

#[inline]
fn srgb_u8_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn decode_embedded_raster(path: &Path, data: &[u8]) -> Result<DecodedRaw, DecodeError> {
    let img = image::load_from_memory(data).map_err(|e| {
        DecodeError::UnsupportedFormat(format!("embedded raster decode failed: {e}"))
    })?;
    let rgb = img.to_rgb8();
    let width = rgb.width() as usize;
    let height = rgb.height() as usize;
    let raw_bytes = rgb.as_raw();
    let mut linear = Vec::with_capacity(width * height);
    for px in raw_bytes.as_chunks::<3>().0 {
        linear.push([
            srgb_u8_to_linear(px[0]),
            srgb_u8_to_linear(px[1]),
            srgb_u8_to_linear(px[2]),
        ]);
    }
    let linear_rgb = LinearRgbBuffer::new(width, height, linear)?;

    let mut metadata = PhotoMetadata {
        camera_make: String::new(),
        camera_model: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Raster")
            .to_owned(),
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: None,
        iso: None,
        width,
        height,
        black_levels: [0.0; 4],
        white_levels: [1.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
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

    let mut orientation = 1_u16;
    let mut exif_tags = vec![
        (
            "Sensor Format".to_owned(),
            "Embedded Raster (Linear sRGB)".to_owned(),
        ),
        (
            "Dimensions".to_owned(),
            format!(
                "{width} x {height} ({:.1} MP)",
                (width * height) as f32 / 1_000_000.0
            ),
        ),
    ];
    exif::extract_dng_and_exif_metadata(path, &mut metadata, &mut orientation, &mut exif_tags);

    Ok(DecodedRaw {
        metadata,
        cfa_signed: SignedCfaBuffer::default(),
        linear_rgb: Some(linear_rgb),
        wb_coeffs: [1.0; 4],
        color_matrix: Some(XYZ_TO_SRGB_D65),
        exif_tags,
        orientation,
    })
}
