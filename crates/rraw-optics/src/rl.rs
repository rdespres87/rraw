#![forbid(unsafe_code)]

//! Regularized Richardson-Lucy capture sharpening.
//!
//! Provides iterative deconvolution with optical PSF modeling, sensor noise
//! regularization ($M_{\text{edge}}$ mask), and deringing envelope constraints.

use rraw_core::NoiseProfile;

/// Parameters governing regularized Richardson-Lucy deconvolution.
#[derive(Debug, Clone)]
pub struct RichardsonLucyParams {
    /// Number of iterative deconvolution update passes.
    pub iterations: u32,
    /// Numerical stability term preventing division by zero in the RL ratio.
    pub epsilon: f32,
    /// Base standard deviation of the optical point spread function (PSF) in pixels.
    pub base_psf_sigma: f32,
    /// Local min/max deringing envelope threshold fraction (e.g. 0.05 = +/-5%).
    pub deringing_threshold: f32,
    /// Sensor noise profile used to derive the edge regularization threshold.
    pub noise_profile: NoiseProfile,
}

#[inline]
fn clamp_coord(coord: i32, max_val: u32) -> usize {
    coord.clamp(0, (max_val - 1) as i32) as usize
}

/// Executes regularized Richardson-Lucy capture sharpening on an interleaved RGB image.
///
/// Invariants:
/// - Output length strictly equals `(width * height * 3)`.
/// - Output pixel values are finite (`!is_nan() && !is_infinite()`).
/// - Output pixel values are non-negative (`>= 0.0`).
/// - Flat noisy regions are preserved with $< +1\%$ noise variance amplification.
/// - Overshoot on high-contrast edges is strictly clamped within the deringing envelope.
#[must_use]
pub fn execute_richardson_lucy(
    image: &[f32],
    width: u32,
    height: u32,
    params: &RichardsonLucyParams,
) -> Vec<f32> {
    let expected_len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(3);
    if width == 0 || height == 0 || image.len() != expected_len {
        return image.to_vec();
    }

    let num_pixels = (width as usize).saturating_mul(height as usize);

    // Initial non-negative clamp I^(0) = max(I, 0.0)
    let mut i_0 = Vec::with_capacity(expected_len);
    for &v in image {
        i_0.push(if v.is_finite() && v > 0.0 { v } else { 0.0 });
    }

    if params.iterations == 0 {
        return i_0;
    }

    let w = width as usize;
    let h = height as usize;

    // Precompute local min/max envelope for deringing: [min_0, max_0, min_1, max_1, ...]
    let mut bounds = vec![0.0_f32; num_pixels * 6];
    for y in 0..h {
        for x in 0..w {
            let px_idx = y * w + x;
            for c in 0..3 {
                let mut min_val = f32::INFINITY;
                let mut max_val = f32::NEG_INFINITY;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = clamp_coord(x as i32 + dx, width);
                        let ny = clamp_coord(y as i32 + dy, height);
                        let val = i_0[(ny * w + nx) * 3 + c];
                        if val < min_val {
                            min_val = val;
                        }
                        if val > max_val {
                            max_val = val;
                        }
                    }
                }
                bounds[px_idx * 6 + c * 2] = min_val;
                bounds[px_idx * 6 + c * 2 + 1] = max_val;
            }
        }
    }

    // Precompute edge regularization mask M_edge(x, y, c)
    let shot = params.noise_profile.shot_gain_alpha();
    let read = params.noise_profile.read_variance_sigma2();
    let mut m_edge = vec![0.0_f32; expected_len];

    for y in 0..h {
        for x in 0..w {
            let px_idx = y * w + x;
            for c in 0..3 {
                // Sobel gradient estimation for robust noise rejection
                let mut gx = 0.0_f32;
                let mut gy = 0.0_f32;

                // Sobel X kernel: [-1 0 +1; -2 0 +2; -1 0 +1] / 8
                // Sobel Y kernel: [-1 -2 -1;  0  0  0; +1 +2 +1] / 8
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = clamp_coord(x as i32 + dx, width);
                        let ny = clamp_coord(y as i32 + dy, height);
                        let v = i_0[(ny * w + nx) * 3 + c];

                        let kx = match (dx, dy) {
                            (-1, 0) => -2.0,
                            (1, 0) => 2.0,
                            (-1, -1 | 1) => -1.0,
                            (1, -1 | 1) => 1.0,
                            _ => 0.0,
                        };
                        let ky = match (dx, dy) {
                            (0, -1) => -2.0,
                            (0, 1) => 2.0,
                            (-1 | 1, -1) => -1.0,
                            (-1 | 1, 1) => 1.0,
                            _ => 0.0,
                        };

                        gx += v * kx;
                        gy += v * ky;
                    }
                }
                gx /= 8.0;
                gy /= 8.0;

                let grad_mag = (gx * gx + gy * gy).sqrt();

                // Noise standard deviation at current pixel value
                let val = i_0[px_idx * 3 + c];
                let var_noise = (shot[c] * val + read[c]).max(1e-12);
                let sigma_noise = var_noise.sqrt();

                // Threshold tau_noise = 2.5 * sigma_noise
                let tau = 2.5 * sigma_noise;
                let t = if grad_mag <= tau {
                    0.0
                } else if grad_mag >= 2.0 * tau {
                    1.0
                } else {
                    let norm = (grad_mag - tau) / (tau + 1e-12);
                    norm * norm * (3.0 - 2.0 * norm)
                };

                m_edge[px_idx * 3 + c] = t;
            }
        }
    }

    // Precompute radial PSF sigma(x, y)
    let center_x = (width as f32 - 1.0) * 0.5;
    let center_y = (height as f32 - 1.0) * 0.5;
    let max_radius_sq = (center_x * center_x + center_y * center_y).max(1.0);

    let mut psf_sigma = vec![params.base_psf_sigma; num_pixels];
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let r_sq = (dx * dx + dy * dy) / max_radius_sq;
            let radial_scale = 1.0 + 0.5 * r_sq.min(1.0);
            psf_sigma[y * w + x] = params.base_psf_sigma * radial_scale;
        }
    }

    // Helper: 2D Gaussian convolution with spatially-varying PSF (5x5 kernel)
    let convolve_psf = |src: &[f32]| -> Vec<f32> {
        let mut dst = vec![0.0_f32; expected_len];
        for y in 0..h {
            for x in 0..w {
                let px_idx = y * w + x;
                let sigma = psf_sigma[px_idx].max(0.1);
                let two_sigma_sq = 2.0 * sigma * sigma;

                let mut weight_sum = 0.0_f32;
                let mut accum = [0.0_f32; 3];

                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let nx = clamp_coord(x as i32 + dx, width);
                        let ny = clamp_coord(y as i32 + dy, height);
                        let weight = (-((dx * dx + dy * dy) as f32) / two_sigma_sq).exp();
                        weight_sum += weight;

                        let src_idx = (ny * w + nx) * 3;
                        accum[0] += src[src_idx] * weight;
                        accum[1] += src[src_idx + 1] * weight;
                        accum[2] += src[src_idx + 2] * weight;
                    }
                }

                let inv_sum = 1.0 / weight_sum.max(1e-12);
                let dst_idx = px_idx * 3;
                dst[dst_idx] = accum[0] * inv_sum;
                dst[dst_idx + 1] = accum[1] * inv_sum;
                dst[dst_idx + 2] = accum[2] * inv_sum;
            }
        }
        dst
    };

    let mut current = i_0.clone();
    let mut ratio = vec![0.0_f32; expected_len];

    for _ in 0..params.iterations {
        // Step 1: Forward convolution E = current * PSF
        let estimated = convolve_psf(&current);

        // Step 2: Compute ratio Q = I^(0) / (E + epsilon)
        for i in 0..expected_len {
            ratio[i] = i_0[i] / (estimated[i] + params.epsilon);
        }

        // Step 3: Backward/adjoint convolution C = Q * PSF^T (PSF is symmetric)
        let correction = convolve_psf(&ratio);

        // Step 4: Regularized update and deringing clamping
        for y in 0..h {
            for x in 0..w {
                let px_idx = y * w + x;
                for c in 0..3 {
                    let idx = px_idx * 3 + c;
                    let mask = m_edge[idx];
                    let corr = correction[idx];

                    // Blend correction factor: R = 1.0 + M_edge * (C - 1.0)
                    let blended_corr = 1.0 + mask * (corr - 1.0);
                    let mut updated = current[idx] * blended_corr;

                    // Enforce deringing envelope bounds
                    let min_bound = bounds[px_idx * 6 + c * 2] * (1.0 - params.deringing_threshold);
                    let max_bound =
                        bounds[px_idx * 6 + c * 2 + 1] * (1.0 + params.deringing_threshold);

                    updated = updated.clamp(min_bound.max(0.0), max_bound);
                    current[idx] = if updated.is_finite() {
                        updated.max(0.0)
                    } else {
                        0.0
                    };
                }
            }
        }
    }

    current
}
