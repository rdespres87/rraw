#![forbid(unsafe_code)]

use rraw_color::DisplayTransform;
use serde::{Deserialize, Serialize};

/// White balance and display color transform parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorGradingParams {
    pub kelvin: f32,
    pub tint: f32,
    pub display_transform: DisplayTransform,
    pub is_monochrome: bool,
}

impl Default for ColorGradingParams {
    fn default() -> Self {
        Self {
            kelvin: 5500.0,
            tint: 0.0,
            display_transform: DisplayTransform::AgX,
            is_monochrome: false,
        }
    }
}

impl ColorGradingParams {
    /// Untouched raw baseline.
    #[must_use]
    pub const fn untouched_raw() -> Self {
        Self {
            kelvin: 5500.0,
            tint: 0.0,
            display_transform: DisplayTransform::AgX,
            is_monochrome: false,
        }
    }

    /// Returns true if white balance deviates from D55 daylight or monochrome is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        (self.kelvin - 5500.0).abs() > 1e-2 || self.tint.abs() > 1e-2 || self.is_monochrome
    }
}

/// Serialized color recipe for sidecars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorRecipeParams {
    pub kelvin: f32,
    pub tint: f32,
    pub display_transform: String,
    #[serde(default)]
    pub is_monochrome: bool,
}

impl From<&ColorGradingParams> for ColorRecipeParams {
    fn from(c: &ColorGradingParams) -> Self {
        Self {
            kelvin: c.kelvin,
            tint: c.tint,
            display_transform: match c.display_transform {
                DisplayTransform::AgX => "AgX".to_string(),
                DisplayTransform::OpenDrt => "OpenDrt".to_string(),
            },
            is_monochrome: c.is_monochrome,
        }
    }
}

impl ColorRecipeParams {
    pub fn apply_to(&self, target: &mut ColorGradingParams) {
        target.kelvin = self.kelvin;
        target.tint = self.tint;
        target.display_transform = if self.display_transform == "OpenDrt" {
            DisplayTransform::OpenDrt
        } else {
            DisplayTransform::AgX
        };
        target.is_monochrome = self.is_monochrome;
    }
}
