//! Correlated Color Temperature (CCT) solver and mired utilities.

use crate::ColorError;

/// Converts CIE 1931 (x, y) chromaticity coordinates into Correlated Color Temperature (Kelvin).
///
/// Implements McCamy's analytical cubic approximation with validation of physical chromaticity bounds:
/// `n = (x - 0.3320) / (0.1858 - y)`
/// `CCT = 449 * n^3 + 3525 * n^2 + 6823.3 * n + 5520.33`
pub fn chromaticity_to_cct(x: f64, y: f64) -> Result<f64, ColorError> {
    if !x.is_finite() || !y.is_finite() || x <= 0.0 || y <= 0.0 || (x + y) >= 1.0 {
        return Err(ColorError::InvalidKelvin {
            value: if x.is_finite() { x } else { f64::NAN },
        });
    }

    let denom = 0.1858 - y;
    if denom.abs() < 1e-9 {
        return Err(ColorError::InvalidKelvin { value: y });
    }

    let n = (x - 0.3320) / denom;
    let cct = 449.0 * n.powi(3) + 3525.0 * n.powi(2) + 6823.3 * n + 5520.33;

    if !cct.is_finite() || cct <= 0.0 {
        return Err(ColorError::InvalidKelvin { value: cct });
    }

    Ok(cct)
}
