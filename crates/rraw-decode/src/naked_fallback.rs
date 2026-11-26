#![forbid(unsafe_code)]

//! Pure-Rust fallback decoder for headerless ("naked") RAW files identified by exact file size
//! (and optional magic prefix) when standard container parsers cannot identify the file.
//!
//! Covers legacy compact cameras (Pentax Optio S4, Minolta DiMAGE Z2, Kodak C330 / C603 / C643),
//! CHDK 12-bit Canon PowerShot dumps, and action-camera raw sensor dumps (GitUp, SJCAM, Paralenz,
//! Xiro, ImBack).

use std::path::Path;

use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, CoreError, NoiseProfile, NoiseProfileSource,
    PhotoMetadata, PickStatus, SignedCfaBuffer,
};

use crate::noise_profile::estimate_noise_profile_wavelet_mad;
use crate::special_sensors::{self, XYZ_TO_SRGB_D65};
use crate::{DecodeError, DecodedRaw, LinearRgbBuffer};

/// Raw sample packing format for headerless sensor dumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NakedPacking {
    /// 8-bit unsigned Bayer samples (1 byte per pixel).
    U8,
    /// 10-bit little-endian packed Bayer samples (5 bytes per 4 pixels).
    U10Le,
    /// 12-bit big-endian packed Bayer samples (3 bytes per 2 pixels).
    U12Be,
    /// 12-bit big-endian packed inside byte-swapped 32-bit words (12 bytes per 8 pixels).
    U12Be32Swap,
    /// 12-bit little-endian packed Bayer samples (3 bytes per 2 pixels).
    U12Le,
    /// 12-bit CHDK Canon PowerShot packed Bayer samples (6 bytes per 4 pixels).
    U12Chdk,
    /// 16-bit little-endian unsigned Bayer samples (2 bytes per pixel).
    U16Le,
    /// 4:2:2 YUYV (`Y0, Cb, Y1, Cr`) 8-bit video/still dump decoded to linear sRGB.
    Yuyv422,
}

/// Static sensor geometry specification matched by exact file byte length.
#[derive(Debug, Clone, Copy)]
struct NakedSpec {
    file_size: usize,
    make: &'static str,
    model: &'static str,
    width: usize,
    height: usize,
    row_stride_bytes: usize,
    data_offset: usize,
    /// Optional byte offset of the second interlaced field (odd rows `1, 3, 5, ...`).
    field2_offset: Option<usize>,
    magic_prefix: &'static [u8],
    packing: NakedPacking,
    bayer: BayerOrigin,
    black_level: f32,
    white_level: f32,
}

