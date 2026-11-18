#![forbid(unsafe_code)]

//! Fritsch-Carlson Monotone Cubic Hermite Spline (PCHIP) evaluator and 1D LUT generator.

use rraw_core::curves::{ChannelCurve, ToneCurvesRecipe};

/// Evaluates a single-channel tone curve at coordinate `x` using Fritsch-Carlson PCHIP interpolation.
///
/// Ensures strict monotonicity preservation without Runge's phenomenon or overshoot.
#[must_use]
pub fn evaluate_channel_curve_pchip(curve: &ChannelCurve, x: f32) -> f32 {
    let pts = &curve.points;
    if pts.is_empty() {
        return x.clamp(0.0, 1.0);
    }
    if pts.len() == 1 {
        return pts[0].y.clamp(0.0, 1.0);
    }

    let x_clamped = x.clamp(0.0, 1.0);
    if x_clamped <= pts[0].x {
        return pts[0].y.clamp(0.0, 1.0);
    }
    if x_clamped >= pts[pts.len() - 1].x {
        return pts[pts.len() - 1].y.clamp(0.0, 1.0);
    }

    let n = pts.len();
    // 1. Calculate secants Δ_k = (y_{k+1} - y_k) / (x_{k+1} - x_k)
    let mut deltas = Vec::with_capacity(n - 1);
    let mut h = Vec::with_capacity(n - 1);
    for k in 0..n - 1 {
        let dx = (pts[k + 1].x - pts[k].x).max(1e-7);
        let dy = pts[k + 1].y - pts[k].y;
        h.push(dx);
        deltas.push(dy / dx);
    }

    // 2. Compute initial tangents d_k
    let mut d = vec![0.0_f32; n];
    d[0] = deltas[0];
    d[n - 1] = deltas[n - 2];

    for k in 1..n - 1 {
        if deltas[k - 1] * deltas[k] <= 0.0 {
            d[k] = 0.0;
        } else {
            d[k] = (deltas[k - 1] + deltas[k]) * 0.5;
        }
    }

    // 3. Fritsch-Carlson monotonicity limiter
    for k in 0..n - 1 {
        if deltas[k].abs() < 1e-7 {
            d[k] = 0.0;
            d[k + 1] = 0.0;
        } else {
            let alpha = d[k] / deltas[k];
            let beta = d[k + 1] / deltas[k];

            if alpha < 0.0 {
                d[k] = 0.0;
            }
            if beta < 0.0 {
                d[k + 1] = 0.0;
            }

            let hyp_sq = alpha * alpha + beta * beta;
            if hyp_sq > 9.0 {
                let tau = 3.0 / hyp_sq.sqrt();
                d[k] = tau * alpha * deltas[k];
                d[k + 1] = tau * beta * deltas[k];
            }
        }
    }

    // 4. Find interval containing x_clamped
    let idx = match pts.binary_search_by(|p| {
        p.x.partial_cmp(&x_clamped)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        Ok(exact) => {
            if exact < n - 1 {
                exact
            } else {
                n - 2
            }
        }
        Err(insert) => {
            if insert == 0 {
                0
            } else {
                insert - 1
            }
        }
    };

    let dx = h[idx];
    let t = ((x_clamped - pts[idx].x) / dx).clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;

    // Hermite basis functions
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;

    let y = h00 * pts[idx].y + h10 * dx * d[idx] + h01 * pts[idx + 1].y + h11 * dx * d[idx + 1];
    y.clamp(0.0, 1.0)
}

/// Generates a 1024-entry 1D LUT containing evaluated `[R, G, B, Luma]` samples.
///
/// Mapping per channel:
/// - R: `curve_R(curve_RGB(x))`
/// - G: `curve_G(curve_RGB(x))`
/// - B: `curve_B(curve_RGB(x))`
/// - Luma: `curve_Luma(x)`
#[must_use]
pub fn generate_1d_lut_1024(recipe: &ToneCurvesRecipe) -> Box<[[f32; 4]; 1024]> {
    let mut lut = Box::new([[0.0_f32; 4]; 1024]);
    let denom = 1023.0_f32;

    for (i, entry) in lut.iter_mut().enumerate() {
        let x = i as f32 / denom;
        let rgb_inter = evaluate_channel_curve_pchip(&recipe.combined_rgb, x);

        let r = evaluate_channel_curve_pchip(&recipe.red, rgb_inter);
        let g = evaluate_channel_curve_pchip(&recipe.green, rgb_inter);
        let b = evaluate_channel_curve_pchip(&recipe.blue, rgb_inter);
        let luma = evaluate_channel_curve_pchip(&recipe.luma, x);

        *entry = [r, g, b, luma];
    }

    lut
}

/// Applies precomputed 1D tone curve LUT to a linear RGB triplet in `ACEScg` color space.
#[must_use]
pub fn apply_tone_curves_lut(rgb: glam::Vec3, lut: &[[f32; 4]; 1024]) -> glam::Vec3 {
    let sample_channel = |val: f32, ch: usize| -> f32 {
        let x = val.clamp(0.0, 1.0) * 1023.0;
        let idx = (x as usize).min(1022);
        let frac = x - idx as f32;
        lut[idx][ch] * (1.0 - frac) + lut[idx + 1][ch] * frac
    };

    let r = sample_channel(rgb.x, 0);
    let g = sample_channel(rgb.y, 1);
    let b = sample_channel(rgb.z, 2);

    // AP1 ACEScg Rec.2020/AP1 luma coefficients
    let cur_luma = 0.272_228_7 * r + 0.674_081_8 * g + 0.053_689_5 * b;
    let target_luma = sample_channel(cur_luma, 3);
    let luma_scale = if cur_luma > 1e-6 {
        (target_luma / cur_luma).clamp(0.0, 4.0)
    } else {
        1.0
    };

    glam::Vec3::new(r * luma_scale, g * luma_scale, b * luma_scale)
}
