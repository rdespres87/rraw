#![forbid(unsafe_code)]

//! ISO 21496-1 HDR Gain Map calculation, reconstruction, binary `tmap`, and XMP serialization.

use crate::error::ExportError;
use byteorder::{BigEndian, ByteOrder};

/// Core ISO 21496-1 parameters for HDR Gain Map representation and reconstruction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Iso21496Params {
    pub gain_map_min: f32,
    pub gain_map_max: f32,
    pub hdr_capacity_min: f32,
    pub hdr_capacity_max: f32,
    pub offset_sdr: f32,
    pub offset_hdr: f32,
    pub gamma: f32,
}

impl Default for Iso21496Params {
    fn default() -> Self {
        Self {
            gain_map_min: 1.0,
            gain_map_max: 8.0,
            hdr_capacity_min: 1.0,
            hdr_capacity_max: 8.0,
            offset_sdr: 1.0 / 64.0,
            offset_hdr: 1.0 / 64.0,
            gamma: 1.0,
        }
    }
}

/// Generates normalized 2D gain maps from paired SDR and HDR image buffers.
pub struct GainMapCalculator;

impl GainMapCalculator {
    /// Computes a normalized single-channel Gain Map `[0.0, 1.0]` from SDR and HDR inputs.
    pub fn compute(
        sdr: &[f32],
        hdr: &[f32],
        width: usize,
        height: usize,
        params: &Iso21496Params,
    ) -> Result<Vec<f32>, ExportError> {
        let count = width.saturating_mul(height);
        let expected = count.saturating_mul(3);
        if sdr.len() != expected || hdr.len() != expected {
            return Err(ExportError::BufferLengthMismatch {
                expected,
                actual: sdr.len(),
            });
        }

        let min_log2 = params.gain_map_min.log2();
        let max_log2 = params.gain_map_max.log2();
        let span_log2 = (max_log2 - min_log2).max(1e-12);

        let mut gain_map = Vec::with_capacity(count);

        for i in 0..count {
            let idx = i * 3;
            let y_sdr = 0.2722287 * sdr[idx] + 0.6740818 * sdr[idx + 1] + 0.0536895 * sdr[idx + 2];
            let y_hdr = 0.2722287 * hdr[idx] + 0.6740818 * hdr[idx + 1] + 0.0536895 * hdr[idx + 2];

            let ratio = (y_hdr + params.offset_hdr) / (y_sdr + params.offset_sdr).max(1e-12);
            let log2_ratio = ratio.max(1e-12).log2();
            let d = ((log2_ratio - min_log2) / span_log2).clamp(0.0, 1.0);
            gain_map.push(d);
        }

        Ok(gain_map)
    }
}

/// Reconstructs HDR images from an SDR base image and Gain Map for any display headroom.
pub struct GainMapReconstructor;

impl GainMapReconstructor {
    /// Reconstructs linear HDR highlights adapted to the active display headroom $H$.
    pub fn reconstruct(
        sdr: &[f32],
        gain_map: &[f32],
        width: usize,
        height: usize,
        params: &Iso21496Params,
        active_headroom: f32,
    ) -> Result<Vec<f32>, ExportError> {
        let count = width.saturating_mul(height);
        let expected = count.saturating_mul(3);
        if sdr.len() != expected || gain_map.len() != count {
            return Err(ExportError::BufferLengthMismatch {
                expected,
                actual: sdr.len(),
            });
        }

        let cap_min_log2 = params.hdr_capacity_min.log2();
        let cap_max_log2 = params.hdr_capacity_max.log2();
        let cap_span = (cap_max_log2 - cap_min_log2).max(1e-12);

        let h_log2 = active_headroom.max(1.0).log2();
        let weight = ((h_log2 - cap_min_log2) / cap_span).clamp(0.0, 1.0);

        let min_log2 = params.gain_map_min.log2();
        let max_log2 = params.gain_map_max.log2();
        let span_log2 = (max_log2 - min_log2).max(1e-12);

        let mut recon = Vec::with_capacity(expected);

        for (i, &d) in gain_map.iter().enumerate().take(count) {
            let log2_boost = (min_log2 + d * span_log2) * weight;
            let boost = 2.0_f32.powf(log2_boost);

            let idx = i * 3;
            for c in 0..3 {
                let v_sdr = sdr[idx + c];
                let val = (v_sdr + params.offset_sdr) * boost - params.offset_hdr;
                recon.push(val.max(0.0));
            }
        }

        Ok(recon)
    }
}

/// Binary ISOBMFF `tmap` box serializer and parser compliant with ISO 21496-1.
pub struct Iso21496TmapBox;

impl Iso21496TmapBox {
    /// Total serialized size: 8-byte box header + 4-byte version/flags + 7 * 4-byte f32 = 40 bytes.
    pub const BOX_SIZE: u32 = 40;

