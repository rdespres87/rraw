#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

pub const fn default_color_denoise() -> f32 {
    25.0
}

/// Noise reduction and capture sharpening parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DenoiseSharpenParams {
    pub wavelet_denoise_sigma: f32,
    #[serde(default = "default_color_denoise")]
    pub color_denoise: f32,
    #[serde(default)]
    pub luma_denoise: f32,
    pub capture_sharpening_amount: f32,
}

impl Default for DenoiseSharpenParams {
    fn default() -> Self {
        Self {
            wavelet_denoise_sigma: 1.0,
            color_denoise: 25.0,
            luma_denoise: 0.0,
            capture_sharpening_amount: 0.5,
        }
    }
}

impl DenoiseSharpenParams {
    /// Untouched raw baseline (zero denoise and sharpening).
    #[must_use]
    pub const fn untouched_raw() -> Self {
        Self {
            wavelet_denoise_sigma: 0.0,
            color_denoise: 0.0,
            luma_denoise: 0.0,
            capture_sharpening_amount: 0.0,
        }
    }

    /// Returns true if custom denoise or sharpening is configured.
    #[must_use]
    pub fn is_active(&self) -> bool {
        (self.wavelet_denoise_sigma - 1.0).abs() > 1e-4
            || (self.color_denoise - 25.0).abs() > 1e-4
            || self.luma_denoise.abs() > 1e-4
            || (self.capture_sharpening_amount - 0.5).abs() > 1e-4
    }
}
