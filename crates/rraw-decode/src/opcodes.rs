//! Pure-Rust parser for binary DNG OpcodeList streams (DNG Specification 1.3–1.7).

use byteorder::{BigEndian, ByteOrder};

use crate::DecodeError;

/// Supported DNG opcodes extracted from binary `OpcodeList1`, `OpcodeList2`, or `OpcodeList3`.
#[derive(Debug, Clone, PartialEq)]
pub enum DngOpcode {
    /// Opcode ID 1: Corrects geometric radial distortion and transverse chromatic aberration.
    WarpRectilinear {
        dng_version: u32,
        flags: u32,
        planes: u32,
        coefficients: Vec<f64>,
    },
    /// Opcode ID 2: Corrects fisheye distortion.
    WarpFisheye {
        dng_version: u32,
        flags: u32,
        planes: u32,
        coefficients: Vec<f64>,
    },
    /// Opcode ID 3: Radial polynomial vignette correction.
    FixVignetteRadial {
        dng_version: u32,
        flags: u32,
        k0: f64,
        k1: f64,
        k2: f64,
        k3: f64,
        k4: f64,
        center_x: f64,
        center_y: f64,
    },
    /// Opcode ID 9: 2D spatially-varying CFA gain map (shading & lens falloff correction).
    GainMap {
        dng_version: u32,
        flags: u32,
        top: f64,
        left: f64,
        bottom: f64,
        right: f64,
        plane: u32,
        planes: u32,
        row_pitch: u32,
        col_pitch: u32,
        map_points_v: u32,
        map_points_h: u32,
        map_gain: Vec<f32>,
    },
    /// Unrecognized or passthrough DNG opcode.
    Unknown {
        opcode_id: u32,
        dng_version: u32,
        flags: u32,
        data: Vec<u8>,
    },
}

/// Parses a big-endian binary DNG OpcodeList stream into strongly-typed `DngOpcode` variants.
pub fn parse_opcode_list(bytes: &[u8]) -> Result<Vec<DngOpcode>, DecodeError> {
    if bytes.len() < 4 {
        return Err(DecodeError::MalformedMetadata(format!(
            "DNG OpcodeList stream too short: expected at least 4 bytes, got {}",
            bytes.len()
        )));
    }

    let count = BigEndian::read_u32(&bytes[0..4]) as usize;
    let mut offset = 4;
    let mut opcodes = Vec::with_capacity(count);

    for idx in 0..count {
        if offset + 16 > bytes.len() {
            return Err(DecodeError::MalformedMetadata(format!(
                "unexpected EOF reading opcode header #{idx} at offset {offset}"
            )));
        }

        let opcode_id = BigEndian::read_u32(&bytes[offset..offset + 4]);
        let dng_version = BigEndian::read_u32(&bytes[offset + 4..offset + 8]);
        let flags = BigEndian::read_u32(&bytes[offset + 8..offset + 12]);
        let size = BigEndian::read_u32(&bytes[offset + 12..offset + 16]) as usize;
        offset += 16;

        if offset + size > bytes.len() {
            return Err(DecodeError::MalformedMetadata(format!(
                "opcode #{idx} payload overflow: size {size} exceeds remaining buffer {} at offset {offset}",
                bytes.len() - offset
            )));
        }

        let payload = &bytes[offset..offset + size];
        offset += size;

        let parsed = match opcode_id {
            1 => parse_warp_rectilinear(dng_version, flags, payload)?,
            2 => parse_warp_fisheye(dng_version, flags, payload)?,
            3 => parse_fix_vignette_radial(dng_version, flags, payload)?,
            9 => parse_gain_map(dng_version, flags, payload)?,
            _ => DngOpcode::Unknown {
                opcode_id,
                dng_version,
                flags,
                data: payload.to_vec(),
            },
        };

        opcodes.push(parsed);
    }

    Ok(opcodes)
}

fn parse_warp_rectilinear(
    dng_version: u32,
    flags: u32,
    payload: &[u8],
) -> Result<DngOpcode, DecodeError> {
    if payload.len() < 4 {
        return Err(DecodeError::MalformedMetadata(
            "WarpRectilinear payload too short for planes header".to_string(),
        ));
    }
    let planes = BigEndian::read_u32(&payload[0..4]);
    let mut offset = 4;
    let coeff_count = (payload.len() - offset) / 8;
    let mut coefficients = Vec::with_capacity(coeff_count);

    while offset + 8 <= payload.len() {
        coefficients.push(BigEndian::read_f64(&payload[offset..offset + 8]));
        offset += 8;
    }

    Ok(DngOpcode::WarpRectilinear {
        dng_version,
        flags,
        planes,
        coefficients,
    })
}

