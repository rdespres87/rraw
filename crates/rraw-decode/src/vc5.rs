#![forbid(unsafe_code)]

//! Pure-Rust SMPTE ST 2073-1 (VC-5) wavelet decoder for GoPro `.GPR` Bayer RAW images.
//!
//! Decodes 4-channel Bayer wavelet bitstreams (3-level 2-6 biorthogonal inverse wavelet transform,
//! Table 17 run-length VLC entropy coding, cubic highpass uncompanding, and inverse Protune log
//! curve) back into a single-plane `u16` Bayer mosaic buffer.

use rraw_core::{BayerOrigin, CoreError};

use crate::vc5_table17::TABLE17;
use crate::DecodeError;

/// Maximum wavelet decomposition levels supported for VC-5 Bayer streams.
const WAVELET_LEVELS: usize = 3;
/// Number of subbands per channel for a 3-level 2D wavelet (`LL3` + 3 × `{LH, HL, HH}`).
const SUBBANDS_PER_CHANNEL: usize = 10;
/// Number of Bayer component channels (`GS`, `RG`, `BG`, `GD`).
const BAYER_CHANNELS: usize = 4;

/// Binary trie node for SMPTE ST 2073-1 Table 17 VLC decoding.
#[derive(Debug, Clone, Copy, Default)]
struct TrieNode {
    child: [u16; 2],
    /// Non-zero when this node is a leaf: stores `index + 1` into `TABLE17`.
    entry_plus_one: u16,
}

/// Decoded VC-5 Table 17 symbol.
#[derive(Debug, Clone, Copy)]
struct VlcSymbol {
    run_count: u16,
    magnitude: u16,
}

struct VlcTrie {
    nodes: Vec<TrieNode>,
}

impl VlcTrie {
    fn build() -> Result<Self, DecodeError> {
        let mut nodes = vec![TrieNode::default()];
        for (idx, &(bit_len, code, _run, _mag)) in TABLE17.iter().enumerate() {
            let mut cur = 0usize;
            for shift in (0..bit_len).rev() {
                let bit = ((code >> shift) & 1) as usize;
                let next = nodes[cur].child[bit];
                if next == 0 {
                    let new_idx = u16::try_from(nodes.len()).map_err(|_| {
                        DecodeError::MalformedMetadata("VC-5 VLC trie overflow".into())
                    })?;
                    nodes.push(TrieNode::default());
                    nodes[cur].child[bit] = new_idx;
                    cur = usize::from(new_idx);
                } else {
                    cur = usize::from(next);
                }
            }
            nodes[cur].entry_plus_one = u16::try_from(idx + 1).map_err(|_| {
                DecodeError::MalformedMetadata("VC-5 VLC entry index overflow".into())
            })?;
        }
        Ok(Self { nodes })
    }

    #[inline]
    fn decode_symbol(&self, reader: &mut BitReader<'_>) -> Result<VlcSymbol, DecodeError> {
        let mut cur = 0usize;
        for _ in 0..32 {
            let bit = reader.read_bit()?;
            let next = self.nodes[cur].child[bit];
            if next == 0 {
                return Err(DecodeError::MalformedMetadata(
                    "invalid VC-5 VLC codeword".into(),
                ));
            }
            cur = usize::from(next);
            let entry = self.nodes[cur].entry_plus_one;
            if entry != 0 {
                let &(_, _, run_count, magnitude) = TABLE17
                    .get(usize::from(entry - 1))
                    .ok_or_else(|| DecodeError::MalformedMetadata("VC-5 VLC table OOB".into()))?;
                return Ok(VlcSymbol {
                    run_count,
                    magnitude,
                });
            }
        }
        Err(DecodeError::MalformedMetadata(
            "VC-5 VLC codeword exceeded maximum length".into(),
        ))
    }
}

