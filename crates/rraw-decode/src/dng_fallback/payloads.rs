#![forbid(unsafe_code)]

//! Compression-specific DNG/TIFF pixel payload decoders (VC-5, JPEG, uncompressed CFA/RGB).

use std::path::Path;

use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, CoreError, PhotoMetadata, PickStatus, SignedCfaBuffer,
};

use super::ifd::{Endian, ParsedIfd};
use super::srgb_u8_to_linear;
use crate::special_sensors::XYZ_TO_SRGB_D65;
use crate::{exif, jpeg12, DecodeError, DecodedRaw, LinearRgbBuffer};

pub(super) fn decode_dng_vc5_cfa(
    data: &[u8],
    ifd0: &ParsedIfd,
    ifd: &ParsedIfd,
    cfa_pattern: &CfaPattern,
) -> Result<(usize, usize, Vec<u16>), DecodeError> {
    let bayer_origin = match cfa_pattern {
        CfaPattern::Bayer(origin) => *origin,
        CfaPattern::XTrans(_) => {
            return Err(DecodeError::UnsupportedFormat(
                "VC-5 DNG fallback only supports 2x2 Bayer CFA".to_owned(),
            ));
        }
    };
    let width = ifd.geometry.width;
    let height = ifd.geometry.height;
    let (off, len) = if let (Some(&o), Some(&c)) = (
        ifd.storage.tile_offsets.first(),
        ifd.storage.tile_byte_counts.first(),
    ) {
        (o, c)
    } else if let (Some(&o), Some(&c)) = (
        ifd.storage.strip_offsets.first(),
        ifd.storage.strip_byte_counts.first(),
    ) {
        (o, c)
    } else {
        return Err(DecodeError::MalformedMetadata(
            "VC-5 DNG missing tile/strip offsets".to_owned(),
        ));
    };
    let end = off
        .checked_add(len)
        .ok_or_else(|| DecodeError::MalformedMetadata("VC-5 DNG offset overflow".to_owned()))?;
    let payload = data.get(off..end).ok_or_else(|| {
        DecodeError::MalformedMetadata("VC-5 DNG tile/strip extends past EOF".to_owned())
    })?;

    let wl = ifd
        .color
        .white_level
        .or(ifd0.color.white_level)
        .map_or(16383.0, |w| w[0]);
    let output_bit_depth = if wl <= 4096.0 {
        12
    } else if wl <= 16384.0 {
        14
    } else {
        16
    };

    let samples =
        crate::vc5::decode_vc5_bayer(payload, width, height, bayer_origin, output_bit_depth)?;
    Ok((width, height, samples))
}

pub(super) fn decode_dng_jpeg_cfa(
    data: &[u8],
    ifd: &ParsedIfd,
) -> Result<(usize, usize, Vec<u16>), DecodeError> {
    let width = ifd.geometry.width;
    let height = ifd.geometry.height;
    let Some(total) = width.checked_mul(height) else {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    };
    let mut out = vec![0_u16; total];

    if !ifd.storage.tile_offsets.is_empty()
        && ifd.storage.tile_width > 0
        && ifd.storage.tile_length > 0
    {
        let tiles_x = width.div_ceil(ifd.storage.tile_width);
        let tiles_y = height.div_ceil(ifd.storage.tile_length);
        let expected_tiles = tiles_x.saturating_mul(tiles_y);
        if ifd.storage.tile_offsets.len() < expected_tiles
            || ifd.storage.tile_byte_counts.len() < expected_tiles
        {
            return Err(DecodeError::MalformedMetadata(
                "DNG tile table shorter than expected tile count".to_owned(),
            ));
        }

        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let idx = ty * tiles_x + tx;
                let off = ifd.storage.tile_offsets[idx];
                let len = ifd.storage.tile_byte_counts[idx];
                let chunk = data.get(off..off.saturating_add(len)).ok_or_else(|| {
                    DecodeError::MalformedMetadata("DNG tile byte range out of bounds".to_owned())
                })?;

                let decoded = jpeg12::decode_jpeg_gray(chunk)?;
                let tile_w = ifd.storage.tile_width;
                let tile_h = ifd.storage.tile_length;

                let copy_w = (width.saturating_sub(tx * tile_w)).min(tile_w);
                let copy_h = (height.saturating_sub(ty * tile_h)).min(tile_h);

                if decoded.width == tile_w && decoded.height == tile_h {
                    for row in 0..copy_h {
                        let dst_off = (ty * tile_h + row) * width + tx * tile_w;
                        let src_off = row * tile_w;
                        out[dst_off..dst_off + copy_w]
                            .copy_from_slice(&decoded.samples[src_off..src_off + copy_w]);
                    }
                } else if decoded.width.is_multiple_of(2)
                    && decoded.width / 2 == tile_w
                    && decoded.height.checked_mul(2) == Some(tile_h)
                {
                    // CinemaDNG 12-bit lossy packing: pairs of CFA rows stored side-by-side
                    // in a (2 * tile_w) x (tile_h / 2) grayscale JPEG tile.
                    for row in 0..copy_h {
                        let dst_off = (ty * tile_h + row) * width + tx * tile_w;
                        let src_off = row * tile_w;
                        if src_off + copy_w <= decoded.samples.len() {
                            out[dst_off..dst_off + copy_w]
                                .copy_from_slice(&decoded.samples[src_off..src_off + copy_w]);
                        }
                    }
                } else {
                    return Err(DecodeError::UnsupportedFormat(format!(
                        "unexpected JPEG tile dimensions {}x{} for tile {}x{}",
                        decoded.width, decoded.height, tile_w, tile_h
                    )));
                }
            }
        }
        return Ok((width, height, out));
    }

    if !ifd.storage.strip_offsets.is_empty() {
        let rows_per_strip = if ifd.storage.rows_per_strip > 0 {
            ifd.storage.rows_per_strip
        } else {
            height
        };
        let mut cur_y = 0_usize;
        for (&off, &len) in ifd
            .storage
            .strip_offsets
            .iter()
            .zip(ifd.storage.strip_byte_counts.iter())
        {
            if cur_y >= height {
                break;
            }
            let chunk = data.get(off..off.saturating_add(len)).ok_or_else(|| {
                DecodeError::MalformedMetadata("DNG strip byte range out of bounds".to_owned())
            })?;
            let decoded = jpeg12::decode_jpeg_gray(chunk)?;
            let strip_h = rows_per_strip.min(height - cur_y);
            if decoded.samples.len() >= width * strip_h {
                for row in 0..strip_h {
                    let dst_off = (cur_y + row) * width;
                    let src_off = row * width;
                    out[dst_off..dst_off + width]
                        .copy_from_slice(&decoded.samples[src_off..src_off + width]);
                }
            }
            cur_y += strip_h;
        }
        return Ok((width, height, out));
    }

    Err(DecodeError::MalformedMetadata(
        "DNG JPEG IFD has neither TileOffsets nor StripOffsets".to_owned(),
    ))
}

