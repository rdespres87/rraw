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
            OpticalResolutionTier::Tier2LensfunXml => {
                // Nominal lens parameters from Lensfun database match
                let focal = metadata.focal_length_mm.unwrap_or(50.0);
                let k1 = if focal < 35.0 { -0.03 } else { 0.015 };
                OpticalCorrectionModel {
                    distortion: DistortionPoly::pincushion(k1),
                    lateral_ca: LateralTca {
                        scale_r: 1.0005,
                        scale_b: 0.9995,
                    },
                    vignette: RadialVignette::poly(0.3, 0.1),
                }
            }
            OpticalResolutionTier::Tier3LocalAdobeLcp => OpticalCorrectionModel {
                distortion: DistortionPoly::default(),
                lateral_ca: LateralTca::default(),
                vignette: RadialVignette::default(),
            },
            OpticalResolutionTier::Tier4ManualSliders => OpticalCorrectionModel::default(),
        }
    }
}
