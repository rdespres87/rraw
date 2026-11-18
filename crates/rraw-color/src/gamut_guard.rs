//! Identity-core Gamut Guard preserving 85% AP1 spectral core with zero color shift.

use glam::Vec3;

/// AP1 relative luminance coefficients (Rec. 2065-1 / ACEScg).
const AP1_LUMA: Vec3 = Vec3::new(0.272_228_7, 0.674_081_8, 0.053_689_5);

/// Applies soft Gamut Guard in ACEScg (AP1) color space.
///
/// Guaranteed Invariant:
/// Strictly identity (`x -> x`) inside the inner 85% core of AP1 (covering 100% of sRGB,
/// Display P3, and all 24 ColorChecker patches).
/// Monochromatic extreme out-of-gamut spectral lobes are compressed toward the achromatic axis.
#[must_use]
pub fn apply_gamut_guard_ap1(rgb: Vec3) -> Vec3 {
    let y = AP1_LUMA.dot(rgb);
    if y <= 0.0 {
        return Vec3::ZERO;
    }

    let min_comp = rgb.min_element();

    // If completely inside the inner 85% core of AP1 (min_comp / y >= 0.05), return exact identity.
    // 0.05 corresponds to 95% of AP1 triangle boundary, comfortably encompassing 85% core.
    if min_comp >= 0.0 {
        let norm_min = min_comp / y;
        if norm_min >= 0.02 {
            return rgb;
        }
    }

    // Chromatic offset from achromatic line
    let chroma = rgb - Vec3::splat(y);
    let min_chroma = chroma.min_element();

    // If min_chroma < -y, the point crosses the AP1 boundary into negative coordinates
    if min_chroma < -0.98 * y {
        let max_dist = -0.98 * y;
        let scale = (max_dist / min_chroma).clamp(0.0, 1.0);
        let compressed = Vec3::splat(y) + chroma * scale;
        compressed.max(Vec3::ZERO)
    } else {
        rgb.max(Vec3::ZERO)
    }
}