/// Curated exact-filesize table for headerless RAW sensor dumps.
const NAKED_SPECS: &[NakedSpec] = &[
    NakedSpec {
        file_size: 2_247_168,
        make: "Kodak",
        model: "C330",
        width: 1216,
        height: 912,
        row_stride_bytes: 2464,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::Yuyv422,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 255.0,
    },
    NakedSpec {
        file_size: 5_869_568,
        make: "Minolta",
        model: "DiMAGE Z2",
        width: 2288,
        height: 1710,
        row_stride_bytes: 3432,
        data_offset: 0,
        field2_offset: Some(2_934_784),
        magic_prefix: b"",
        packing: NakedPacking::U12Be32Swap,
        bayer: BayerOrigin::Bggr,
        black_level: 0.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 6_114_240,
        make: "Pentax",
        model: "Optio S4",
        width: 2316,
        height: 1736,
        row_stride_bytes: 3520,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U12Be,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 6_163_328,
        make: "Kodak",
        model: "C603 / C643",
        width: 2864,
        height: 2152,
        row_stride_bytes: 2864,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U8,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 255.0,
    },
    NakedSpec {
        file_size: 6_166_488,
        make: "Kodak",
        model: "C603 / C643",
        width: 2864,
        height: 2152,
        row_stride_bytes: 2864,
        data_offset: 3160,
        field2_offset: None,
        magic_prefix: b"KodakCommonI",
        packing: NakedPacking::U8,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 255.0,
    },
    NakedSpec {
        file_size: 15_335_424,
        make: "ImBack",
        model: "ImB35mm",
        width: 2304,
        height: 3328,
        row_stride_bytes: 4608,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U16Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 16383.0,
    },
    NakedSpec {
        file_size: 15_972_480,
        make: "SJCAM",
        model: "SJ6 Legend",
        width: 3760,
        height: 2832,
        row_stride_bytes: 5640,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U12Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 16_588_800,
        make: "Paralenz",
        model: "Dive Camera",
        width: 3840,
        height: 2160,
        row_stride_bytes: 7680,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U16Le,
        bayer: BayerOrigin::Rggb,
        black_level: 1008.0,
        white_level: 16383.0,
    },
    NakedSpec {
        file_size: 17_496_000,
        make: "Xiro",
        model: "Xplorer V",
        width: 4320,
        height: 3240,
        row_stride_bytes: 5400,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U10Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 1023.0,
    },
    NakedSpec {
        file_size: 18_000_000,
        make: "SJCAM",
        model: "SJ5000X Elite",
        width: 4000,
        height: 3000,
        row_stride_bytes: 6000,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U12Le,
        bayer: BayerOrigin::Rggb,
        black_level: 128.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 18_325_296,
        make: "SJCAM",
        model: "SJ6 Legend",
        width: 4024,
        height: 3036,
        row_stride_bytes: 6036,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U12Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 18_653_760,
        make: "Canon",
        model: "PowerShot SX20 IS",
        width: 4080,
        height: 3048,
        row_stride_bytes: 6120,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U12Chdk,
        bayer: BayerOrigin::Rggb,
        black_level: 127.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 23_887_872,
        make: "GitUp",
        model: "Git2 (16:9)",
        width: 4608,
        height: 2592,
        row_stride_bytes: 9216,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U16Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 65535.0,
    },
    NakedSpec {
        file_size: 24_000_000,
        make: "GitUp",
        model: "G3 Duo",
        width: 4000,
        height: 3000,
        row_stride_bytes: 8000,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U16Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 65535.0,
    },
    NakedSpec {
        file_size: 24_192_768,
        make: "GitUp",
        model: "Git2P (16:9)",
        width: 4624,
        height: 2616,
        row_stride_bytes: 9248,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U16Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 65535.0,
    },
    NakedSpec {
        file_size: 30_858_240,
        make: "Canon",
        model: "PowerShot ELPH 180",
        width: 5248,
        height: 3920,
        row_stride_bytes: 7872,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U12Chdk,
        bayer: BayerOrigin::Rggb,
        black_level: 127.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 31_663_200,
        make: "Canon",
        model: "PowerShot SX710 HS",
        width: 5344,
        height: 3950,
        row_stride_bytes: 8016,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U12Chdk,
        bayer: BayerOrigin::Rggb,
        black_level: 127.0,
        white_level: 4095.0,
    },
    NakedSpec {
        file_size: 31_850_496,
        make: "GitUp",
        model: "Git2 (4:3)",
        width: 4608,
        height: 3456,
        row_stride_bytes: 9216,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U16Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 65535.0,
    },
    NakedSpec {
        file_size: 32_257_024,
        make: "GitUp",
        model: "Git2P (4:3)",
        width: 4624,
        height: 3488,
        row_stride_bytes: 9248,
        data_offset: 0,
        field2_offset: None,
        magic_prefix: b"",
        packing: NakedPacking::U16Le,
        bayer: BayerOrigin::Rggb,
        black_level: 0.0,
        white_level: 65535.0,
    },
];

/// Attempts to decode a headerless ("naked") RAW file by matching its exact byte length
/// (and optional magic header) against the curated sensor table.
pub fn try_decode_naked(path: &Path) -> Result<DecodedRaw, DecodeError> {
    let file_len = std::fs::metadata(path)
        .map_err(|e| DecodeError::Io(format!("failed to stat {}: {e}", path.display())))?
        .len();
    let file_size = usize::try_from(file_len).map_err(|_| {
        DecodeError::UnsupportedFormat("file size exceeds addressable range".into())
    })?;

    let candidates: Vec<&NakedSpec> = NAKED_SPECS
        .iter()
        .filter(|s| s.file_size == file_size)
        .collect();
    if candidates.is_empty() {
        return Err(DecodeError::UnsupportedFormat(format!(
            "no headerless RAW profile matches file size {file_size}"
        )));
    }

    let bytes = std::fs::read(path)
        .map_err(|e| DecodeError::Io(format!("failed to read {}: {e}", path.display())))?;
    if bytes.len() != file_size {
        return Err(DecodeError::Io(format!(
            "file size changed while reading {}",
            path.display()
        )));
    }

    let spec = candidates
        .into_iter()
        .find(|s| s.magic_prefix.is_empty() || bytes.starts_with(s.magic_prefix))
        .ok_or_else(|| {
            DecodeError::UnsupportedFormat(format!(
                "headerless RAW magic mismatch for size {file_size}"
            ))
        })?;

    decode_naked_slice(&bytes, spec)
}