    /// Serializes `Iso21496Params` into a standardized binary ISOBMFF `tmap` box.
    pub fn encode(params: &Iso21496Params) -> Result<Vec<u8>, ExportError> {
        let mut buf = vec![0_u8; Self::BOX_SIZE as usize];
        BigEndian::write_u32(&mut buf[0..4], Self::BOX_SIZE);
        buf[4..8].copy_from_slice(b"tmap");
        BigEndian::write_u32(&mut buf[8..12], 0); // version = 0, flags = 0
        BigEndian::write_f32(&mut buf[12..16], params.gain_map_min);
        BigEndian::write_f32(&mut buf[16..20], params.gain_map_max);
        BigEndian::write_f32(&mut buf[20..24], params.hdr_capacity_min);
        BigEndian::write_f32(&mut buf[24..28], params.hdr_capacity_max);
        BigEndian::write_f32(&mut buf[28..32], params.offset_sdr);
        BigEndian::write_f32(&mut buf[32..36], params.offset_hdr);
        BigEndian::write_f32(&mut buf[36..40], params.gamma);
        Ok(buf)
    }

    /// Deserializes binary ISOBMFF `tmap` box into `Iso21496Params`.
    pub fn decode(bytes: &[u8]) -> Result<Iso21496Params, ExportError> {
        if bytes.len() < Self::BOX_SIZE as usize {
            return Err(ExportError::TmapDecodeFailed(format!(
                "Payload too small: expected {}, got {}",
                Self::BOX_SIZE,
                bytes.len()
            )));
        }
        if &bytes[4..8] != b"tmap" {
            return Err(ExportError::TmapDecodeFailed(format!(
                "Invalid FourCC: expected 'tmap', got {:?}",
                &bytes[4..8]
            )));
        }

        let gain_map_min = BigEndian::read_f32(&bytes[12..16]);
        let gain_map_max = BigEndian::read_f32(&bytes[16..20]);
        let hdr_capacity_min = BigEndian::read_f32(&bytes[20..24]);
        let hdr_capacity_max = BigEndian::read_f32(&bytes[24..28]);
        let offset_sdr = BigEndian::read_f32(&bytes[28..32]);
        let offset_hdr = BigEndian::read_f32(&bytes[32..36]);
        let gamma = BigEndian::read_f32(&bytes[36..40]);

        Ok(Iso21496Params {
            gain_map_min,
            gain_map_max,
            hdr_capacity_min,
            hdr_capacity_max,
            offset_sdr,
            offset_hdr,
            gamma,
        })
    }
}

/// XMP `hdrgm:*` metadata packet serializer and parser.
pub struct Iso21496XmpPacket;

impl Iso21496XmpPacket {
    /// Serializes parameters into an XMP packet with Apple / ISO HDR Gain Map namespace.
    #[must_use]
    pub fn encode(params: &Iso21496Params) -> String {
        format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:hdrgm="http://ns.apple.com/HDRGainMap/1.0/">
   <hdrgm:Version>1.0</hdrgm:Version>
   <hdrgm:GainMapMin>{:.6}</hdrgm:GainMapMin>
   <hdrgm:GainMapMax>{:.6}</hdrgm:GainMapMax>
   <hdrgm:HDRCapacityMin>{:.6}</hdrgm:HDRCapacityMin>
   <hdrgm:HDRCapacityMax>{:.6}</hdrgm:HDRCapacityMax>
   <hdrgm:OffsetSDR>{:.6}</hdrgm:OffsetSDR>
   <hdrgm:OffsetHDR>{:.6}</hdrgm:OffsetHDR>
   <hdrgm:Gamma>{:.6}</hdrgm:Gamma>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#,
            params.gain_map_min,
            params.gain_map_max,
            params.hdr_capacity_min,
            params.hdr_capacity_max,
            params.offset_sdr,
            params.offset_hdr,
            params.gamma
        )
    }

    /// Parses an XMP packet containing `hdrgm:*` elements.
    pub fn decode(xml: &str) -> Result<Iso21496Params, ExportError> {
        let extract_tag = |tag: &str| -> Option<f32> {
            let open = format!("<{tag}>");
            let close = format!("</{tag}>");
            let start = xml.find(&open)? + open.len();
            let end = xml[start..].find(&close)? + start;
            xml[start..end].trim().parse::<f32>().ok()
        };

        let gain_map_min = extract_tag("hdrgm:GainMapMin")
            .ok_or_else(|| ExportError::XmpDecodeFailed("Missing GainMapMin".to_owned()))?;
        let gain_map_max = extract_tag("hdrgm:GainMapMax")
            .ok_or_else(|| ExportError::XmpDecodeFailed("Missing GainMapMax".to_owned()))?;
        let hdr_capacity_min = extract_tag("hdrgm:HDRCapacityMin")
            .ok_or_else(|| ExportError::XmpDecodeFailed("Missing HDRCapacityMin".to_owned()))?;
        let hdr_capacity_max = extract_tag("hdrgm:HDRCapacityMax")
            .ok_or_else(|| ExportError::XmpDecodeFailed("Missing HDRCapacityMax".to_owned()))?;
        let offset_sdr = extract_tag("hdrgm:OffsetSDR").unwrap_or(0.015625);
        let offset_hdr = extract_tag("hdrgm:OffsetHDR").unwrap_or(0.015625);
        let gamma = extract_tag("hdrgm:Gamma").unwrap_or(1.0);

        Ok(Iso21496Params {
            gain_map_min,
            gain_map_max,
            hdr_capacity_min,
            hdr_capacity_max,
            offset_sdr,
            offset_hdr,
            gamma,
        })
    }
}
