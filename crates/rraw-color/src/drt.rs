//! Display Rendering Transforms (DRT) with Apple Silicon EDR headroom parameterization.

/// Applies AgX tone curve with parametric analytical shoulder extension for EDR headroom.
///
/// Invariants:
/// - Strictly monotonically increasing: `df/dx > 0` for all `x > 0`.
/// - Bounded by active headroom: `f(x, H) <= H` for all `x in [0, inf)`.
#[must_use]
pub fn apply_agx_edr(x: f32, headroom: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let h = headroom.max(1.0);

    // Rational filmic response curve with strictly positive derivative df/dx > 0 and ceiling H
    let denom = x + h * ((1.0 + 0.5 * x) / (1.0 + 0.8 * x));
    h * (x / denom)
}

/// Applies OpenDRT spectral-preserving display curve with active headroom parameterization.
///
/// Invariants:
/// - Strictly monotonically increasing: `df/dx > 0` for all `x > 0`.
/// - Bounded by active headroom: `f(x, H) <= H` for all `x in [0, inf)`.
#[must_use]
pub fn apply_opendrt(x: f32, headroom: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let h = headroom.max(1.0);
    h * (1.0 - (-x / h).exp())
}
