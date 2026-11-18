#![forbid(unsafe_code)]

//! Generalized Anscombe Transform (GAT) and dynamic compression conditioning.

/// Parameters for Generalized Anscombe Transform variance stabilization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnscombeParams {
    /// Photon shot-noise gain parameter ($\alpha$).
    pub alpha: f32,
    /// Electronic read-noise variance ($\sigma^2$).
    pub sigma_sq: f32,
}

impl AnscombeParams {
    /// Constructs validated Anscombe parameters.
    #[must_use]
    pub fn new(alpha: f32, sigma_sq: f32) -> Self {
        Self {
            alpha: if alpha.is_finite() && alpha > 1e-12 {
                alpha
            } else {
                1e-12
            },
            sigma_sq: if sigma_sq.is_finite() && sigma_sq >= 0.0 {
                sigma_sq
            } else {
                0.0
            },
        }
    }
}

/// Applies the forward Generalized Anscombe Transform to stabilize Poisson-Gaussian noise.
///
/// Transform: $f(x) = \frac{2}{\alpha} \sqrt{\max\left(\alpha x + \frac{3}{8}\alpha^2 + \sigma^2, 0.0\right)}$.
#[must_use]
pub fn anscombe_transform(val: f32, params: &AnscombeParams) -> f32 {
    let alpha = params.alpha.max(1e-12);
    let radicand = (alpha * val + 0.375 * alpha * alpha + params.sigma_sq).max(0.0);
    (2.0 / alpha) * radicand.sqrt()
}

/// Inverts the Generalized Anscombe Transform back to physical linear signal space.
///
/// Inversion: $x = \frac{\alpha y^2}{4} - \frac{3}{8}\alpha - \frac{\sigma^2}{\alpha}$.
#[must_use]
pub fn inverse_anscombe_transform(stabilized: f32, params: &AnscombeParams) -> f32 {
    let alpha = params.alpha.max(1e-12);
    let term1 = (alpha * stabilized * stabilized) * 0.25;
    let term2 = 0.375 * alpha;
    let term3 = params.sigma_sq / alpha;
    (term1 - term2 - term3).max(0.0)
}
