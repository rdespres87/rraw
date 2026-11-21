use crate::db::LensDatabase;
use crate::model::{DistortionPoly, LateralTca, OpticalCorrectionModel, RadialVignette};
use crate::OpticalResolutionTier;
use rraw_core::PhotoMetadata;

/// 4-tier optical profile resolution engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct OpticalResolver;

impl OpticalResolver {
    /// Selects the appropriate optical resolution tier based on metadata availability.
    #[must_use]
    pub fn resolve_tier(&self, metadata: &PhotoMetadata) -> OpticalResolutionTier {
        if metadata.vignette_handled_by_gainmap {
            OpticalResolutionTier::Tier1EmbeddedRawOpcodes
        } else if metadata.lens_model.is_some() {
            OpticalResolutionTier::Tier2LensfunXml
        } else {
            OpticalResolutionTier::Tier4ManualSliders
        }
    }

    /// Resolves an optical correction model for the selected tier and photo metadata.
    #[must_use]
    pub fn resolve_model(
        &self,
        metadata: &PhotoMetadata,
        tier: OpticalResolutionTier,
    ) -> OpticalCorrectionModel {
        match tier {
            OpticalResolutionTier::Tier1EmbeddedRawOpcodes => OpticalCorrectionModel {
                distortion: DistortionPoly::default(),
                lateral_ca: LateralTca::default(),
                vignette: RadialVignette::default(),
            },
            OpticalResolutionTier::Tier2LensfunXml => LensDatabase::match_metadata(metadata).model,
            OpticalResolutionTier::Tier3LocalAdobeLcp => OpticalCorrectionModel {
                distortion: DistortionPoly::default(),
                lateral_ca: LateralTca::default(),
                vignette: RadialVignette::default(),
            },
            OpticalResolutionTier::Tier4ManualSliders => OpticalCorrectionModel::default(),
        }
    }

    /// Resolves an optical correction model scaled by user strength percentages (0% - 200%).
    #[must_use]
    pub fn resolve_with_strength(
        &self,
        metadata: &PhotoMetadata,
        tier: OpticalResolutionTier,
        distortion_strength: f32,
        vignette_strength: f32,
        ca_strength: f32,
    ) -> OpticalCorrectionModel {
        let base = self.resolve_model(metadata, tier);
        let dist_scale = (distortion_strength / 100.0).max(0.0);
        let vig_scale = (vignette_strength / 100.0).max(0.0);
        let ca_scale = (ca_strength / 100.0).max(0.0);

        OpticalCorrectionModel {
            distortion: DistortionPoly {
                k1: base.distortion.k1 * dist_scale,
                k2: base.distortion.k2 * dist_scale,
                k3: base.distortion.k3 * dist_scale,
            },
            lateral_ca: LateralTca {
                scale_r: 1.0 + (base.lateral_ca.scale_r - 1.0) * ca_scale,
                scale_b: 1.0 + (base.lateral_ca.scale_b - 1.0) * ca_scale,
            },
            vignette: RadialVignette {
                v1: base.vignette.v1 * vig_scale,
                v2: base.vignette.v2 * vig_scale,
                v3: base.vignette.v3 * vig_scale,
            },
        }
    }
}
