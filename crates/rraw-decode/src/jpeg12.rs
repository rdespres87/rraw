#![forbid(unsafe_code)]

//! Pure-safe-Rust 12-bit and 8-bit single-component Sequential DCT JPEG decoder (`SOF0` / `SOF1`).
//!
//! Used by the DNG fallback decoder to unpack 12-bit Extended Sequential JPEG CFA tiles
//! (such as CinemaDNG lossy 3:1 / 4:1 / 12-bit streams) where lossless-only JPEG decoders fail.

use crate::DecodeError;

/// Decoded single-component 8-bit or 12-bit grayscale JPEG tile/strip.
#[derive(Debug, Clone)]
pub struct DecodedJpegGray {
    pub width: usize,
    pub height: usize,
    pub precision: u8,
    pub samples: Vec<u16>,
}

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Returns `true` when `data` starts with a JPEG `SOI` marker and contains a 12-bit `SOF1` frame.
#[must_use]
pub fn is_jpeg12_stream(data: &[u8]) -> bool {
    if data.len() < 12 || data[0] != 0xFF || data[1] != 0xD8 {
        return false;
    }
    let mut pos = 2;
    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        if pos >= data.len() {
            break;
        }
        let marker = data[pos];
        pos += 1;
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 || marker == 0x00 {
            continue;
        }
        if pos + 2 > data.len() {
            break;
        }
        let seg_len = usize::from(u16::from_be_bytes([data[pos], data[pos + 1]]));
        if seg_len < 2 || pos + seg_len > data.len() {
            break;
        }
        if marker == 0xC1 && seg_len >= 8 && data[pos + 2] == 12 {
            return true;
        }
        pos += seg_len;
    }
    false
}

#[derive(Debug, Clone)]
struct HuffTable {
    max_code: [i32; 17],
    val_offset: [i32; 17],
    values: Vec<u8>,
}

impl HuffTable {
    fn from_dht(counts: &[u8; 16], values: Vec<u8>) -> Self {
        let mut max_code = [-1_i32; 17];
        let mut val_offset = [0_i32; 17];
        let mut code = 0_i32;
        let mut idx = 0_i32;

        for bits in 1..=16 {
            let count = i32::from(counts[bits - 1]);
            if count > 0 {
                val_offset[bits] = idx - code;
                idx += count;
                code += count;
                max_code[bits] = code - 1;
            }
            code <<= 1;
        }

        Self {
            max_code,
            val_offset,
            values,
        }
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u64,
    bits_left: u32,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8], start: usize) -> Self {
        Self {
            data,
            pos: start,
            bit_buf: 0,
            bits_left: 0,
        }
    }

    #[inline]
    fn refill(&mut self) {
        while self.bits_left <= 56 && self.pos < self.data.len() {
            let byte = self.data[self.pos];
            self.pos += 1;
            if byte == 0xFF {
                if self.pos < self.data.len() {
                    let next = self.data[self.pos];
                    if next == 0x00 {
                        // Stuffed 0xFF data byte
                        self.pos += 1;
                    } else if next == 0xFF {
                        // Padding 0xFF byte: rewind one so the next iteration handles it
                        continue;
                    } else {
                        // Encountered a marker (e.g., RSTn or EOI): step back so caller can consume it
                        self.pos -= 1;
                        break;
                    }
                } else {
                    self.pos -= 1;
                    break;
                }
            }
            self.bit_buf = (self.bit_buf << 8) | u64::from(byte);
            self.bits_left += 8;
        }
    }

    #[inline]
    fn read_bits(&mut self, count: u32) -> Result<u32, DecodeError> {
        if count == 0 {
            return Ok(0);
        }
        if count > 32 {
            return Err(DecodeError::MalformedMetadata(
                "JPEG bit read exceeds 32 bits".to_owned(),
            ));
        }
        if self.bits_left < count {
            self.refill();
        }
        if self.bits_left < count {
            // Zero-pad at stream tail to avoid crashing on slightly truncated trailing MCUs
            let missing = count - self.bits_left;
            let val = if self.bits_left == 0 {
                0
            } else {
                let mask = (1_u64 << self.bits_left) - 1;
                ((self.bit_buf & mask) << missing) as u32
            };
            self.bits_left = 0;
            return Ok(val);
        }
        self.bits_left -= count;
        let mask = (1_u64 << count) - 1;
        Ok(((self.bit_buf >> self.bits_left) & mask) as u32)
    }

    #[inline]
    fn decode_huff(&mut self, table: &HuffTable) -> Result<u8, DecodeError> {
        let mut code = 0_i32;
        for bits in 1..=16 {
            let bit = self.read_bits(1)? as i32;
            code = (code << 1) | bit;
            if code <= table.max_code[bits] {
                let idx = (code + table.val_offset[bits]) as usize;
                return table.values.get(idx).copied().ok_or_else(|| {
                    DecodeError::MalformedMetadata(
                        "JPEG Huffman symbol index out of range".to_owned(),
                    )
                });
            }
        }
        Err(DecodeError::MalformedMetadata(
            "invalid JPEG Huffman code".to_owned(),
        ))
    }

    fn consume_restart_marker(&mut self) {
        self.bit_buf = 0;
        self.bits_left = 0;
        while self.pos + 1 < self.data.len() {
            if self.data[self.pos] == 0xFF {
                let m = self.data[self.pos + 1];
                if (0xD0..=0xD7).contains(&m) {
                    self.pos += 2;
                    return;
                }
                if m == 0xFF {
                    self.pos += 1;
                    continue;
                }
                return;
            }
            self.pos += 1;
        }
    }
}

