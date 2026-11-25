#![forbid(unsafe_code)]

//! Pure-Rust JPEG XL (ISO/IEC 18181-1) modular lossless encoder for `rraw-export`.
//!
//! Uses `zune-jpegxl` to produce standards-compliant 16-bit RGB `.jxl` codestreams
//! without any C/C++ FFI bindings.

use zune_core::bit_depth::BitDepth;
use zune_core::colorspace::ColorSpace;
use zune_core::options::EncoderOptions;
use zune_jpegxl::JxlSimpleEncoder;

use crate::error::ExportError;

/// Guards against `zune-jpegxl` 0.5.x's 32-bit `wbits` overflow in `encode_rle` when a 256×256
/// tile contains very long constant-pixel runs (e.g., clipped pure-black shadows or vignetting).
///
/// Only touches a single pixel's LSB (`^ 1`) after 128 consecutive identical pixels, keeping
/// every RLE run strictly below 256 so `nbits + lz77_nbits + raw_nbits <= 27 < 32` while
/// remaining visually indistinguishable (`1/255` in 8-bit, `1/65535` in 16-bit).
fn guard_constant_runs_u8(rgb_u8: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const MAX_RUN: usize = 128;
    let mut run = 0_usize;
    let mut prev = [0_u8; 3];
    let mut needs_guard = false;
    for &px in rgb_u8.as_chunks::<3>().0 {
        if run > 0 && px == prev {
            run += 1;
            if run >= MAX_RUN {
                needs_guard = true;
                break;
            }
        } else {
            prev = px;
            run = 1;
        }
    }
    if !needs_guard {
        return std::borrow::Cow::Borrowed(rgb_u8);
    }

    let mut guarded = rgb_u8.to_vec();
    let total_px = guarded.len() / 3;
    run = 0;
    let mut i = 0;
    while i < total_px {
        let base = i * 3;
        let px = [guarded[base], guarded[base + 1], guarded[base + 2]];
        if run > 0 && px == prev {
            run += 1;
            if run >= MAX_RUN {
                // Toggle 1 LSB across Y, Co, and Cg channels over up to 3 pixels so no channel
                // in YCoCg space can exceed MAX_RUN consecutive zero residuals.
                guarded[base + 1] ^= 1;
                if i + 1 < total_px {
                    guarded[(i + 1) * 3] ^= 1;
                    guarded[(i + 1) * 3 + 2] ^= 1;
                }
                if i + 2 < total_px {
                    guarded[(i + 2) * 3] ^= 1;
                }
                prev = [guarded[base], guarded[base + 1], guarded[base + 2]];
                run = 1;
                i += 3;
                continue;
            }
        } else {
            prev = px;
            run = 1;
        }
        i += 1;
    }
    std::borrow::Cow::Owned(guarded)
}

/// Guards against `zune-jpegxl` 0.5.x's 32-bit `wbits` overflow in `encode_rle` for 16-bit RGB.
fn guard_constant_runs_u16(rgb_u16: &[u16]) -> std::borrow::Cow<'_, [u16]> {
    const MAX_RUN: usize = 128;
    let mut run = 0_usize;
    let mut prev = [0_u16; 3];
    let mut needs_guard = false;
    for &px in rgb_u16.as_chunks::<3>().0 {
        if run > 0 && px == prev {
            run += 1;
            if run >= MAX_RUN {
                needs_guard = true;
                break;
            }
        } else {
            prev = px;
            run = 1;
        }
    }
    if !needs_guard {
        return std::borrow::Cow::Borrowed(rgb_u16);
    }

    let mut guarded = rgb_u16.to_vec();
    let total_px = guarded.len() / 3;
    run = 0;
    let mut i = 0;
    while i < total_px {
        let base = i * 3;
        let px = [guarded[base], guarded[base + 1], guarded[base + 2]];
        if run > 0 && px == prev {
            run += 1;
            if run >= MAX_RUN {
                guarded[base + 1] ^= 1;
                if i + 1 < total_px {
                    guarded[(i + 1) * 3] ^= 1;
                    guarded[(i + 1) * 3 + 2] ^= 1;
                }
                if i + 2 < total_px {
                    guarded[(i + 2) * 3] ^= 1;
                }
                prev = [guarded[base], guarded[base + 1], guarded[base + 2]];
                run = 1;
                i += 3;
                continue;
            }
        } else {
            prev = px;
            run = 1;
        }
        i += 1;
    }
    std::borrow::Cow::Owned(guarded)
}

/// Encodes 8-bit interleaved RGB pixels (`width * height * 3`) into a lossless modular
/// JPEG XL codestream (`0xFF 0x0A` signature).
pub fn encode_jxl_rgb_u8(
    rgb_u8: &[u8],
    width: usize,
    height: usize,
    effort: u8,
) -> Result<Vec<u8>, ExportError> {
    if width <= 1 || height <= 1 {
        return Err(ExportError::EncodingError(format!(
            "JPEG XL encoder requires dimensions > 1x1, got {width}x{height}"
        )));
    }
    let expected = width.saturating_mul(height).saturating_mul(3);
    if rgb_u8.len() != expected {
        return Err(ExportError::BufferLengthMismatch {
            expected,
            actual: rgb_u8.len(),
        });
    }

    let safe_rgb = guard_constant_runs_u8(rgb_u8);
    let options = EncoderOptions::new(width, height, ColorSpace::RGB, BitDepth::Eight)
        .set_effort(effort.clamp(1, 9));
    let encoder = JxlSimpleEncoder::new(&safe_rgb, options);

    let mut out = Vec::new();
    encoder
        .encode(&mut out)
        .map_err(|e| ExportError::EncodingError(format!("JPEG XL 8-bit encode failed: {e:?}")))?;
    Ok(out)
}

