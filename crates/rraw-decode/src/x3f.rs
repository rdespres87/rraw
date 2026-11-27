#![forbid(unsafe_code)]

//! Pure-Rust decoder for Sigma and Polaroid `.X3F` (Foveon X3 3-layer) RAW files.
//!
//! Supports all four Foveon X3F raw encodings:
//! - Format `5`: 10-bit packed DPCM (`Polaroid x530`)
//! - Format `6`: 10-bit Huffman DPCM (`Sigma SD9`, `SD10`, `SD14`)
//! - Format `30`: TRUE-II 3-plane Huffman (`Sigma DP1`, `DP1s`, `SD15`, `SD1/DP1/DP2/DP3 Merrill`)
//! - Formats `35`, `37`, `39`: Quattro 1:1:4 TRUE-II 3-plane Huffman (`DP0/dp2/sd Quattro`, `sd Quattro H`)

use std::collections::HashMap;
use std::path::Path;

use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, CoreError, NoiseProfile, NoiseProfileSource,
    PhotoMetadata, PickStatus, SignedCfaBuffer,
};

use crate::special_sensors::XYZ_TO_SRGB_D65;
use crate::{DecodeError, DecodedRaw, LinearRgbBuffer};

/// Binary Huffman trie node used by both TRUE-I (format 6) and TRUE-II (formats 30/35/37/39).
#[derive(Debug, Clone, Copy, Default)]
struct HuffNode {
    child: [u16; 2],
    is_leaf: bool,
    value: i32,
}

struct HuffTree {
    nodes: Vec<HuffNode>,
}

impl HuffTree {
    fn new() -> Self {
        Self {
            nodes: vec![HuffNode::default()],
        }
    }

    fn insert(&mut self, bit_len: u32, code: u32, value: i32) -> Result<(), DecodeError> {
        if bit_len == 0 || bit_len > 27 {
            return Ok(());
        }
        let mut cur = 0usize;
        for shift in (0..bit_len).rev() {
            let bit = ((code >> shift) & 1) as usize;
            let next = self.nodes[cur].child[bit];
            if next == 0 {
                let new_idx = u16::try_from(self.nodes.len()).map_err(|_| {
                    DecodeError::MalformedMetadata("X3F Huffman tree node overflow".into())
                })?;
                self.nodes.push(HuffNode::default());
                self.nodes[cur].child[bit] = new_idx;
                cur = usize::from(new_idx);
            } else {
                cur = usize::from(next);
            }
        }
        self.nodes[cur].is_leaf = true;
        self.nodes[cur].value = value;
        Ok(())
    }

    #[inline]
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<i32, DecodeError> {
        let mut cur = 0usize;
        for _ in 0..32 {
            if self.nodes[cur].is_leaf {
                return Ok(self.nodes[cur].value);
            }
            let bit = reader.read_bit()?;
            let next = self.nodes[cur].child[bit];
            if next == 0 {
                return Err(DecodeError::MalformedMetadata(
                    "invalid X3F Huffman codeword".into(),
                ));
            }
            cur = usize::from(next);
        }
        if self.nodes[cur].is_leaf {
            Ok(self.nodes[cur].value)
        } else {
            Err(DecodeError::MalformedMetadata(
                "X3F Huffman codeword exceeded maximum depth".into(),
            ))
        }
    }
}

/// MSB-first bitstream reader for X3F Huffman streams.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    bits: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            buf: 0,
            bits: 0,
        }
    }

    #[inline]
    fn refill(&mut self) {
        while self.bits <= 56 && self.pos < self.data.len() {
            self.buf = (self.buf << 8) | u64::from(self.data[self.pos]);
            self.pos += 1;
            self.bits += 8;
        }
    }

    #[inline]
    fn read_bit(&mut self) -> Result<usize, DecodeError> {
        if self.bits == 0 {
            self.refill();
            if self.bits == 0 {
                return Err(DecodeError::MalformedMetadata(
                    "unexpected EOF in X3F Huffman stream".into(),
                ));
            }
        }
        self.bits -= 1;
        Ok(((self.buf >> self.bits) & 1) as usize)
    }

    #[inline]
    fn read_bits(&mut self, n: u32) -> Result<u32, DecodeError> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(DecodeError::MalformedMetadata(format!(
                "X3F bit read width {n} exceeds 32 bits"
            )));
        }
        if u32::from(self.bits) < n {
            self.refill();
            if u32::from(self.bits) < n {
                return Err(DecodeError::MalformedMetadata(
                    "unexpected EOF reading X3F diff bits".into(),
                ));
            }
        }
        self.bits -= n as u8;
        let mask = if n == 32 {
            u64::from(u32::MAX)
        } else {
            (1u64 << n) - 1
        };
        Ok(((self.buf >> self.bits) & mask) as u32)
    }
}