pub(super) fn decode_dng_uncompressed_cfa(
    data: &[u8],
    ifd: &ParsedIfd,
    endian: Endian,
) -> Result<(usize, usize, Vec<u16>), DecodeError> {
    if ifd.geometry.bps != 16 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "unsupported uncompressed DNG fallback bps={}",
            ifd.geometry.bps
        )));
    }
    if ifd.storage.strip_offsets.is_empty() || ifd.storage.strip_byte_counts.is_empty() {
        return Err(DecodeError::MalformedMetadata(
            "uncompressed DNG IFD missing StripOffsets/StripByteCounts".to_owned(),
        ));
    }

    let total_strip_bytes = ifd
        .storage
        .strip_byte_counts
        .iter()
        .copied()
        .try_fold(0_usize, usize::checked_add)
        .ok_or_else(|| {
            DecodeError::MalformedMetadata("uncompressed DNG strip byte count overflow".to_owned())
        })?;
    let expected_bytes = ifd
        .geometry
        .width
        .checked_mul(ifd.geometry.height)
        .and_then(|n| n.checked_mul(2))
        .ok_or(CoreError::InvalidDimensions {
            width: ifd.geometry.width,
            height: ifd.geometry.height,
        })?;

    // Handle firmware quirks where StripByteCounts encodes the true even active row width
    // (smaller than the declared TIFF ImageWidth, e.g., 5280 vs 5376 on 20MP sensors).
    let effective_width = if total_strip_bytes >= expected_bytes {
        ifd.geometry.width
    } else if ifd.geometry.height > 0 && total_strip_bytes.is_multiple_of(ifd.geometry.height * 2) {
        let inferred = total_strip_bytes / (ifd.geometry.height * 2);
        if inferred >= 2 && inferred.is_multiple_of(2) {
            inferred
        } else {
            return Err(DecodeError::MalformedMetadata(format!(
                "uncompressed DNG strip byte count {total_strip_bytes} does not match {}x{}",
                ifd.geometry.width, ifd.geometry.height
            )));
        }
    } else {
        return Err(DecodeError::MalformedMetadata(format!(
            "uncompressed DNG strip byte count {total_strip_bytes} shorter than expected {expected_bytes}"
        )));
    };

    let target_pixels = effective_width * ifd.geometry.height;
    let mut out = Vec::with_capacity(target_pixels);
    for (&off, &len) in ifd
        .storage
        .strip_offsets
        .iter()
        .zip(ifd.storage.strip_byte_counts.iter())
    {
        let slice = data.get(off..off.saturating_add(len)).ok_or_else(|| {
            DecodeError::MalformedMetadata("uncompressed DNG strip range out of bounds".to_owned())
        })?;
        for pair in slice.as_chunks::<2>().0 {
            if out.len() == target_pixels {
                break;
            }
            let sample = match endian {
                Endian::Little => u16::from_le_bytes(*pair),
                Endian::Big => u16::from_be_bytes(*pair),
            };
            out.push(sample);
        }
    }

    if out.len() != target_pixels {
        return Err(CoreError::BufferLengthMismatch {
            expected: target_pixels,
            actual: out.len(),
        }
        .into());
    }

    Ok((effective_width, ifd.geometry.height, out))
}

