#![forbid(unsafe_code)]

//! Creative effects parameters (creative vignetting, styling).

use serde::{Deserialize, Serialize};

pub const fn default_midpoint() -> f32 {
    50.0
}

pub const fn default_feather() -> f32 {
    50.0
}

/// Creative effects (creative vignetting, post-crop effects).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectsGradingParams {
    #[serde(default)]
    pub creative_vignette_amount: f32,
    #[serde(default = "default_midpoint")]
    pub creative_vignette_midpoint: f32,
    #[serde(default = "default_feather")]
    pub creative_vignette_feather: f32,
    #[serde(default)]
    pub creative_vignette_roundness: f32,
}

impl Default for EffectsGradingParams {
    fn default() -> Self {
        Self {
            creative_vignette_amount: 0.0,
            creative_vignette_midpoint: 50.0,
            creative_vignette_feather: 50.0,
            creative_vignette_roundness: 0.0,
        }
    }
}

impl EffectsGradingParams {
    /// Untouched raw baseline (all creative effects at zero / neutral).
    #[must_use]
    pub const fn untouched_raw() -> Self {
        Self {
            creative_vignette_amount: 0.0,
            creative_vignette_midpoint: 50.0,
            creative_vignette_feather: 50.0,
            creative_vignette_roundness: 0.0,
        }
    }

    /// Returns `true` if any creative effect parameter is modified from its neutral state.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.creative_vignette_amount.abs() > 0.01
    }
}