#[inline]
const fn extend_sign(val: u32, bits: u32) -> i32 {
    if bits == 0 {
        return 0;
    }
    let threshold = 1_u32 << (bits - 1);
    if val < threshold {
        (val as i32) - ((1_i32 << bits) - 1)
    } else {
        val as i32
    }
}

struct IdctTables {
    cos: [[f32; 8]; 8],
}

impl IdctTables {
    fn new() -> Self {
        let mut cos = [[0.0_f32; 8]; 8];
        let inv_sqrt2 = std::f64::consts::FRAC_1_SQRT_2;
        for (u, row) in cos.iter_mut().enumerate() {
            let alpha = if u == 0 { inv_sqrt2 } else { 1.0 };
            for (x, cell) in row.iter_mut().enumerate() {
                let angle = ((2 * x + 1) as f64) * (u as f64) * std::f64::consts::PI / 16.0;
                *cell = (0.5 * alpha * angle.cos()) as f32;
            }
        }
        Self { cos }
    }

    fn idct_8x8(&self, block: &[f32; 64], out: &mut [f32; 64]) {
        let mut tmp = [0.0_f32; 64];

        // 1D IDCT on rows
        for r in 0..8 {
            let row_off = r * 8;
            for x in 0..8 {
                let mut sum = 0.0_f32;
                for u in 0..8 {
                    sum += block[row_off + u] * self.cos[u][x];
                }
                tmp[row_off + x] = sum;
            }
        }

        // 1D IDCT on columns
        for x in 0..8 {
            for y in 0..8 {
                let mut sum = 0.0_f32;
                for v in 0..8 {
                    sum += tmp[v * 8 + x] * self.cos[v][y];
                }
                out[y * 8 + x] = sum;
            }
        }
    }
}

struct ParsedJpegHeaders {
    width: usize,
    height: usize,
    precision: u8,
    restart_interval: usize,
    qtable: [f32; 64],
    dc_table: HuffTable,
    ac_table: HuffTable,
    scan_start: usize,
}

