#![forbid(unsafe_code)]

//! High-precision pure-Rust CFA demosaicing engine.
//!
//! Provides both fast 3x3 bilinear demosaicing and high-fidelity edge-aware adaptive demosaicing
//! (Hamilton-Adams / RCD-inspired gradient-directed interpolation) eliminating zipper artifacts
//! and chromatic false color along high-contrast boundaries.

use rayon::prelude::*;

use crate::{BayerOrigin, CfaColor, CfaPattern, CoreError, SignedCfaBuffer};

/// Demosaicing algorithm variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DemosaicMethod {
    /// Standard 3x3 local neighborhood bilinear interpolation.
    Bilinear,
    /// Edge-aware directional gradient interpolation with chrominance difference guidance.
    #[default]
    EdgeAwareAdaptive,
}

/// Demosaics a `SignedCfaBuffer` into a continuous planar RGB floating-point buffer `Vec<[f32; 3]>`.
///
/// # Errors
/// Returns `CoreError::InvalidDimensions` if `width < 2` or `height < 2`.
/// Returns `CoreError::BufferLengthMismatch` if the sample count does not equal `width * height`.
pub fn demosaic_cfa(
    cfa: &SignedCfaBuffer,
    method: DemosaicMethod,
) -> Result<Vec<[f32; 3]>, CoreError> {
    let width = cfa.width();
    let height = cfa.height();

    if width < 2 || height < 2 {
        return Err(CoreError::InvalidDimensions { width, height });
    }

    let samples = cfa.samples();
    let expected_len = width
        .checked_mul(height)
        .ok_or(CoreError::InvalidDimensions { width, height })?;

    if samples.len() != expected_len {
        return Err(CoreError::BufferLengthMismatch {
            expected: expected_len,
            actual: samples.len(),
        });
    }

    match method {
        DemosaicMethod::Bilinear => demosaic_bilinear(cfa),
        DemosaicMethod::EdgeAwareAdaptive => match cfa.pattern() {
            CfaPattern::Bayer(origin) => demosaic_bayer_edge_aware(width, height, origin, samples),
            CfaPattern::XTrans(_) => {
                demosaic_xtrans_edge_aware(width, height, &cfa.pattern(), samples)
            }
        },
    }
}

