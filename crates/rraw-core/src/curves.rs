#![forbid(unsafe_code)]

//! Parametric tone curve representations with monotonicity invariants.

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// A 2D control point in normalized coordinate space `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}

impl CurvePoint {
    /// Creates a new control point, validating that coordinates are finite and in `[0.0, 1.0]`.
    pub fn new(x: f32, y: f32) -> Result<Self, CoreError> {
        if !x.is_finite() || !(0.0..=1.0).contains(&x) {
            return Err(CoreError::InvalidPhysicalValue {
                field: "curve_point_x",
                value: x,
            });
        }
        if !y.is_finite() || !(0.0..=1.0).contains(&y) {
            return Err(CoreError::InvalidPhysicalValue {
                field: "curve_point_y",
                value: y,
            });
        }
        Ok(Self { x, y })
    }
}

/// A monotonic single-channel tone curve specified by sorted control points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelCurve {
    pub points: Vec<CurvePoint>,
}

impl Default for ChannelCurve {
    fn default() -> Self {
        Self::new_linear()
    }
}

impl ChannelCurve {
    /// Constructs a standard identity linear curve from `(0.0, 0.0)` to `(1.0, 1.0)`.
    #[must_use]
    pub fn new_linear() -> Self {
        Self {
            points: vec![CurvePoint { x: 0.0, y: 0.0 }, CurvePoint { x: 1.0, y: 1.0 }],
        }
    }

    /// Returns true if this curve is an identity linear map.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        if self.points.len() == 2 {
            let p0 = self.points[0];
            let p1 = self.points[1];
            (p0.x - 0.0).abs() < 1e-6
                && (p0.y - 0.0).abs() < 1e-6
                && (p1.x - 1.0).abs() < 1e-6
                && (p1.y - 1.0).abs() < 1e-6
        } else {
            false
        }
    }

    /// Adds or updates a control point while preserving strictly sorted x order.
    pub fn add_point(&mut self, point: CurvePoint) {
        let idx = self.points.partition_point(|p| p.x < point.x);
        if idx < self.points.len() && (self.points[idx].x - point.x).abs() < 1e-4 {
            self.points[idx] = point;
        } else {
            self.points.insert(idx, point);
        }
    }

    /// Removes an interior control point. Endpoints (first and last) cannot be removed.
    pub fn remove_point(&mut self, index: usize) -> bool {
        if self.points.len() > 2 && index > 0 && index < self.points.len() - 1 {
            self.points.remove(index);
            true
        } else {
            false
        }
    }
}

/// Multi-channel parametric tone curve recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ToneCurvesRecipe {
    pub combined_rgb: ChannelCurve,
    pub red: ChannelCurve,
    pub green: ChannelCurve,
    pub blue: ChannelCurve,
    pub luma: ChannelCurve,
}

impl ToneCurvesRecipe {
    /// Returns true if all channels are in identity linear state.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.combined_rgb.is_identity()
            && self.red.is_identity()
            && self.green.is_identity()
            && self.blue.is_identity()
            && self.luma.is_identity()
    }
}