fn parse_dqt_segment(seg: &[u8], qtables: &mut [Option<[f32; 64]>; 4]) -> Result<(), DecodeError> {
    let mut cur = 0_usize;
    while cur < seg.len() {
        let pq_tq = seg[cur];
        cur += 1;
        let pq = (pq_tq >> 4) & 0x0F;
        let tq = usize::from(pq_tq & 0x0F);
        if tq >= 4 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid JPEG DQT table id {tq}"
            )));
        }
        let mut tbl = [0.0_f32; 64];
        if pq == 0 {
            if cur + 64 > seg.len() {
                return Err(DecodeError::MalformedMetadata(
                    "truncated 8-bit JPEG DQT table".to_owned(),
                ));
            }
            for (dst, &b) in tbl.iter_mut().zip(&seg[cur..cur + 64]) {
                *dst = f32::from(b);
            }
            cur += 64;
        } else if pq == 1 {
            if cur + 128 > seg.len() {
                return Err(DecodeError::MalformedMetadata(
                    "truncated 16-bit JPEG DQT table".to_owned(),
                ));
            }
            for (dst, pair) in tbl.iter_mut().zip(seg[cur..cur + 128].as_chunks::<2>().0) {
                *dst = f32::from(u16::from_be_bytes(*pair));
            }
            cur += 128;
        } else {
            return Err(DecodeError::UnsupportedFormat(format!(
                "unsupported JPEG DQT precision {pq}"
            )));
        }
        qtables[tq] = Some(tbl);
    }
    Ok(())
}

fn parse_dht_segment(
    seg: &[u8],
    dc_tables: &mut [Option<HuffTable>; 4],
    ac_tables: &mut [Option<HuffTable>; 4],
) -> Result<(), DecodeError> {
    let mut cur = 0_usize;
    while cur < seg.len() {
        if cur + 17 > seg.len() {
            return Err(DecodeError::MalformedMetadata(
                "truncated JPEG DHT header".to_owned(),
            ));
        }
        let tc_th = seg[cur];
        cur += 1;
        let tc = (tc_th >> 4) & 0x0F;
        let th = usize::from(tc_th & 0x0F);
        if th >= 4 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid JPEG DHT table id {th}"
            )));
        }
        let mut counts = [0_u8; 16];
        counts.copy_from_slice(&seg[cur..cur + 16]);
        cur += 16;
        let total: usize = counts.iter().map(|&c| usize::from(c)).sum();
        if cur + total > seg.len() {
            return Err(DecodeError::MalformedMetadata(
                "truncated JPEG DHT symbol list".to_owned(),
            ));
        }
        let values = seg[cur..cur + total].to_vec();
        cur += total;
        let table = HuffTable::from_dht(&counts, values);
        if tc == 0 {
            dc_tables[th] = Some(table);
        } else if tc == 1 {
            ac_tables[th] = Some(table);
        }
    }
    Ok(())
}

fn parse_sof_segment(seg: &[u8]) -> Result<(u8, usize, usize, usize), DecodeError> {
    if seg.len() < 9 {
        return Err(DecodeError::MalformedMetadata(
            "truncated JPEG SOF segment".to_owned(),
        ));
    }
    let precision = seg[0];
    if precision != 8 && precision != 12 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "unsupported JPEG sample precision {precision}"
        )));
    }
    let height = usize::from(u16::from_be_bytes([seg[1], seg[2]]));
    let width = usize::from(u16::from_be_bytes([seg[3], seg[4]]));
    let num_comp = seg[5];
    if num_comp != 1 {
        return Err(DecodeError::UnsupportedFormat(format!(
            "expected single-component JPEG CFA tile, got {num_comp} components"
        )));
    }
    let qtable_id = usize::from(seg[8]);
    Ok((precision, width, height, qtable_id))
}

