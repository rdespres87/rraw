#![forbid(unsafe_code)]

use crate::state::OpticalCorrectionTab;
use serde::{Deserialize, Serialize};

pub const fn default_true_bool() -> bool {
    true
}

pub const fn default_100_f32() -> f32 {
    100.0
}

pub const fn default_defringe_threshold() -> f32 {
    0.08
}

/// Optical corrections (lens profile, manual distortion, CA defringe, vignetting).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticsGradingParams {
    #[serde(default)]
    pub optical_tab: OpticalCorrectionTab,
    #[serde(default = "default_true_bool")]
    pub lens_profile_auto: bool,
    #[serde(default = "default_100_f32")]
    pub lens_distortion_strength: f32,
    #[serde(default = "default_100_f32")]
    pub lens_vignette_strength: f32,
    #[serde(default = "default_100_f32")]
    pub lens_ca_strength: f32,
    pub distortion_correction: bool,
    pub distortion_amount: f32,
    pub chromatic_aberration_correction: bool,
    #[serde(default)]
    pub defringe_purple_amount: f32,
    #[serde(default)]
    pub defringe_green_amount: f32,
    #[serde(default = "default_defringe_threshold")]
    pub defringe_threshold: f32,
    #[serde(default)]
    pub defringe_preview_mask: bool,
    pub vignette_correction: bool,
    pub vignette_amount: f32,
}

impl Default for OpticsGradingParams {
    fn default() -> Self {
        Self {
            optical_tab: OpticalCorrectionTab::Profile,
            lens_profile_auto: true,
            lens_distortion_strength: 100.0,
            lens_vignette_strength: 100.0,
            lens_ca_strength: 100.0,
            distortion_correction: true,
            distortion_amount: 0.0,
            chromatic_aberration_correction: true,
            defringe_purple_amount: 0.0,
            defringe_green_amount: 0.0,
            defringe_threshold: 0.08,
            defringe_preview_mask: false,
            vignette_correction: true,
            vignette_amount: 50.0,
        }
    }
}

impl OpticsGradingParams {
    /// Untouched raw baseline (all corrections disabled).
    #[must_use]
    pub const fn untouched_raw() -> Self {
        Self {
            optical_tab: OpticalCorrectionTab::Manual,
            lens_profile_auto: false,
            lens_distortion_strength: 0.0,
            lens_vignette_strength: 0.0,
            lens_ca_strength: 0.0,
            distortion_correction: false,
            distortion_amount: 0.0,
            chromatic_aberration_correction: false,
            defringe_purple_amount: 0.0,
            defringe_green_amount: 0.0,
            defringe_threshold: 0.08,
            defringe_preview_mask: false,
            vignette_correction: false,
            vignette_amount: 0.0,
        }
    }
}
