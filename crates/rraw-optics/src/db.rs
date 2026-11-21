//! Lens correction database and profile matching for the `rraw-optics` crate.
//!
//! Provides a curated catalog of popular prime and zoom lenses with calibrated
//! optical parameters (polynomial distortion, lateral TCA, and radial vignetting),
//! smooth focal/aperture interpolation, and fuzzy metadata matching.

use crate::model::{DistortionPoly, LateralTca, OpticalCorrectionModel, RadialVignette};
use rraw_core::PhotoMetadata;

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A single calibrated focal-length sample within a lens profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibratedFocalPoint {
    /// Focal length in millimetres at which this sample was measured.
    pub focal_mm: f32,
    /// Radial distortion polynomial coefficients (k1, k2, k3).
    pub distortion: DistortionPoly,
    /// Lateral chromatic aberration scale factors (red, blue) relative to green.
    pub lateral_ca: LateralTca,
    /// Radial vignette coefficients at the lens's widest aperture.
    pub vignette_wide_open: RadialVignette,
    /// Radial vignette coefficients at the stopped-down reference (f/5.6 - f/8.0).
    pub vignette_stopped_down: RadialVignette,
}

/// A complete lens profile with a set of calibrated focal points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensProfile {
    /// Canonical lens name (e.g. `"Sony FE 24-70mm F2.8 GM"`).
    pub name: &'static str,
    /// Lens manufacturer (e.g. `"Sony"`, `"Canon"`, `"Nikon"`).
    pub maker: &'static str,
    /// Alternative model names / aliases for fuzzy matching.
    pub aliases: &'static [&'static str],
    /// Inclusive focal-length range in mm: `(min, max)`.
    pub focal_range: (f32, f32),
    /// Aperture range: `(wide_open, max_stopped_down)` in f-numbers.
    pub aperture_range: (f32, f32),
    /// Calibrated samples sorted by ascending focal length.
    pub calibrated_points: &'static [CalibratedFocalPoint],
}

impl LensProfile {
    /// Interpolates an [`OpticalCorrectionModel`] for the given focal length
    /// and optional aperture.
    ///
    /// * Focal length is linearly interpolated between the two bracketing
    ///   calibrated points. Values outside `focal_range` are clamped.
    /// * Vignette is interpolated between `vignette_wide_open` and
    ///   `vignette_stopped_down` based on `aperture`.
    #[must_use]
    pub fn interpolate_model(
        &self,
        focal_mm: f32,
        aperture: Option<f32>,
    ) -> OpticalCorrectionModel {
        let (f_min, f_max) = self.focal_range;
        let clamped_f = focal_mm.clamp(f_min, f_max);

        let (dist, lat, vig_wide, vig_stopped) = self.interpolate_focal(clamped_f);

        let (a_wide, _) = self.aperture_range;
        let a_stopped_ref = 8.0_f32;

        let vignette = match aperture {
            None => vig_wide,
            Some(a) if a <= a_wide => vig_wide,
            Some(a) if a >= a_stopped_ref => vig_stopped,
            Some(a) => {
                let range = (a_stopped_ref - a_wide).max(0.1);
                let t = ((a - a_wide) / range).clamp(0.0, 1.0);
                RadialVignette {
                    v1: lerp(vig_wide.v1, vig_stopped.v1, t),
                    v2: lerp(vig_wide.v2, vig_stopped.v2, t),
                    v3: lerp(vig_wide.v3, vig_stopped.v3, t),
                }
            }
        };

        OpticalCorrectionModel {
            distortion: dist,
            lateral_ca: lat,
            vignette,
        }
    }