fn parse_jpeg_headers(data: &[u8]) -> Result<ParsedJpegHeaders, DecodeError> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err(DecodeError::MalformedMetadata(
            "missing JPEG SOI marker".to_owned(),
        ));
    }

    let mut qtables: [Option<[f32; 64]>; 4] = [None, None, None, None];
    let mut dc_tables: [Option<HuffTable>; 4] = [None, None, None, None];
    let mut ac_tables: [Option<HuffTable>; 4] = [None, None, None, None];
    let mut restart_interval = 0_usize;

    let mut width = 0_usize;
    let mut height = 0_usize;
    let mut precision = 0_u8;
    let mut qtable_id = 0_usize;
    let mut dc_id = 0_usize;
    let mut ac_id = 0_usize;
    let mut scan_start = None;

    let mut pos = 2_usize;
    while pos + 2 <= data.len() {
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        if pos >= data.len() {
            break;
        }
        let marker = data[pos];
        pos += 1;

        if marker == 0xD9 {
            break;
        }
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 || marker == 0x00 {
            continue;
        }
        if pos + 2 > data.len() {
            break;
        }
        let seg_len = usize::from(u16::from_be_bytes([data[pos], data[pos + 1]]));
        if seg_len < 2 || pos + seg_len > data.len() {
            return Err(DecodeError::MalformedMetadata(
                "truncated JPEG marker segment".to_owned(),
            ));
        }
        let seg = &data[pos + 2..pos + seg_len];
        pos += seg_len;

        match marker {
            0xDB => parse_dqt_segment(seg, &mut qtables)?,
            0xC4 => parse_dht_segment(seg, &mut dc_tables, &mut ac_tables)?,
            0xDD => {
                if seg.len() >= 2 {
                    restart_interval = usize::from(u16::from_be_bytes([seg[0], seg[1]]));
                }
            }
            0xC0 | 0xC1 => {
                (precision, width, height, qtable_id) = parse_sof_segment(seg)?;
            }
            0xDA => {
                if seg.len() < 6 {
                    return Err(DecodeError::MalformedMetadata(
                        "truncated JPEG SOS segment".to_owned(),
                    ));
                }
                let num_comp = seg[0];
                if num_comp != 1 {
                    return Err(DecodeError::UnsupportedFormat(format!(
                        "expected single-component JPEG scan, got {num_comp} components"
                    )));
                }
                let td_ta = seg[2];
                dc_id = usize::from((td_ta >> 4) & 0x0F);
                ac_id = usize::from(td_ta & 0x0F);
                scan_start = Some(pos);
                break;
            }
            _ => {}
        }
    }

    let Some(scan_start) = scan_start else {
        return Err(DecodeError::MalformedMetadata(
            "missing JPEG SOS marker".to_owned(),
        ));
    };
    if width == 0 || height == 0 {
        return Err(DecodeError::MalformedMetadata(
            "invalid JPEG dimensions".to_owned(),
        ));
    }
    let Some(qtable) = qtables.get(qtable_id).and_then(|q| *q) else {
        return Err(DecodeError::MalformedMetadata(format!(
            "missing JPEG DQT table {qtable_id}"
        )));
    };
    let Some(dc_table) = dc_tables.get_mut(dc_id).and_then(Option::take) else {
        return Err(DecodeError::MalformedMetadata(format!(
            "missing JPEG DC Huffman table {dc_id}"
        )));
    };
    let Some(ac_table) = ac_tables.get_mut(ac_id).and_then(Option::take) else {
        return Err(DecodeError::MalformedMetadata(format!(
            "missing JPEG AC Huffman table {ac_id}"
        )));
    };

    Ok(ParsedJpegHeaders {
        width,
        height,
        precision,
        restart_interval,
        qtable,
        dc_table,
        ac_table,
        scan_start,
    })
}

fn decode_mcu_block(
    reader: &mut BitReader<'_>,
    dc_table: &HuffTable,
    ac_table: &HuffTable,
    qtable: &[f32; 64],
    dc_pred: &mut i64,
) -> Result<[f32; 64], DecodeError> {
    let mut coeffs = [0.0_f32; 64];

    let dc_cat = u32::from(reader.decode_huff(dc_table)?);
    if dc_cat > 15 {
        return Err(DecodeError::MalformedMetadata(format!(
            "invalid JPEG DC category {dc_cat}"
        )));
    }
    let dc_bits = reader.read_bits(dc_cat)?;
    *dc_pred += i64::from(extend_sign(dc_bits, dc_cat));
    coeffs[0] = (*dc_pred as f32) * qtable[0];

    let mut k = 1_usize;
    while k < 64 {
        let sym = reader.decode_huff(ac_table)?;
        if sym == 0x00 {
            break;
        }
        let run = usize::from(sym >> 4);
        let size = u32::from(sym & 0x0F);
        if sym == 0xF0 {
            k += 16;
            continue;
        }
        k += run;
        if k >= 64 {
            break;
        }
        let ac_bits = reader.read_bits(size)?;
        let ac_val = extend_sign(ac_bits, size);
        coeffs[ZIGZAG[k]] = (ac_val as f32) * qtable[k];
        k += 1;
    }

    Ok(coeffs)
}