/// Demosaics a Bayer CFA mosaic using directional green gradient estimation and color difference interpolation.
fn demosaic_bayer_edge_aware(
    width: usize,
    height: usize,
    origin: BayerOrigin,
    samples: &[f32],
) -> Result<Vec<[f32; 3]>, CoreError> {
    let mut out_rgb = vec![[0.0_f32; 3]; width * height];

    // Precompute Bayer origin offsets: (red_x, red_y) and (blue_x, blue_y)
    let (red_x, red_y, blue_x, blue_y) = match origin {
        BayerOrigin::Rggb => (0, 0, 1, 1),
        BayerOrigin::Grbg => (1, 0, 0, 1),
        BayerOrigin::Gbrg => (0, 1, 1, 0),
        BayerOrigin::Bggr => (1, 1, 0, 0),
    };

    // Helper closure for clamped 2D pixel access
    let sample_at = |x: isize, y: isize| -> f32 {
        let cx = x.clamp(0, width as isize - 1) as usize;
        let cy = y.clamp(0, height as isize - 1) as usize;
        samples[cy * width + cx]
    };

    // Step 1: Reconstruct the full Green plane across the entire image
    let mut full_green = vec![0.0_f32; width * height];

    full_green
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row_g)| {
            let iy = y as isize;
            for (x, g_val) in row_g.iter_mut().enumerate() {
                let ix = x as isize;
                let is_red = (x % 2 == red_x) && (y % 2 == red_y);
                let is_blue = (x % 2 == blue_x) && (y % 2 == blue_y);

                if !is_red && !is_blue {
                    // Current photosite is already native Green
                    *g_val = sample_at(ix, iy);
                } else {
                    // Photosite is Red or Blue: interpolate Green using horizontal/vertical gradients
                    // Green neighbors in cross: (x-1, y), (x+1, y), (x, y-1), (x, y+1)
                    let g_left = sample_at(ix - 1, iy);
                    let g_right = sample_at(ix + 1, iy);
                    let g_up = sample_at(ix, iy - 1);
                    let g_down = sample_at(ix, iy + 1);

                    // Same-color 2nd-order neighbor samples for curvature / laplacian estimation
                    let c_center = sample_at(ix, iy);
                    let c_left2 = sample_at(ix - 2, iy);
                    let c_right2 = sample_at(ix + 2, iy);
                    let c_up2 = sample_at(ix, iy - 2);
                    let c_down2 = sample_at(ix, iy + 2);

                    let grad_h =
                        (g_left - g_right).abs() + (2.0 * c_center - c_left2 - c_right2).abs();
                    let grad_v = (g_up - g_down).abs() + (2.0 * c_center - c_up2 - c_down2).abs();

                    let g_est_h =
                        0.5 * (g_left + g_right) + 0.25 * (2.0 * c_center - c_left2 - c_right2);
                    let g_est_v = 0.5 * (g_up + g_down) + 0.25 * (2.0 * c_center - c_up2 - c_down2);

                    const EPS: f32 = 1e-5;
                    if (grad_v + EPS) < (grad_h + EPS) * 0.75 {
                        // Dominant vertical edge -> interpolate along the edge (vertically)
                        *g_val = g_est_v;
                    } else if (grad_h + EPS) < (grad_v + EPS) * 0.75 {
                        // Dominant horizontal edge -> interpolate along the edge (horizontally)
                        *g_val = g_est_h;
                    } else {
                        // Isotropic or diagonal edge -> weighted blend
                        let w_h = 1.0 / (grad_h + 1e-4);
                        let w_v = 1.0 / (grad_v + 1e-4);
                        *g_val = (w_h * g_est_h + w_v * g_est_v) / (w_h + w_v);
                    }
                }
            }
        });

    // Step 2 & 3: Reconstruct Red and Blue channels guided by Green color difference (R - G, B - G)
    out_rgb
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, out_row)| {
            let iy = y as isize;
            for (x, out_px) in out_row.iter_mut().enumerate() {
                let ix = x as isize;
                let g = full_green[y * width + x];

                let is_red_site = (x % 2 == red_x) && (y % 2 == red_y);
                let is_blue_site = (x % 2 == blue_x) && (y % 2 == blue_y);

                // Helper to compute (R - G) at known red coordinates
                let red_diff_at = |sx: isize, sy: isize| -> f32 {
                    let cx = sx.clamp(0, width as isize - 1) as usize;
                    let cy = sy.clamp(0, height as isize - 1) as usize;
                    samples[cy * width + cx] - full_green[cy * width + cx]
                };

                // Helper to compute (B - G) at known blue coordinates
                let blue_diff_at = |sx: isize, sy: isize| -> f32 {
                    let cx = sx.clamp(0, width as isize - 1) as usize;
                    let cy = sy.clamp(0, height as isize - 1) as usize;
                    samples[cy * width + cx] - full_green[cy * width + cx]
                };

                // Determine Red value at (x, y)
                let r = if is_red_site {
                    // Exactly preserve native sensor sample on red site
                    sample_at(ix, iy)
                } else if is_blue_site {
                    // Red neighbors are at 4 diagonal corners: (x-1, y-1), (x+1, y-1), (x-1, y+1), (x+1, y+1)
                    let d_tl = red_diff_at(ix - 1, iy - 1);
                    let d_tr = red_diff_at(ix + 1, iy - 1);
                    let d_bl = red_diff_at(ix - 1, iy + 1);
                    let d_br = red_diff_at(ix + 1, iy + 1);

                    let grad_diag1 = (d_tl - d_br).abs();
                    let grad_diag2 = (d_tr - d_bl).abs();

                    const EPS: f32 = 1e-5;
                    let diff_r = if (grad_diag1 + EPS) < (grad_diag2 + EPS) * 0.75 {
                        0.5 * (d_tl + d_br)
                    } else if (grad_diag2 + EPS) < (grad_diag1 + EPS) * 0.75 {
                        0.5 * (d_tr + d_bl)
                    } else {
                        0.25 * (d_tl + d_tr + d_bl + d_br)
                    };
                    g + diff_r
                } else {
                    // Green site: check whether red neighbors are horizontal (x-1, x+1) or vertical (y-1, y+1)
                    let is_red_row = y % 2 == red_y;
                    let diff_r = if is_red_row {
                        0.5 * (red_diff_at(ix - 1, iy) + red_diff_at(ix + 1, iy))
                    } else {
                        0.5 * (red_diff_at(ix, iy - 1) + red_diff_at(ix, iy + 1))
                    };
                    g + diff_r
                };

                // Determine Blue value at (x, y)
                let b = if is_blue_site {
                    // Exactly preserve native sensor sample on blue site
                    sample_at(ix, iy)
                } else if is_red_site {
                    // Blue neighbors are at 4 diagonal corners: (x-1, y-1), (x+1, y-1), (x-1, y+1), (x+1, y+1)
                    let d_tl = blue_diff_at(ix - 1, iy - 1);
                    let d_tr = blue_diff_at(ix + 1, iy - 1);
                    let d_bl = blue_diff_at(ix - 1, iy + 1);
                    let d_br = blue_diff_at(ix + 1, iy + 1);

                    let grad_diag1 = (d_tl - d_br).abs();
                    let grad_diag2 = (d_tr - d_bl).abs();

                    const EPS: f32 = 1e-5;
                    let diff_b = if (grad_diag1 + EPS) < (grad_diag2 + EPS) * 0.75 {
                        0.5 * (d_tl + d_br)
                    } else if (grad_diag2 + EPS) < (grad_diag1 + EPS) * 0.75 {
                        0.5 * (d_tr + d_bl)
                    } else {
                        0.25 * (d_tl + d_tr + d_bl + d_br)
                    };
                    g + diff_b
                } else {
                    // Green site: check whether blue neighbors are horizontal (x-1, x+1) or vertical (y-1, y+1)
                    let is_blue_row = y % 2 == blue_y;
                    let diff_b = if is_blue_row {
                        0.5 * (blue_diff_at(ix - 1, iy) + blue_diff_at(ix + 1, iy))
                    } else {
                        0.5 * (blue_diff_at(ix, iy - 1) + blue_diff_at(ix, iy + 1))
                    };
                    g + diff_b
                };

                *out_px = [r, g, b];
            }
        });

    Ok(out_rgb)
}