fn decode_naked_slice(data: &[u8], spec: &NakedSpec) -> Result<DecodedRaw, DecodeError> {
    let required_bytes = spec
        .height
        .checked_mul(spec.row_stride_bytes)
        .and_then(|n| n.checked_add(spec.data_offset))
        .ok_or(CoreError::InvalidDimensions {
            width: spec.width,
            height: spec.height,
        })?;
    if data.len() < required_bytes {
        return Err(DecodeError::MalformedMetadata(format!(
            "headerless RAW truncated: need {required_bytes} bytes, got {}",
            data.len()
        )));
    }

    match spec.packing {
        NakedPacking::Yuyv422 => decode_yuyv422(data, spec),
        _ => decode_bayer_naked(data, spec),
    }
}

/// Unpacks a single Bayer scanline from `row_bytes` into `out` (appending at most `width` samples).
pub fn unpack_bayer_row(
    row_bytes: &[u8],
    width: usize,
    packing: NakedPacking,
    out: &mut Vec<u16>,
) -> Result<(), DecodeError> {
    let start_len = out.len();
    match packing {
        NakedPacking::U8 => {
            if row_bytes.len() < width {
                return Err(DecodeError::MalformedMetadata(
                    "U8 row shorter than image width".into(),
                ));
            }
            out.extend(row_bytes.iter().take(width).map(|&b| u16::from(b)));
        }
        NakedPacking::U10Le => {
            let (chunks, _) = row_bytes.as_chunks::<5>();
            for &[b0, b1, b2, b3, b4] in chunks {
                let v = u64::from(b0)
                    | (u64::from(b1) << 8)
                    | (u64::from(b2) << 16)
                    | (u64::from(b3) << 24)
                    | (u64::from(b4) << 32);
                for shift in [0u32, 10, 20, 30] {
                    if out.len() - start_len < width {
                        out.push(((v >> shift) & 0x03FF) as u16);
                    }
                }
                if out.len() - start_len >= width {
                    break;
                }
            }
        }
        NakedPacking::U12Be => {
            let (chunks, _) = row_bytes.as_chunks::<3>();
            for &[b0, b1, b2] in chunks {
                let p0 = (u16::from(b0) << 4) | (u16::from(b1) >> 4);
                let p1 = ((u16::from(b1) & 0x0F) << 8) | u16::from(b2);
                if out.len() - start_len < width {
                    out.push(p0);
                }
                if out.len() - start_len < width {
                    out.push(p1);
                }
                if out.len() - start_len >= width {
                    break;
                }
            }
        }
        NakedPacking::U12Be32Swap => {
            let (chunks, _) = row_bytes.as_chunks::<12>();
            for &[b0, b1, b2, b3, b4, b5, b6, b7, b8, b9, b10, b11] in chunks {
                let s = [b3, b2, b1, b0, b7, b6, b5, b4, b11, b10, b9, b8];
                let (triples, _) = s.as_chunks::<3>();
                for &[t0, t1, t2] in triples {
                    let p0 = (u16::from(t0) << 4) | (u16::from(t1) >> 4);
                    let p1 = ((u16::from(t1) & 0x0F) << 8) | u16::from(t2);
                    if out.len() - start_len < width {
                        out.push(p0);
                    }
                    if out.len() - start_len < width {
                        out.push(p1);
                    }
                }
                if out.len() - start_len >= width {
                    break;
                }
            }
        }
        NakedPacking::U12Le => {
            let (chunks, _) = row_bytes.as_chunks::<3>();
            for &[b0, b1, b2] in chunks {
                let p0 = u16::from(b0) | ((u16::from(b1) & 0x0F) << 8);
                let p1 = (u16::from(b1) >> 4) | (u16::from(b2) << 4);
                if out.len() - start_len < width {
                    out.push(p0);
                }
                if out.len() - start_len < width {
                    out.push(p1);
                }
                if out.len() - start_len >= width {
                    break;
                }
            }
        }
        NakedPacking::U12Chdk => {
            let (chunks, _) = row_bytes.as_chunks::<6>();
            for &[b0, b1, b2, b3, b4, b5] in chunks {
                let p0 = (u16::from(b1) << 4) | (u16::from(b0) >> 4);
                let p1 = ((u16::from(b0) & 0x0F) << 8) | u16::from(b3);
                let p2 = (u16::from(b2) << 4) | (u16::from(b5) >> 4);
                let p3 = ((u16::from(b5) & 0x0F) << 8) | u16::from(b4);
                for p in [p0, p1, p2, p3] {
                    if out.len() - start_len < width {
                        out.push(p);
                    }
                }
                if out.len() - start_len >= width {
                    break;
                }
            }
        }
        NakedPacking::U16Le => {
            let (chunks, _) = row_bytes.as_chunks::<2>();
            for &pair in chunks.iter().take(width) {
                out.push(u16::from_le_bytes(pair));
            }
        }
        NakedPacking::Yuyv422 => {
            return Err(DecodeError::UnsupportedFormat(
                "YUYV 4:2:2 is not a Bayer packing format".into(),
            ));
        }
    }

    if out.len() - start_len != width {
        return Err(DecodeError::MalformedMetadata(format!(
            "unpacked row length mismatch: expected {width}, got {}",
            out.len() - start_len
        )));
    }
    Ok(())
}

