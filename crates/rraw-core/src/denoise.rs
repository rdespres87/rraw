#![forbid(unsafe_code)]

//! High-performance pure-Rust guided chroma & luma denoising engine.
//!
//! Provides luminance-guided bilateral chrominance smoothing (suppressing high-ISO color
//! mottle/speckles without blurring sharp luminance details) and edge-preserving luminance
//! noise reduction.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Configuration parameters for photographic noise reduction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DenoiseConfig {
    /// Strength of luminance-guided chrominance denoising (`0.0 ..= 1.0`).
    /// Typical pro default is ~0.25 (automatic color speckle suppression).
    pub chroma_amount: f32,
    /// Strength of edge-preserving luminance grain denoising (`0.0 ..= 1.0`).
    pub luma_amount: f32,
}

impl Default for DenoiseConfig {
    fn default() -> Self {
        Self {
            chroma_amount: 0.25,
            luma_amount: 0.0,
        }
    }
}

/// Applies guided chrominance and luminance noise reduction in-place to an RGB image buffer.
pub fn denoise_rgb(samples: &mut [[f32; 3]], width: usize, height: usize, config: &DenoiseConfig) {
    if width < 3 || height < 3 {
        return;
    }
    if config.chroma_amount <= 1e-4 && config.luma_amount <= 1e-4 {
        return;
    }

    let chroma_str = config.chroma_amount.clamp(0.0, 1.0);
    let luma_str = config.luma_amount.clamp(0.0, 1.0);

    // Step 1: Precompute Y, U, V planes across the image in contiguous RAM
    // where U = B - Y (blue-yellow chroma) and V = R - Y (red-cyan chroma)
    let mut y_plane = vec![0.0_f32; width * height];
    let mut u_plane = vec![0.0_f32; width * height];
    let mut v_plane = vec![0.0_f32; width * height];

    for (i, &[r, g, b]) in samples.iter().enumerate() {
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let u = b - y;
        let v = r - y;
        y_plane[i] = y;
        u_plane[i] = u;
        v_plane[i] = v;
    }

    // Step 2: Denoise Chrominance (U, V) using Luminance-Guided Bilateral Filter
    let filtered_uv = if chroma_str > 1e-4 {
        let radius: isize = if chroma_str > 0.5 { 2 } else { 1 };
        let spatial_sigma_sq = (radius as f32 * 0.8).powi(2).max(0.5);
        let luma_sigma_sq = (0.05 + 0.15 * chroma_str).powi(2);

        let mut out_chroma = vec![[0.0_f32; 2]; width * height];

        out_chroma
            .par_chunks_mut(width)
            .enumerate()
            .for_each(|(y, out_row)| {
                let iy = y as isize;
                for (x, out_c) in out_row.iter_mut().enumerate() {
                    let ix = x as isize;
                    let center_idx = y * width + x;
                    let center_y = y_plane[center_idx];
                    let orig_u = u_plane[center_idx];
                    let orig_v = v_plane[center_idx];

                    let mut w_sum = 0.0_f32;
                    let mut u_sum = 0.0_f32;
                    let mut v_sum = 0.0_f32;

                    for dy in -radius..=radius {
                        let ny = (iy + dy).clamp(0, height as isize - 1) as usize;
                        let row_off = ny * width;
                        for dx in -radius..=radius {
                            let nx = (ix + dx).clamp(0, width as isize - 1) as usize;
                            let neighbor_idx = row_off + nx;

                            let spatial_dist_sq = (dx * dx + dy * dy) as f32;
                            let w_spatial = (-spatial_dist_sq / (2.0 * spatial_sigma_sq)).exp();

                            let neighbor_y = y_plane[neighbor_idx];
                            let luma_diff_sq = (neighbor_y - center_y).powi(2);
                            let w_luma = (-luma_diff_sq / (2.0 * luma_sigma_sq)).exp();

                            let w = w_spatial * w_luma;
                            w_sum += w;
                            u_sum += w * u_plane[neighbor_idx];
                            v_sum += w * v_plane[neighbor_idx];
                        }
                    }

                    if w_sum > 1e-5 {
                        let smooth_u = u_sum / w_sum;
                        let smooth_v = v_sum / w_sum;
                        // Blend smoothed chrominance with original based on chroma_str
                        let final_u = orig_u * (1.0 - chroma_str) + smooth_u * chroma_str;
                        let final_v = orig_v * (1.0 - chroma_str) + smooth_v * chroma_str;
                        *out_c = [final_u, final_v];
                    } else {
                        *out_c = [orig_u, orig_v];
                    }
                }
            });

        Some(out_chroma)
    } else {
        None
    };

    // Step 3: Denoise Luminance (Y) using Edge-Preserving Bilateral Smoothing
    let filtered_y = if luma_str > 1e-4 {
        let radius: isize = 1;
        let spatial_sigma_sq = 1.0_f32;
        let range_sigma_sq = (0.02 + 0.12 * luma_str).powi(2);

        let mut out_y = vec![0.0_f32; width * height];

        out_y
            .par_chunks_mut(width)
            .enumerate()
            .for_each(|(y, out_row)| {
                let iy = y as isize;
                for (x, out_val) in out_row.iter_mut().enumerate() {
                    let ix = x as isize;
                    let center_idx = y * width + x;
                    let center_y = y_plane[center_idx];

                    let mut w_sum = 0.0_f32;
                    let mut y_sum = 0.0_f32;

                    for dy in -radius..=radius {
                        let ny = (iy + dy).clamp(0, height as isize - 1) as usize;
                        let row_off = ny * width;
                        for dx in -radius..=radius {
                            let nx = (ix + dx).clamp(0, width as isize - 1) as usize;
                            let neighbor_idx = row_off + nx;

                            let spatial_dist_sq = (dx * dx + dy * dy) as f32;
                            let w_spatial = (-spatial_dist_sq / (2.0 * spatial_sigma_sq)).exp();

                            let neighbor_y = y_plane[neighbor_idx];
                            let diff_sq = (neighbor_y - center_y).powi(2);
                            let w_range = (-diff_sq / (2.0 * range_sigma_sq)).exp();

                            let w = w_spatial * w_range;
                            w_sum += w;
                            y_sum += w * neighbor_y;
                        }
                    }

                    if w_sum > 1e-5 {
                        let smooth_y = y_sum / w_sum;
                        *out_val = center_y * (1.0 - luma_str) + smooth_y * luma_str;
                    } else {
                        *out_val = center_y;
                    }
                }
            });

        Some(out_y)
    } else {
        None
    };

    // Step 4: Recombine back into in-place RGB samples
    samples
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, out_row)| {
            for (x, px) in out_row.iter_mut().enumerate() {
                let idx = y * width + x;
                let y_val = if let Some(ref fy) = filtered_y {
                    fy[idx]
                } else {
                    y_plane[idx]
                };

                let (u_val, v_val) = if let Some(ref f_chroma) = filtered_uv {
                    (f_chroma[idx][0], f_chroma[idx][1])
                } else {
                    (u_plane[idx], v_plane[idx])
                };

                // Exact YUV -> RGB Inverse Transformation:
                // R = Y + V
                // B = Y + U
                // G = Y - (0.2126 / 0.7152)*V - (0.0722 / 0.7152)*U
                let r = y_val + v_val;
                let b = y_val + u_val;
                let g = y_val - 0.2972595 * v_val - 0.1009508 * u_val;

                *px = [r, g, b];
            }
        });
}