/// Demosaics a 6x6 Fujifilm X-Trans CFA mosaic using edge-aware directional green reconstruction
/// and smooth chrominance difference interpolation (R - G, B - G).
fn demosaic_xtrans_edge_aware(
    width: usize,
    height: usize,
    pattern: &CfaPattern,
    samples: &[f32],
) -> Result<Vec<[f32; 3]>, CoreError> {
    // Precompute 6x6 periodic CFA color LUT
    let mut cfa_lut = [[CfaColor::Green; 6]; 6];
    for (ly, row_lut) in cfa_lut.iter_mut().enumerate() {
        for (lx, cell) in row_lut.iter_mut().enumerate() {
            *cell = pattern.color_at(lx, ly);
        }
    }

    let sample_at = |x: isize, y: isize| -> f32 {
        let cx = x.clamp(0, width as isize - 1) as usize;
        let cy = y.clamp(0, height as isize - 1) as usize;
        samples[cy * width + cx]
    };

    let color_at = |x: isize, y: isize| -> CfaColor {
        let lx = (x.rem_euclid(6)) as usize;
        let ly = (y.rem_euclid(6)) as usize;
        cfa_lut[ly][lx]
    };

    // Step 1: Directional Green plane reconstruction
    let mut full_green = vec![0.0_f32; width * height];

    full_green
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row_g)| {
            let iy = y as isize;
            for (x, g_val) in row_g.iter_mut().enumerate() {
                let ix = x as isize;
                let center_color = color_at(ix, iy);
                if center_color == CfaColor::Green {
                    *g_val = sample_at(ix, iy);
                    continue;
                }

                // Search nearest green neighbors horizontally (left & right)
                let mut left_g = None;
                for dx in 1..=3 {
                    let nx = ix - dx;
                    if nx >= 0 && color_at(nx, iy) == CfaColor::Green {
                        left_g = Some((sample_at(nx, iy), dx as f32));
                        break;
                    }
                }
                let mut right_g = None;
                for dx in 1..=3 {
                    let nx = ix + dx;
                    if nx < width as isize && color_at(nx, iy) == CfaColor::Green {
                        right_g = Some((sample_at(nx, iy), dx as f32));
                        break;
                    }
                }

                // Search nearest green neighbors vertically (up & down)
                let mut up_g = None;
                for dy in 1..=3 {
                    let ny = iy - dy;
                    if ny >= 0 && color_at(ix, ny) == CfaColor::Green {
                        up_g = Some((sample_at(ix, ny), dy as f32));
                        break;
                    }
                }
                let mut down_g = None;
                for dy in 1..=3 {
                    let ny = iy + dy;
                    if ny < height as isize && color_at(ix, ny) == CfaColor::Green {
                        down_g = Some((sample_at(ix, ny), dy as f32));
                        break;
                    }
                }

                let c_center = sample_at(ix, iy);

                // Horizontal estimate and gradient
                let (g_est_h, grad_h) = match (left_g, right_g) {
                    (Some((gl, dl)), Some((gr, dr))) => {
                        let w_total = dl + dr;
                        let g_lin = (gl * dr + gr * dl) / w_total;
                        let grad = (gl - gr).abs();
                        (g_lin, grad)
                    }
                    (Some((gl, _)), None) => (gl, 1.0),
                    (None, Some((gr, _))) => (gr, 1.0),
                    (None, None) => (c_center, 10.0),
                };

                // Vertical estimate and gradient
                let (g_est_v, grad_v) = match (up_g, down_g) {
                    (Some((gu, du)), Some((gd, dd))) => {
                        let w_total = du + dd;
                        let g_lin = (gu * dd + gd * du) / w_total;
                        let grad = (gu - gd).abs();
                        (g_lin, grad)
                    }
                    (Some((gu, _)), None) => (gu, 1.0),
                    (None, Some((gd, _))) => (gd, 1.0),
                    (None, None) => (c_center, 10.0),
                };

                const EPS: f32 = 1e-5;
                if (grad_v + EPS) < (grad_h + EPS) * 0.70 {
                    *g_val = g_est_v;
                } else if (grad_h + EPS) < (grad_v + EPS) * 0.70 {
                    *g_val = g_est_h;
                } else {
                    let w_h = 1.0 / (grad_h + 1e-4);
                    let w_v = 1.0 / (grad_v + 1e-4);
                    *g_val = (w_h * g_est_h + w_v * g_est_v) / (w_h + w_v);
                }
            }
        });

    // Step 2 & 3: Reconstruct Red and Blue channels guided by Green color difference
    let mut out_rgb = vec![[0.0_f32; 3]; width * height];

    out_rgb
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, out_row)| {
            let iy = y as isize;
            for (x, out_px) in out_row.iter_mut().enumerate() {
                let ix = x as isize;
                let center_color = color_at(ix, iy);
                let g = full_green[y * width + x];
                let center_sample = sample_at(ix, iy);

                let red_diff_at = |sx: isize, sy: isize| -> f32 {
                    let cx = sx.clamp(0, width as isize - 1) as usize;
                    let cy = sy.clamp(0, height as isize - 1) as usize;
                    samples[cy * width + cx] - full_green[cy * width + cx]
                };

                let blue_diff_at = |sx: isize, sy: isize| -> f32 {
                    let cx = sx.clamp(0, width as isize - 1) as usize;
                    let cy = sy.clamp(0, height as isize - 1) as usize;
                    samples[cy * width + cx] - full_green[cy * width + cx]
                };

                // Compute Red
                let r = if center_color == CfaColor::Red {
                    center_sample
                } else {
                    let mut diff_sum = 0.0_f32;
                    let mut weight_sum = 0.0_f32;
                    for dy in -2..=2 {
                        let ny = iy + dy;
                        if ny < 0 || ny >= height as isize {
                            continue;
                        }
                        for dx in -2..=2 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nx = ix + dx;
                            if nx < 0 || nx >= width as isize {
                                continue;
                            }
                            if color_at(nx, ny) == CfaColor::Red {
                                let dist2 = (dx * dx + dy * dy) as f32;
                                let w = 1.0 / dist2;
                                diff_sum += w * red_diff_at(nx, ny);
                                weight_sum += w;
                            }
                        }
                    }
                    if weight_sum > 0.0 {
                        g + (diff_sum / weight_sum)
                    } else {
                        g
                    }
                };

                // Compute Blue
                let b = if center_color == CfaColor::Blue {
                    center_sample
                } else {
                    let mut diff_sum = 0.0_f32;
                    let mut weight_sum = 0.0_f32;
                    for dy in -2..=2 {
                        let ny = iy + dy;
                        if ny < 0 || ny >= height as isize {
                            continue;
                        }
                        for dx in -2..=2 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nx = ix + dx;
                            if nx < 0 || nx >= width as isize {
                                continue;
                            }
                            if color_at(nx, ny) == CfaColor::Blue {
                                let dist2 = (dx * dx + dy * dy) as f32;
                                let w = 1.0 / dist2;
                                diff_sum += w * blue_diff_at(nx, ny);
                                weight_sum += w;
                            }
                        }
                    }
                    if weight_sum > 0.0 {
                        g + (diff_sum / weight_sum)
                    } else {
                        g
                    }
                };

                *out_px = [r, g, b];
            }
        });

    Ok(out_rgb)
}

