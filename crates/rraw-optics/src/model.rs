/// Radial polynomial distortion model `r_d = r_u * (1 + k1*r^2 + k2*r^4 + k3*r^6)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistortionPoly {
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
}

impl Default for DistortionPoly {
    fn default() -> Self {
        Self {
            k1: 0.0,
            k2: 0.0,
            k3: 0.0,
        }
    }
}

impl DistortionPoly {
    /// Creates a pure pincushion distortion model.
    #[must_use]
    pub const fn pincushion(amount: f32) -> Self {
        Self {
            k1: amount,
            k2: 0.0,
            k3: 0.0,
        }
    }

    /// Creates a pure barrel distortion model.
    #[must_use]
    pub const fn barrel(amount: f32) -> Self {
        Self {
            k1: amount,
            k2: 0.0,
            k3: 0.0,
        }
    }

    /// Evaluates distorted radius from undistorted normalized radius.
    #[must_use]
    pub fn evaluate_radius(&self, r: f32) -> f32 {
        let r2 = r * r;
        let r4 = r2 * r2;
        let r6 = r4 * r2;
        r * (1.0 + self.k1 * r2 + self.k2 * r4 + self.k3 * r6)
    }
}

/// Lateral transverse chromatic aberration (TCA) radial scaling relative to Green.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LateralTca {
    pub scale_r: f32,
    pub scale_b: f32,
}

impl Default for LateralTca {
    fn default() -> Self {
        Self {
            scale_r: 1.0,
            scale_b: 1.0,
        }
    }
}

/// Radial polynomial optical vignetting model `V(r) = 1 + v1*r^2 + v2*r^4 + v3*r^6`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadialVignette {
    pub v1: f32,
    pub v2: f32,
    pub v3: f32,
}

impl Default for RadialVignette {
    fn default() -> Self {
        Self {
            v1: 0.0,
            v2: 0.0,
            v3: 0.0,
        }
    }
}

impl RadialVignette {
    /// Constructs a polynomial vignetting model.
    #[must_use]
    pub const fn poly(v1: f32, v2: f32) -> Self {
        Self { v1, v2, v3: 0.0 }
    }

    /// Evaluates vignetting gain factor at normalized radius `r`.
    #[must_use]
    pub fn evaluate(&self, r: f32) -> f32 {
        let r2 = r * r;
        let r4 = r2 * r2;
        let r6 = r4 * r2;
        (1.0 + self.v1 * r2 + self.v2 * r4 + self.v3 * r6).max(0.0)
    }
}

/// Comprehensive optical lens aberration correction profile.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OpticalCorrectionModel {
    pub distortion: DistortionPoly,
    pub lateral_ca: LateralTca,
    pub vignette: RadialVignette,
}