    fn interpolate_focal(
        &self,
        focal_mm: f32,
    ) -> (DistortionPoly, LateralTca, RadialVignette, RadialVignette) {
        let points = self.calibrated_points;
        if points.is_empty() {
            return (
                DistortionPoly::default(),
                LateralTca::default(),
                RadialVignette::default(),
                RadialVignette::default(),
            );
        }

        if points.len() == 1 || focal_mm <= points[0].focal_mm {
            let p = &points[0];
            return (
                p.distortion,
                p.lateral_ca,
                p.vignette_wide_open,
                p.vignette_stopped_down,
            );
        }

        let last_idx = points.len() - 1;
        if focal_mm >= points[last_idx].focal_mm {
            let p = &points[last_idx];
            return (
                p.distortion,
                p.lateral_ca,
                p.vignette_wide_open,
                p.vignette_stopped_down,
            );
        }

        // Find bracketing interval
        let mut idx = 0;
        while idx < last_idx && points[idx + 1].focal_mm < focal_mm {
            idx += 1;
        }

        let p0 = &points[idx];
        let p1 = &points[idx + 1];
        let span = (p1.focal_mm - p0.focal_mm).max(1e-4);
        let t = ((focal_mm - p0.focal_mm) / span).clamp(0.0, 1.0);

        let dist = DistortionPoly {
            k1: lerp(p0.distortion.k1, p1.distortion.k1, t),
            k2: lerp(p0.distortion.k2, p1.distortion.k2, t),
            k3: lerp(p0.distortion.k3, p1.distortion.k3, t),
        };

        let lat = LateralTca {
            scale_r: lerp(p0.lateral_ca.scale_r, p1.lateral_ca.scale_r, t),
            scale_b: lerp(p0.lateral_ca.scale_b, p1.lateral_ca.scale_b, t),
        };

        let vig_wide = RadialVignette {
            v1: lerp(p0.vignette_wide_open.v1, p1.vignette_wide_open.v1, t),
            v2: lerp(p0.vignette_wide_open.v2, p1.vignette_wide_open.v2, t),
            v3: lerp(p0.vignette_wide_open.v3, p1.vignette_wide_open.v3, t),
        };

        let vig_stopped = RadialVignette {
            v1: lerp(p0.vignette_stopped_down.v1, p1.vignette_stopped_down.v1, t),
            v2: lerp(p0.vignette_stopped_down.v2, p1.vignette_stopped_down.v2, t),
            v3: lerp(p0.vignette_stopped_down.v3, p1.vignette_stopped_down.v3, t),
        };

        (dist, lat, vig_wide, vig_stopped)
    }
}

/// Result of matching a photo's metadata against the lens database.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchedLens {
    /// Matched profile name or generic description.
    pub profile_name: String,
    /// Lens manufacturer name.
    pub maker: String,
    /// Effective optical correction model for this photo.
    pub model: OpticalCorrectionModel,
    /// True if an exact or alias match was found; false if a generic model was generated.
    pub is_exact_match: bool,
}

/// Catalog of well-known optical lenses with calibrated aberration profiles.
pub struct LensDatabase;