fn store_mcu_block(
    spatial: &[f32; 64],
    samples: &mut [u16],
    width: usize,
    height: usize,
    base: (usize, usize),
    clamp: (f32, f32),
) {
    let (base_x, base_y) = base;
    let (level_shift, max_sample) = clamp;
    let max_dy = height.saturating_sub(base_y).min(8);
    let max_dx = width.saturating_sub(base_x).min(8);
    for dy in 0..max_dy {
        let dst_row = (base_y + dy) * width + base_x;
        let src_row = dy * 8;
        for dx in 0..max_dx {
            let val = (spatial[src_row + dx] + level_shift)
                .round()
                .clamp(0.0, max_sample);
            samples[dst_row + dx] = val as u16;
        }
    }
}

/// Decodes a single-component 8-bit (`SOF0`) or 12-bit (`SOF1`) Sequential DCT JPEG stream.
pub fn decode_jpeg_gray(data: &[u8]) -> Result<DecodedJpegGray, DecodeError> {
    let headers = parse_jpeg_headers(data)?;
    let width = headers.width;
    let height = headers.height;
    let precision = headers.precision;

    let mcu_w = width.div_ceil(8);
    let mcu_h = height.div_ceil(8);
    let mut samples = vec![0_u16; width * height];

    let mut reader = BitReader::new(data, headers.scan_start);
    let idct = IdctTables::new();
    let level_shift = (1_u32 << (precision - 1)) as f32;
    let max_sample = ((1_u32 << precision) - 1) as f32;

    let mut dc_pred = 0_i64;
    let mut mcu_index = 0_usize;

    for mcu_y in 0..mcu_h {
        for mcu_x in 0..mcu_w {
            if headers.restart_interval > 0
                && mcu_index > 0
                && mcu_index.is_multiple_of(headers.restart_interval)
            {
                reader.consume_restart_marker();
                dc_pred = 0;
            }
            mcu_index += 1;

            let coeffs = decode_mcu_block(
                &mut reader,
                &headers.dc_table,
                &headers.ac_table,
                &headers.qtable,
                &mut dc_pred,
            )?;
            let mut spatial = [0.0_f32; 64];
            idct.idct_8x8(&coeffs, &mut spatial);
            store_mcu_block(
                &spatial,
                &mut samples,
                width,
                height,
                (mcu_x * 8, mcu_y * 8),
                (level_shift, max_sample),
            );
        }
    }

    Ok(DecodedJpegGray {
        width,
        height,
        precision,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::{is_jpeg12_stream, IdctTables};

    #[test]
    fn idct_dc_only_recovers_constant_block() {
        let idct = IdctTables::new();
        let mut coeffs = [0.0_f32; 64];
        coeffs[0] = 800.0;
        let mut out = [0.0_f32; 64];
        idct.idct_8x8(&coeffs, &mut out);
        // DC term scales by 1/8 = 100.0 at every pixel
        for &v in &out {
            assert!((v - 100.0).abs() < 1e-3, "expected 100.0, got {v}");
        }
    }

    #[test]
    fn is_jpeg12_stream_detects_sof1_p12() {
        let sof1_12bit = [
            0xFF, 0xD8, 0xFF, 0xC1, 0x00, 0x0B, 12, 0x00, 0x10, 0x00, 0x10, 0x01, 0x01, 0x11, 0x00,
            0xFF, 0xD9,
        ];
        assert!(is_jpeg12_stream(&sof1_12bit));

        let sof0_8bit = [
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 8, 0x00, 0x10, 0x00, 0x10, 0x01, 0x01, 0x11, 0x00,
            0xFF, 0xD9,
        ];
        assert!(!is_jpeg12_stream(&sof0_8bit));
    }

    #[test]
    fn decode_jpeg_gray_8bit_luma_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let img =
            image::GrayImage::from_fn(16, 16, |x, y| image::Luma([((x * 16 + y) % 256) as u8]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageLuma8(img).write_to(
            &mut std::io::Cursor::new(&mut buf),
            image::ImageFormat::Jpeg,
        )?;
        let dec = super::decode_jpeg_gray(&buf)?;
        assert_eq!(dec.width, 16);
        assert_eq!(dec.height, 16);
        assert_eq!(dec.precision, 8);
        assert_eq!(dec.samples.len(), 256);
        Ok(())
    }

    #[test]
    fn decode_jpeg_gray_error_paths() {
        assert!(super::decode_jpeg_gray(&[]).is_err());
        assert!(super::decode_jpeg_gray(&[0xFF, 0xD8, 0xFF, 0xD9]).is_err());
        assert!(!super::is_jpeg12_stream(&[0xFF, 0xD8]));
    }

    #[test]
    fn decode_jpeg_gray_12bit_sof1_synthetic() -> Result<(), Box<dyn std::error::Error>> {
        let mut stream: Vec<u8> = Vec::new();
        stream.extend_from_slice(&[0xFF, 0xD8]);
        stream.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x83, 0x10]);
        for _ in 0..64 {
            stream.extend_from_slice(&[0x00, 0x01]);
        }
        stream.extend_from_slice(&[
            0xFF, 0xC1, 0x00, 0x0B, 12, 0x00, 0x08, 0x00, 0x08, 1, 1, 0x11, 0,
        ]);
        stream.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x14, 0x00]);
        stream.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        stream.extend_from_slice(&[0]);
        stream.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x14, 0x10]);
        stream.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        stream.extend_from_slice(&[0x00]);
        stream.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0x00, 0, 63, 0]);
        stream.extend_from_slice(&[0x00, 0x00]);
        stream.extend_from_slice(&[0xFF, 0xD9]);

        assert!(super::is_jpeg12_stream(&stream));
        let dec = super::decode_jpeg_gray(&stream)?;
        assert_eq!(dec.width, 8);
        assert_eq!(dec.height, 8);
        assert_eq!(dec.precision, 12);
        assert_eq!(dec.samples.len(), 64);
        Ok(())
    }

    #[test]
    fn decode_jpeg_gray_12bit_with_dri_and_nonzero_coeffs() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut stream: Vec<u8> = Vec::new();
        stream.extend_from_slice(&[0xFF, 0xD8]);
        // DQT 16-bit
        stream.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x83, 0x10]);
        for _ in 0..64 {
            stream.extend_from_slice(&[0x00, 0x01]);
        }
        // DRI = 1
        stream.extend_from_slice(&[0xFF, 0xDD, 0x00, 0x04, 0x00, 0x01]);
        // SOF1 16x8, 12-bit
        stream.extend_from_slice(&[
            0xFF, 0xC1, 0x00, 0x0B, 12, 0x00, 0x08, 0x00, 0x10, 1, 1, 0x11, 0,
        ]);
        // DHT DC: '0'->0, '10'->1
        stream.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x15, 0x00]);
        stream.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        stream.extend_from_slice(&[0x00, 0x01]);
        // DHT AC: '0'->0x00 (EOB), '10'->0x01 (run=0, cat=1)
        stream.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x15, 0x10]);
        stream.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        stream.extend_from_slice(&[0x00, 0x01]);
        // SOS
        stream.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0x00, 0, 63, 0]);
        // Block 0: DC='10'(cat 1) + '1'(+1), AC='10'(0x01) + '1'(+1), EOB='0' -> 10_1_10_1_0 = 0xB4
        // RST0 marker: FF D0
        // Block 1: DC='0', EOB='0' -> 0x00
        stream.extend_from_slice(&[0xB4, 0xFF, 0xD0, 0x00]);
        stream.extend_from_slice(&[0xFF, 0xD9]);

        let dec = super::decode_jpeg_gray(&stream)?;
        assert_eq!((dec.width, dec.height, dec.precision), (16, 8, 12));
        assert_eq!(dec.samples.len(), 128);
        Ok(())
    }
}
