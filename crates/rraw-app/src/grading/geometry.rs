#![forbid(unsafe_code)]

use crate::state::CropAspectRatio;
use serde::{Deserialize, Serialize};

pub const fn default_one_f32() -> f32 {
    1.0
}

pub const fn default_true_bool() -> bool {
    true
}

/// Geometric transformation parameters (crop, rotation, keystone, perspective).
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryGradingParams {
    pub crop_enabled: bool,
    pub crop_aspect: CropAspectRatio,
    pub crop_left: f32,
    pub crop_top: f32,
    pub crop_right: f32,
    pub crop_bottom: f32,
    pub rotation_degrees: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub geometry_enabled: bool,
    pub keystone_vertical: f32,
    pub keystone_horizontal: f32,
    pub auto_crop_geometry: bool,
}

impl Default for GeometryGradingParams {
    fn default() -> Self {
        Self {
            crop_enabled: false,
            crop_aspect: CropAspectRatio::Free,
            crop_left: 0.0,
            crop_top: 0.0,
            crop_right: 1.0,
            crop_bottom: 1.0,
            rotation_degrees: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            geometry_enabled: true,
            keystone_vertical: 0.0,
            keystone_horizontal: 0.0,
            auto_crop_geometry: true,
        }
    }
}

impl GeometryGradingParams {
    /// Untouched raw baseline (no crop, no rotation, no keystone).
    #[must_use]
    pub const fn untouched_raw() -> Self {
        Self {
            crop_enabled: false,
            crop_aspect: CropAspectRatio::Free,
            crop_left: 0.0,
            crop_top: 0.0,
            crop_right: 1.0,
            crop_bottom: 1.0,
            rotation_degrees: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            geometry_enabled: false,
            keystone_vertical: 0.0,
            keystone_horizontal: 0.0,
            auto_crop_geometry: false,
        }
    }
}

/// Serialized geometry recipe for sidecars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryRecipeParams {
    #[serde(default)]
    pub crop_enabled: bool,
    pub crop_aspect: Option<String>,
    #[serde(default)]
    pub crop_left: f32,
    #[serde(default)]
    pub crop_top: f32,
    #[serde(default = "default_one_f32")]
    pub crop_right: f32,
    #[serde(default = "default_one_f32")]
    pub crop_bottom: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default)]
    pub flip_horizontal: bool,
    #[serde(default)]
    pub flip_vertical: bool,
    #[serde(default = "default_true_bool")]
    pub geometry_enabled: bool,
    #[serde(default)]
    pub keystone_vertical: f32,
    #[serde(default)]
    pub keystone_horizontal: f32,
}

impl From<&GeometryGradingParams> for GeometryRecipeParams {
    fn from(g: &GeometryGradingParams) -> Self {
        Self {
            crop_enabled: g.crop_enabled,
            crop_aspect: Some(format!("{:?}", g.crop_aspect)),
            crop_left: g.crop_left,
            crop_top: g.crop_top,
            crop_right: g.crop_right,
            crop_bottom: g.crop_bottom,
            rotation_degrees: g.rotation_degrees,
            flip_horizontal: g.flip_horizontal,
            flip_vertical: g.flip_vertical,
            geometry_enabled: g.geometry_enabled,
            keystone_vertical: g.keystone_vertical,
            keystone_horizontal: g.keystone_horizontal,
        }
    }
}

impl GeometryRecipeParams {
    pub fn apply_to(&self, target: &mut GeometryGradingParams) {
        target.crop_enabled = self.crop_enabled;
        target.crop_aspect = match self.crop_aspect.as_deref() {
            Some("Square1x1") | Some("Ratio1x1") => CropAspectRatio::Square1x1,
            Some("Ratio3x2") => CropAspectRatio::Ratio3x2,
            Some("Ratio4x3") => CropAspectRatio::Ratio4x3,
            Some("Ratio16x9") => CropAspectRatio::Ratio16x9,
            Some("Ratio5x4") => CropAspectRatio::Ratio5x4,
            Some("Ratio9x16") => CropAspectRatio::Ratio9x16,
            Some("Original") => CropAspectRatio::Original,
            _ => CropAspectRatio::Free,
        };
        target.crop_left = self.crop_left;
        target.crop_top = self.crop_top;
        target.crop_right = if self.crop_right <= 0.01 {
            1.0
        } else {
            self.crop_right
        };
        target.crop_bottom = if self.crop_bottom <= 0.01 {
            1.0
        } else {
            self.crop_bottom
        };
        target.rotation_degrees = self.rotation_degrees;
        target.flip_horizontal = self.flip_horizontal;
        target.flip_vertical = self.flip_vertical;
        target.geometry_enabled = self.geometry_enabled;
        target.keystone_vertical = self.keystone_vertical;
        target.keystone_horizontal = self.keystone_horizontal;
    }
}
