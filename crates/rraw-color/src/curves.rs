#![forbid(unsafe_code)]

//! Fritsch-Carlson Monotone Cubic Hermite Spline (PCHIP) evaluator and 1D LUT generator.

use rraw_core::curves::{ChannelCurve, ToneCurvesRecipe};

/// Precomputed Fritsch-Carlson Monotone Cubic Hermite Spline (PCHIP).
///
/// Precomputes segment intervals `h` and boundary/interior tangents `d`
/// adhering strictly to the Brodlie / Fritsch-Carlson weighted harmonic mean
/// for arbitrary non-uniform knot spacing.
///
/// Evaluating `PchipSpline::evaluate(&self, x)` is strictly zero-allocation,
/// cache-friendly, and thread-safe.
#[derive(Debug, Clone, PartialEq)]
pub struct PchipSpline {
    xs: Vec<f32>,
    ys: Vec<f32>,
    h: Vec<f32>,
    d: Vec<f32>,
}

impl PchipSpline {
    /// Constructs a precomputed `PchipSpline` from a `ChannelCurve`.
    #[must_use]
    pub fn new(curve: &ChannelCurve) -> Self {
        let pts = &curve.points;
        let n = pts.len();

        if n <= 1 {
            let (xs, ys) = if n == 1 {
                (vec![pts[0].x], vec![pts[0].y])
            } else {
                (Vec::new(), Vec::new())
            };
            return Self {
                xs,
                ys,
                h: Vec::new(),
                d: Vec::new(),
            };
        }

        let mut xs = Vec::with_capacity(n);
        let mut ys = Vec::with_capacity(n);
        for p in pts {
            xs.push(p.x);
            ys.push(p.y);
        }

        // 1. Calculate intervals h_k and secants Δ_k = (y_{k+1} - y_k) / h_k
        let mut deltas = Vec::with_capacity(n - 1);
        let mut h = Vec::with_capacity(n - 1);
        for k in 0..n - 1 {
            let dx = (xs[k + 1] - xs[k]).max(1e-7);
            let dy = ys[k + 1] - ys[k];
            h.push(dx);
            deltas.push(dy / dx);
        }

        // 2. Compute initial tangents d_k
        let mut d = vec![0.0_f32; n];
        if n == 2 {
            d[0] = deltas[0];
            d[1] = deltas[0];
        } else {
            // Interior points: Brodlie / Fritsch-Carlson weighted harmonic mean
            // for non-uniform spacing (preserves monotonicity without overshooting)
            for k in 1..n - 1 {
                if deltas[k - 1] * deltas[k] <= 0.0 {
                    d[k] = 0.0;
                } else {
                    let w1 = 2.0 * h[k] + h[k - 1];
                    let w2 = h[k] + 2.0 * h[k - 1];
                    let denom = w1 * deltas[k] + w2 * deltas[k - 1];
                    if denom.abs() > 1e-12 {
                        d[k] = (w1 + w2) * deltas[k - 1] * deltas[k] / denom;
                    } else {
                        d[k] = 0.0;
                    }
                }
            }

            // Endpoints: standard shape-preserving one-sided 3-point estimates
            let d0 = ((2.0 * h[0] + h[1]) * deltas[0] - h[0] * deltas[1]) / (h[0] + h[1]);
            d[0] = if d0 * deltas[0] <= 0.0 {
                0.0
            } else if deltas[0] * deltas[1] <= 0.0 && d0.abs() > 3.0 * deltas[0].abs() {
                3.0 * deltas[0]
            } else {
                d0
            };

            let dn = ((2.0 * h[n - 2] + h[n - 3]) * deltas[n - 2] - h[n - 2] * deltas[n - 3])
                / (h[n - 2] + h[n - 3]);
            d[n - 1] = if dn * deltas[n - 2] <= 0.0 {
                0.0
            } else if deltas[n - 2] * deltas[n - 3] <= 0.0 && dn.abs() > 3.0 * deltas[n - 2].abs() {
                3.0 * deltas[n - 2]
            } else {
                dn
            };
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

        Self { xs, ys, h, d }
    }

    /// Evaluates the spline at coordinate `x` in `[0.0, 1.0]` with zero heap allocations.
    #[must_use]
    #[inline]
    pub fn evaluate(&self, x: f32) -> f32 {
        let n = self.xs.len();
        if n == 0 {
            return x.clamp(0.0, 1.0);
        }
        if n == 1 {
            return self.ys[0].clamp(0.0, 1.0);
        }

        let x_clamped = x.clamp(0.0, 1.0);
        if x_clamped <= self.xs[0] {
            return self.ys[0].clamp(0.0, 1.0);
        }
        if x_clamped >= self.xs[n - 1] {
            return self.ys[n - 1].clamp(0.0, 1.0);
        }

        // Robust binary search with partition_point
        let partition = self.xs.partition_point(|&knot_x| knot_x <= x_clamped);
        let idx = if partition == 0 {
            0
        } else if partition >= n {
            n - 2
        } else {
            partition - 1
        };

        let dx = self.h[idx];
        let t = ((x_clamped - self.xs[idx]) / dx).clamp(0.0, 1.0);
        let t2 = t * t;
        let t3 = t2 * t;

        // Hermite basis functions
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;

        let y = h00 * self.ys[idx]
            + h10 * dx * self.d[idx]
            + h01 * self.ys[idx + 1]
            + h11 * dx * self.d[idx + 1];

        y.clamp(0.0, 1.0)
    }
}

/// Evaluates a single-channel tone curve at coordinate `x` using Fritsch-Carlson PCHIP interpolation.
///
/// Ensures strict monotonicity preservation without Runge's phenomenon or overshoot.
#[must_use]
pub fn evaluate_channel_curve_pchip(curve: &ChannelCurve, x: f32) -> f32 {
    PchipSpline::new(curve).evaluate(x)
}

/// Generates a 1024-entry 1D LUT containing evaluated `[R, G, B, Luma]` samples.
///
/// Precomputes `PchipSpline` once per channel to achieve strictly zero heap allocations during LUT evaluation.
///
/// Mapping per channel:
/// - R: `curve_R(curve_RGB(x))`
/// - G: `curve_G(curve_RGB(x))`
/// - B: `curve_B(curve_RGB(x))`
/// - Luma: `curve_Luma(x)`
#[must_use]
pub fn generate_1d_lut_1024(recipe: &ToneCurvesRecipe) -> Box<[[f32; 4]; 1024]> {
    let spline_rgb = PchipSpline::new(&recipe.combined_rgb);
    let spline_r = PchipSpline::new(&recipe.red);
    let spline_g = PchipSpline::new(&recipe.green);
    let spline_b = PchipSpline::new(&recipe.blue);
    let spline_luma = PchipSpline::new(&recipe.luma);

    let mut lut = Box::new([[0.0_f32; 4]; 1024]);
    let denom = 1023.0_f32;

    for (i, entry) in lut.iter_mut().enumerate() {
        let x = i as f32 / denom;
        let rgb_inter = spline_rgb.evaluate(x);

        let r = spline_r.evaluate(rgb_inter);
        let g = spline_g.evaluate(rgb_inter);
        let b = spline_b.evaluate(rgb_inter);
        let luma = spline_luma.evaluate(x);

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
