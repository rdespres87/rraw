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

#[cfg(test)]
mod tests {
    use super::*;

    fn push_be_u32(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_be_bytes());
    }

    fn push_be_f64(buf: &mut Vec<u8>, v: f64) {
        buf.extend_from_slice(&v.to_be_bytes());
    }

    fn push_be_f32(buf: &mut Vec<u8>, v: f32) {
        buf.extend_from_slice(&v.to_be_bytes());
    }

    fn wrap_opcode_list(entries: &[(u32, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        push_be_u32(&mut out, entries.len() as u32);
        for &(id, payload) in entries {
            push_be_u32(&mut out, id);
            push_be_u32(&mut out, 0x0103_0000);
            push_be_u32(&mut out, 0);
            push_be_u32(&mut out, payload.len() as u32);
            out.extend_from_slice(payload);
        }
        out
    }

    fn build_tiff_with_opcode(
        is_little: bool,
        use_subifd: bool,
        tag: u16,
        opcode_stream: &[u8],
    ) -> Vec<u8> {
        let w16 = |buf: &mut Vec<u8>, v: u16| {
            if is_little {
                buf.extend_from_slice(&v.to_le_bytes());
            } else {
                buf.extend_from_slice(&v.to_be_bytes());
            }
        };
        let w32 = |buf: &mut Vec<u8>, v: u32| {
            if is_little {
                buf.extend_from_slice(&v.to_le_bytes());
            } else {
                buf.extend_from_slice(&v.to_be_bytes());
            }
        };

        let mut buf = Vec::new();
        buf.extend_from_slice(if is_little { b"II" } else { b"MM" });
        w16(&mut buf, 42);
        w32(&mut buf, 8); // IFD0 offset

        if use_subifd {
            // IFD0: 1 entry (SubIFDs = 0x014A) pointing to offset 26
            w16(&mut buf, 1);
            w16(&mut buf, 0x014A);
            w16(&mut buf, 4); // LONG
            w32(&mut buf, 1);
            w32(&mut buf, 26); // SubIFD offset
            w32(&mut buf, 0); // next IFD
            debug_assert_eq!(buf.len(), 26);
            // SubIFD: 1 entry (tag) pointing to offset 44
            w16(&mut buf, 1);
            w16(&mut buf, tag);
            w16(&mut buf, 7); // UNDEFINED
            w32(&mut buf, opcode_stream.len() as u32);
            w32(&mut buf, 44);
            w32(&mut buf, 0);
            debug_assert_eq!(buf.len(), 44);
            buf.extend_from_slice(opcode_stream);
        } else {
            // IFD0: 1 entry (tag) pointing to offset 26
            w16(&mut buf, 1);
            w16(&mut buf, tag);
            w16(&mut buf, 7); // UNDEFINED
            w32(&mut buf, opcode_stream.len() as u32);
            w32(&mut buf, 26);
            w32(&mut buf, 0);
            debug_assert_eq!(buf.len(), 26);
            buf.extend_from_slice(opcode_stream);
        }
        buf
    }

    #[test]
    fn parse_opcode_list_covers_all_variants_and_error_paths() -> Result<(), DecodeError> {
        // Stream too short (< 4)
        assert!(parse_opcode_list(&[0, 0]).is_err());
        // Truncated header (count = 1, < 20 bytes)
        assert!(parse_opcode_list(&[0, 0, 0, 1, 0, 0, 0, 1]).is_err());
        // Payload overflow (declared size = 100, 0 bytes remaining)
        let mut overflow = Vec::new();
        push_be_u32(&mut overflow, 1);
        push_be_u32(&mut overflow, 1);
        push_be_u32(&mut overflow, 0);
        push_be_u32(&mut overflow, 0);
        push_be_u32(&mut overflow, 100);
        assert!(parse_opcode_list(&overflow).is_err());

        // Short payloads for IDs 1, 2, 3, 9
        assert!(parse_opcode_list(&wrap_opcode_list(&[(1, &[0, 1])])).is_err());
        assert!(parse_opcode_list(&wrap_opcode_list(&[(2, &[0, 1])])).is_err());
        assert!(parse_opcode_list(&wrap_opcode_list(&[(3, &[0; 16])])).is_err());
        assert!(parse_opcode_list(&wrap_opcode_list(&[(9, &[0; 16])])).is_err());

        // GainMap grid overflow & short gain payload
        let mut gm_overflow = vec![0u8; 56];
        gm_overflow[48..52].copy_from_slice(&u32::MAX.to_be_bytes());
        gm_overflow[52..56].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_opcode_list(&wrap_opcode_list(&[(9, &gm_overflow)])).is_err());

        let mut gm_short = vec![0u8; 56];
        gm_short[48..52].copy_from_slice(&2u32.to_be_bytes());
        gm_short[52..56].copy_from_slice(&2u32.to_be_bytes());
        assert!(parse_opcode_list(&wrap_opcode_list(&[(9, &gm_short)])).is_err());

        // Valid WarpRectilinear (1), WarpFisheye (2), FixVignetteRadial (3), GainMap (9), Unknown (99)
        let mut warp = Vec::new();
        push_be_u32(&mut warp, 1);
        push_be_f64(&mut warp, 1.0);
        push_be_f64(&mut warp, -0.02);

        let mut vig = Vec::new();
        for v in [0.1, -0.05, 0.01, 0.0, 0.0, 0.5, 0.5] {
            push_be_f64(&mut vig, v);
        }

        let mut gm = Vec::new();
        for v in [0.0, 0.0, 1.0, 1.0] {
            push_be_f64(&mut gm, v);
        }
        for v in [0u32, 1, 1, 1, 2, 2] {
            push_be_u32(&mut gm, v);
        }
        for v in [1.0f32, 1.05, 1.05, 1.1] {
            push_be_f32(&mut gm, v);
        }

        let stream = wrap_opcode_list(&[
            (1, &warp),
            (2, &warp),
            (3, &vig),
            (9, &gm),
            (99, &[0xDE, 0xAD]),
        ]);
        let ops = parse_opcode_list(&stream)?;
        assert_eq!(ops.len(), 5);
        assert!(matches!(
            ops[0],
            DngOpcode::WarpRectilinear { planes: 1, .. }
        ));
        assert!(matches!(ops[1], DngOpcode::WarpFisheye { planes: 1, .. }));
        assert!(matches!(ops[2], DngOpcode::FixVignetteRadial { .. }));
        assert!(matches!(
            ops[3],
            DngOpcode::GainMap {
                map_points_v: 2,
                map_points_h: 2,
                ..
            }
        ));
        assert!(matches!(ops[4], DngOpcode::Unknown { opcode_id: 99, .. }));
        Ok(())
    }

    #[test]
    fn extract_dng_opcodes_handles_le_be_subifds_and_invalid_headers() {
        assert!(extract_dng_opcodes(&[0, 1, 2]).is_empty());
        assert!(extract_dng_opcodes(b"XX000000").is_empty());
        assert!(extract_dng_warp_rectilinear(b"XX000000").is_none());

        // Little-Endian IFD0 with OpcodeList1 (0xC685) containing WarpRectilinear
        let mut warp = Vec::new();
        push_be_u32(&mut warp, 1);
        push_be_f64(&mut warp, 1.0);
        push_be_f64(&mut warp, -0.01);
        let warp_stream = wrap_opcode_list(&[(1, &warp)]);
        let le_tiff = build_tiff_with_opcode(true, false, 0xC685, &warp_stream);
        let extracted = extract_dng_warp_rectilinear(&le_tiff);
        assert!(extracted.is_some());

        // Big-Endian SubIFD with OpcodeList2 (0xC686) containing FixVignetteRadial
        let mut vig = Vec::new();
        for v in [0.1, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5] {
            push_be_f64(&mut vig, v);
        }
        let vig_stream = wrap_opcode_list(&[(3, &vig)]);
        let be_tiff = build_tiff_with_opcode(false, true, 0xC686, &vig_stream);
        let ops = extract_dng_opcodes(&be_tiff);
        assert!(matches!(
            ops.first(),
            Some(DngOpcode::FixVignetteRadial { .. })
        ));
    }
}