/// Fast parallel 3x3 bilinear demosaicing supporting both Bayer and X-Trans.
fn demosaic_bilinear(cfa: &SignedCfaBuffer) -> Result<Vec<[f32; 3]>, CoreError> {
    let width = cfa.width();
    let height = cfa.height();
    let samples = cfa.samples();
    let pattern = cfa.pattern();

    let mut out_rgb = vec![[0.0_f32; 3]; width * height];

    // Precompute 6x6 periodic CFA color LUT (covers both 2x2 Bayer and 6x6 X-Trans without modulo per neighbor)
    let mut cfa_lut = [[CfaColor::Green; 6]; 6];
    for (ly, row_lut) in cfa_lut.iter_mut().enumerate() {
        for (lx, cell) in row_lut.iter_mut().enumerate() {
            *cell = pattern.color_at(lx, ly);
        }
    }

    out_rgb
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, out_row)| {
            let y0 = y.saturating_sub(1);
            let y1 = (y + 1).min(height - 1);
            let lut_y = y % 6;

            for (x, out_px) in out_row.iter_mut().enumerate() {
                let center_color = cfa_lut[lut_y][x % 6];
                let center_val = samples[y * width + x];

                let x0 = x.saturating_sub(1);
                let x1 = (x + 1).min(width - 1);

                let mut r_sum = 0.0_f32;
                let mut r_cnt = 0_u32;
                let mut g_sum = 0.0_f32;
                let mut g_cnt = 0_u32;
                let mut b_sum = 0.0_f32;
                let mut b_cnt = 0_u32;

                for ny in y0..=y1 {
                    let n_lut_y = ny % 6;
                    let row_off = ny * width;
                    for nx in x0..=x1 {
                        let val = samples[row_off + nx];
                        match cfa_lut[n_lut_y][nx % 6] {
                            CfaColor::Red => {
                                r_sum += val;
                                r_cnt += 1;
                            }
                            CfaColor::Green => {
                                g_sum += val;
                                g_cnt += 1;
                            }
                            CfaColor::Blue => {
                                b_sum += val;
                                b_cnt += 1;
                            }
                        }
                    }
                }

                let r = if center_color == CfaColor::Red {
                    center_val
                } else if r_cnt > 0 {
                    r_sum / (r_cnt as f32)
                } else {
                    center_val
                };
                let g = if center_color == CfaColor::Green {
                    center_val
                } else if g_cnt > 0 {
                    g_sum / (g_cnt as f32)
                } else {
                    center_val
                };
                let b = if center_color == CfaColor::Blue {
                    center_val
                } else if b_cnt > 0 {
                    b_sum / (b_cnt as f32)
                } else {
                    center_val
                };

                *out_px = [r, g, b];
            }
        });

    Ok(out_rgb)
}