impl LensDatabase {
    /// Curated static catalog of prime and zoom lens profiles.
    pub const CATALOG: &'static [LensProfile] = &[
        // Sony FE 24-70mm F2.8 GM
        LensProfile {
            name: "Sony FE 24-70mm F2.8 GM",
            maker: "Sony",
            aliases: &[
                "FE 24-70mm F2.8 GM",
                "SEL2470GM",
                "24-70mm F2.8 GM",
                "24-70mm f/2.8",
            ],
            focal_range: (24.0, 70.0),
            aperture_range: (2.8, 22.0),
            calibrated_points: &[
                CalibratedFocalPoint {
                    focal_mm: 24.0,
                    distortion: DistortionPoly {
                        k1: -0.045,
                        k2: 0.008,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0008,
                        scale_b: 0.9992,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.65,
                        v2: 0.25,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.20,
                        v2: 0.05,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 35.0,
                    distortion: DistortionPoly {
                        k1: -0.015,
                        k2: 0.002,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0004,
                        scale_b: 0.9996,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.45,
                        v2: 0.15,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.15,
                        v2: 0.02,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 50.0,
                    distortion: DistortionPoly {
                        k1: 0.012,
                        k2: -0.001,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0002,
                        scale_b: 0.9998,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.40,
                        v2: 0.10,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.12,
                        v2: 0.01,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 70.0,
                    distortion: DistortionPoly {
                        k1: 0.032,
                        k2: -0.004,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0003,
                        scale_b: 0.9997,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.50,
                        v2: 0.18,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.15,
                        v2: 0.03,
                        v3: 0.0,
                    },
                },
            ],
        },
        // Sony FE 50mm F1.2 GM
        LensProfile {
            name: "Sony FE 50mm F1.2 GM",
            maker: "Sony",
            aliases: &["FE 50mm F1.2 GM", "SEL50F12GM", "50mm F1.2 GM"],
            focal_range: (50.0, 50.0),
            aperture_range: (1.2, 16.0),
            calibrated_points: &[CalibratedFocalPoint {
                focal_mm: 50.0,
                distortion: DistortionPoly {
                    k1: -0.008,
                    k2: 0.001,
                    k3: 0.0,
                },
                lateral_ca: LateralTca {
                    scale_r: 1.0002,
                    scale_b: 0.9998,
                },
                vignette_wide_open: RadialVignette {
                    v1: 0.75,
                    v2: 0.35,
                    v3: 0.0,
                },
                vignette_stopped_down: RadialVignette {
                    v1: 0.18,
                    v2: 0.04,
                    v3: 0.0,
                },
            }],
        },
        // Canon RF 24-70mm F2.8 L IS USM
        LensProfile {
            name: "Canon RF 24-70mm F2.8 L IS USM",
            maker: "Canon",
            aliases: &[
                "RF 24-70mm F2.8 L IS USM",
                "RF24-70mm F2.8 L",
                "RF 24-70mm F2.8L",
            ],
            focal_range: (24.0, 70.0),
            aperture_range: (2.8, 22.0),
            calibrated_points: &[
                CalibratedFocalPoint {
                    focal_mm: 24.0,
                    distortion: DistortionPoly {
                        k1: -0.042,
                        k2: 0.007,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0007,
                        scale_b: 0.9993,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.60,
                        v2: 0.22,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.18,
                        v2: 0.04,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 70.0,
                    distortion: DistortionPoly {
                        k1: 0.028,
                        k2: -0.003,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0003,
                        scale_b: 0.9997,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.48,
                        v2: 0.15,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.14,
                        v2: 0.02,
                        v3: 0.0,
                    },
                },
            ],
        },
        // Canon RF 50mm F1.2 L USM
        LensProfile {
            name: "Canon RF 50mm F1.2 L USM",
            maker: "Canon",
            aliases: &["RF 50mm F1.2 L USM", "RF 50mm F1.2L", "RF50mm F1.2"],
            focal_range: (50.0, 50.0),
            aperture_range: (1.2, 16.0),
            calibrated_points: &[CalibratedFocalPoint {
                focal_mm: 50.0,
                distortion: DistortionPoly {
                    k1: -0.006,
                    k2: 0.001,
                    k3: 0.0,
                },
                lateral_ca: LateralTca {
                    scale_r: 1.0003,
                    scale_b: 0.9997,
                },
                vignette_wide_open: RadialVignette {
                    v1: 0.72,
                    v2: 0.30,
                    v3: 0.0,
                },
                vignette_stopped_down: RadialVignette {
                    v1: 0.16,
                    v2: 0.03,
                    v3: 0.0,
                },
            }],
        },
        // Nikon NIKKOR Z 24-70mm f/2.8 S
        LensProfile {
            name: "Nikon NIKKOR Z 24-70mm f/2.8 S",
            maker: "Nikon",
            aliases: &[
                "NIKKOR Z 24-70mm f/2.8 S",
                "Z 24-70mm f/2.8 S",
                "NIKKOR Z 24-70/2.8 S",
            ],
            focal_range: (24.0, 70.0),
            aperture_range: (2.8, 22.0),
            calibrated_points: &[
                CalibratedFocalPoint {
                    focal_mm: 24.0,
                    distortion: DistortionPoly {
                        k1: -0.038,
                        k2: 0.006,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0006,
                        scale_b: 0.9994,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.58,
                        v2: 0.20,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.16,
                        v2: 0.03,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 70.0,
                    distortion: DistortionPoly {
                        k1: 0.025,
                        k2: -0.003,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0002,
                        scale_b: 0.9998,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.45,
                        v2: 0.12,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.12,
                        v2: 0.02,
                        v3: 0.0,
                    },
                },
            ],
        },
        // Fujifilm XF 16-55mm F2.8 R LM WR
        LensProfile {
            name: "Fujifilm XF 16-55mm F2.8 R LM WR",
            maker: "Fujifilm",
            aliases: &[
                "XF 16-55mm F2.8 R LM WR",
                "XF16-55mmF2.8",
                "XF 16-55mm F2.8",
            ],
            focal_range: (16.0, 55.0),
            aperture_range: (2.8, 22.0),
            calibrated_points: &[
                CalibratedFocalPoint {
                    focal_mm: 16.0,
                    distortion: DistortionPoly {
                        k1: -0.052,
                        k2: 0.010,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0009,
                        scale_b: 0.9991,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.62,
                        v2: 0.24,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.18,
                        v2: 0.04,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 55.0,
                    distortion: DistortionPoly {
                        k1: 0.030,
                        k2: -0.003,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0004,
                        scale_b: 0.9996,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.46,
                        v2: 0.14,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.14,
                        v2: 0.02,
                        v3: 0.0,
                    },
                },
            ],
        },
        // Sigma 24-70mm F2.8 DG DN Art
        LensProfile {
            name: "Sigma 24-70mm F2.8 DG DN Art",
            maker: "Sigma",
            aliases: &[
                "24-70mm F2.8 DG DN | Art",
                "Sigma 24-70mm Art",
                "24-70mm F2.8 DG DN",
            ],
            focal_range: (24.0, 70.0),
            aperture_range: (2.8, 22.0),
            calibrated_points: &[
                CalibratedFocalPoint {
                    focal_mm: 24.0,
                    distortion: DistortionPoly {
                        k1: -0.048,
                        k2: 0.009,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0008,
                        scale_b: 0.9992,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.68,
                        v2: 0.26,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.22,
                        v2: 0.05,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 70.0,
                    distortion: DistortionPoly {
                        k1: 0.034,
                        k2: -0.004,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0004,
                        scale_b: 0.9996,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.52,
                        v2: 0.16,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.16,
                        v2: 0.03,
                        v3: 0.0,
                    },
                },
            ],
        },
        // Tamron 28-75mm F/2.8 Di III VXD G2
        LensProfile {
            name: "Tamron 28-75mm F/2.8 Di III VXD G2",
            maker: "Tamron",
            aliases: &["28-75mm F/2.8 Di III VXD G2", "Tamron 28-75mm", "A063"],
            focal_range: (28.0, 75.0),
            aperture_range: (2.8, 22.0),
            calibrated_points: &[
                CalibratedFocalPoint {
                    focal_mm: 28.0,
                    distortion: DistortionPoly {
                        k1: -0.036,
                        k2: 0.005,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0006,
                        scale_b: 0.9994,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.55,
                        v2: 0.18,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.16,
                        v2: 0.03,
                        v3: 0.0,
                    },
                },
                CalibratedFocalPoint {
                    focal_mm: 75.0,
                    distortion: DistortionPoly {
                        k1: 0.031,
                        k2: -0.003,
                        k3: 0.0,
                    },
                    lateral_ca: LateralTca {
                        scale_r: 1.0003,
                        scale_b: 0.9997,
                    },
                    vignette_wide_open: RadialVignette {
                        v1: 0.48,
                        v2: 0.14,
                        v3: 0.0,
                    },
                    vignette_stopped_down: RadialVignette {
                        v1: 0.15,
                        v2: 0.02,
                        v3: 0.0,
                    },
                },
            ],
        },
    ];

    /// Finds a matching lens profile from a query string.
    ///
    /// The search is case-insensitive, ignores extra spaces, and checks
    /// exact names, aliases, and partial token matches.
    #[must_use]
    pub fn find_lens(query: &str) -> Option<&'static LensProfile> {
        let norm_query = normalize_string(query);
        if norm_query.is_empty() {
            return None;
        }

        // 1. Exact canonical name or alias match
        for profile in Self::CATALOG {
            if normalize_string(profile.name) == norm_query {
                return Some(profile);
            }
            for alias in profile.aliases {
                if normalize_string(alias) == norm_query {
                    return Some(profile);
                }
            }
        }

        // 2. Substring or token containment match
        for profile in Self::CATALOG {
            let norm_name = normalize_string(profile.name);
            if norm_query.contains(&norm_name) || norm_name.contains(&norm_query) {
                return Some(profile);
            }
            for alias in profile.aliases {
                let norm_alias = normalize_string(alias);
                if norm_query.contains(&norm_alias) || norm_alias.contains(&norm_query) {
                    return Some(profile);
                }
            }
        }

        None
    }

    /// Matches photo metadata against the lens database, returning a resolved
    /// [`MatchedLens`] with either a profile model or a generic fallback.
    #[must_use]
    pub fn match_metadata(metadata: &PhotoMetadata) -> MatchedLens {
        let focal = metadata.focal_length_mm.unwrap_or(50.0);
        let aperture = metadata.aperture_f_number;

        if let Some(lens_str) = &metadata.lens_model {
            if let Some(profile) = Self::find_lens(lens_str) {
                let model = profile.interpolate_model(focal, aperture);
                return MatchedLens {
                    profile_name: profile.name.to_owned(),
                    maker: profile.maker.to_owned(),
                    model,
                    is_exact_match: true,
                };
            }
        }

        // Generic fallback model based on focal length and aperture
        let model = Self::generic_fallback(focal, aperture);
        let maker = if metadata.camera_make.is_empty() {
            "Generic".to_owned()
        } else {
            metadata.camera_make.clone()
        };
        let profile_name = match &metadata.lens_model {
            Some(name) => format!("{name} (Générique)"),
            None => format!("{maker} {focal:.0}mm (Générique)"),
        };

        MatchedLens {
            profile_name,
            maker,
            model,
            is_exact_match: false,
        }
    }

    /// Computes a continuous generic optical aberration model based on focal length.
    ///
    /// * Wide angles (< 35mm): barrel distortion (k1 < 0) and moderate vignetting.
    /// * Standard (35mm - 60mm): slight distortion, minimal TCA.
    /// * Telephoto (> 60mm): slight pincushion distortion (k1 > 0).
    #[must_use]
    pub fn generic_fallback(focal_mm: f32, aperture: Option<f32>) -> OpticalCorrectionModel {
        let clamped_f = focal_mm.clamp(10.0, 600.0);

        let k1 = if clamped_f < 35.0 {
            // Wide angle: barrel distortion from -0.05 at 14mm to -0.01 at 35mm
            -0.01 - (35.0 - clamped_f) * (0.04 / 21.0)
        } else if clamped_f <= 60.0 {
            // Standard: -0.01 to +0.01
            (clamped_f - 45.0) * (0.02 / 15.0)
        } else {
            // Telephoto: pincushion up to +0.025 at 200mm+
            0.01 + ((clamped_f - 60.0) / 140.0).clamp(0.0, 1.0) * 0.015
        };

        let distortion = DistortionPoly {
            k1: k1.clamp(-0.08, 0.06),
            k2: 0.0,
            k3: 0.0,
        };

        let lateral_ca = if clamped_f < 28.0 {
            LateralTca {
                scale_r: 1.0006,
                scale_b: 0.9994,
            }
        } else {
            LateralTca {
                scale_r: 1.0002,
                scale_b: 0.9998,
            }
        };

        let wide_open_v1 = if clamped_f < 28.0 { 0.55 } else { 0.35 };
        let ap = aperture.unwrap_or(2.8);
        let t = ((ap - 2.8) / 3.0).clamp(0.0, 1.0);
        let v1 = lerp(wide_open_v1, 0.12, t);

        let vignette = RadialVignette {
            v1,
            v2: v1 * 0.3,
            v3: 0.0,
        };

        OpticalCorrectionModel {
            distortion,
            lateral_ca,
            vignette,
        }
    }
}

fn normalize_string(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}