fn parse_warp_fisheye(
    dng_version: u32,
    flags: u32,
    payload: &[u8],
) -> Result<DngOpcode, DecodeError> {
    if payload.len() < 4 {
        return Err(DecodeError::MalformedMetadata(
            "WarpFisheye payload too short for planes header".to_string(),
        ));
    }
    let planes = BigEndian::read_u32(&payload[0..4]);
    let mut offset = 4;
    let coeff_count = (payload.len() - offset) / 8;
    let mut coefficients = Vec::with_capacity(coeff_count);

    while offset + 8 <= payload.len() {
        coefficients.push(BigEndian::read_f64(&payload[offset..offset + 8]));
        offset += 8;
    }

    Ok(DngOpcode::WarpFisheye {
        dng_version,
        flags,
        planes,
        coefficients,
    })
}

fn parse_fix_vignette_radial(
    dng_version: u32,
    flags: u32,
    payload: &[u8],
) -> Result<DngOpcode, DecodeError> {
    // 5 polynomial terms k0..k4 + center_x + center_y = 7 * f64 = 56 bytes
    if payload.len() < 56 {
        return Err(DecodeError::MalformedMetadata(format!(
            "FixVignetteRadial payload too short: expected 56 bytes, got {}",
            payload.len()
        )));
    }

    let k0 = BigEndian::read_f64(&payload[0..8]);
    let k1 = BigEndian::read_f64(&payload[8..16]);
    let k2 = BigEndian::read_f64(&payload[16..24]);
    let k3 = BigEndian::read_f64(&payload[24..32]);
    let k4 = BigEndian::read_f64(&payload[32..40]);
    let center_x = BigEndian::read_f64(&payload[40..48]);
    let center_y = BigEndian::read_f64(&payload[48..56]);

    Ok(DngOpcode::FixVignetteRadial {
        dng_version,
        flags,
        k0,
        k1,
        k2,
        k3,
        k4,
        center_x,
        center_y,
    })
}

fn parse_gain_map(dng_version: u32, flags: u32, payload: &[u8]) -> Result<DngOpcode, DecodeError> {
    // top (8), left (8), bottom (8), right (8), plane (4), planes (4),
    // row_pitch (4), col_pitch (4), map_points_v (4), map_points_h (4) = 56 bytes header
    if payload.len() < 56 {
        return Err(DecodeError::MalformedMetadata(format!(
            "GainMap payload too short for header: expected 56 bytes, got {}",
            payload.len()
        )));
    }

    let top = BigEndian::read_f64(&payload[0..8]);
    let left = BigEndian::read_f64(&payload[8..16]);
    let bottom = BigEndian::read_f64(&payload[16..24]);
    let right = BigEndian::read_f64(&payload[24..32]);

    let plane = BigEndian::read_u32(&payload[32..36]);
    let planes = BigEndian::read_u32(&payload[36..40]);
    let row_pitch = BigEndian::read_u32(&payload[40..44]);
    let col_pitch = BigEndian::read_u32(&payload[44..48]);
    let map_points_v = BigEndian::read_u32(&payload[48..52]);
    let map_points_h = BigEndian::read_u32(&payload[52..56]);

    let total_points = (map_points_v as usize)
        .checked_mul(map_points_h as usize)
        .ok_or_else(|| {
            DecodeError::MalformedMetadata("gain map grid dimension overflow".to_string())
        })?;

    let expected_data_bytes = total_points.checked_mul(4).ok_or_else(|| {
        DecodeError::MalformedMetadata("gain map byte length overflow".to_string())
    })?;

    if payload.len() < 56 + expected_data_bytes {
        return Err(DecodeError::MalformedMetadata(format!(
            "GainMap payload too short: expected {} bytes, got {}",
            56 + expected_data_bytes,
            payload.len()
        )));
    }

    let mut map_gain = Vec::with_capacity(total_points);
    let mut offset = 56;
    for _ in 0..total_points {
        map_gain.push(BigEndian::read_f32(&payload[offset..offset + 4]));
        offset += 4;
    }

    Ok(DngOpcode::GainMap {
        dng_version,
        flags,
        top,
        left,
        bottom,
        right,
        plane,
        planes,
        row_pitch,
        col_pitch,
        map_points_v,
        map_points_h,
        map_gain,
    })
}

