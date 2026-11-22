use crate::model::OpticalCorrectionModel;

/// $256 \times 256$ per-channel UV mapping lookup table.
#[derive(Debug, Clone)]
pub struct UvLutTable {
    pub dimension: usize,
    /// Interleaved [U, V] normalized coordinates for Red channel.
    pub uv_r: Vec<f32>,
    /// Interleaved [U, V] normalized coordinates for Green channel.
    pub uv_g: Vec<f32>,
    /// Interleaved [U, V] normalized coordinates for Blue channel.
    pub uv_b: Vec<f32>,
}

/// Generator for Stage 4 UV lookup tables.
pub struct UvLutGenerator;

impl UvLutGenerator {
    /// Generates $N \times N$ UV lookup tables for Red, Green, and Blue channels.
    ///
    /// Resolves radial distortion and lateral chromatic aberration (TCA).
    #[must_use]
    pub fn generate(model: &OpticalCorrectionModel, dimension: usize) -> UvLutTable {
        let total_samples = dimension * dimension * 2;
        let mut uv_r = Vec::with_capacity(total_samples);
        let mut uv_g = Vec::with_capacity(total_samples);
        let mut uv_b = Vec::with_capacity(total_samples);

        let inv_dim = 1.0 / (dimension as f32);

        for y in 0..dimension {
            for x in 0..dimension {
                // Centered normalized coordinates [-0.5, 0.5]
                let u = (x as f32 + 0.5) * inv_dim - 0.5;
                let v = (y as f32 + 0.5) * inv_dim - 0.5;

                let r = (u * u + v * v).sqrt();

                // Green channel: base distortion
                let r_d = model.distortion.evaluate_radius(r);
                let scale_g = if r > 1e-6 { r_d / r } else { 1.0 };

                let u_g = (0.5 + u * scale_g).clamp(0.0, 1.0);
                let v_g = (0.5 + v * scale_g).clamp(0.0, 1.0);

                // Red channel: scaled by lateral TCA
                let scale_r = scale_g * model.lateral_ca.scale_r;
                let u_r = (0.5 + u * scale_r).clamp(0.0, 1.0);
                let v_r = (0.5 + v * scale_r).clamp(0.0, 1.0);

                // Blue channel: scaled by lateral TCA
                let scale_b = scale_g * model.lateral_ca.scale_b;
                let u_b = (0.5 + u * scale_b).clamp(0.0, 1.0);
                let v_b = (0.5 + v * scale_b).clamp(0.0, 1.0);

                uv_r.push(u_r);
                uv_r.push(v_r);

                uv_g.push(u_g);
                uv_g.push(v_g);

                uv_b.push(u_b);
                uv_b.push(v_b);
            }
        }

        UvLutTable {
            dimension,
            uv_r,
            uv_g,
            uv_b,
        }
    }

    /// Generates a $N \times N$ UV lookup table directly from DNG `WarpRectilinear` opcode parameters.
    #[must_use]
    pub fn generate_from_dng_warp_rectilinear(
        planes: u32,
        coefficients: &[f64],
        dimension: usize,
    ) -> UvLutTable {
        let total_samples = dimension * dimension * 2;
        let mut uv_r = Vec::with_capacity(total_samples);
        let mut uv_g = Vec::with_capacity(total_samples);
        let mut uv_b = Vec::with_capacity(total_samples);

        let inv_dim = 1.0 / (dimension as f32);
        let plane_len = coefficients.len() / (planes as usize).max(1);

        let eval_plane = |plane_idx: usize, r: f32| -> f32 {
            let offset = plane_idx * plane_len;
            if offset + 4 <= coefficients.len() {
                let k0 = coefficients[offset] as f32;
                let k1 = coefficients[offset + 1] as f32;
                let k2 = coefficients[offset + 2] as f32;
                let k3 = coefficients[offset + 3] as f32;
                let r2 = r * r;
                let r4 = r2 * r2;
                let r6 = r4 * r2;
                k0 * r + k1 * r * r2 + k2 * r * r4 + k3 * r * r6
            } else {
                r
            }
        };

        let (has_tca, r_idx, g_idx, b_idx) = if planes >= 3 && coefficients.len() >= 3 * plane_len {
            (true, 0, 1, 2)
        } else {
            (false, 0, 0, 0)
        };

        for y in 0..dimension {
            for x in 0..dimension {
                let u = (x as f32 + 0.5) * inv_dim - 0.5;
                let v = (y as f32 + 0.5) * inv_dim - 0.5;
                let r = (u * u + v * v).sqrt();

                if r <= 1e-6 {
                    uv_r.push(0.5);
                    uv_r.push(0.5);
                    uv_g.push(0.5);
                    uv_g.push(0.5);
                    uv_b.push(0.5);
                    uv_b.push(0.5);
                    continue;
                }

                let r_g = eval_plane(g_idx, r);
                let scale_g = if r_g.is_finite() && r > 1e-6 {
                    (r_g / r).clamp(0.0, 10.0)
                } else {
                    1.0
                };
                let u_g = (0.5 + u * scale_g).clamp(0.0, 1.0);
                let v_g = (0.5 + v * scale_g).clamp(0.0, 1.0);
                uv_g.push(u_g);
                uv_g.push(v_g);

                if has_tca {
                    let r_r = eval_plane(r_idx, r);
                    let scale_r = if r_r.is_finite() && r > 1e-6 {
                        (r_r / r).clamp(0.0, 10.0)
                    } else {
                        scale_g
                    };
                    uv_r.push((0.5 + u * scale_r).clamp(0.0, 1.0));
                    uv_r.push((0.5 + v * scale_r).clamp(0.0, 1.0));

                    let r_b = eval_plane(b_idx, r);
                    let scale_b = if r_b.is_finite() && r > 1e-6 {
                        (r_b / r).clamp(0.0, 10.0)
                    } else {
                        scale_g
                    };
                    uv_b.push((0.5 + u * scale_b).clamp(0.0, 1.0));
                    uv_b.push((0.5 + v * scale_b).clamp(0.0, 1.0));
                } else {
                    uv_r.push(u_g);
                    uv_r.push(v_g);
                    uv_b.push(u_g);
                    uv_b.push(v_g);
                }
            }
        }

        UvLutTable {
            dimension,
            uv_r,
            uv_g,
            uv_b,
        }
    }
}