/// MSB-first bitstream reader over concatenated 32-bit VC-5 chunk slices.
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u64,
    bits_in_buf: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bits_in_buf: 0,
        }
    }

    #[inline]
    fn refill(&mut self) {
        while self.bits_in_buf <= 56 && self.byte_pos < self.data.len() {
            self.bit_buf = (self.bit_buf << 8) | u64::from(self.data[self.byte_pos]);
            self.byte_pos += 1;
            self.bits_in_buf += 8;
        }
    }

    #[inline]
    fn read_bit(&mut self) -> Result<usize, DecodeError> {
        if self.bits_in_buf == 0 {
            self.refill();
            if self.bits_in_buf == 0 {
                return Err(DecodeError::MalformedMetadata(
                    "unexpected EOF in VC-5 highpass bitstream".into(),
                ));
            }
        }
        self.bits_in_buf -= 1;
        Ok(((self.bit_buf >> self.bits_in_buf) & 1) as usize)
    }
}

/// Builds the 256-entry cubic uncompanding LUT: `m + floor((768 * m^3) / 255^3)`.
fn build_uncompand_lut() -> [i32; 256] {
    let mut lut = [0i32; 256];
    let denom = 255.0_f64 * 255.0_f64 * 255.0_f64;
    for (m, slot) in lut.iter_mut().enumerate() {
        let mf = m as f64;
        let cubic = (mf * mf * mf * 768.0) / denom;
        *slot = (m as i32) + (cubic as i32);
    }
    lut
}

/// Builds the 4096-entry GoPro inverse Protune log LUT (`12-bit -> 16-bit`).
fn build_decoder_log_lut() -> [u16; 4096] {
    let mut lut = [0u16; 4096];
    for (i, slot) in lut.iter_mut().enumerate() {
        let output = 65535.0_f64 * (113.0_f64.powf(i as f64 / 4095.0_f64) - 1.0_f64) / 112.0_f64;
        *slot = (output.clamp(0.0, 65535.0)) as u16;
    }
    lut
}