fn decode_bayer_naked(data: &[u8], spec: &NakedSpec) -> Result<DecodedRaw, DecodeError> {
    let pixel_count = spec
        .width
        .checked_mul(spec.height)
        .ok_or(CoreError::InvalidDimensions {
            width: spec.width,
            height: spec.height,
        })?;
    let mut raw_u16 = Vec::with_capacity(pixel_count);

    for row_idx in 0..spec.height {
        let row_start = match spec.field2_offset {
            Some(f2_off) => {
                let field_row = row_idx / 2;
                let base = if row_idx.is_multiple_of(2) {
                    spec.data_offset
                } else {
                    f2_off
                };
                base + field_row * spec.row_stride_bytes
            }
            None => spec.data_offset + row_idx * spec.row_stride_bytes,
        };
        let row_end = row_start + spec.row_stride_bytes;
        let row_slice = data.get(row_start..row_end).ok_or_else(|| {
            DecodeError::MalformedMetadata("headerless RAW row slice out of bounds".into())
        })?;
        unpack_bayer_row(row_slice, spec.width, spec.packing, &mut raw_u16)?;
    }

    let range = (spec.white_level - spec.black_level).max(1.0);
    let inv_range = 1.0 / range;
    let mut normalized: Vec<f32> = raw_u16
        .into_iter()
        .map(|v| (f32::from(v) - spec.black_level) * inv_range)
        .collect();

    let fixes =
        special_sensors::detect_isolated_bayer_hot_pixels(&normalized, spec.width, spec.height);
    for (idx, healed) in fixes {
        if let Some(slot) = normalized.get_mut(idx) {
            *slot = healed;
        }
    }

    let wb_coeffs = estimate_gray_world_wb(&normalized, spec.width, spec.height, spec.bayer);
    let cfa_pattern = CfaPattern::Bayer(spec.bayer);
    let cfa_signed = SignedCfaBuffer::new(spec.width, spec.height, cfa_pattern, normalized)?;

    let noise_profile =
        estimate_noise_profile_wavelet_mad(&cfa_signed).unwrap_or(NoiseProfile::new(
            [0.001; 3],
            [0.0001; 3],
            NoiseProfileSource::WaveletMadFallback,
        )?);

    let metadata = PhotoMetadata {
        camera_make: spec.make.to_owned(),
        camera_model: spec.model.to_owned(),
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: None,
        iso: None,
        width: spec.width,
        height: spec.height,
        black_levels: [spec.black_level; 4],
        white_levels: [spec.white_level; 4],
        cfa_pattern,
        noise_profile,
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        rating: 0,
        pick: PickStatus::Unflagged,
        label: ColorLabel::None,
        keywords: Vec::new(),
    };

    let exif_tags = vec![
        ("Camera Make".to_owned(), metadata.camera_make.clone()),
        ("Camera Model".to_owned(), metadata.camera_model.clone()),
        (
            "Dimensions".to_owned(),
            format!(
                "{} x {} ({:.1} MP)",
                spec.width,
                spec.height,
                (spec.width * spec.height) as f32 / 1_000_000.0
            ),
        ),
        (
            "Sensor Format".to_owned(),
            format!("Headerless Bayer ({:?})", spec.packing),
        ),
        (
            "As-Shot WB Multipliers".to_owned(),
            format!(
                "R={:.3}, G={:.3}, B={:.3}",
                wb_coeffs[0], wb_coeffs[1], wb_coeffs[2]
            ),
        ),
        ("Orientation".to_owned(), "Normal".to_owned()),
    ];

    Ok(DecodedRaw {
        metadata,
        cfa_signed,
        linear_rgb: None,
        wb_coeffs,
        color_matrix: Some(XYZ_TO_SRGB_D65),
        exif_tags,
        orientation: 1,
    })
}

