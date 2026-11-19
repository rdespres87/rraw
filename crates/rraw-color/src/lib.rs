#![forbid(unsafe_code)]

//! Spectral colorimetry (`ACEScg` `AP1` D60), tone mapping, and shared CPU reference pipeline.

use rraw_core::{ChannelSaturation, CoreError, WhiteBalanceMultipliers};
use thiserror::Error;

pub mod cct;
pub mod clut;
pub mod colorchecker;
pub mod curves;
pub mod drt;
pub mod gamut_guard;
pub mod matrix;
pub mod pipeline;
pub mod soft_proof;

pub use cct::*;
pub use clut::*;
pub use colorchecker::*;
pub use curves::*;
pub use drt::*;
pub use gamut_guard::*;
pub use matrix::*;
pub use pipeline::*;
pub use soft_proof::*;

/// Typed errors returned by `rraw-color`.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ColorError {
    /// Correlated color temperature was non-finite or non-positive.
    #[error("invalid color temperature in Kelvin: {value}")]
    InvalidKelvin { value: f64 },
    /// Mired interpolation input was `NaN` or infinite.
    #[error("invalid mired input for `{field}`: {value}")]
    InvalidMired { field: &'static str, value: f64 },
    /// Invalid 3D LUT dimension (expected between 2 and 65).
    #[error("invalid 3D LUT size {size}, expected between 2 and 65")]
    InvalidLutSize { size: usize },
    /// 3D LUT data length does not match expected size^3.
    #[error("3D LUT data length {actual} does not match expected size^3 ({expected})")]
    LutDataLengthMismatch { actual: usize, expected: usize },
    /// Failed to parse `.cube` 3D LUT format.
    #[error("failed to parse .cube LUT: {reason}")]
    InvalidCubeFile { reason: String },
    /// Propagated invariant error from `rraw-core`.
    #[error(transparent)]
    Core(#[from] CoreError),
}

/// Selects the DNG matrix calibration path into `ACEScg` (`AP1` D60).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DngMatrixPath {
    /// Priority path when `ForwardMatrix1/2` tags exist (takes white-balanced camera RGB to `XYZ D50`).
    ForwardMatrixToD50ThenBradfordD60,
    /// Fallback path inverting `(AB * CC(M) * CM(M))^-1 * diag(w)^-1` then Bradford to D60.
    ColorMatrixInverseThenBradfordD60,
}

/// Display Rendering Transform (DRT) options at Stage 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayTransform {
    AgX,
    OpenDrt,
}

/// Converts correlated color temperature in Kelvin to reciprocal megakelvin (mireds).
pub fn kelvin_to_mireds(kelvin: f64) -> Result<f64, ColorError> {
    if !kelvin.is_finite() || kelvin <= 0.0 {
        return Err(ColorError::InvalidKelvin { value: kelvin });
    }
    Ok(1_000_000.0 / kelvin)
}

/// Computes the clamped 1D linear mired interpolation weight `t in [0.0, 1.0]`.
///
/// Supports both ascending (`m1 < m2`) and descending (`m1 > m2`) calibration illuminants,
/// handles identical illuminants (`m1 == m2`) cleanly by returning `Ok(0.5)`, and rejects
/// non-finite (`NaN`/`Inf`) arguments with `Err(ColorError::InvalidMired)`.
pub fn mired_interpolation_factor(
    mired_scene: f64,
    mired_illuminant_1: f64,
    mired_illuminant_2: f64,
) -> Result<f64, ColorError> {
    for (field, value) in [
        ("mired_scene", mired_scene),
        ("mired_illuminant_1", mired_illuminant_1),
        ("mired_illuminant_2", mired_illuminant_2),
    ] {
        if !value.is_finite() {
            return Err(ColorError::InvalidMired { field, value });
        }
    }

    let denom = mired_illuminant_2 - mired_illuminant_1;
    if denom.abs() < 1e-9 {
        return Ok(0.5);
    }
    Ok(((mired_scene - mired_illuminant_1) / denom).clamp(0.0, 1.0))
}