#[inline]
fn clamp_i16_pixel(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[inline]
fn clamp_u14_as_i16(v: i32) -> i16 {
    v.clamp(0, 16383) as i16
}

/// Decodes a highpass subband using Table 17 run-length VLC + sign bit + cubic uncompanding + dequantization.
fn decode_highpass_subband(
    chunk_data: &[u8],
    width: usize,
    height: usize,
    quant: i32,
    trie: &VlcTrie,
    uncompand: &[i32; 256],
) -> Result<Vec<i16>, DecodeError> {
    let total = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;
    let mut out = vec![0i16; total];
    let mut reader = BitReader::new(chunk_data);
    let q = quant.max(1);
    let mut written = 0usize;

    while written < total {
        let sym = trie.decode_symbol(&mut reader)?;
        if sym.run_count == 0 {
            // Special band-end marker encountered early; remaining samples stay zero.
            break;
        }
        let count = usize::from(sym.run_count);
        if sym.magnitude == 0 {
            // Run of zeros (output buffer is already zero-initialized).
            written = written.saturating_add(count).min(total);
        } else {
            let sign_bit = reader.read_bit()?;
            let mag = uncompand
                .get(usize::from(sym.magnitude))
                .copied()
                .unwrap_or_else(|| i32::from(sym.magnitude));
            let signed = if sign_bit == 1 { -mag } else { mag };
            let dequant = clamp_i16_pixel(signed.saturating_mul(q));
            let end = written.saturating_add(count).min(total);
            for slot in &mut out[written..end] {
                *slot = dequant;
            }
            written = end;
        }
    }

    Ok(out)
}

/// Applies the 1D inverse 2-6 biorthogonal wavelet filter along a single row.
fn invert_horizontal_2_6(
    lowpass: &[i16],
    highpass: &[i16],
    output: &mut [i16],
    in_w: usize,
    out_w: usize,
    descale: u8,
) {
    if in_w == 0 || out_w == 0 {
        return;
    }
    if in_w < 3 {
        for x in 0..in_w {
            let l = i32::from(lowpass[x]);
            let h = i32::from(highpass[x]);
            let (e, o) = apply_descale(l + h, l - h, descale);
            if 2 * x < out_w {
                output[2 * x] = clamp_u14_as_i16(e);
            }
            if 2 * x + 1 < out_w {
                output[2 * x + 1] = clamp_u14_as_i16(o);
            }
        }
        return;
    }

    // Left border (x = 0)
    {
        let l0 = i32::from(lowpass[0]);
        let l1 = i32::from(lowpass[1]);
        let l2 = i32::from(lowpass[2]);
        let h0 = i32::from(highpass[0]);
        let even = ((11 * l0 - 4 * l1 + l2 + 4) >> 3) + h0;
        let odd = ((5 * l0 + 4 * l1 - l2 + 4) >> 3) - h0;
        let (e, o) = apply_descale(even, odd, descale);
        output[0] = clamp_u14_as_i16(e);
        if out_w > 1 {
            output[1] = clamp_u14_as_i16(o);
        }
    }

    // Interior columns
    let last = in_w - 1;
    for x in 1..last {
        let lm = i32::from(lowpass[x - 1]);
        let lc = i32::from(lowpass[x]);
        let lp = i32::from(lowpass[x + 1]);
        let hc = i32::from(highpass[x]);
        let even = lc + ((lm - lp + 4) >> 3) + hc;
        let odd = lc + ((lp - lm + 4) >> 3) - hc;
        let (e, o) = apply_descale(even, odd, descale);
        output[2 * x] = clamp_u14_as_i16(e);
        if 2 * x + 1 < out_w {
            output[2 * x + 1] = clamp_u14_as_i16(o);
        }
    }

    // Right border (x = last)
    {
        let l0 = i32::from(lowpass[last]);
        let l1 = i32::from(lowpass[last - 1]);
        let l2 = i32::from(lowpass[last - 2]);
        let h = i32::from(highpass[last]);
        let even = ((5 * l0 + 4 * l1 - l2 + 4) >> 3) + h;
        let odd = ((11 * l0 - 4 * l1 + l2 + 4) >> 3) - h;
        let (e, o) = apply_descale(even, odd, descale);
        if 2 * last < out_w {
            output[2 * last] = clamp_u14_as_i16(e);
        }
        if 2 * last + 1 < out_w {
            output[2 * last + 1] = clamp_u14_as_i16(o);
        }
    }
}

#[inline]
fn apply_descale(even: i32, odd: i32, descale: u8) -> (i32, i32) {
    match descale {
        0 => (even >> 1, odd >> 1),
        1 => (even, odd),
        s => {
            let shift = u32::from(s - 1);
            (even << shift, odd << shift)
        }
    }
}

/// Applies the 2D inverse 2-6 wavelet transform for one decomposition level:
/// vertical 2-6 on `(LL, HL)` and `(LH, HH)` followed by horizontal 2-6 into `out_w × out_h`.
#[allow(clippy::too_many_arguments)]
fn invert_2d_level(
    ll: &[i16],
    lh: &[i16],
    hl: &[i16],
    hh: &[i16],
    in_w: usize,
    in_h: usize,
    out_w: usize,
    out_h: usize,
    descale: u8,
) -> Vec<i16> {
    let mut output = vec![0i16; out_w * out_h];
    let mut even_lp = vec![0i16; in_w];
    let mut odd_lp = vec![0i16; in_w];
    let mut even_hp = vec![0i16; in_w];
    let mut odd_hp = vec![0i16; in_w];

    let last_row = in_h.saturating_sub(1);

    for y in 0..in_h {
        let r_hl = &hl[y * in_w..(y + 1) * in_w];
        let r_hh = &hh[y * in_w..(y + 1) * in_w];

        if in_h < 3 {
            let r_ll = &ll[y * in_w..(y + 1) * in_w];
            let r_lh = &lh[y * in_w..(y + 1) * in_w];
            for x in 0..in_w {
                even_lp[x] = clamp_i16_pixel((i32::from(r_ll[x]) + i32::from(r_hl[x])) >> 1);
                odd_lp[x] = clamp_i16_pixel((i32::from(r_ll[x]) - i32::from(r_hl[x])) >> 1);
                even_hp[x] = clamp_i16_pixel((i32::from(r_lh[x]) + i32::from(r_hh[x])) >> 1);
                odd_hp[x] = clamp_i16_pixel((i32::from(r_lh[x]) - i32::from(r_hh[x])) >> 1);
            }
        } else if y == 0 {
            let ll0 = &ll[0..in_w];
            let ll1 = &ll[in_w..2 * in_w];
            let ll2 = &ll[2 * in_w..3 * in_w];
            let lh0 = &lh[0..in_w];
            let lh1 = &lh[in_w..2 * in_w];
            let lh2 = &lh[2 * in_w..3 * in_w];
            for x in 0..in_w {
                let e_l =
                    (((11 * i32::from(ll0[x]) - 4 * i32::from(ll1[x]) + i32::from(ll2[x]) + 4)
                        >> 3)
                        + i32::from(r_hl[x]))
                        >> 1;
                let o_l = (((5 * i32::from(ll0[x]) + 4 * i32::from(ll1[x]) - i32::from(ll2[x])
                    + 4)
                    >> 3)
                    - i32::from(r_hl[x]))
                    >> 1;
                let e_h =
                    (((11 * i32::from(lh0[x]) - 4 * i32::from(lh1[x]) + i32::from(lh2[x]) + 4)
                        >> 3)
                        + i32::from(r_hh[x]))
                        >> 1;
                let o_h = (((5 * i32::from(lh0[x]) + 4 * i32::from(lh1[x]) - i32::from(lh2[x])
                    + 4)
                    >> 3)
                    - i32::from(r_hh[x]))
                    >> 1;
                even_lp[x] = clamp_i16_pixel(e_l);
                odd_lp[x] = clamp_i16_pixel(o_l);
                even_hp[x] = clamp_i16_pixel(e_h);
                odd_hp[x] = clamp_i16_pixel(o_h);
            }
        } else if y == last_row {
            let ll0 = &ll[last_row * in_w..(last_row + 1) * in_w];
            let ll1 = &ll[(last_row - 1) * in_w..last_row * in_w];
            let ll2 = &ll[(last_row - 2) * in_w..(last_row - 1) * in_w];
            let lh0 = &lh[last_row * in_w..(last_row + 1) * in_w];
            let lh1 = &lh[(last_row - 1) * in_w..last_row * in_w];
            let lh2 = &lh[(last_row - 2) * in_w..(last_row - 1) * in_w];
            for x in 0..in_w {
                let e_l = (((5 * i32::from(ll0[x]) + 4 * i32::from(ll1[x]) - i32::from(ll2[x])
                    + 4)
                    >> 3)
                    + i32::from(r_hl[x]))
                    >> 1;
                let o_l =
                    (((11 * i32::from(ll0[x]) - 4 * i32::from(ll1[x]) + i32::from(ll2[x]) + 4)
                        >> 3)
                        - i32::from(r_hl[x]))
                        >> 1;
                let e_h = (((5 * i32::from(lh0[x]) + 4 * i32::from(lh1[x]) - i32::from(lh2[x])
                    + 4)
                    >> 3)
                    + i32::from(r_hh[x]))
                    >> 1;
                let o_h =
                    (((11 * i32::from(lh0[x]) - 4 * i32::from(lh1[x]) + i32::from(lh2[x]) + 4)
                        >> 3)
                        - i32::from(r_hh[x]))
                        >> 1;
                even_lp[x] = clamp_i16_pixel(e_l);
                odd_lp[x] = clamp_i16_pixel(o_l);
                even_hp[x] = clamp_i16_pixel(e_h);
                odd_hp[x] = clamp_i16_pixel(o_h);
            }
        } else {
            let llm = &ll[(y - 1) * in_w..y * in_w];
            let llc = &ll[y * in_w..(y + 1) * in_w];
            let llp = &ll[(y + 1) * in_w..(y + 2) * in_w];
            let lhm = &lh[(y - 1) * in_w..y * in_w];
            let lhc = &lh[y * in_w..(y + 1) * in_w];
            let lhp = &lh[(y + 1) * in_w..(y + 2) * in_w];
            for x in 0..in_w {
                let e_l = (i32::from(llc[x])
                    + ((i32::from(llm[x]) - i32::from(llp[x]) + 4) >> 3)
                    + i32::from(r_hl[x]))
                    >> 1;
                let o_l = (i32::from(llc[x]) + ((i32::from(llp[x]) - i32::from(llm[x]) + 4) >> 3)
                    - i32::from(r_hl[x]))
                    >> 1;
                let e_h = (i32::from(lhc[x])
                    + ((i32::from(lhm[x]) - i32::from(lhp[x]) + 4) >> 3)
                    + i32::from(r_hh[x]))
                    >> 1;
                let o_h = (i32::from(lhc[x]) + ((i32::from(lhp[x]) - i32::from(lhm[x]) + 4) >> 3)
                    - i32::from(r_hh[x]))
                    >> 1;
                even_lp[x] = clamp_i16_pixel(e_l);
                odd_lp[x] = clamp_i16_pixel(o_l);
                even_hp[x] = clamp_i16_pixel(e_h);
                odd_hp[x] = clamp_i16_pixel(o_h);
            }
        }

        let out_y0 = 2 * y;
        if out_y0 < out_h {
            invert_horizontal_2_6(
                &even_lp,
                &even_hp,
                &mut output[out_y0 * out_w..(out_y0 + 1) * out_w],
                in_w,
                out_w,
                descale,
            );
        }
        let out_y1 = 2 * y + 1;
        if out_y1 < out_h {
            invert_horizontal_2_6(
                &odd_lp,
                &odd_hp,
                &mut output[out_y1 * out_w..(out_y1 + 1) * out_w],
                in_w,
                out_w,
                descale,
            );
        }
    }

    output
}

/// Decodes a VC-5 (SMPTE ST 2073-1) Bayer bitstream into a single-plane `u16` Bayer buffer
/// of dimensions `expected_width × expected_height`.
pub fn decode_vc5_bayer(
    data: &[u8],
    expected_width: usize,
    expected_height: usize,
    bayer_origin: BayerOrigin,
    output_bit_depth: u16,
) -> Result<Vec<u16>, DecodeError> {
    if data.len() < 8 || &data[0..4] != b"VC-5" {
        return Err(DecodeError::UnsupportedFormat(
            "missing VC-5 bitstream magic header".into(),
        ));
    }

    let mut width = expected_width;
    let mut height = expected_height;
    let mut channel_count = BAYER_CHANNELS;
    let mut cur_channel = 0usize;
    let mut cur_subband = 0usize;
    let mut cur_quant = 0i32;
    let mut prescale = [0u8, 2, 2];

    let mut subband_payloads: Vec<Vec<Vec<u8>>> =
        vec![vec![Vec::new(); SUBBANDS_PER_CHANNEL]; BAYER_CHANNELS];
    let mut subband_quants: Vec<[i32; SUBBANDS_PER_CHANNEL]> =
        vec![[1i32; SUBBANDS_PER_CHANNEL]; BAYER_CHANNELS];

    let mut pos = 4usize;
    while pos + 4 <= data.len() {
        let tag = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let val = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        pos += 4;

        match tag {
            0x000c => channel_count = usize::from(val),
            0x0014 => width = usize::from(val),
            0x0015 => height = usize::from(val),
            0x003e => cur_channel = usize::from(val),
            0x0030 => cur_subband = usize::from(val),
            0x0035 => {
                cur_quant = i32::from(val);
                if cur_channel < BAYER_CHANNELS && cur_subband < SUBBANDS_PER_CHANNEL {
                    subband_quants[cur_channel][cur_subband] = cur_quant;
                }
            }
            0x006d => {
                // PrescaleShift: 2-bit shift per wavelet level starting from MSB
                prescale = [
                    ((val >> 14) & 3) as u8,
                    ((val >> 12) & 3) as u8,
                    ((val >> 10) & 3) as u8,
                ];
            }
            // Chunk payload:
            // - Negated large chunk payload (`0x9f00..=0xa000`): `size_hi = 0xa000 - tag`,
            //   24-bit word count `((size_hi << 16) | val)`.
            // - Positive chunk payload (`0x2000..=0x20ff`): `size_hi = tag & 0x00ff`.
            0x9f00..=0xa000 | 0x2000..=0x20ff => {
                let size_hi = if tag >= 0x9f00 {
                    usize::from(0xa000 - tag)
                } else {
                    usize::from(tag & 0x00ff)
                };
                let words = (size_hi << 16) | usize::from(val);
                let byte_len = words.checked_mul(4).ok_or_else(|| {
                    DecodeError::MalformedMetadata("VC-5 chunk size overflow".into())
                })?;
                let end = pos.checked_add(byte_len).ok_or_else(|| {
                    DecodeError::MalformedMetadata("VC-5 chunk size overflow".into())
                })?;
                let payload = data.get(pos..end).ok_or_else(|| {
                    DecodeError::MalformedMetadata("VC-5 chunk extends past EOF".into())
                })?;
                if cur_channel < BAYER_CHANNELS && cur_subband < SUBBANDS_PER_CHANNEL {
                    subband_payloads[cur_channel][cur_subband].extend_from_slice(payload);
                    if cur_quant > 0 {
                        subband_quants[cur_channel][cur_subband] = cur_quant;
                    }
                }
                pos = end;
            }
            // Optional metadata / unique-ID small chunk (`0xbffc` = -0x4004): skip `val` words
            0xbffc | 0x3ffc => {
                let byte_len = usize::from(val) * 4;
                pos = pos.saturating_add(byte_len).min(data.len());
            }
            // Container headers (`0xe000`, `0xdxxx`, etc.) and informational tags: continue scanning
            _ => {}
        }
    }

    if channel_count != BAYER_CHANNELS {
        return Err(DecodeError::UnsupportedFormat(format!(
            "unsupported VC-5 channel count {channel_count} (expected 4 for Bayer)"
        )));
    }
    if width < 8 || height < 8 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    }

    let cw = width / 2;
    let ch = height / 2;
    let w1 = cw.div_ceil(2);
    let h1 = ch.div_ceil(2);
    let w2 = w1.div_ceil(2);
    let h2 = h1.div_ceil(2);
    let w3 = w2.div_ceil(2);
    let h3 = h2.div_ceil(2);

    let trie = VlcTrie::build()?;
    let uncompand = build_uncompand_lut();
    let log_lut = build_decoder_log_lut();

    let mut channels: Vec<Vec<i16>> = Vec::with_capacity(BAYER_CHANNELS);
    for ch_idx in 0..BAYER_CHANNELS {
        // Subband 0: uncompressed 16-bit big-endian LL3
        let ll3_bytes = &subband_payloads[ch_idx][0];
        let ll3_len = w3 * h3;
        if ll3_bytes.len() < ll3_len * 2 {
            return Err(DecodeError::MalformedMetadata(format!(
                "VC-5 channel {ch_idx} LL3 too short: {} < {}",
                ll3_bytes.len(),
                ll3_len * 2
            )));
        }
        let (pairs, _) = ll3_bytes[..ll3_len * 2].as_chunks::<2>();
        let mut ll: Vec<i16> = pairs
            .iter()
            .map(|&p| u16::from_be_bytes(p).min(i16::MAX as u16) as i16)
            .collect();

        // Level 3 -> Level 2 -> Level 1 -> Full channel
        let levels: [(usize, usize, usize, usize, usize, u8); WAVELET_LEVELS] = [
            (w3, h3, w2, h2, 1, prescale[2]),
            (w2, h2, w1, h1, 4, prescale[1]),
            (w1, h1, cw, ch, 7, prescale[0]),
        ];

        for (in_w, in_h, out_w, out_h, base_sb, descale) in levels {
            let lh = decode_highpass_subband(
                &subband_payloads[ch_idx][base_sb],
                in_w,
                in_h,
                subband_quants[ch_idx][base_sb],
                &trie,
                &uncompand,
            )?;
            let hl = decode_highpass_subband(
                &subband_payloads[ch_idx][base_sb + 1],
                in_w,
                in_h,
                subband_quants[ch_idx][base_sb + 1],
                &trie,
                &uncompand,
            )?;
            let hh = decode_highpass_subband(
                &subband_payloads[ch_idx][base_sb + 2],
                in_w,
                in_h,
                subband_quants[ch_idx][base_sb + 2],
                &trie,
                &uncompand,
            )?;
            ll = invert_2d_level(&ll, &lh, &hl, &hh, in_w, in_h, out_w, out_h, descale);
        }

        channels.push(ll);
    }

    // Recombine 4 VC-5 Bayer channels (GS, RG, BG, GD) -> 2x2 Bayer mosaic with inverse Protune log
    let shift = 16u16.saturating_sub(output_bit_depth.clamp(8, 16));
    let mut bayer = vec![0u16; width * height];
    let gs_ch = &channels[0];
    let rg_ch = &channels[1];
    let bg_ch = &channels[2];
    let gd_ch = &channels[3];

    for y in 0..ch {
        let row_in = y * cw;
        let row_out0 = (2 * y) * width;
        let row_out1 = (2 * y + 1) * width;
        for x in 0..cw {
            let idx = row_in + x;
            let gs = i32::from(gs_ch[idx]);
            let rg = i32::from(rg_ch[idx]) - 2048;
            let bg = i32::from(bg_ch[idx]) - 2048;
            let gd = i32::from(gd_ch[idx]) - 2048;

            let r_i = ((rg << 1) + gs).clamp(0, 4095) as usize;
            let b_i = ((bg << 1) + gs).clamp(0, 4095) as usize;
            let g1_i = (gs + gd).clamp(0, 4095) as usize;
            let g2_i = (gs - gd).clamp(0, 4095) as usize;

            let r = log_lut[r_i] >> shift;
            let b = log_lut[b_i] >> shift;
            let g1 = log_lut[g1_i] >> shift;
            let g2 = log_lut[g2_i] >> shift;

            let (p00, p01, p10, p11) = match bayer_origin {
                BayerOrigin::Rggb => (r, g1, g2, b),
                BayerOrigin::Bggr => (b, g1, g2, r),
                BayerOrigin::Grbg => (g1, r, b, g2),
                BayerOrigin::Gbrg => (g1, b, r, g2),
            };
            bayer[row_out0 + 2 * x] = p00;
            bayer[row_out0 + 2 * x + 1] = p01;
            bayer[row_out1 + 2 * x] = p10;
            bayer[row_out1 + 2 * x + 1] = p11;
        }
    }

    Ok(bayer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vlc_trie_decodes_basic_table17_symbols() -> Result<(), Box<dyn std::error::Error>> {
        let trie = VlcTrie::build()?;
        // Bit 0 -> (run=1, mag=0); bits 10 -> (run=1, mag=1); bits 111 -> (run=1, mag=2)
        // Byte: 0_10_111_00 = 0b0101_1100 = 0x5C
        let buf = [0x5Cu8];
        let mut reader = BitReader::new(&buf);
        let s0 = trie.decode_symbol(&mut reader)?;
        assert_eq!((s0.run_count, s0.magnitude), (1, 0));
        let s1 = trie.decode_symbol(&mut reader)?;
        assert_eq!((s1.run_count, s1.magnitude), (1, 1));
        let s2 = trie.decode_symbol(&mut reader)?;
        assert_eq!((s2.run_count, s2.magnitude), (1, 2));
        Ok(())
    }

    #[test]
    fn test_uncompand_and_decoder_log_lut_monotonic() {
        let uncompand = build_uncompand_lut();
        assert_eq!(uncompand[0], 0);
        assert_eq!(uncompand[1], 1);
        assert_eq!(uncompand[255], 255 + 768);
        for w in uncompand.windows(2) {
            assert!(w[1] >= w[0]);
        }

        let log_lut = build_decoder_log_lut();
        assert_eq!(log_lut[0], 0);
        assert_eq!(log_lut[4095], 65535);
        for w in log_lut.windows(2) {
            assert!(w[1] >= w[0]);
        }
    }

    #[test]
    fn test_invert_2d_level_preserves_constant_field() {
        // Vertical pass divides by 2; horizontal pass with descale=2 multiplies by 2,
        // restoring the exact constant level 1000.
        let ll = vec![1000i16; 16];
        let zero = vec![0i16; 16];
        let out = invert_2d_level(&ll, &zero, &zero, &zero, 4, 4, 8, 8, 2);
        assert_eq!(out.len(), 64);
        for &v in &out {
            assert!((i32::from(v) - 1000).abs() <= 1, "expected ~1000, got {v}");
        }
    }
}