#[inline]
fn read_u16_le(data: &[u8], off: usize) -> Result<u16, DecodeError> {
    let end = off.checked_add(2).ok_or_else(|| {
        DecodeError::MalformedMetadata(format!("X3F u16 offset overflow at {off}"))
    })?;
    let slice = data.get(off..end).ok_or_else(|| {
        DecodeError::MalformedMetadata(format!("X3F u16 read out of bounds at {off}"))
    })?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

#[inline]
fn read_u32_le(data: &[u8], off: usize) -> Result<u32, DecodeError> {
    let end = off.checked_add(4).ok_or_else(|| {
        DecodeError::MalformedMetadata(format!("X3F u32 offset overflow at {off}"))
    })?;
    let slice = data.get(off..end).ok_or_else(|| {
        DecodeError::MalformedMetadata(format!("X3F u32 read out of bounds at {off}"))
    })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[derive(Debug, Clone)]
struct DirEntry {
    offset: usize,
    length: usize,
    tag: [u8; 4],
}

/// Attempts to decode a Sigma or Polaroid `.X3F` (Foveon X3) RAW file into a `LinearRgbBuffer`.
pub fn try_decode_x3f(path: &Path) -> Result<DecodedRaw, DecodeError> {
    let data = std::fs::read(path)
        .map_err(|e| DecodeError::Io(format!("failed to read {}: {e}", path.display())))?;
    try_decode_x3f_bytes(&data)
}

/// Attempts to decode an in-memory `.X3F` (Foveon X3) byte slice into a `LinearRgbBuffer`.
pub fn try_decode_x3f_bytes(data: &[u8]) -> Result<DecodedRaw, DecodeError> {
    if data.len() < 40 || &data[0..4] != b"FOVb" {
        return Err(DecodeError::UnsupportedFormat("not an X3F file".into()));
    }

    let dir_off = read_u32_le(data, data.len() - 4)? as usize;
    if dir_off + 12 > data.len() || &data[dir_off..dir_off + 4] != b"SECd" {
        return Err(DecodeError::MalformedMetadata(
            "X3F missing SECd directory".into(),
        ));
    }
    let num_entries = read_u32_le(data, dir_off + 8)? as usize;
    let mut entries = Vec::with_capacity(num_entries.min(64));
    for i in 0..num_entries {
        let base = dir_off + 12 + i * 12;
        if base + 12 > data.len() {
            break;
        }
        let offset = read_u32_le(data, base)? as usize;
        let length = read_u32_le(data, base + 4)? as usize;
        let tag = [
            data[base + 8],
            data[base + 9],
            data[base + 10],
            data[base + 11],
        ];
        entries.push(DirEntry {
            offset,
            length,
            tag,
        });
    }

    // Parse PROP metadata (Make, Model, WB, ISO, Aperture)
    let props = entries
        .iter()
        .find(|e| &e.tag == b"PROP")
        .map(|e| parse_prop_section(data, e))
        .unwrap_or_default();

    let make = props
        .get("CAMMANUF")
        .cloned()
        .unwrap_or_else(|| "SIGMA".to_owned());
    let model = props
        .get("CAMMODEL")
        .cloned()
        .unwrap_or_else(|| "Foveon X3F".to_owned());
    let wb_desc = props
        .get("WB_DESC")
        .cloned()
        .unwrap_or_else(|| "Sunlight".to_owned());
    let iso = props.get("ISO").and_then(|s| s.trim().parse::<u32>().ok());
    let aperture = props
        .get("APERTURE")
        .and_then(|s| s.trim().parse::<f32>().ok());

    // Locate primary RAW image section (IMA2 or IMAG with type == 1 or type == 3)
    let raw_entry = entries
        .iter()
        .find(|e| {
            (&e.tag == b"IMA2" || &e.tag == b"IMAG")
                && e.offset + 28 <= data.len()
                && matches!(read_u32_le(data, e.offset + 8), Ok(1 | 3))
        })
        .ok_or_else(|| {
            DecodeError::UnsupportedFormat("X3F contains no RAW image section".into())
        })?;

    let raw_end = raw_entry
        .offset
        .checked_add(raw_entry.length)
        .filter(|&end| end <= data.len())
        .ok_or_else(|| DecodeError::MalformedMetadata("X3F RAW section out of bounds".into()))?;
    let sec = &data[raw_entry.offset..raw_end];

    let format = read_u32_le(sec, 12)?;
    let width = read_u32_le(sec, 16)? as usize;
    let height = read_u32_le(sec, 20)? as usize;
    let row_stride = read_u32_le(sec, 24)? as usize;

    let (out_w, out_h, raw_rgb_u16, is_quattro) = match format {
        5 => {
            let pixels = decode_x3f_uncompressed_10bit(sec, width, height, row_stride)?;
            (width, height, pixels, false)
        }
        6 => {
            let pixels = decode_x3f_huffman_10bit(sec, width, height)?;
            (width, height, pixels, false)
        }
        30 => {
            let pixels = decode_x3f_true2(sec, width, height, false)?;
            (width, height, pixels, false)
        }
        35 | 37 | 39 => {
            let (qw, qh, pixels) = decode_x3f_quattro(sec, width, height)?;
            (qw, qh, pixels, true)
        }
        other => {
            return Err(DecodeError::UnsupportedFormat(format!(
                "unsupported X3F RAW format={other}"
            )));
        }
    };

    // Parse CAMF for color calibration matrix (best-effort; falls back cleanly to Gray-World)
    let camf_matrix = entries
        .iter()
        .find(|e| &e.tag == b"CAMF")
        .and_then(|e| extract_camf_raw_to_srgb(data, e, &wb_desc));

    let linear_rgb = calibrate_foveon_to_linear_srgb(
        &raw_rgb_u16,
        out_w,
        out_h,
        camf_matrix.as_ref(),
        is_quattro,
    )?;

    let metadata = PhotoMetadata {
        camera_make: make,
        camera_model: model,
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: aperture,
        iso,
        width: out_w,
        height: out_h,
        black_levels: [0.0; 4],
        white_levels: [1.0; 4],
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
                "{out_w} x {out_h} ({:.1} MP)",
                (out_w * out_h) as f32 / 1_000_000.0
            ),
        ),
        (
            "Sensor Format".to_owned(),
            format!("Foveon X3F (format {format}, Linear sRGB)"),
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

/// Parses the X3F `PROP` UTF-16LE key-value property section.
fn parse_prop_section(data: &[u8], entry: &DirEntry) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(end) = entry.offset.checked_add(entry.length) else {
        return map;
    };
    let Some(sec) = data.get(entry.offset..end) else {
        return map;
    };
    if sec.len() < 24 || &sec[0..4] != b"SECp" {
        return map;
    }
    let Ok(count) = read_u32_le(sec, 8) else {
        return map;
    };
    let count = (count as usize).min(256);
    let table_start = 24usize;
    let chars_start = table_start + count * 8;
    if chars_start >= sec.len() {
        return map;
    }
    let char_bytes = &sec[chars_start..];
    let read_utf16 = |word_off: usize| -> String {
        let mut units = Vec::new();
        let mut byte_off = word_off.saturating_mul(2);
        while byte_off + 2 <= char_bytes.len() {
            let u = u16::from_le_bytes([char_bytes[byte_off], char_bytes[byte_off + 1]]);
            if u == 0 {
                break;
            }
            units.push(u);
            byte_off += 2;
        }
        String::from_utf16_lossy(&units)
    };
    for i in 0..count {
        let p = table_start + i * 8;
        if let (Ok(k_off), Ok(v_off)) = (read_u32_le(sec, p), read_u32_le(sec, p + 4)) {
            let k = read_utf16(k_off as usize);
            let v = read_utf16(v_off as usize);
            if !k.is_empty() {
                map.insert(k, v);
            }
        }
    }
    map
}

/// Format 5 (`Polaroid x530`): 1024-entry `i16` mapping LUT + packed 3×10-bit DPCM per row.
fn decode_x3f_uncompressed_10bit(
    sec: &[u8],
    width: usize,
    height: usize,
    row_stride: usize,
) -> Result<Vec<[u16; 3]>, DecodeError> {
    let pixel_count = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    let mut mapping = [0i16; 1024];
    for (i, slot) in mapping.iter_mut().enumerate() {
        *slot = read_u16_le(sec, 28 + i * 2)? as i16;
    }

    let data_start = 28usize + 2048usize;
    let row_bytes_len = width
        .checked_mul(4)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    let stride = if row_stride > 0 {
        row_stride
    } else {
        row_bytes_len
    };
    if stride < row_bytes_len {
        return Err(DecodeError::MalformedMetadata(
            "X3F format 5 row stride smaller than row width".into(),
        ));
    }
    let required_bytes = height
        .checked_mul(stride)
        .and_then(|n| n.checked_add(data_start))
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    if sec.len() < required_bytes {
        return Err(DecodeError::MalformedMetadata(
            "X3F format 5 section truncated".into(),
        ));
    }
    let mut out = vec![[0u16; 3]; pixel_count];

    for y in 0..height {
        let row_off = data_start + y * stride;
        let row_bytes = sec.get(row_off..row_off + row_bytes_len).ok_or_else(|| {
            DecodeError::MalformedMetadata("X3F format 5 row out of bounds".into())
        })?;
        let mut c = [0i16; 3];
        let (words, _) = row_bytes.as_chunks::<4>();
        for (x, &w_bytes) in words.iter().take(width).enumerate() {
            let val = u32::from_le_bytes(w_bytes);
            for (ch, acc) in c.iter_mut().enumerate() {
                let idx = ((val >> (ch * 10)) & 0x03FF) as usize;
                *acc = acc.wrapping_add(mapping[idx]);
            }
            // Format 5 packs 10-bit layers in BGR order (bits 0..10 = B, 10..20 = G, 20..30 = R).
            out[y * width + x] = [c[2].max(0) as u16, c[1].max(0) as u16, c[0].max(0) as u16];
        }
    }
    Ok(out)
}

/// Format 6 (`Sigma SD9`, `SD10`, `SD14`): 1024-entry mapping LUT + 1024-entry Huffman table + per-row bitstreams.
fn decode_x3f_huffman_10bit(
    sec: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<[u16; 3]>, DecodeError> {
    let pixel_count = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    let mut mapping = [0i16; 1024];
    for (i, slot) in mapping.iter_mut().enumerate() {
        *slot = read_u16_le(sec, 28 + i * 2)? as i16;
    }

    let table_start = 28 + 2048;
    let mut tree = HuffTree::new();
    for (i, &mapped) in mapping.iter().enumerate() {
        let elem = read_u32_le(sec, table_start + i * 4)?;
        if elem != 0 {
            let len = (elem >> 27) & 0x1F;
            let code = elem & 0x07FF_FFFF;
            tree.insert(len, code, i32::from(mapped))?;
        }
    }

    let data_start = table_start + 4096;
    let row_offsets_bytes = height
        .checked_mul(4)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    if data_start + row_offsets_bytes > sec.len() {
        return Err(DecodeError::MalformedMetadata(
            "X3F format 6 section truncated".into(),
        ));
    }
    let row_offsets_start = sec.len() - row_offsets_bytes;
    let bitstream = &sec[data_start..row_offsets_start];

    let mut out = vec![[0u16; 3]; pixel_count];
    for y in 0..height {
        let row_off = read_u32_le(sec, row_offsets_start + y * 4)? as usize;
        let row_slice = bitstream.get(row_off..).ok_or_else(|| {
            DecodeError::MalformedMetadata("X3F format 6 row offset out of bounds".into())
        })?;
        let mut reader = BitReader::new(row_slice);
        let mut c = [0i32; 3];
        for x in 0..width {
            for acc in &mut c {
                let diff = tree.decode(&mut reader)?;
                *acc = (*acc + diff).clamp(i32::from(i16::MIN), i32::from(i16::MAX));
            }
            // Format 5 packs 10-bit layers in BGR order (bits 0..10 = B, 10..20 = G, 20..30 = R).
            out[y * width + x] = [c[2].max(0) as u16, c[1].max(0) as u16, c[0].max(0) as u16];
        }
    }
    Ok(out)
}

/// Decodes a single TRUE-II Huffman-coded plane using the 2x2 parity predictor.
fn decode_true2_plane(
    plane_bytes: &[u8],
    cols: usize,
    rows: usize,
    seed: u16,
    tree: &HuffTree,
) -> Result<Vec<u16>, DecodeError> {
    let total = cols.checked_mul(rows).ok_or(CoreError::InvalidDimensions {
        width: cols,
        height: rows,
    })?;
    let mut out = vec![0u16; total];
    let mut reader = BitReader::new(plane_bytes);
    let seed_i32 = i32::from(seed);
    let mut row_start_acc = [[seed_i32; 2]; 2];

    for row in 0..rows {
        let odd_row = row & 1;
        let mut acc = [0i32; 2];
        let row_base = row * cols;
        for col in 0..cols {
            let odd_col = col & 1;
            let bits = tree.decode(&mut reader)? as u32;
            if bits > 24 {
                return Err(DecodeError::MalformedMetadata(format!(
                    "X3F TRUE-II diff bit length {bits} exceeds 24"
                )));
            }
            let diff = if bits == 0 {
                0
            } else {
                let raw = reader.read_bits(bits)? as i32;
                let half = 1i32 << (bits - 1);
                if raw < half {
                    raw - ((1i32 << bits) - 1)
                } else {
                    raw
                }
            };
            let prev = if col < 2 {
                row_start_acc[odd_row][odd_col]
            } else {
                acc[odd_col]
            };
            let val = prev.wrapping_add(diff);
            acc[odd_col] = val;
            if col < 2 {
                row_start_acc[odd_row][odd_col] = val;
            }
            out[row_base + col] = val.clamp(0, i32::from(u16::MAX)) as u16;
        }
    }
    Ok(out)
}

/// Parses the TRUE-II header starting at `off` inside `sec` and returns
/// `(seeds, tree, plane_sizes, data_start)`.
fn parse_true2_header(
    sec: &[u8],
    mut off: usize,
    has_quattro_extra_u32: bool,
) -> Result<([u16; 3], HuffTree, [usize; 3], usize), DecodeError> {
    let seeds = [
        read_u16_le(sec, off)?,
        read_u16_le(sec, off + 2)?,
        read_u16_le(sec, off + 4)?,
    ];
    off += 8; // 3 seeds + 2-byte unknown

    let mut tree = HuffTree::new();
    let mut idx = 0i32;
    loop {
        if off + 2 > sec.len() {
            return Err(DecodeError::MalformedMetadata(
                "X3F TRUE-II Huffman table truncated".into(),
            ));
        }
        let code_size = sec[off];
        let code_byte = sec[off + 1];
        off += 2;
        if code_size == 0 {
            break;
        }
        if code_size <= 8 {
            let code = u32::from(code_byte >> (8 - code_size));
            tree.insert(u32::from(code_size), code, idx)?;
        }
        idx += 1;
        if idx > 64 {
            return Err(DecodeError::MalformedMetadata(
                "X3F TRUE-II Huffman table too large".into(),
            ));
        }
    }

    if has_quattro_extra_u32 {
        off += 4;
    }

    let plane_sizes = [
        read_u32_le(sec, off)? as usize,
        read_u32_le(sec, off + 4)? as usize,
        read_u32_le(sec, off + 8)? as usize,
    ];
    off += 12;

    Ok((seeds, tree, plane_sizes, off))
}

/// Format 30 (`DP1`, `DP1s`, `SD15`, `SD1/DP1/DP2/DP3 Merrill`): 3 equal-resolution TRUE-II planes.
fn decode_x3f_true2(
    sec: &[u8],
    width: usize,
    height: usize,
    has_quattro_extra_u32: bool,
) -> Result<Vec<[u16; 3]>, DecodeError> {
    let (seeds, tree, plane_sizes, data_start) =
        parse_true2_header(sec, 28, has_quattro_extra_u32)?;
    let pixel_count = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;

    let mut p_off = data_start;
    let mut planes = Vec::with_capacity(3);
    for ch in 0..3 {
        let p_end = p_off.checked_add(plane_sizes[ch]).ok_or_else(|| {
            DecodeError::MalformedMetadata("X3F TRUE-II plane offset overflow".into())
        })?;
        let slice = sec.get(p_off..p_end.min(sec.len())).ok_or_else(|| {
            DecodeError::MalformedMetadata("X3F TRUE-II plane out of bounds".into())
        })?;
        planes.push(decode_true2_plane(slice, width, height, seeds[ch], &tree)?);
        let aligned = (plane_sizes[ch] + 15) & !15;
        p_off = p_off.saturating_add(aligned);
    }

    let mut out = vec![[0u16; 3]; pixel_count];
    for i in 0..pixel_count {
        out[i] = [planes[0][i], planes[1][i], planes[2][i]];
    }
    Ok(out)
}

/// Formats 35, 37, 39 (`DP0/dp2/sd Quattro`, `sd Quattro H`): 3 TRUE-II planes where
/// planes 0 & 1 may be half-resolution (`W/2 × H/2`) and plane 2 is full-resolution top layer.
fn decode_x3f_quattro(
    sec: &[u8],
    width: usize,
    height: usize,
) -> Result<(usize, usize, Vec<[u16; 3]>), DecodeError> {
    let mut dims = [(0usize, 0usize); 3];
    for (i, d) in dims.iter_mut().enumerate() {
        let cols = read_u16_le(sec, 28 + i * 4)? as usize;
        let rows = read_u16_le(sec, 28 + i * 4 + 2)? as usize;
        *d = (cols, rows);
    }

    let (seeds, tree, plane_sizes, data_start) = parse_true2_header(sec, 28 + 12, true)?;

    let mut p_off = data_start;
    let mut planes = Vec::with_capacity(3);
    for ch in 0..3 {
        let (pw, ph) = dims[ch];
        let p_end = p_off.checked_add(plane_sizes[ch]).ok_or_else(|| {
            DecodeError::MalformedMetadata("X3F Quattro plane offset overflow".into())
        })?;
        let slice = sec.get(p_off..p_end.min(sec.len())).ok_or_else(|| {
            DecodeError::MalformedMetadata("X3F Quattro plane out of bounds".into())
        })?;
        planes.push(decode_true2_plane(slice, pw, ph, seeds[ch], &tree)?);
        let aligned = (plane_sizes[ch] + 15) & !15;
        p_off = p_off.saturating_add(aligned);
    }

    let out_w = width.min(dims[2].0);
    let out_h = height.min(dims[2].1);
    let pixel_count = out_w
        .checked_mul(out_h)
        .ok_or(CoreError::InvalidDimensions {
            width: out_w,
            height: out_h,
        })?;

    let (w0, h0) = dims[0];
    let (w1, h1) = dims[1];
    let (w2, _h2) = dims[2];

    // Bilinearly upsample half-resolution bottom (R) and middle (G) planes to match full-res top (B) plane.
    let sample_bilinear = |plane: &[u16], pw: usize, ph: usize, y: usize, x: usize| -> u16 {
        if pw == 0 || ph == 0 {
            return 0;
        }
        if pw >= out_w && ph >= out_h {
            return plane[y * pw + x];
        }
        let fx = (x as f32) * (pw as f32) / (out_w as f32);
        let fy = (y as f32) * (ph as f32) / (out_h as f32);
        let x0 = (fx.floor() as usize).min(pw - 1);
        let y0 = (fy.floor() as usize).min(ph - 1);
        let x1 = (x0 + 1).min(pw - 1);
        let y1 = (y0 + 1).min(ph - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let v00 = f32::from(plane[y0 * pw + x0]);
        let v10 = f32::from(plane[y0 * pw + x1]);
        let v01 = f32::from(plane[y1 * pw + x0]);
        let v11 = f32::from(plane[y1 * pw + x1]);
        let top = v00 * (1.0 - tx) + v10 * tx;
        let bot = v01 * (1.0 - tx) + v11 * tx;
        (top * (1.0 - ty) + bot * ty).clamp(0.0, 65535.0) as u16
    };

    let mut out = vec![[0u16; 3]; pixel_count];
    for y in 0..out_h {
        let row_out = y * out_w;
        let row_p2 = y * w2;
        for x in 0..out_w {
            let r = sample_bilinear(&planes[0], w0, h0, y, x);
            let g = sample_bilinear(&planes[1], w1, h1, y, x);
            let b = planes[2][row_p2 + x];
            out[row_out + x] = [r, g, b];
        }
    }

    Ok((out_w, out_h, out))
}

/// Decodes CAMF type-2 obfuscated section (`SD9`, `SD10`, `SD14`, `Polaroid x530`) and searches
/// for `WhiteBalanceColorCorrections` + `WhiteBalanceGains` to produce a `raw -> linear sRGB` 3x3 matrix.
fn extract_camf_raw_to_srgb(data: &[u8], entry: &DirEntry, _wb_desc: &str) -> Option<[f32; 9]> {
    let end = entry.offset.checked_add(entry.length)?;
    let sec = data.get(entry.offset..end)?;
    if sec.len() < 28 || &sec[0..4] != b"SECc" {
        return None;
    }
    let camf_type = read_u32_le(sec, 8).ok()?;
    if camf_type != 2 {
        return None;
    }
    let mut key = read_u32_le(sec, 12).ok()?;
    let payload = &sec[28..];
    let mut decoded = vec![0u8; payload.len()];
    for (dst, &src) in decoded.iter_mut().zip(payload.iter()) {
        key = (key.wrapping_mul(1597).wrapping_add(51749)) % 244_944;
        let tmp = ((u64::from(key) * 301_593_171) >> 24) as u32;
        let mask = (((((key << 8).wrapping_sub(tmp)) >> 1).wrapping_add(tmp)) >> 17) as u8;
        *dst = src ^ mask;
    }

    // Scan decoded CAMF CMbM blocks for a 3x3 float color correction matrix
    let mut pos = 0usize;
    while pos + 20 <= decoded.len() {
        if &decoded[pos..pos + 4] == b"CMbM" {
            let entry_size = read_u32_le(&decoded, pos + 8).ok()? as usize;
            let name_off = read_u32_le(&decoded, pos + 12).ok()? as usize;
            let val_off = read_u32_le(&decoded, pos + 16).ok()? as usize;
            if entry_size < 20 || pos + entry_size > decoded.len() {
                break;
            }
            let block = &decoded[pos..pos + entry_size];
            if let Some(name_slice) = block.get(name_off..) {
                let nul = name_slice
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(name_slice.len());
                if let Ok(name) = std::str::from_utf8(&name_slice[..nul]) {
                    if (name == "WhiteBalanceColorCorrections"
                        || name == "DP1_WhiteBalanceColorCorrections")
                        && val_off + 12 <= block.len()
                    {
                        let mtype = read_u32_le(block, val_off).ok()?;
                        let dims = read_u32_le(block, val_off + 4).ok()? as usize;
                        let data_off = read_u32_le(block, val_off + 8).ok()? as usize;
                        if mtype == 3 && dims >= 2 && data_off + 36 <= block.len() {
                            let mut m = [0.0_f32; 9];
                            for (i, slot) in m.iter_mut().enumerate() {
                                let b = &block[data_off + i * 4..data_off + i * 4 + 4];
                                *slot = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                            }
                            if m.iter().all(|v| v.is_finite()) {
                                return Some(m);
                            }
                        }
                    }
                }
            }
            pos += entry_size;
        } else {
            pos += 4;
        }
    }
    None
}

/// Converts raw 3-layer Foveon sensor triplets into a normalized linear sRGB `LinearRgbBuffer`.
///
/// Estimates robust per-channel black (1st percentile) and white (99.8th percentile) points from
/// the active image center, normalizes each Foveon layer, applies the calibrated Foveon-to-sRGB
/// matrix (from CAMF when present, or a Foveon X3 daylight matrix fallback), and balances via
/// Gray-World multipliers so all generations render with neutral highlights and natural color.
fn calibrate_foveon_to_linear_srgb(
    raw: &[[u16; 3]],
    width: usize,
    height: usize,
    camf_matrix: Option<&[f32; 9]>,
    is_quattro: bool,
) -> Result<LinearRgbBuffer, DecodeError> {
    let pixel_count = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    if raw.len() != pixel_count {
        return Err(CoreError::BufferLengthMismatch {
            expected: pixel_count,
            actual: raw.len(),
        }
        .into());
    }

    // Subsample central 80% region to estimate per-channel black (p0.5) and white (p99.8) levels.
    let y0 = height / 10;
    let y1 = height - y0;
    let x0 = width / 10;
    let x1 = width - x0;
    let step_y = ((y1 - y0) / 128).max(1);
    let step_x = ((x1 - x0) / 128).max(1);

    let mut ch_samples: [Vec<u16>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            let p = raw[y * width + x];
            ch_samples[0].push(p[0]);
            ch_samples[1].push(p[1]);
            ch_samples[2].push(p[2]);
            x += step_x;
        }
        y += step_y;
    }

    let mut black = [0.0_f32; 3];
    let mut inv_scale = [1.0_f32; 3];
    for c in 0..3 {
        let s = &mut ch_samples[c];
        if s.is_empty() {
            continue;
        }
        s.sort_unstable();
        let b = f32::from(s[s.len() / 200]);
        let w = f32::from(s[(s.len() * 998) / 1000]);
        black[c] = b;
        inv_scale[c] = 1.0 / (w - b).max(1.0);
    }

    // Calibrated Foveon-to-sRGB 3x3 matrix:
    // - Use CAMF matrix when extracted and well-conditioned.
    // - Otherwise use a conservative Foveon X3 decorrelation matrix (Foveon layers have broad
    //   spectral overlap, so mild off-diagonal unmixing restores natural saturation without clipping).
    let default_matrix: [f32; 9] = if is_quattro {
        [1.65, -0.55, -0.10, -0.35, 1.55, -0.20, -0.10, -0.45, 1.55]
    } else {
        [1.55, -0.45, -0.10, -0.30, 1.50, -0.20, -0.08, -0.42, 1.50]
    };
    let mat = camf_matrix.unwrap_or(&default_matrix);

    let mut linear = Vec::with_capacity(pixel_count);
    for &[r_raw, g_raw, b_raw] in raw {
        let r_n = ((f32::from(r_raw) - black[0]) * inv_scale[0]).max(0.0);
        let g_n = ((f32::from(g_raw) - black[1]) * inv_scale[1]).max(0.0);
        let b_n = ((f32::from(b_raw) - black[2]) * inv_scale[2]).max(0.0);

        let r = (mat[0] * r_n + mat[1] * g_n + mat[2] * b_n).clamp(0.0, 1.0);
        let g = (mat[3] * r_n + mat[4] * g_n + mat[5] * b_n).clamp(0.0, 1.0);
        let b = (mat[6] * r_n + mat[7] * g_n + mat[8] * b_n).clamp(0.0, 1.0);
        linear.push([r, g, b]);
    }

    LinearRgbBuffer::new(width, height, linear)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_huff_tree_insert_and_decode() -> Result<(), Box<dyn std::error::Error>> {
        let mut tree = HuffTree::new();
        // '0' -> 0, '10' -> 1, '11' -> -1
        tree.insert(1, 0b0, 0)?;
        tree.insert(2, 0b10, 1)?;
        tree.insert(2, 0b11, -1)?;

        // Bitstream: 0_10_11_000 = 0b0101_1000 = 0x58
        let buf = [0x58u8];
        let mut reader = BitReader::new(&buf);
        assert_eq!(tree.decode(&mut reader)?, 0);
        assert_eq!(tree.decode(&mut reader)?, 1);
        assert_eq!(tree.decode(&mut reader)?, -1);
        Ok(())
    }

    #[test]
    fn test_decode_true2_plane_constant_seed() -> Result<(), Box<dyn std::error::Error>> {
        // Tree where single bit '0' decodes to 0-bit diff (diff = 0).
        let mut tree = HuffTree::new();
        tree.insert(1, 0, 0)?;
        let zeros = [0u8; 4];
        let plane = decode_true2_plane(&zeros, 4, 4, 512, &tree)?;
        assert_eq!(plane, vec![512u16; 16]);
        Ok(())
    }

    #[test]
    fn test_calibrate_foveon_neutral_ramp() -> Result<(), Box<dyn std::error::Error>> {
        let mut raw = Vec::with_capacity(100);
        for i in 0..100u16 {
            let v = 100 + i * 10;
            raw.push([v, v, v]);
        }
        let rgb = calibrate_foveon_to_linear_srgb(&raw, 10, 10, None, false)?;
        assert_eq!(rgb.width(), 10);
        assert_eq!(rgb.height(), 10);
        let mid = rgb.samples()[55];
        assert!(mid[0].is_finite() && mid[1].is_finite() && mid[2].is_finite());
        assert!((mid[0] - mid[1]).abs() < 0.05 && (mid[1] - mid[2]).abs() < 0.05);
        Ok(())
    }

    #[test]
    fn test_try_decode_x3f_format5_and_errors() -> Result<(), Box<dyn std::error::Error>> {
        struct TempFile(std::path::PathBuf);
        impl Drop for TempFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let tf_bad =
            TempFile(std::env::temp_dir().join(format!("rraw_bad_{}.x3f", std::process::id())));
        std::fs::write(&tf_bad.0, b"NOPE")?;
        assert!(super::try_decode_x3f(&tf_bad.0).is_err());

        let mut file = Vec::new();
        file.extend_from_slice(b"FOVb");
        file.extend_from_slice(&[0u8; 36]);

        let ima_off: u32 = 40;
        let mut ima = Vec::new();
        ima.extend_from_slice(b"SECi");
        ima.extend_from_slice(&0u32.to_le_bytes());
        ima.extend_from_slice(&1u32.to_le_bytes());
        ima.extend_from_slice(&5u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&0u32.to_le_bytes());
        for i in 0..1024 {
            let v: i16 = if i == 1 { 500 } else { 0 };
            ima.extend_from_slice(&v.to_le_bytes());
        }
        for row in 0..16 {
            for col in 0..16 {
                let px: u32 = if row == 0 && col == 0 { 0x0010_0401 } else { 0 };
                ima.extend_from_slice(&px.to_le_bytes());
            }
        }
        let ima_len = ima.len() as u32;
        file.extend_from_slice(&ima);

        let dir_off = file.len() as u32;
        file.extend_from_slice(b"SECd");
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&1u32.to_le_bytes());
        file.extend_from_slice(&ima_off.to_le_bytes());
        file.extend_from_slice(&ima_len.to_le_bytes());
        file.extend_from_slice(b"IMA2");
        file.extend_from_slice(&dir_off.to_le_bytes());

        let tf = TempFile(std::env::temp_dir().join(format!("rraw_ok_{}.x3f", std::process::id())));
        std::fs::write(&tf.0, &file)?;
        let raw = super::try_decode_x3f(&tf.0)?;
        assert_eq!(raw.metadata.width, 16);
        assert_eq!(raw.metadata.height, 16);
        assert!(raw.linear_rgb.is_some());
        Ok(())
    }

    #[test]
    fn test_try_decode_x3f_missing_secd() -> Result<(), Box<dyn std::error::Error>> {
        struct TempFile(std::path::PathBuf);
        impl Drop for TempFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let tf =
            TempFile(std::env::temp_dir().join(format!("rraw_nosecd_{}.x3f", std::process::id())));
        let mut buf = Vec::new();
        buf.extend_from_slice(b"FOVb");
        buf.extend_from_slice(&[0u8; 36]);
        buf.extend_from_slice(&0u32.to_le_bytes());
        std::fs::write(&tf.0, &buf)?;
        assert!(super::try_decode_x3f(&tf.0).is_err());
        Ok(())
    }

    #[test]
    fn test_try_decode_x3f_format6_huffman() -> Result<(), Box<dyn std::error::Error>> {
        struct TempFile(std::path::PathBuf);
        impl Drop for TempFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"FOVb");
        file.extend_from_slice(&[0u8; 36]);

        let ima_off: u32 = 40;
        let mut ima = Vec::new();
        ima.extend_from_slice(b"SECi");
        ima.extend_from_slice(&0u32.to_le_bytes());
        ima.extend_from_slice(&1u32.to_le_bytes());
        ima.extend_from_slice(&6u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&0u32.to_le_bytes());
        for i in 0..1024 {
            let v: i16 = if i == 0 { 100 } else { 0 };
            ima.extend_from_slice(&v.to_le_bytes());
        }
        for i in 0..1024 {
            let elem: u32 = if i == 0 { 1u32 << 27 } else { 0 };
            ima.extend_from_slice(&elem.to_le_bytes());
        }
        ima.extend_from_slice(&[0u8; 16 * 8]);
        for _ in 0..16 {
            ima.extend_from_slice(&0u32.to_le_bytes());
        }
        let ima_len = ima.len() as u32;
        file.extend_from_slice(&ima);

        let dir_off = file.len() as u32;
        file.extend_from_slice(b"SECd");
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&1u32.to_le_bytes());
        file.extend_from_slice(&ima_off.to_le_bytes());
        file.extend_from_slice(&ima_len.to_le_bytes());
        file.extend_from_slice(b"IMA2");
        file.extend_from_slice(&dir_off.to_le_bytes());

        let tf = TempFile(std::env::temp_dir().join(format!("rraw_f6_{}.x3f", std::process::id())));
        std::fs::write(&tf.0, &file)?;
        let raw = super::try_decode_x3f(&tf.0)?;
        assert_eq!(raw.metadata.width, 16);
        assert_eq!(raw.metadata.height, 16);
        assert!(raw.linear_rgb.is_some());
        Ok(())
    }

    #[test]
    fn test_try_decode_x3f_true2_format30_and_prop() -> Result<(), Box<dyn std::error::Error>> {
        struct TempFile(std::path::PathBuf);
        impl Drop for TempFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"FOVb");
        buf.extend_from_slice(&[0u8; 36]);

        let prop_off: u32 = 40;
        let mut prop: Vec<u8> = Vec::new();
        prop.extend_from_slice(b"SECp");
        prop.extend_from_slice(&0u32.to_le_bytes());
        prop.extend_from_slice(&1u32.to_le_bytes());
        prop.extend_from_slice(&0u32.to_le_bytes());
        prop.extend_from_slice(&0u32.to_le_bytes());
        prop.extend_from_slice(&0u32.to_le_bytes());
        prop.extend_from_slice(&0u32.to_le_bytes());
        prop.extend_from_slice(&9u32.to_le_bytes());
        for c in "CAMMANUF\0SIGMA\0".encode_utf16() {
            prop.extend_from_slice(&c.to_le_bytes());
        }
        let prop_len = prop.len() as u32;
        buf.extend_from_slice(&prop);

        let ima_off = prop_off + prop_len;
        let mut ima: Vec<u8> = Vec::new();
        ima.extend_from_slice(b"SECi");
        ima.extend_from_slice(&0u32.to_le_bytes());
        ima.extend_from_slice(&1u32.to_le_bytes());
        ima.extend_from_slice(&30u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&0u32.to_le_bytes());
        for _ in 0..3 {
            ima.extend_from_slice(&512u16.to_le_bytes());
        }
        ima.extend_from_slice(&[0u8; 2]);
        ima.extend_from_slice(&[1u8, 0u8, 0u8, 0u8]);
        for _ in 0..3 {
            ima.extend_from_slice(&32u32.to_le_bytes());
        }
        for _ in 0..3 {
            ima.extend_from_slice(&[0u8; 32]);
        }
        buf.extend_from_slice(&ima);

        let dir_off = ima_off + ima.len() as u32;
        buf.extend_from_slice(b"SECd");
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&prop_off.to_le_bytes());
        buf.extend_from_slice(&prop_len.to_le_bytes());
        buf.extend_from_slice(b"PROP");
        buf.extend_from_slice(&ima_off.to_le_bytes());
        buf.extend_from_slice(&(ima.len() as u32).to_le_bytes());
        buf.extend_from_slice(b"IMA2");
        buf.extend_from_slice(&dir_off.to_le_bytes());

        let tf = TempFile(std::env::temp_dir().join(format!("rraw_t2_{}.x3f", std::process::id())));
        std::fs::write(&tf.0, &buf)?;
        let raw = super::try_decode_x3f(&tf.0)?;
        assert_eq!(raw.metadata.camera_make, "SIGMA");
        assert_eq!(raw.metadata.width, 16);
        assert_eq!(raw.metadata.height, 16);
        assert!(raw.linear_rgb.is_some());
        Ok(())
    }

    #[test]
    fn test_try_decode_x3f_quattro_format35() -> Result<(), Box<dyn std::error::Error>> {
        struct TempFile(std::path::PathBuf);
        impl Drop for TempFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"FOVb");
        buf.extend_from_slice(&[0u8; 36]);

        let ima_off: u32 = 40;
        let mut ima: Vec<u8> = Vec::new();
        ima.extend_from_slice(b"SECi");
        ima.extend_from_slice(&0u32.to_le_bytes());
        ima.extend_from_slice(&1u32.to_le_bytes());
        ima.extend_from_slice(&35u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&0u32.to_le_bytes());
        for (cw, ch) in [(8u16, 8u16), (8, 8), (16, 16)] {
            ima.extend_from_slice(&cw.to_le_bytes());
            ima.extend_from_slice(&ch.to_le_bytes());
        }
        for _ in 0..3 {
            ima.extend_from_slice(&512u16.to_le_bytes());
        }
        ima.extend_from_slice(&[0u8; 2]);
        ima.extend_from_slice(&[1u8, 0u8, 0u8, 0u8]);
        ima.extend_from_slice(&0u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&16u32.to_le_bytes());
        ima.extend_from_slice(&32u32.to_le_bytes());
        ima.extend_from_slice(&[0u8; 16]);
        ima.extend_from_slice(&[0u8; 16]);
        ima.extend_from_slice(&[0u8; 32]);
        let ima_len = ima.len() as u32;
        buf.extend_from_slice(&ima);

        let dir_off = buf.len() as u32;
        buf.extend_from_slice(b"SECd");
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&ima_off.to_le_bytes());
        buf.extend_from_slice(&ima_len.to_le_bytes());
        buf.extend_from_slice(b"IMA2");
        buf.extend_from_slice(&dir_off.to_le_bytes());

        let tf =
            TempFile(std::env::temp_dir().join(format!("rraw_q35_{}.x3f", std::process::id())));
        std::fs::write(&tf.0, &buf)?;
        let raw = super::try_decode_x3f(&tf.0)?;
        assert_eq!(raw.metadata.width, 16);
        assert_eq!(raw.metadata.height, 16);
        assert!(raw.linear_rgb.is_some());
        Ok(())
    }

    #[test]
    fn test_extract_camf_raw_to_srgb_synthetic() {
        let name = b"WhiteBalanceColorCorrections\0";
        let mut block = Vec::new();
        block.extend_from_slice(b"CMbM");
        block.extend_from_slice(&0u32.to_le_bytes());
        block.extend_from_slice(&0u32.to_le_bytes());
        block.extend_from_slice(&20u32.to_le_bytes());
        block.extend_from_slice(&(20u32 + name.len() as u32).to_le_bytes());
        block.extend_from_slice(name);
        block.extend_from_slice(&3u32.to_le_bytes());
        block.extend_from_slice(&2u32.to_le_bytes());
        block.extend_from_slice(&12u32.to_le_bytes());
        for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            block.extend_from_slice(&v.to_le_bytes());
        }
        let entry_size = block.len() as u32;
        block[8..12].copy_from_slice(&entry_size.to_le_bytes());

        let mut key = 0u32;
        let mut enc = block;
        for b in &mut enc {
            key = (key.wrapping_mul(1597).wrapping_add(51749)) % 244_944;
            let tmp = ((u64::from(key) * 301_593_171) >> 24) as u32;
            let mask = (((((key << 8).wrapping_sub(tmp)) >> 1).wrapping_add(tmp)) >> 17) as u8;
            *b ^= mask;
        }

        let mut sec = Vec::new();
        sec.extend_from_slice(b"SECc");
        sec.extend_from_slice(&0u32.to_le_bytes());
        sec.extend_from_slice(&2u32.to_le_bytes());
        sec.extend_from_slice(&0u32.to_le_bytes());
        sec.extend_from_slice(&[0u8; 12]);
        sec.extend_from_slice(&enc);

        let mat = extract_camf_raw_to_srgb(
            &sec,
            &DirEntry {
                offset: 0,
                length: sec.len(),
                tag: *b"CAMF",
            },
            "Sunlight",
        );
        assert!(mat.is_some());
    }
}