/// Computes effective Laplacian pyramid octaves preserving scale invariance across mipmap levels.
///
/// Implements `N_effective = N_octaves - L` where `L = log2(downsample_factor) in {0, 1, 2}`.
#[must_use]
pub const fn effective_laplacian_octaves(base_octaves: u8, mipmap_level: u8) -> u8 {
    base_octaves.saturating_sub(mipmap_level)
}

/// Computes per-channel saturation ceilings `sat_c = 1.0 * w_c` for Stage 2/5 coordination.
pub fn stage2_saturation_ceilings(
    wb: WhiteBalanceMultipliers,
) -> Result<ChannelSaturation, ColorError> {
    Ok(ChannelSaturation::from_wb(wb)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mired_interpolation_handles_stda_to_d65_reversed_equal_and_errors() -> Result<(), ColorError>
    {
        let m_stda = kelvin_to_mireds(2856.0)?;
        let m_d65 = kelvin_to_mireds(6504.0)?;
        let m_cool = kelvin_to_mireds(10_000.0)?;
        let m_warm = kelvin_to_mireds(2000.0)?;

        // 1. Standard ordering: StdA (~350.14) -> D65 (~153.75), m1 > m2
        let m_mid = 0.5 * (m_stda + m_d65);
        assert!((mired_interpolation_factor(m_stda, m_stda, m_d65)? - 0.0).abs() < 1e-12);
        assert!((mired_interpolation_factor(m_mid, m_stda, m_d65)? - 0.5).abs() < 1e-12);
        assert!((mired_interpolation_factor(m_d65, m_stda, m_d65)? - 1.0).abs() < 1e-12);
        assert!((mired_interpolation_factor(m_cool, m_stda, m_d65)? - 1.0).abs() < 1e-12);
        assert!((mired_interpolation_factor(m_warm, m_stda, m_d65)? - 0.0).abs() < 1e-12);

        // 2. Reversed ordering: D65 -> StdA, m1 < m2
        assert!((mired_interpolation_factor(m_d65, m_d65, m_stda)? - 0.0).abs() < 1e-12);
        assert!((mired_interpolation_factor(m_stda, m_d65, m_stda)? - 1.0).abs() < 1e-12);
        assert!((mired_interpolation_factor(m_cool, m_d65, m_stda)? - 0.0).abs() < 1e-12);
        assert!((mired_interpolation_factor(m_warm, m_d65, m_stda)? - 1.0).abs() < 1e-12);

        // 3. Equal illuminants: m1 == m2 returns finite midpoint 0.5 without division by zero
        let t_equal = mired_interpolation_factor(m_d65, m_d65, m_d65)?;
        assert!(t_equal.is_finite());
        assert!((t_equal - 0.5).abs() < 1e-12);

        // 4. Error handling for NaN, Inf, 0.0, and negative Kelvin / mired inputs
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -2856.0] {
            assert!(matches!(
                kelvin_to_mireds(bad),
                Err(ColorError::InvalidKelvin { .. })
            ));
        }
        assert!(matches!(
            mired_interpolation_factor(f64::NAN, m_stda, m_d65),
            Err(ColorError::InvalidMired {
                field: "mired_scene",
                ..
            })
        ));
        assert!(matches!(
            mired_interpolation_factor(m_d65, f64::INFINITY, m_d65),
            Err(ColorError::InvalidMired {
                field: "mired_illuminant_1",
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn scale_invariance_reduces_effective_octaves_by_mipmap_level() {
        assert_eq!(effective_laplacian_octaves(6, 0), 6);
        assert_eq!(effective_laplacian_octaves(6, 1), 5);
        assert_eq!(effective_laplacian_octaves(6, 2), 4);
    }

    #[test]
    fn saturation_ceilings_match_white_balance_multipliers() -> Result<(), ColorError> {
        let wb = WhiteBalanceMultipliers::new(2.0, 1.0, 1.5)?;
        let sat = stage2_saturation_ceilings(wb)?;
        assert!((sat.r() - 2.0).abs() < f32::EPSILON);
        assert!((sat.g() - 1.0).abs() < f32::EPSILON);
        assert!((sat.b() - 1.5).abs() < f32::EPSILON);
        Ok(())
    }
}
