//! Synthetic deterministic RAW stream and CFA buffer generator for headless testing.

use byteorder::{BigEndian, ByteOrder};
use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, CoreError, NoiseProfile, NoiseProfileSource,
    PhotoMetadata, PickStatus, SignedCfaBuffer,
};

use crate::DecodeError;

/// Builder for deterministic synthetic RAW fixtures and binary DNG streams.
pub struct SyntheticRawBuilder;

impl SyntheticRawBuilder {
    /// Generates a binary DNG `OpcodeList` byte stream containing:
    /// - Opcode 9: `GainMap` (4x4 gain grid)
    /// - Opcode 3: `FixVignetteRadial`
    pub fn build_synthetic_opcode_list_stream() -> Result<Vec<u8>, DecodeError> {
        let mut stream = Vec::new();

        // 1. Header: count = 2
        let mut count_buf = [0u8; 4];
        BigEndian::write_u32(&mut count_buf, 2);
        stream.extend_from_slice(&count_buf);

        // 2. Opcode 9: GainMap
        let mut op9_header = [0u8; 16];
        BigEndian::write_u32(&mut op9_header[0..4], 9); // opcode_id
        BigEndian::write_u32(&mut op9_header[4..8], 0x0103_0000); // dng_version 1.3.0.0
        BigEndian::write_u32(&mut op9_header[8..12], 0); // flags
        let gain_map_payload_size = 56 + (4 * 4 * 4); // 56 header + 16 floats = 120 bytes
        BigEndian::write_u32(&mut op9_header[12..16], gain_map_payload_size as u32);
        stream.extend_from_slice(&op9_header);

        let mut op9_payload = vec![0u8; gain_map_payload_size];
        BigEndian::write_f64(&mut op9_payload[0..8], 0.0); // top
        BigEndian::write_f64(&mut op9_payload[8..16], 0.0); // left
        BigEndian::write_f64(&mut op9_payload[16..24], 1.0); // bottom
        BigEndian::write_f64(&mut op9_payload[24..32], 1.0); // right
        BigEndian::write_u32(&mut op9_payload[32..36], 0); // plane
        BigEndian::write_u32(&mut op9_payload[36..40], 1); // planes
        BigEndian::write_u32(&mut op9_payload[40..44], 1); // row_pitch
        BigEndian::write_u32(&mut op9_payload[44..48], 1); // col_pitch
        BigEndian::write_u32(&mut op9_payload[48..52], 4); // map_points_v
        BigEndian::write_u32(&mut op9_payload[52..56], 4); // map_points_h

        let mut offset = 56;
        for _ in 0..16 {
            BigEndian::write_f32(&mut op9_payload[offset..offset + 4], 1.05);
            offset += 4;
        }
        stream.extend_from_slice(&op9_payload);

        // 3. Opcode 3: FixVignetteRadial
        let mut op3_header = [0u8; 16];
        BigEndian::write_u32(&mut op3_header[0..4], 3); // opcode_id
        BigEndian::write_u32(&mut op3_header[4..8], 0x0103_0000);
        BigEndian::write_u32(&mut op3_header[8..12], 0);
        let vignette_payload_size = 56;
        BigEndian::write_u32(&mut op3_header[12..16], vignette_payload_size as u32);
        stream.extend_from_slice(&op3_header);

        let mut op3_payload = [0u8; 56];
        BigEndian::write_f64(&mut op3_payload[0..8], 1.0); // k0
        BigEndian::write_f64(&mut op3_payload[8..16], -0.15); // k1
        BigEndian::write_f64(&mut op3_payload[16..24], 0.05); // k2
        BigEndian::write_f64(&mut op3_payload[24..32], -0.01); // k3
        BigEndian::write_f64(&mut op3_payload[32..40], 0.002); // k4
        BigEndian::write_f64(&mut op3_payload[40..48], 0.5); // center_x
        BigEndian::write_f64(&mut op3_payload[48..56], 0.5); // center_y
        stream.extend_from_slice(&op3_payload);

        Ok(stream)
    }

    /// Generates a synthetic flat CFA buffer populated with deterministic Gaussian noise.
    pub fn generate_flat_cfa_with_noise(
        width: usize,
        height: usize,
        pattern: CfaPattern,
        base_level: f32,
        sigma: f32,
    ) -> Result<SignedCfaBuffer, DecodeError> {
        let total = width
            .checked_mul(height)
            .ok_or(CoreError::InvalidDimensions { width, height })?;

        let mut samples = Vec::with_capacity(total);
        let mut rng = DeterministicRng::new(0x1337_c0de);

        for _ in 0..total {
            let noise = rng.next_gaussian() * sigma;
            samples.push(base_level + noise);
        }

        Ok(SignedCfaBuffer::new(width, height, pattern, samples)?)
    }

    /// Generates a raw uint16 dark frame with Gaussian noise centered around `black_level`.
    pub fn generate_dark_frame_raw(
        width: usize,
        height: usize,
        black_level: f32,
        white_level: f32,
        noise_sigma: f32,
    ) -> Result<(Vec<u16>, PhotoMetadata), DecodeError> {
        let total = width
            .checked_mul(height)
            .ok_or(CoreError::InvalidDimensions { width, height })?;

        let mut raw_samples = Vec::with_capacity(total);
        let mut rng = DeterministicRng::new(0xdead_beef);

        for _ in 0..total {
            let noise = rng.next_gaussian() * noise_sigma;
            let val = (black_level + noise).round();
            let clamped = val.clamp(0.0, 65535.0) as u16;
            raw_samples.push(clamped);
        }

        let meta = PhotoMetadata {
            camera_make: "Synthetic".to_string(),
            camera_model: "DarkFrameRig".to_string(),
            lens_model: None,
            focal_length_mm: None,
            aperture_f_number: None,
            iso: Some(100),
            width,
            height,
            black_levels: [black_level; 4],
            white_levels: [white_level; 4],
            cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
            noise_profile: NoiseProfile::new(
                [0.001; 3],
                [0.0001; 3],
                NoiseProfileSource::DngTag0xc761,
            )?,
            vignette_handled_by_gainmap: false,
            rating: 0,
            pick: PickStatus::Unflagged,
            label: ColorLabel::None,
            keywords: Vec::new(),
        };

        Ok((raw_samples, meta))
    }
}

/// Simple, deterministic pseudo-random number generator using Xorshift64* and Box-Muller transform.
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x5a17_u64 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn next_f32(&mut self) -> f32 {
        // Uniform in (0, 1]
        let v = (self.next_u64() >> 40) as u32;
        (v as f32 + 1.0) / 16_777_217.0
    }

    /// Box-Muller transform producing standard normal variate N(0, 1).
    fn next_gaussian(&mut self) -> f32 {
        let u1 = self.next_f32();
        let u2 = self.next_f32();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}
