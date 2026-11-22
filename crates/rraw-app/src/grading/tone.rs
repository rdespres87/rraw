#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Basic tone grading parameters (exposure, contrast, highlights, shadows, blacks, saturation, vibrance).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToneGradingParams {
    pub exposure_ev: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    #[serde(default)]
    pub blacks: f32,
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub vibrance: f32,
}

impl Default for ToneGradingParams {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            blacks: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
        }
    }
}

impl ToneGradingParams {
    /// Untouched raw baseline (all neutral at 0.0).
    #[must_use]
    pub const fn untouched_raw() -> Self {
        Self {
            exposure_ev: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            blacks: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
        }
    }

    /// Returns true if any tone parameter deviates from neutral 0.0.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.exposure_ev.abs() > 1e-4
            || self.contrast.abs() > 1e-4
            || self.highlights.abs() > 1e-4
            || self.shadows.abs() > 1e-4
            || self.blacks.abs() > 1e-4
            || self.saturation.abs() > 1e-4
            || self.vibrance.abs() > 1e-4
    }
}