/// Encodes 16-bit interleaved RGB samples (`width * height * 3`) into a lossless modular
/// JPEG XL codestream (`0xFF 0x0A` signature).
pub fn encode_jxl_rgb_u16(
    rgb_u16: &[u16],
    width: usize,
    height: usize,
    effort: u8,
) -> Result<Vec<u8>, ExportError> {
    if width <= 1 || height <= 1 {
        return Err(ExportError::EncodingError(format!(
            "JPEG XL encoder requires dimensions > 1x1, got {width}x{height}"
        )));
    }
    let expected = width.saturating_mul(height).saturating_mul(3);
    if rgb_u16.len() != expected {
        return Err(ExportError::BufferLengthMismatch {
            expected,
            actual: rgb_u16.len(),
        });
    }

    let safe_rgb = guard_constant_runs_u16(rgb_u16);
    let raw_bytes: &[u8] = bytemuck::cast_slice(&safe_rgb);
    let options = EncoderOptions::new(width, height, ColorSpace::RGB, BitDepth::Sixteen)
        .set_effort(effort.clamp(1, 9));
    let encoder = JxlSimpleEncoder::new(raw_bytes, options);

    let mut out = Vec::new();
    encoder
        .encode(&mut out)
        .map_err(|e| ExportError::EncodingError(format!("JPEG XL 16-bit encode failed: {e:?}")))?;
    Ok(out)
}

/// Encodes linear floating-point RGB pixels (`width * height * 3`, normalized to `[0.0, 1.0]`)
/// into a 16-bit lossless modular JPEG XL codestream (`0xFF 0x0A` signature).
pub fn encode_jxl_rgb16(
    rgb_linear: &[f32],
    width: usize,
    height: usize,
    effort: u8,
) -> Result<Vec<u8>, ExportError> {
    if width <= 1 || height <= 1 {
        return Err(ExportError::EncodingError(format!(
            "JPEG XL encoder requires dimensions > 1x1, got {width}x{height}"
        )));
    }
    let expected = width.saturating_mul(height).saturating_mul(3);
    if rgb_linear.len() != expected {
        return Err(ExportError::BufferLengthMismatch {
            expected,
            actual: rgb_linear.len(),
        });
    }

    let u16_samples: Vec<u16> = rgb_linear
        .iter()
        .map(|&v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
        .collect();
    encode_jxl_rgb_u16(&u16_samples, width, height, effort)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_valid_jxl_codestream_with_magic_signature() -> Result<(), ExportError> {
        let width = 16;
        let height = 16;
        let mut rgb = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                rgb.push(x as f32 / (width - 1) as f32);
                rgb.push(y as f32 / (height - 1) as f32);
                rgb.push(0.5);
            }
        }

        let bytes = encode_jxl_rgb16(&rgb, width, height, 4)?;
        assert!(bytes.len() > 16);
        // Bare JPEG XL codestream signature: 0xFF 0x0A
        assert_eq!(&bytes[0..2], &[0xFF, 0x0A]);

        let rgb_u8: Vec<u8> = rgb
            .iter()
            .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();
        let bytes_u8 = encode_jxl_rgb_u8(&rgb_u8, width, height, 4)?;
        assert!(bytes_u8.len() > 16);
        assert_eq!(&bytes_u8[0..2], &[0xFF, 0x0A]);
        Ok(())
    }

    #[test]
    fn rejects_invalid_dimensions_and_buffer_length() {
        assert!(matches!(
            encode_jxl_rgb16(&[0.0; 3], 1, 1, 4),
            Err(ExportError::EncodingError(_))
        ));
        assert!(matches!(
            encode_jxl_rgb16(&[0.0; 6], 4, 4, 4),
            Err(ExportError::BufferLengthMismatch {
                expected: 48,
                actual: 6
            })
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn pure_black_512x512_tiles_decode_cleanly_via_macos_imageio() -> Result<(), ExportError> {
        let w = 512;
        let h = 512;
        let black_u16 = vec![0_u16; w * h * 3];
        let black_u8 = vec![0_u8; w * h * 3];

        let dir = std::env::temp_dir().join(format!("rraw_jxl_sips_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        for (label, bytes) in [
            ("black16", encode_jxl_rgb_u16(&black_u16, w, h, 4)?),
            ("black8", encode_jxl_rgb_u8(&black_u8, w, h, 4)?),
        ] {
            let jxl_path = dir.join(format!("{label}.jxl"));
            let png_path = dir.join(format!("{label}.png"));
            std::fs::write(&jxl_path, &bytes)?;

            let status = std::process::Command::new("sips")
                .args(["-s", "format", "png"])
                .arg(&jxl_path)
                .arg("--out")
                .arg(&png_path)
                .output()?;
            assert!(
                status.status.success(),
                "macOS sips failed to decode {label}.jxl: {}",
                String::from_utf8_lossy(&status.stderr)
            );
            let decoded = image::open(&png_path)
                .map_err(|e| ExportError::EncodingError(e.to_string()))?
                .to_rgb8();
            assert_eq!(decoded.dimensions(), (w as u32, h as u32));
            // Every pixel must remain near-black (<= 2/255), never dropped/gray.
            for px in decoded.pixels() {
                assert!(
                    px[0] <= 2 && px[1] <= 2 && px[2] <= 2,
                    "{label}.jxl tile corrupted on decode: got RGB {:?}",
                    px.0
                );
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