/// Scans raw source bytes for TIFF/DNG `OpcodeList1`, `OpcodeList2`, or `OpcodeList3` tags
/// and returns all successfully decoded opcodes.
#[must_use]
pub fn extract_dng_opcodes(raw_bytes: &[u8]) -> Vec<DngOpcode> {
    let mut results = Vec::new();
    if raw_bytes.len() < 8 {
        return results;
    }

    let is_little = &raw_bytes[0..2] == b"II";
    let is_big = &raw_bytes[0..2] == b"MM";
    if !is_little && !is_big {
        return results;
    }

    let read_u16 = |buf: &[u8], off: usize| -> Option<u16> {
        if off + 2 <= buf.len() {
            if is_little {
                Some(u16::from_le_bytes([buf[off], buf[off + 1]]))
            } else {
                Some(u16::from_be_bytes([buf[off], buf[off + 1]]))
            }
        } else {
            None
        }
    };

    let read_u32 = |buf: &[u8], off: usize| -> Option<u32> {
        if off + 4 <= buf.len() {
            if is_little {
                Some(u32::from_le_bytes([
                    buf[off],
                    buf[off + 1],
                    buf[off + 2],
                    buf[off + 3],
                ]))
            } else {
                Some(u32::from_be_bytes([
                    buf[off],
                    buf[off + 1],
                    buf[off + 2],
                    buf[off + 3],
                ]))
            }
        } else {
            None
        }
    };

    let mut ifd_offset = read_u32(raw_bytes, 4).unwrap_or(0) as usize;
    let mut visited = std::collections::HashSet::new();

    while ifd_offset > 0 && ifd_offset + 2 <= raw_bytes.len() && visited.insert(ifd_offset) {
        let num_entries = read_u16(raw_bytes, ifd_offset).unwrap_or(0) as usize;
        let mut sub_ifds = Vec::new();

        for i in 0..num_entries {
            let entry_off = ifd_offset + 2 + i * 12;
            if entry_off + 12 > raw_bytes.len() {
                break;
            }
            let tag = read_u16(raw_bytes, entry_off).unwrap_or(0);
            let count = read_u32(raw_bytes, entry_off + 4).unwrap_or(0) as usize;
            let val_or_offset = read_u32(raw_bytes, entry_off + 8).unwrap_or(0) as usize;

            // SubIFDs tag 0x014A
            if tag == 0x014A {
                sub_ifds.push(val_or_offset);
            }

            // OpcodeList1 (0xC685), OpcodeList2 (0xC686), OpcodeList3 (0xC687)
            if (tag == 0xC685 || tag == 0xC686 || tag == 0xC687) && count > 0 {
                let stream_off = if count <= 4 {
                    entry_off + 8
                } else {
                    val_or_offset
                };
                if stream_off + count <= raw_bytes.len() {
                    if let Ok(mut ops) =
                        parse_opcode_list(&raw_bytes[stream_off..stream_off + count])
                    {
                        results.append(&mut ops);
                    }
                }
            }
        }

        for sub_off in sub_ifds {
            if sub_off > 0 && sub_off + 2 <= raw_bytes.len() && visited.insert(sub_off) {
                let sub_entries = read_u16(raw_bytes, sub_off).unwrap_or(0) as usize;
                for i in 0..sub_entries {
                    let entry_off = sub_off + 2 + i * 12;
                    if entry_off + 12 > raw_bytes.len() {
                        break;
                    }
                    let tag = read_u16(raw_bytes, entry_off).unwrap_or(0);
                    let count = read_u32(raw_bytes, entry_off + 4).unwrap_or(0) as usize;
                    let val_or_offset = read_u32(raw_bytes, entry_off + 8).unwrap_or(0) as usize;

                    if (tag == 0xC685 || tag == 0xC686 || tag == 0xC687) && count > 0 {
                        let stream_off = if count <= 4 {
                            entry_off + 8
                        } else {
                            val_or_offset
                        };
                        if stream_off + count <= raw_bytes.len() {
                            if let Ok(mut ops) =
                                parse_opcode_list(&raw_bytes[stream_off..stream_off + count])
                            {
                                results.append(&mut ops);
                            }
                        }
                    }
                }
            }
        }

        let next_ifd_pos = ifd_offset + 2 + num_entries * 12;
        ifd_offset = read_u32(raw_bytes, next_ifd_pos).unwrap_or(0) as usize;
    }

    results
}

/// Helper that extracts `WarpRectilinear` parameters from raw DNG source bytes.
#[must_use]
pub fn extract_dng_warp_rectilinear(raw_bytes: &[u8]) -> Option<rraw_core::DngWarpRectilinearData> {
    let opcodes = extract_dng_opcodes(raw_bytes);
    for op in opcodes {
        if let DngOpcode::WarpRectilinear {
            planes,
            coefficients,
            ..
        } = op
        {
            return Some(rraw_core::DngWarpRectilinearData {
                planes,
                coefficients,
            });
        }
    }
    None
}