fn srgb_to_linear(c: f32) -> f32 {
    let clamped = c.clamp(0.0, 1.0);
    if clamped <= 0.04045 {
        clamped / 12.92
    } else {
        ((clamped + 0.055) / 1.055).powf(2.4)
    }
}

/// Decodes a 4:2:2 YUYV (`Y0, Cb, Y1, Cr`) buffer into a linear sRGB `LinearRgbBuffer`.
pub fn decode_yuyv422_pixels(
    data: &[u8],
    width: usize,
    height: usize,
    row_stride_bytes: usize,
    data_offset: usize,
) -> Result<LinearRgbBuffer, DecodeError> {
    let pixel_count = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    let mut rgb = Vec::with_capacity(pixel_count);

    for row_idx in 0..height {
        let row_start = data_offset + row_idx * row_stride_bytes;
        let row_end = row_start + row_stride_bytes;
        let row_slice = data
            .get(row_start..row_end)
            .ok_or_else(|| DecodeError::MalformedMetadata("YUYV row slice out of bounds".into()))?;
        let (chunks, _) = row_slice.as_chunks::<4>();
        let mut produced = 0usize;
        for &[y0, cb, y1, cr] in chunks {
            let cb_f = f32::from(cb) - 128.0;
            let cr_f = f32::from(cr) - 128.0;
            for y_byte in [y0, y1] {
                if produced < width {
                    let y_f = f32::from(y_byte);
                    let r = (y_f + 1.402 * cr_f) / 255.0;
                    let g = (y_f - 0.344_136 * cb_f - 0.714_136 * cr_f) / 255.0;
                    let b = (y_f + 1.772 * cb_f) / 255.0;
                    rgb.push([srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)]);
                    produced += 1;
                }
            }
            if produced >= width {
                break;
            }
        }
        if produced != width {
            return Err(DecodeError::MalformedMetadata(format!(
                "YUYV row produced {produced} pixels, expected {width}"
            )));
        }
    }

    LinearRgbBuffer::new(width, height, rgb)
}

