#![forbid(unsafe_code)]

//! 4-tier optical aberration correction resolver and Stage 3/4 reference kernels.

use rraw_core::PhotoMetadata;

/// Resolution tier used to correct distortion, chromatic aberration, and vignetting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpticalResolutionTier {
    Tier1EmbeddedRawOpcodes,
    Tier2LensfunXml,
    Tier3LocalAdobeLcp,
    Tier4ManualSliders,
}

/// Fused Stage 4 optical warp plan derived from metadata and user overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpticalStagePlan {
    pub active_tier: OpticalResolutionTier,
    /// Stage 4 radial vignetting is enabled ONLY if Stage 1 DNG `GainMap` did not handle it.
    pub apply_radial_vignette_in_warp: bool,
    /// Per-channel UV lookup table dimension (`256x256`).
    pub lut_dimension: usize,
}

impl OpticalStagePlan {
    /// Fixed per-channel UV warp LUT grid resolution (`256x256`).
    pub const DEFAULT_LUT_DIMENSION: usize = 256;

    /// Plans Stage 4 optical execution, forbidding double vignetting when `GainMap` was applied.
    #[must_use]
    pub const fn plan(metadata: &PhotoMetadata, preferred_tier: OpticalResolutionTier) -> Self {
        Self {
            active_tier: preferred_tier,
            apply_radial_vignette_in_warp: !metadata.vignette_handled_by_gainmap,
            lut_dimension: Self::DEFAULT_LUT_DIMENSION,
        }
    }
}

/// Resolves an initial optical tier based on RAW metadata capabilities.
#[must_use]
pub fn detect_initial_tier(metadata: &PhotoMetadata) -> OpticalResolutionTier {
    if metadata.vignette_handled_by_gainmap {
        OpticalResolutionTier::Tier1EmbeddedRawOpcodes
    } else if metadata.lens_model.is_some() {
        OpticalResolutionTier::Tier2LensfunXml
    } else {
        OpticalResolutionTier::Tier4ManualSliders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_core::{
        BayerOrigin, CfaPattern, ColorLabel, CoreError, NoiseProfile, NoiseProfileSource,
        PickStatus,
    };

    fn test_metadata(vignette_handled_by_gainmap: bool) -> Result<PhotoMetadata, CoreError> {
        Ok(PhotoMetadata {
            camera_make: "Test".to_owned(),
            camera_model: "Body".to_owned(),
            lens_model: Some("35mm f/1.4".to_owned()),
            focal_length_mm: Some(35.0),
            aperture_f_number: Some(1.4),
            iso: Some(100),
            width: 100,
            height: 100,
            black_levels: [0.0; 4],
            white_levels: [1.0; 4],
            cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
            noise_profile: NoiseProfile::new(
                [0.0; 3],
                [0.0001; 3],
                NoiseProfileSource::WaveletMadFallback,
            )?,
            vignette_handled_by_gainmap,
            rating: 0,
            pick: PickStatus::Unflagged,
            label: ColorLabel::None,
            keywords: Vec::new(),
        })
    }

    #[test]
    fn stage4_skips_radial_vignette_when_handled_by_gainmap() -> Result<(), CoreError> {
        let with_gainmap = test_metadata(true)?;
        let plan_with = OpticalStagePlan::plan(&with_gainmap, detect_initial_tier(&with_gainmap));
        assert!(!plan_with.apply_radial_vignette_in_warp);
        assert_eq!(plan_with.lut_dimension, 256);

        let without_gainmap = test_metadata(false)?;
        let plan_without =
            OpticalStagePlan::plan(&without_gainmap, detect_initial_tier(&without_gainmap));
        assert!(plan_without.apply_radial_vignette_in_warp);
        assert_eq!(
            plan_without.active_tier,
            OpticalResolutionTier::Tier2LensfunXml
        );
        Ok(())
    }

    #[test]
    fn detect_initial_tier_falls_back_to_manual_without_lens_or_gainmap() -> Result<(), CoreError> {
        let mut meta = test_metadata(false)?;
        meta.lens_model = None;
        assert_eq!(
            detect_initial_tier(&meta),
            OpticalResolutionTier::Tier4ManualSliders
        );
        Ok(())
    }
}
