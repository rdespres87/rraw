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
    let raw_bytes: &[u8] = bytemuck::cast_slice(&u16_samples);

    let options = EncoderOptions::new(width, height, ColorSpace::RGB, BitDepth::Sixteen)
        .set_effort(effort.clamp(1, 9));
    let encoder = JxlSimpleEncoder::new(raw_bytes, options);

    let mut out = Vec::new();
    encoder
        .encode(&mut out)
        .map_err(|e| ExportError::EncodingError(format!("JPEG XL encode failed: {e:?}")))?;
    Ok(out)
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
}