fn decode_yuyv422(data: &[u8], spec: &NakedSpec) -> Result<DecodedRaw, DecodeError> {
    let linear_rgb = decode_yuyv422_pixels(
        data,
        spec.width,
        spec.height,
        spec.row_stride_bytes,
        spec.data_offset,
    )?;

    let metadata = PhotoMetadata {
        camera_make: spec.make.to_owned(),
        camera_model: spec.model.to_owned(),
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: None,
        iso: None,
        width: spec.width,
        height: spec.height,
        black_levels: [0.0; 4],
        white_levels: [255.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile: NoiseProfile::new(
            [0.001; 3],
            [0.0001; 3],
            NoiseProfileSource::WaveletMadFallback,
        )?,
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        rating: 0,
        pick: PickStatus::Unflagged,
        label: ColorLabel::None,
        keywords: Vec::new(),
    };

    let exif_tags = vec![
        ("Camera Make".to_owned(), metadata.camera_make.clone()),
        ("Camera Model".to_owned(), metadata.camera_model.clone()),
        (
            "Dimensions".to_owned(),
            format!(
                "{} x {} ({:.1} MP)",
                spec.width,
                spec.height,
                (spec.width * spec.height) as f32 / 1_000_000.0
            ),
        ),
        (
            "Sensor Format".to_owned(),
            "Headerless 4:2:2 YUYV (Linear sRGB)".to_owned(),
        ),
        ("Orientation".to_owned(), "Normal".to_owned()),
    ];

    Ok(DecodedRaw {
        metadata,
        cfa_signed: SignedCfaBuffer::default(),
        linear_rgb: Some(linear_rgb),
        wb_coeffs: [1.0; 4],
        color_matrix: Some(XYZ_TO_SRGB_D65),
        exif_tags,
        orientation: 1,
    })
}

/// Computes Gray-World white balance multipliers `[R, G, B, G]` from normalized Bayer samples.
fn estimate_gray_world_wb(
    samples: &[f32],
    width: usize,
    height: usize,
    origin: BayerOrigin,
) -> [f32; 4] {
    let mut sum_r = 0.0_f64;
    let mut sum_g = 0.0_f64;
    let mut sum_b = 0.0_f64;
    let mut cnt_r = 0u64;
    let mut cnt_g = 0u64;
    let mut cnt_b = 0u64;

    // Map (row & 1, col & 1) -> 0=R, 1=G, 2=B
    let color_lut: [[usize; 2]; 2] = match origin {
        BayerOrigin::Rggb => [[0, 1], [1, 2]],
        BayerOrigin::Bggr => [[2, 1], [1, 0]],
        BayerOrigin::Grbg => [[1, 0], [2, 1]],
        BayerOrigin::Gbrg => [[1, 2], [0, 1]],
    };

    // Subsample every 4th 2x2 quad for fast, deterministic WB estimation.
    let mut y = 0usize;
    while y + 1 < height {
        let mut x = 0usize;
        while x + 1 < width {
            for (dy, row_lut) in color_lut.iter().enumerate() {
                let row_off = (y + dy) * width;
                for (dx, &chan) in row_lut.iter().enumerate() {
                    if let Some(&v) = samples.get(row_off + x + dx) {
                        if v > 0.005 && v < 0.95 {
                            match chan {
                                0 => {
                                    sum_r += f64::from(v);
                                    cnt_r += 1;
                                }
                                1 => {
                                    sum_g += f64::from(v);
                                    cnt_g += 1;
                                }
                                _ => {
                                    sum_b += f64::from(v);
                                    cnt_b += 1;
                                }
                            }
                        }
                    }
                }
            }
            x += 8;
        }
        y += 8;
    }

    if cnt_r == 0 || cnt_g == 0 || cnt_b == 0 {
        return [1.0; 4];
    }
    let avg_r = sum_r / cnt_r as f64;
    let avg_g = sum_g / cnt_g as f64;
    let avg_b = sum_b / cnt_b as f64;
    if avg_r <= 1e-6 || avg_g <= 1e-6 || avg_b <= 1e-6 {
        return [1.0; 4];
    }

    let wb_r = (avg_g / avg_r).clamp(0.5, 4.0) as f32;
    let wb_b = (avg_g / avg_b).clamp(0.5, 4.0) as f32;
    [wb_r, 1.0, wb_b, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unpack_u8_and_u16le() -> Result<(), Box<dyn std::error::Error>> {
        let mut out_u8 = Vec::new();
        unpack_bayer_row(&[10, 20, 30, 255], 4, NakedPacking::U8, &mut out_u8)?;
        assert_eq!(out_u8, vec![10, 20, 30, 255]);

        let mut out_u16 = Vec::new();
        let bytes = [0x34, 0x12, 0xFF, 0x3F];
        unpack_bayer_row(&bytes, 2, NakedPacking::U16Le, &mut out_u16)?;
        assert_eq!(out_u16, vec![0x1234, 0x3FFF]);
        Ok(())
    }

    #[test]
    fn test_unpack_u10le() -> Result<(), Box<dyn std::error::Error>> {
        let p0 = 0x123u64;
        let p1 = 0x2ABu64;
        let p2 = 0x3FFu64;
        let p3 = 0x055u64;
        let packed = p0 | (p1 << 10) | (p2 << 20) | (p3 << 30);
        let bytes = packed.to_le_bytes();
        let mut out = Vec::new();
        unpack_bayer_row(&bytes[..5], 4, NakedPacking::U10Le, &mut out)?;
        assert_eq!(out, vec![0x123, 0x2AB, 0x3FF, 0x055]);
        Ok(())
    }

    #[test]
    fn test_unpack_u12be_u12le_and_u12chdk() -> Result<(), Box<dyn std::error::Error>> {
        // U12Be: 0xABC, 0xDEF -> [0xAB, 0xCD, 0xEF]
        let mut out_be = Vec::new();
        unpack_bayer_row(&[0xAB, 0xCD, 0xEF], 2, NakedPacking::U12Be, &mut out_be)?;
        assert_eq!(out_be, vec![0xABC, 0xDEF]);

        // U12Be32Swap: 12 bytes = three byte-swapped 32-bit words -> 8 pixels
        let raw_12 = [
            0x01, 0xEF, 0xCD, 0xAB, // swapped -> AB CD EF 01 -> 0xABC, 0xDEF, ...
            0x67, 0x45, 0x23, 0x00, // swapped -> 00 23 45 67
            0xDE, 0xBC, 0x9A, 0x89, // swapped -> 89 9A BC DE
        ];
        let mut out_sw = Vec::new();
        unpack_bayer_row(&raw_12, 8, NakedPacking::U12Be32Swap, &mut out_sw)?;
        assert_eq!(out_sw[0], 0xABC);
        assert_eq!(out_sw[1], 0xDEF);
        assert_eq!(out_sw.len(), 8);

        // U12Le: p0=0x123, p1=0x456 -> b0=0x23, b1=(0x6<<4)|0x1=0x61, b2=0x45
        let mut out_le = Vec::new();
        unpack_bayer_row(&[0x23, 0x61, 0x45], 2, NakedPacking::U12Le, &mut out_le)?;
        assert_eq!(out_le, vec![0x123, 0x456]);

        // U12Chdk: 6 bytes [b0, b1, b2, b3, b4, b5]
        let mut out_chdk = Vec::new();
        unpack_bayer_row(
            &[0xA1, 0xBC, 0xDE, 0x23, 0x45, 0xF6],
            4,
            NakedPacking::U12Chdk,
            &mut out_chdk,
        )?;
        assert_eq!(out_chdk, vec![0xBCA, 0x123, 0xDEF, 0x645]);
        Ok(())
    }

    #[test]
    fn test_decode_yuyv422_neutral_gray() -> Result<(), Box<dyn std::error::Error>> {
        // Y=128, Cb=128, Cr=128 -> neutral mid-gray in sRGB -> ~0.216 linear
        let row = [128u8, 128, 255, 128];
        let buf = decode_yuyv422_pixels(&row, 2, 1, 4, 0)?;
        assert_eq!(buf.width(), 2);
        assert_eq!(buf.height(), 1);
        let p0 = buf.samples()[0];
        let p1 = buf.samples()[1];
        assert!((p0[0] - p0[1]).abs() < 1e-4);
        assert!((p0[1] - p0[2]).abs() < 1e-4);
        assert!((p1[0] - 1.0).abs() < 1e-4);
        Ok(())
    }

    #[test]
    fn test_estimate_gray_world_wb_balances_green_cast() {
        // 8x8 RGGB with R=0.25, G=0.50, B=0.20 -> expected wb_r=2.0, wb_g=1.0, wb_b=2.5
        let mut samples = vec![0.0_f32; 64];
        for y in 0..8 {
            for x in 0..8 {
                samples[y * 8 + x] = match (y & 1, x & 1) {
                    (0, 0) => 0.25,
                    (1, 1) => 0.20,
                    _ => 0.50,
                };
            }
        }
        let wb = estimate_gray_world_wb(&samples, 8, 8, BayerOrigin::Rggb);
        assert!((wb[0] - 2.0).abs() < 1e-3);
        assert!((wb[1] - 1.0).abs() < 1e-3);
        assert!((wb[2] - 2.5).abs() < 1e-3);
    }
}