pub(super) fn decode_uncompressed_rgb_tiff(
    path: &Path,
    data: &[u8],
    ifd0: &ParsedIfd,
    rgb_ifd: &ParsedIfd,
    endian: Endian,
) -> Result<DecodedRaw, DecodeError> {
    let width = rgb_ifd.geometry.width;
    let height = rgb_ifd.geometry.height;
    let pixel_count = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    let bytes_per_sample = usize::from(rgb_ifd.geometry.bps / 8);
    let expected_bytes = pixel_count
        .checked_mul(3)
        .and_then(|n| n.checked_mul(bytes_per_sample))
        .ok_or(CoreError::InvalidDimensions { width, height })?;

    let mut raw_bytes = Vec::with_capacity(expected_bytes);
    for (&off, &len) in rgb_ifd
        .storage
        .strip_offsets
        .iter()
        .zip(rgb_ifd.storage.strip_byte_counts.iter())
    {
        let end = off.checked_add(len).ok_or_else(|| {
            DecodeError::MalformedMetadata("uncompressed RGB TIFF strip offset overflow".to_owned())
        })?;
        let slice = data.get(off..end).ok_or_else(|| {
            DecodeError::MalformedMetadata("uncompressed RGB TIFF strip out of bounds".to_owned())
        })?;
        let remaining = expected_bytes.saturating_sub(raw_bytes.len());
        if remaining == 0 {
            break;
        }
        raw_bytes.extend_from_slice(&slice[..slice.len().min(remaining)]);
    }
    if raw_bytes.len() != expected_bytes {
        return Err(CoreError::BufferLengthMismatch {
            expected: expected_bytes,
            actual: raw_bytes.len(),
        }
        .into());
    }

    let mut linear = Vec::with_capacity(pixel_count);
    if rgb_ifd.geometry.bps == 8 {
        for px in raw_bytes.as_chunks::<3>().0 {
            linear.push([
                srgb_u8_to_linear(px[0]),
                srgb_u8_to_linear(px[1]),
                srgb_u8_to_linear(px[2]),
            ]);
        }
    } else {
        let inv_max = 1.0_f32 / 65535.0_f32;
        for px in raw_bytes.as_chunks::<6>().0 {
            let (r, g, b) = match endian {
                Endian::Little => (
                    u16::from_le_bytes([px[0], px[1]]),
                    u16::from_le_bytes([px[2], px[3]]),
                    u16::from_le_bytes([px[4], px[5]]),
                ),
                Endian::Big => (
                    u16::from_be_bytes([px[0], px[1]]),
                    u16::from_be_bytes([px[2], px[3]]),
                    u16::from_be_bytes([px[4], px[5]]),
                ),
            };
            linear.push([
                f32::from(r) * inv_max,
                f32::from(g) * inv_max,
                f32::from(b) * inv_max,
            ]);
        }
    }

    let linear_rgb = LinearRgbBuffer::new(width, height, linear)?;
    let camera_make = ifd0
        .camera
        .make
        .clone()
        .or_else(|| rgb_ifd.camera.make.clone())
        .unwrap_or_default();
    let camera_model = ifd0
        .camera
        .model
        .clone()
        .or_else(|| rgb_ifd.camera.model.clone())
        .unwrap_or_else(|| "TIFF RGB".to_owned());

    let mut metadata = PhotoMetadata {
        camera_make,
        camera_model,
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

    let mut orientation = ifd0
        .camera
        .orientation
        .or(rgb_ifd.camera.orientation)
        .filter(|c| (1..=8).contains(c))
        .unwrap_or(1);

    let mut exif_tags = vec![
        ("Camera Make".to_owned(), metadata.camera_make.clone()),
        ("Camera Model".to_owned(), metadata.camera_model.clone()),
        (
            "Dimensions".to_owned(),
            format!(
                "{width} x {height} ({:.1} MP)",
                (width * height) as f32 / 1_000_000.0
            ),
        ),
        (
            "Sensor Format".to_owned(),
            format!("Uncompressed {}-bit RGB TIFF", rgb_ifd.geometry.bps),
        ),
    ];
    exif::extract_dng_and_exif_metadata(path, &mut metadata, &mut orientation, &mut exif_tags);
    exif_tags.push((
        "Orientation".to_owned(),
        exif::orientation_label(orientation).to_owned(),
    ));

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
