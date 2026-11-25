#![forbid(unsafe_code)]

//! Dynamic CFA pattern resolution and phase-preserving cropping utilities.

use rraw_core::{BayerOrigin, CfaPattern};

/// Strictly resolves a canonical `rraw_core::CfaPattern` from a `rawler::cfa::CFA` structure.
///
/// Returns `None` when the sensor CFA is empty or uses a non-Bayer / non-X-Trans layout
/// (e.g., 4-color CYGM/RGBE or monochrome sensors).
#[must_use]
pub fn try_resolve_cfa_pattern_from_rawler(cfa: &rawler::cfa::CFA) -> Option<CfaPattern> {
    if (cfa.width == 6 && cfa.height == 6)
        || cfa.name.len() == 36
        || cfa.name.to_uppercase().contains("XTRANS")
    {
        return Some(CfaPattern::fujifilm_xtrans());
    }

    let name_upper = cfa.name.to_uppercase();
    match name_upper.as_str() {
        "RGGB" => return Some(CfaPattern::Bayer(BayerOrigin::Rggb)),
        "BGGR" => return Some(CfaPattern::Bayer(BayerOrigin::Bggr)),
        "GRBG" => return Some(CfaPattern::Bayer(BayerOrigin::Grbg)),
        "GBRG" => return Some(CfaPattern::Bayer(BayerOrigin::Gbrg)),
        _ => {}
    }

    if cfa.width == 2 && cfa.height == 2 {
        let tl = cfa.color_at(0, 0);
        let tr = cfa.color_at(0, 1);
        let bl = cfa.color_at(1, 0);
        let br = cfa.color_at(1, 1);

        const R: usize = 0;
        const G: usize = 1;
        const B: usize = 2;

        return match (tl, tr, bl, br) {
            (R, G, G, B) => Some(CfaPattern::Bayer(BayerOrigin::Rggb)),
            (B, G, G, R) => Some(CfaPattern::Bayer(BayerOrigin::Bggr)),
            (G, R, B, G) => Some(CfaPattern::Bayer(BayerOrigin::Grbg)),
            (G, B, R, G) => Some(CfaPattern::Bayer(BayerOrigin::Gbrg)),
            _ => None,
        };
    }

    None
}

/// Resolves a canonical `rraw_core::CfaPattern` from a `rawler::cfa::CFA` structure,
/// falling back to `Bayer(Rggb)` when unrecognized.
#[must_use]
pub fn resolve_cfa_pattern_from_rawler(cfa: &rawler::cfa::CFA) -> CfaPattern {
    try_resolve_cfa_pattern_from_rawler(cfa).unwrap_or(CfaPattern::Bayer(BayerOrigin::Rggb))
}

/// Sensor crop rectangle in raw sensor coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorCropRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Full sensor dimensions in raw sensor pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorDimensions {
    pub width: usize,
    pub height: usize,
}

/// Aligns crop margins to preserve the underlying sensor CFA phase.
///
/// - For Bayer (`period = 2`), coordinates are aligned to multiples of 2.
/// - For Fujifilm X-Trans (`period = 6`), coordinates are aligned to multiples of 6.
///
/// Boundaries are clamped to `bounds` to prevent out-of-bounds indexing.
#[must_use]
pub fn align_crop_to_cfa_phase(
    cfa_pattern: &CfaPattern,
    crop: SensorCropRect,
    bounds: SensorDimensions,
) -> SensorCropRect {
    let align = match cfa_pattern {
        CfaPattern::Bayer(_) => 2,
        CfaPattern::XTrans(_) => 6,
    };

    let ax = (crop.x / align) * align;
    let ay = (crop.y / align) * align;
    let aw = (crop.width / align) * align;
    let ah = (crop.height / align) * align;

    let clamped_w = if ax + aw <= bounds.width {
        aw
    } else {
        ((bounds.width.saturating_sub(ax)) / align) * align
    };

    let clamped_h = if ay + ah <= bounds.height {
        ah
    } else {
        ((bounds.height.saturating_sub(ay)) / align) * align
    };

    SensorCropRect {
        x: ax,
        y: ay,
        width: clamped_w,
        height: clamped_h,
    }
}
