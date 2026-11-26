#![forbid(unsafe_code)]

//! Pure-safe-Rust fallback decoder for DNG/TIFF files and misnamed raster images
//! rejected by `rawler` (e.g., 12-bit Lossy CinemaDNG `SOF1` tiles, 8-bit Lossy DNG
//! compression `34892`, uncompressed DNGs with mismatched strip width, and JPEG/PNG
//! files saved with a `.dng` extension).

use std::path::Path;

use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, CoreError, PhotoMetadata, PickStatus, SignedCfaBuffer,
};

use crate::noise_profile::estimate_noise_profile_wavelet_mad;
use crate::special_sensors::XYZ_TO_SRGB_D65;
use crate::{
    align_crop_to_cfa_phase, exif, jpeg12, normalize_cfa_signed, sensor, DecodeError, DecodedRaw,
    LinearRgbBuffer, SensorCropRect, SensorDimensions,
};

/// Attempts to decode a RAW file using the pure-Rust DNG/TIFF/raster fallback decoder.
pub fn try_decode_fallback(path: &Path) -> Result<DecodedRaw, DecodeError> {
    let data = std::fs::read(path).map_err(|e| {
        DecodeError::Io(format!(
            "failed to read RAW file {} for fallback decode: {e}",
            path.display()
        ))
    })?;

    if is_jpeg_or_png_magic(&data) {
        return decode_embedded_raster(path, &data);
    }

    decode_dng_tiff(path, &data)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    #[inline]
    fn u16(self, data: &[u8], off: usize) -> Option<u16> {
        let end = off.checked_add(2)?;
        let b = data.get(off..end)?;
        Some(match self {
            Self::Little => u16::from_le_bytes([b[0], b[1]]),
            Self::Big => u16::from_be_bytes([b[0], b[1]]),
        })
    }

    #[inline]
    fn u32(self, data: &[u8], off: usize) -> Option<u32> {
        let end = off.checked_add(4)?;
        let b = data.get(off..end)?;
        Some(match self {
            Self::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Self::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    #[inline]
    fn i32(self, data: &[u8], off: usize) -> Option<i32> {
        self.u32(data, off).map(|v| v as i32)
    }
}

#[derive(Debug, Clone, Default)]
struct ParsedIfd {
    new_subfile_type: u32,
    width: usize,
    height: usize,
    bps: u16,
    compression: u16,
    photometric: u16,
    spp: u16,
    make: Option<String>,
    model: Option<String>,
    orientation: Option<u16>,
    strip_offsets: Vec<usize>,
    rows_per_strip: usize,
    strip_byte_counts: Vec<usize>,
    tile_width: usize,
    tile_length: usize,
    tile_offsets: Vec<usize>,
    tile_byte_counts: Vec<usize>,
    sub_ifds: Vec<usize>,
    cfa_pattern: Option<[u8; 4]>,
    linearization_table: Option<Vec<u16>>,
    black_level: Option<[f32; 4]>,
    white_level: Option<[f32; 4]>,
    color_matrix1: Option<[f32; 9]>,
    color_matrix2: Option<[f32; 9]>,
    calib_illuminant2: Option<u16>,
    as_shot_neutral: Option<[f32; 3]>,
    active_area: Option<[usize; 4]>,
}

fn read_u32_list(data: &[u8], e: Endian, typ: u16, count: usize, entry_off: usize) -> Vec<usize> {
    let elem_size = match typ {
        1 => 1,
        3 => 2,
        4 => 4,
        _ => return Vec::new(),
    };
    let total_bytes = count.saturating_mul(elem_size);
    let base = if total_bytes <= 4 {
        entry_off + 8
    } else {
        let Some(off) = e.u32(data, entry_off + 8) else {
            return Vec::new();
        };
        off as usize
    };
    if base.saturating_add(total_bytes) > data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let pos = base + i * elem_size;
        let val = match typ {
            1 => usize::from(data[pos]),
            3 => e.u16(data, pos).map_or(0, usize::from),
            4 => e.u32(data, pos).map_or(0, |v| v as usize),
            _ => 0,
        };
        out.push(val);
    }
    out
}

fn read_f32_list(data: &[u8], e: Endian, typ: u16, count: usize, entry_off: usize) -> Vec<f32> {
    let elem_size = match typ {
        1 => 1,
        3 => 2,
        4 => 4,
        5 | 10 => 8,
        _ => return Vec::new(),
    };
    let total_bytes = count.saturating_mul(elem_size);
    let base = if total_bytes <= 4 {
        entry_off + 8
    } else {
        let Some(off) = e.u32(data, entry_off + 8) else {
            return Vec::new();
        };
        off as usize
    };
    if base.saturating_add(total_bytes) > data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let pos = base + i * elem_size;
        let val = match typ {
            1 => f32::from(data[pos]),
            3 => e.u16(data, pos).map_or(0.0, f32::from),
            4 => e.u32(data, pos).map_or(0.0, |v| v as f32),
            5 => {
                let num = e.u32(data, pos).unwrap_or(0) as f64;
                let den = e.u32(data, pos + 4).unwrap_or(1) as f64;
                if den.abs() > 1e-12 {
                    (num / den) as f32
                } else {
                    0.0
                }
            }
            10 => {
                let num = f64::from(e.i32(data, pos).unwrap_or(0));
                let den = f64::from(e.i32(data, pos + 4).unwrap_or(1));
                if den.abs() > 1e-12 {
                    (num / den) as f32
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };
        out.push(val);
    }
    out
}

fn read_ascii(data: &[u8], e: Endian, count: usize, entry_off: usize) -> Option<String> {
    if count == 0 {
        return None;
    }
    let base = if count <= 4 {
        entry_off + 8
    } else {
        e.u32(data, entry_off + 8)? as usize
    };
    let slice = data.get(base..base.saturating_add(count))?;
    let trimmed = slice.split(|&b| b == 0).next().unwrap_or(slice);
    let s = String::from_utf8_lossy(trimmed).trim().to_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn parse_ifd(data: &[u8], off: usize, e: Endian) -> Option<ParsedIfd> {
    let count = usize::from(e.u16(data, off)?);
    let mut ifd = ParsedIfd {
        spp: 1,
        ..ParsedIfd::default()
    };

    for i in 0..count {
        let entry_off = off.checked_add(2)?.checked_add(i.checked_mul(12)?)?;
        if entry_off.checked_add(12).is_none_or(|end| end > data.len()) {
            break;
        }
        let tag = e.u16(data, entry_off)?;
        let typ = e.u16(data, entry_off + 2)?;
        let n = e.u32(data, entry_off + 4)? as usize;

        match tag {
            254 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.new_subfile_type = v as u32;
                }
            }
            256 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.width = v;
                }
            }
            257 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.height = v;
                }
            }
            258 => {
                // BitsPerSample has count=3 for RGB TIFFs (stored out-of-line when total_bytes > 4),
                // so pass the real count (capped at 16) rather than 1 to preserve pointer dereferencing.
                if let Some(&v) = read_u32_list(data, e, typ, n.min(16), entry_off).first() {
                    ifd.bps = v as u16;
                }
            }
            259 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.compression = v as u16;
                }
            }
            262 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.photometric = v as u16;
                }
            }
            271 => ifd.make = read_ascii(data, e, n, entry_off),
            272 => ifd.model = read_ascii(data, e, n, entry_off),
            273 => ifd.strip_offsets = read_u32_list(data, e, typ, n, entry_off),
            274 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.orientation = Some(v as u16);
                }
            }
            277 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.spp = v as u16;
                }
            }
            278 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.rows_per_strip = v;
                }
            }
            279 => ifd.strip_byte_counts = read_u32_list(data, e, typ, n, entry_off),
            322 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.tile_width = v;
                }
            }
            323 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.tile_length = v;
                }
            }
            324 => ifd.tile_offsets = read_u32_list(data, e, typ, n, entry_off),
            325 => ifd.tile_byte_counts = read_u32_list(data, e, typ, n, entry_off),
            330 => ifd.sub_ifds = read_u32_list(data, e, typ, n, entry_off),
            33422 => {
                let vals = read_u32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 4 {
                    ifd.cfa_pattern =
                        Some([vals[0] as u8, vals[1] as u8, vals[2] as u8, vals[3] as u8]);
                }
            }
            50712 => {
                let vals = read_u32_list(data, e, typ, n.min(65536), entry_off);
                if !vals.is_empty() {
                    ifd.linearization_table = Some(vals.into_iter().map(|v| v as u16).collect());
                }
            }
            50714 => {
                let vals = read_f32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 1 {
                    ifd.black_level = Some([vals[0]; 4]);
                } else if vals.len() >= 4 {
                    ifd.black_level = Some([vals[0], vals[1], vals[2], vals[3]]);
                }
            }
            50717 => {
                let vals = read_f32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 1 {
                    ifd.white_level = Some([vals[0]; 4]);
                } else if vals.len() >= 4 {
                    ifd.white_level = Some([vals[0], vals[1], vals[2], vals[3]]);
                }
            }
            50721 => {
                let vals = read_f32_list(data, e, typ, n.min(9), entry_off);
                if vals.len() == 9 && vals.iter().all(|v| v.is_finite()) {
                    let mut m = [0.0_f32; 9];
                    m.copy_from_slice(&vals);
                    ifd.color_matrix1 = Some(m);
                }
            }
            50722 => {
                let vals = read_f32_list(data, e, typ, n.min(9), entry_off);
                if vals.len() == 9 && vals.iter().all(|v| v.is_finite()) {
                    let mut m = [0.0_f32; 9];
                    m.copy_from_slice(&vals);
                    ifd.color_matrix2 = Some(m);
                }
            }
            50779 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.calib_illuminant2 = Some(v as u16);
                }
            }
            50728 => {
                let vals = read_f32_list(data, e, typ, n.min(3), entry_off);
                if vals.len() == 3 && vals.iter().all(|&v| v.is_finite() && v > 0.0) {
                    ifd.as_shot_neutral = Some([vals[0], vals[1], vals[2]]);
                }
            }
            50829 => {
                let vals = read_u32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 4 {
                    ifd.active_area = Some([vals[0], vals[1], vals[2], vals[3]]);
                }
            }
            _ => {}
        }
    }

    Some(ifd)
}

