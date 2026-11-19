#![forbid(unsafe_code)]

//! Optical chromatic aberration defringing engine.
//!
//! Detects and suppresses purple and green chromatic fringes along high-contrast
//! luminance transitions (e.g. backlit tree branches, architectural window frames)
//! while preserving genuine saturated colors in low-gradient regions.

use rayon::prelude::*;

/// Configuration parameters for optical fringe suppression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefringeConfig {
    /// Purple fringe suppression strength (0.0 to 1.0).
    pub purple_amount: f32,
    /// Green fringe suppression strength (0.0 to 1.0).
    pub green_amount: f32,
    /// Luminance edge contrast threshold (0.0 to 1.0, default 0.08).
    pub threshold: f32,
}

impl Default for DefringeConfig {
    fn default() -> Self {
        Self {
            purple_amount: 0.0,
            green_amount: 0.0,
            threshold: 0.08,
        }
    }
}

/// Applies selective purple and green chromatic defringing in-place to an RGB image buffer.
pub fn defringe_rgb(
    samples: &mut [[f32; 3]],
    width: usize,
    height: usize,
    config: &DefringeConfig,
) {
    let purple_str = config.purple_amount.clamp(0.0, 1.0);
    let green_str = config.green_amount.clamp(0.0, 1.0);

    if (purple_str <= 1e-4 && green_str <= 1e-4) || width < 3 || height < 3 {
        return;
    }

    if samples.len() < width * height {
        return;
    }

    // Step 1: Precompute luminance plane across image
    let mut y_plane = vec![0.0_f32; width * height];
    for (i, &[r, g, b]) in samples[..width * height].iter().enumerate() {
        y_plane[i] = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    }

    let thresh = config.threshold.clamp(0.01, 0.50);
    let thresh_denom = (thresh * 1.5).max(0.02);

    // Step 2: Rayon parallel edge detection & selective chroma neutralization
    samples[..width * height]
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row)| {
            let iy = y as isize;
            let h_isize = height as isize;
            let w_isize = width as isize;

            for (x, px) in row.iter_mut().enumerate() {
                let ix = x as isize;
                let mut min_luma = f32::INFINITY;
                let mut max_luma = f32::NEG_INFINITY;

                for dy in -1..=1 {
                    let ny = (iy + dy).clamp(0, h_isize - 1) as usize;
                    let row_idx = ny * width;
                    for dx in -1..=1 {
                        let nx = (ix + dx).clamp(0, w_isize - 1) as usize;
                        let luma = y_plane[row_idx + nx];
                        if luma < min_luma {
                            min_luma = luma;
                        }
                        if luma > max_luma {
                            max_luma = luma;
                        }
                    }
                }

                let contrast = max_luma - min_luma;
                if contrast < thresh {
                    continue;
                }

                let edge_factor = ((contrast - thresh) / thresh_denom).clamp(0.0, 1.0);

                let [mut r, mut g, mut b] = *px;

                // Purple / Magenta fringe suppression (R > G and B > G)
                if purple_str > 1e-4 && r > g && b > g {
                    let purple_excess = (r - g).min(b - g);
                    if purple_excess > 0.005 {
                        let alpha =
                            purple_str * edge_factor * (purple_excess / 0.05).clamp(0.0, 1.0);
                        r -= alpha * (r - g);
                        b -= alpha * (b - g);
                    }
                }

                // Green fringe suppression (G > R and G > B)
                if green_str > 1e-4 && g > r && g > b {
                    let max_rb = r.max(b);
                    let green_excess = g - max_rb;
                    if green_excess > 0.005 {
                        let alpha = green_str * edge_factor * (green_excess / 0.05).clamp(0.0, 1.0);
                        g -= alpha * green_excess;
                    }
                }

                *px = [r, g, b];
            }
        });
}
