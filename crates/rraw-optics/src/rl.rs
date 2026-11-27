#![forbid(unsafe_code)]

//! Regularized Richardson-Lucy capture sharpening.
//!
//! Provides iterative deconvolution with optical PSF modeling, sensor noise
//! regularization ($M_{\text{edge}}$ mask), and deringing envelope constraints.

use rayon::prelude::*;
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

/// Precomputes the 3×3 local min/max envelope for deringing: `[min_0, max_0, ...]`.
fn compute_deringing_bounds(i_0: &[f32], width: u32, height: u32) -> Vec<f32> {
    let w = width as usize;
    let num_pixels = w.saturating_mul(height as usize);
    let mut bounds = vec![0.0_f32; num_pixels * 6];
    bounds
        .par_chunks_mut(w * 6)
        .enumerate()
        .for_each(|(y, row_bounds)| {
            for x in 0..w {
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
                    row_bounds[x * 6 + c * 2] = min_val;
                    row_bounds[x * 6 + c * 2 + 1] = max_val;
                }
            }
        });
    bounds
}

/// Precomputes the Sobel-based edge regularization mask `M_edge(x, y, c)`.
fn compute_edge_regularization_mask(
    i_0: &[f32],
    width: u32,
    height: u32,
    noise_profile: &NoiseProfile,
) -> Vec<f32> {
    let w = width as usize;
    let shot = noise_profile.shot_gain_alpha();
    let read = noise_profile.read_variance_sigma2();
    let mut m_edge = vec![0.0_f32; i_0.len()];

    m_edge
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y, row_edge)| {
            for x in 0..w {
                let px_idx = y * w + x;
                for c in 0..3 {
                    let mut gx = 0.0_f32;
                    let mut gy = 0.0_f32;

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
                    let val = i_0[px_idx * 3 + c];
                    let var_noise = (shot[c] * val + read[c]).max(1e-12);
                    let tau = 2.5 * var_noise.sqrt();
                    let t = if grad_mag <= tau {
                        0.0
                    } else if grad_mag >= 2.0 * tau {
                        1.0
                    } else {
                        let norm = (grad_mag - tau) / (tau + 1e-12);
                        norm * norm * (3.0 - 2.0 * norm)
                    };

                    row_edge[x * 3 + c] = t;
                }
            }
        });

    m_edge
}

/// Precomputes spatially varying radial PSF sigma values across the sensor plane.
fn compute_radial_psf_sigma(width: u32, height: u32, base_psf_sigma: f32) -> Vec<f32> {
    let w = width as usize;
    let num_pixels = w.saturating_mul(height as usize);
    let center_x = (width as f32 - 1.0) * 0.5;
    let center_y = (height as f32 - 1.0) * 0.5;
    let max_radius_sq = (center_x * center_x + center_y * center_y).max(1.0);

    let mut psf_sigma = vec![base_psf_sigma; num_pixels];
    psf_sigma
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row_sigma)| {
            for (x, slot) in row_sigma.iter_mut().enumerate() {
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                let r_sq = (dx * dx + dy * dy) / max_radius_sq;
                let radial_scale = 1.0 + 0.5 * r_sq.min(1.0);
                *slot = base_psf_sigma * radial_scale;
            }
        });
    psf_sigma
}

/// Performs 2D Gaussian convolution with a spatially varying 5×5 radial PSF.
fn convolve_radial_psf(src: &[f32], psf_sigma: &[f32], width: u32, height: u32) -> Vec<f32> {
    let w = width as usize;
    let mut dst = vec![0.0_f32; src.len()];
    dst.par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(y, dst_row)| {
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
                dst_row[x * 3] = accum[0] * inv_sum;
                dst_row[x * 3 + 1] = accum[1] * inv_sum;
                dst_row[x * 3 + 2] = accum[2] * inv_sum;
            }
        });
    dst
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

    // Initial non-negative clamp I^(0) = max(I, 0.0)
    let mut i_0 = Vec::with_capacity(expected_len);
    for &v in image {
        i_0.push(if v.is_finite() && v > 0.0 { v } else { 0.0 });
    }

    if params.iterations == 0 {
        return i_0;
    }

    let w = width as usize;
    let bounds = compute_deringing_bounds(&i_0, width, height);
    let m_edge = compute_edge_regularization_mask(&i_0, width, height, &params.noise_profile);
    let psf_sigma = compute_radial_psf_sigma(width, height, params.base_psf_sigma);

    let mut current = i_0.clone();
    let mut ratio = vec![0.0_f32; expected_len];

    for _ in 0..params.iterations {
        let estimated = convolve_radial_psf(&current, &psf_sigma, width, height);

        ratio
            .par_iter_mut()
            .zip(i_0.par_iter().zip(estimated.par_iter()))
            .for_each(|(r_slot, (&orig, &est))| {
                *r_slot = orig / (est + params.epsilon);
            });

        let correction = convolve_radial_psf(&ratio, &psf_sigma, width, height);

        current
            .par_chunks_mut(w * 3)
            .enumerate()
            .for_each(|(y, cur_row)| {
                for x in 0..w {
                    let px_idx = y * w + x;
                    for c in 0..3 {
                        let idx = px_idx * 3 + c;
                        let mask = m_edge[idx];
                        let corr = correction[idx];

                        let blended_corr = 1.0 + mask * (corr - 1.0);
                        let mut updated = cur_row[x * 3 + c] * blended_corr;

                        let min_bound =
                            bounds[px_idx * 6 + c * 2] * (1.0 - params.deringing_threshold);
                        let max_bound =
                            bounds[px_idx * 6 + c * 2 + 1] * (1.0 + params.deringing_threshold);

                        updated = updated.clamp(min_bound.max(0.0), max_bound);
                        cur_row[x * 3 + c] = if updated.is_finite() {
                            updated.max(0.0)
                        } else {
                            0.0
                        };
                    }
                }
            });
    }

    current
}