fn cfa_pattern_from_bytes(pat: [u8; 4]) -> Option<CfaPattern> {
    match pat {
        [0, 1, 1, 2] => Some(CfaPattern::Bayer(BayerOrigin::Rggb)),
        [2, 1, 1, 0] => Some(CfaPattern::Bayer(BayerOrigin::Bggr)),
        [1, 0, 2, 1] => Some(CfaPattern::Bayer(BayerOrigin::Grbg)),
        [1, 2, 0, 1] => Some(CfaPattern::Bayer(BayerOrigin::Gbrg)),
        _ => None,
    }
}

fn decode_dng_tiff(path: &Path, data: &[u8]) -> Result<DecodedRaw, DecodeError> {
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
        .filter(|c| c.width > 0 && c.height > 0 && matches!(c.photometric, 32803 | 34892))
        .max_by_key(|c| (c.new_subfile_type == 0, c.width.saturating_mul(c.height)))
        .cloned();

    let Some(raw_ifd) = raw_ifd else {
        // Fallback for uncompressed 8-bit / 16-bit interleaved RGB TIFF/NEF scanners
        // (e.g., Nikon COOLSCAN IV/V ED).
        if let Some(rgb_ifd) = candidates
            .into_iter()
            .filter(|c| {
                c.width > 0
                    && c.height > 0
                    && c.photometric == 2
                    && c.spp == 3
                    && c.compression == 1
                    && matches!(c.bps, 8 | 16)
                    && !c.strip_offsets.is_empty()
            })
            .max_by_key(|c| c.width.saturating_mul(c.height))
        {
            return decode_uncompressed_rgb_tiff(path, data, &ifd0, &rgb_ifd, endian);
        }
        return Err(DecodeError::UnsupportedFormat(
            "no CFA, LinearRaw, or uncompressed RGB IFD found in DNG/TIFF".to_owned(),
        ));
    };

    let cfa_pattern = raw_ifd
        .cfa_pattern
        .or(ifd0.cfa_pattern)
        .and_then(cfa_pattern_from_bytes)
        .ok_or_else(|| {
            DecodeError::UnsupportedFormat(
                "missing or unsupported 2x2 Bayer CFAPattern in DNG fallback".to_owned(),
            )
        })?;

    if raw_ifd.spp != 1 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "unsupported DNG fallback SamplesPerPixel={}",
            raw_ifd.spp
        )));
    }

    let (full_w, full_h, mut samples_u16) = match raw_ifd.compression {
        7 | 34892 => decode_dng_jpeg_cfa(data, &raw_ifd)?,
        1 => decode_dng_uncompressed_cfa(data, &raw_ifd, endian)?,
        comp => {
            return Err(DecodeError::UnsupportedFormat(format!(
                "unsupported DNG fallback compression={comp}"
            )));
        }
    };

    // Apply DNG LinearizationTable if present
    if let Some(lut) = raw_ifd
        .linearization_table
        .as_ref()
        .or(ifd0.linearization_table.as_ref())
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
        if let Some([top, left, bottom, right]) = raw_ifd.active_area {
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

    let black_levels = raw_ifd.black_level.or(ifd0.black_level).unwrap_or([0.0; 4]);
    let default_white = if raw_ifd.linearization_table.is_some() {
        65535.0
    } else if raw_ifd.bps > 0 && raw_ifd.bps <= 16 {
        ((1_u32 << raw_ifd.bps) - 1) as f32
    } else {
        65535.0
    };
    let white_levels = raw_ifd
        .white_level
        .or(ifd0.white_level)
        .unwrap_or([default_white; 4]);

    if black_levels[0] >= white_levels[0] {
        return Err(DecodeError::MalformedMetadata(format!(
            "invalid DNG fallback black/white level range: black={}, white={}",
            black_levels[0], white_levels[0]
        )));
    }

    let color_matrix = if ifd0.calib_illuminant2 == Some(21) {
        ifd0.color_matrix2
            .or(raw_ifd.color_matrix2)
            .or(ifd0.color_matrix1)
            .or(raw_ifd.color_matrix1)
    } else {
        ifd0.color_matrix1
            .or(raw_ifd.color_matrix1)
            .or(ifd0.color_matrix2)
            .or(raw_ifd.color_matrix2)
    };

    let raw_wb = if let Some([nr, ng, nb]) = ifd0.as_shot_neutral.or(raw_ifd.as_shot_neutral) {
        [1.0 / nr, 1.0 / ng, 1.0 / nb, 1.0]
    } else {
        [f32::NAN; 4]
    };
    let wb_coeffs = sensor::sanitize_wb_coeffs_with_matrix(raw_wb, color_matrix.as_ref());

    let camera_make = ifd0
        .make
        .clone()
        .or_else(|| raw_ifd.make.clone())
        .unwrap_or_default();
    let camera_model = ifd0
        .model
        .clone()
        .or_else(|| raw_ifd.model.clone())
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
        .orientation
        .or(raw_ifd.orientation)
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
        format!("DNG Fallback (Compression={})", raw_ifd.compression),
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

fn decode_dng_jpeg_cfa(
    data: &[u8],
    ifd: &ParsedIfd,
) -> Result<(usize, usize, Vec<u16>), DecodeError> {
    let width = ifd.width;
    let height = ifd.height;
    let Some(total) = width.checked_mul(height) else {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    };
    let mut out = vec![0_u16; total];

    if !ifd.tile_offsets.is_empty() && ifd.tile_width > 0 && ifd.tile_length > 0 {
        let tiles_x = width.div_ceil(ifd.tile_width);
        let tiles_y = height.div_ceil(ifd.tile_length);
        let expected_tiles = tiles_x.saturating_mul(tiles_y);
        if ifd.tile_offsets.len() < expected_tiles || ifd.tile_byte_counts.len() < expected_tiles {
            return Err(DecodeError::MalformedMetadata(
                "DNG tile table shorter than expected tile count".to_owned(),
            ));
        }

        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let idx = ty * tiles_x + tx;
                let off = ifd.tile_offsets[idx];
                let len = ifd.tile_byte_counts[idx];
                let chunk = data.get(off..off.saturating_add(len)).ok_or_else(|| {
                    DecodeError::MalformedMetadata("DNG tile byte range out of bounds".to_owned())
                })?;

                let decoded = jpeg12::decode_jpeg_gray(chunk)?;
                let tile_w = ifd.tile_width;
                let tile_h = ifd.tile_length;

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

    if !ifd.strip_offsets.is_empty() {
        let rows_per_strip = if ifd.rows_per_strip > 0 {
            ifd.rows_per_strip
        } else {
            height
        };
        let mut cur_y = 0_usize;
        for (&off, &len) in ifd.strip_offsets.iter().zip(ifd.strip_byte_counts.iter()) {
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

fn decode_dng_uncompressed_cfa(
    data: &[u8],
    ifd: &ParsedIfd,
    endian: Endian,
) -> Result<(usize, usize, Vec<u16>), DecodeError> {
    if ifd.bps != 16 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "unsupported uncompressed DNG fallback bps={}",
            ifd.bps
        )));
    }
    if ifd.strip_offsets.is_empty() || ifd.strip_byte_counts.is_empty() {
        return Err(DecodeError::MalformedMetadata(
            "uncompressed DNG IFD missing StripOffsets/StripByteCounts".to_owned(),
        ));
    }

    let total_strip_bytes = ifd
        .strip_byte_counts
        .iter()
        .copied()
        .try_fold(0_usize, usize::checked_add)
        .ok_or_else(|| {
            DecodeError::MalformedMetadata("uncompressed DNG strip byte count overflow".to_owned())
        })?;
    let expected_bytes = ifd
        .width
        .checked_mul(ifd.height)
        .and_then(|n| n.checked_mul(2))
        .ok_or(CoreError::InvalidDimensions {
            width: ifd.width,
            height: ifd.height,
        })?;

    // Handle firmware quirks where StripByteCounts encodes the true even active row width
    // (smaller than the declared TIFF ImageWidth, e.g., 5280 vs 5376 on 20MP sensors).
    let effective_width = if total_strip_bytes >= expected_bytes {
        ifd.width
    } else if ifd.height > 0 && total_strip_bytes.is_multiple_of(ifd.height * 2) {
        let inferred = total_strip_bytes / (ifd.height * 2);
        if inferred >= 2 && inferred.is_multiple_of(2) {
            inferred
        } else {
            return Err(DecodeError::MalformedMetadata(format!(
                "uncompressed DNG strip byte count {total_strip_bytes} does not match {}x{}",
                ifd.width, ifd.height
            )));
        }
    } else {
        return Err(DecodeError::MalformedMetadata(format!(
            "uncompressed DNG strip byte count {total_strip_bytes} shorter than expected {expected_bytes}"
        )));
    };

    let target_pixels = effective_width * ifd.height;
    let mut out = Vec::with_capacity(target_pixels);
    for (&off, &len) in ifd.strip_offsets.iter().zip(ifd.strip_byte_counts.iter()) {
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

    Ok((effective_width, ifd.height, out))
}

fn decode_uncompressed_rgb_tiff(
    path: &Path,
    data: &[u8],
    ifd0: &ParsedIfd,
    rgb_ifd: &ParsedIfd,
    endian: Endian,
) -> Result<DecodedRaw, DecodeError> {
    let width = rgb_ifd.width;
    let height = rgb_ifd.height;
    let pixel_count = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    let bytes_per_sample = usize::from(rgb_ifd.bps / 8);
    let expected_bytes = pixel_count
        .checked_mul(3)
        .and_then(|n| n.checked_mul(bytes_per_sample))
        .ok_or(CoreError::InvalidDimensions { width, height })?;

    let mut raw_bytes = Vec::with_capacity(expected_bytes);
    for (&off, &len) in rgb_ifd
        .strip_offsets
        .iter()
        .zip(rgb_ifd.strip_byte_counts.iter())
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
    if rgb_ifd.bps == 8 {
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
        .make
        .clone()
        .or_else(|| rgb_ifd.make.clone())
        .unwrap_or_default();
    let camera_model = ifd0
        .model
        .clone()
        .or_else(|| rgb_ifd.model.clone())
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
        .orientation
        .or(rgb_ifd.orientation)
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
            format!("Uncompressed {}-bit RGB TIFF", rgb_ifd.bps),
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
