//! Versioned `.rraw.json` recipe DAG, canonical serialization, and stage-scoped cache hashing.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CoreError, WhiteBalanceMultipliers};

/// Current supported `.rraw.json` schema version.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Unique identifier for a recipe or virtual copy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecipeId(pub String);

impl Default for RecipeId {
    fn default() -> Self {
        Self("default".to_string())
    }
}

impl fmt::Display for RecipeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Denoising mode for raw CFA sensor data (Stage 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CfaDenoiseMode {
    /// GPU Generalized Anscombe Transform (GAT) À-Trous wavelets (real-time 120 Hz).
    InteractiveWavelet {
        /// Wavelet detail threshold multiplier.
        threshold: f32,
    },
    /// Asynchronous studio-grade Apple Neural Engine / CoreML denoiser (504x504 tiles).
    NeuralCoreMl,
    /// Denoising bypass.
    Disabled,
}

impl Default for CfaDenoiseMode {
    fn default() -> Self {
        Self::InteractiveWavelet { threshold: 1.0 }
    }
}

/// Stage 1 parameter set: Sensor pre-processing, black/white points, defect repair, CFA denoising.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage1SensorParams {
    /// Additive per-channel black level offset (R, Gr, Gb, B).
    pub black_level_offset: [f32; 4],
    /// Optional manual white point override.
    pub white_level_override: Option<f32>,
    /// Statistical standard deviation threshold for hot/dead pixel detection.
    pub hot_pixel_threshold: f32,
    /// Denoising mode.
    pub cfa_denoise_mode: CfaDenoiseMode,
}

impl Default for Stage1SensorParams {
    fn default() -> Self {
        Self {
            black_level_offset: [0.0; 4],
            white_level_override: None,
            hot_pixel_threshold: 4.0,
            cfa_denoise_mode: CfaDenoiseMode::default(),
        }
    }
}

/// Supported demosaicing algorithms (Stage 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemosaicAlgorithm {
    /// Residual Color Difference (high-frequency Bayer demosaicing).
    #[default]
    Rcd,
    /// Markesteijn 1-pass / 3-pass (Fujifilm 6x6 X-Trans demosaicing).
    Markesteijn,
    /// Passthrough for monochrome or diagnostic visualization.
    Passthrough,
}

/// Highlight reconstruction strategy (Stage 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightReconstructionMode {
    /// Reconstructs clipped highlights using chromatic ratio propagation.
    #[default]
    ColorRatio,
    /// Luminance blend from remaining unsaturated channels.
    Blend,
    /// Clips saturated channels strictly at physical saturation `sat_c`.
    Disabled,
}

/// Stage 2 parameter set: White balance multipliers, demosaicing, highlight reconstruction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage2DemosaicParams {
    /// Positive white balance multipliers (w_R, w_G, w_B).
    pub wb_multipliers: WhiteBalanceMultipliers,
    /// Demosaicing algorithm.
    pub demosaic_algorithm: DemosaicAlgorithm,
    /// Highlight reconstruction mode.
    pub highlight_reconstruction: HighlightReconstructionMode,
}

impl Default for Stage2DemosaicParams {
    fn default() -> Self {
        Self {
            wb_multipliers: WhiteBalanceMultipliers::UNITY,
            demosaic_algorithm: DemosaicAlgorithm::default(),
            highlight_reconstruction: HighlightReconstructionMode::default(),
        }
    }
}

/// Stage 3 parameter set: Regularized Richardson-Lucy capture sharpening.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3SharpenParams {
    /// Number of deconvolution iterations (typically 5 to 10).
    pub iterations: u32,
    /// Deconvolution blend amount in `[0.0, 1.0]`.
    pub amount: f32,
    /// Optical Point Spread Function (PSF) standard deviation at sensor center (pixels).
    pub psf_radius_center: f32,
    /// Optical Point Spread Function (PSF) standard deviation at sensor corner (pixels).
    pub psf_radius_corner: f32,
    /// Local min/max overshoot deringing protection in `[0.0, 1.0]`.
    pub dering_amount: f32,
    /// Whether capture sharpening is active.
    pub enabled: bool,
}

impl Default for Stage3SharpenParams {
    fn default() -> Self {
        Self {
            iterations: 5,
            amount: 0.5,
            psf_radius_center: 0.8,
            psf_radius_corner: 1.2,
            dering_amount: 0.2,
            enabled: true,
        }
    }
}

/// Stage 4 parameter set: Optical geometry, chromatic aberration, and vignetting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage4OpticsParams {
    /// Radial distortion 3rd-order polynomial coefficient k1.
    pub distortion_k1: f32,
    /// Radial distortion 5th-order polynomial coefficient k2.
    pub distortion_k2: f32,
    /// Lateral chromatic aberration red scaling factor.
    pub lateral_ca_red: f32,
    /// Lateral chromatic aberration blue scaling factor.
    pub lateral_ca_blue: f32,
    /// Radial vignetting compensation amount in `[0.0, 1.0]`.
    pub vignette_amount: f32,
    /// Radial vignetting midpoint normalized radius.
    pub vignette_radius: f32,
    /// Selected lens profile identifier (e.g. Lensfun or Adobe LCP).
    pub lens_profile_name: Option<String>,
}

impl Default for Stage4OpticsParams {
    fn default() -> Self {
        Self {
            distortion_k1: 0.0,
            distortion_k2: 0.0,
            lateral_ca_red: 0.0,
            lateral_ca_blue: 0.0,
            vignette_amount: 0.0,
            vignette_radius: 1.0,
            lens_profile_name: None,
        }
    }
}

/// Stage 5 parameter set: Scene colorimetry, exposure, and tone grading (ACEScg AP1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5GradingParams {
    /// Scene exposure adjustment in Exposure Values (EV / stops).
    pub exposure_ev: f32,
    /// Global contrast adjustment.
    pub contrast: f32,
    /// Highlight recovery adjustment in `[-1.0, 1.0]`.
    pub highlights: f32,
    /// Shadow lift adjustment in `[-1.0, 1.0]`.
    pub shadows: f32,
    /// White clip adjustment.
    pub whites: f32,
    /// Black floor adjustment.
    pub blacks: f32,
    /// Color saturation.
    pub saturation: f32,
    /// Color vibrance (selective saturation).
    pub vibrance: f32,
    /// Laplacian pyramid multi-scale local contrast amount.
    pub laplacian_local_contrast: f32,
}

impl Default for Stage5GradingParams {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            laplacian_local_contrast: 0.0,
        }
    }
}

/// Range specification for ACEScg parametric color qualifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricRange {
    pub min_val: f32,
    pub max_val: f32,
    pub feather: f32,
}

/// Geometric mask specification defined in `[0.0, 1.0]²` post-warp coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MaskKind {
    /// Linear directional gradient in normalized `[0, 1]²` coordinates.
    LinearGradient {
        /// Starting point `[x, y]` in `[0, 1]²`.
        start: [f32; 2],
        /// Ending point `[x, y]` in `[0, 1]²`.
        end: [f32; 2],
    },
    /// Radial elliptical gradient in normalized `[0, 1]²` coordinates.
    RadialGradient {
        /// Center position `[x, y]` in `[0, 1]²`.
        center: [f32; 2],
        /// Semi-axes radii `[rx, ry]`.
        radius: [f32; 2],
        /// Rotation angle in degrees.
        rotation_deg: f32,
    },
    /// ACEScg parametric color qualifier (Luminance, Chroma, Hue).
    ParametricColor {
        /// Luminance range in ACEScg space ($Y \in [0, \infty)$).
        luminance: Option<ParametricRange>,
        /// Chroma range in ACEScg space ($C \in [0, \infty)$).
        chroma: Option<ParametricRange>,
        /// Hue angle range in degrees ($H \in [0, 360)$).
        hue_deg: Option<ParametricRange>,
    },
}

/// Local adjustment mask record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometricMask {
    /// Unique identifier for this mask.
    pub id: String,
    /// Geometric shape in `[0, 1]²` post-warp frame.
    pub kind: MaskKind,
    /// Exposure adjustment applied inside this mask in EV.
    pub exposure_offset_ev: f32,
    /// Mask opacity in `[0.0, 1.0]`.
    pub opacity: f32,
    /// Whether this mask is currently active.
    pub enabled: bool,
}

/// Stage 6 parameter set: Post-warp geometric and ACEScg parametric masks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stage6MasksParams {
    /// Ordered list of local adjustment masks.
    pub masks: Vec<GeometricMask>,
}

/// Tone mapping transform algorithm (Stage 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToneMapperKind {
    /// AgX robust color-preserving tone mapper with EDR shoulder extension.
    #[default]
    AgX,
    /// OpenDRT spectral-preserving display rendering transform.
    OpenDrt,
}

/// Target display color space for Stage 7 output rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputColorSpace {
    /// Apple Display P3 wide color gamut.
    #[default]
    DisplayP3,
    /// Standard sRGB / Rec.709 color gamut.
    Srgb,
    /// ITU-R BT.2020 wide color gamut.
    Rec2020,
}

/// Stage 7 parameter set: Display Rendering Transform (DRT) and output encoding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage7DisplayParams {
    /// Display rendering transform algorithm.
    pub tone_mapper: ToneMapperKind,
    /// Creative look or curve name.
    pub look: String,
    /// Target output color space.
    pub output_color_space: OutputColorSpace,
    /// Spatial dithering flag to eliminate 8-bit / 10-bit display banding.
    pub dither_enabled: bool,
}

impl Default for Stage7DisplayParams {
    fn default() -> Self {
        Self {
            tone_mapper: ToneMapperKind::default(),
            look: "default".to_string(),
            output_color_space: OutputColorSpace::default(),
            dither_enabled: true,
        }
    }
}

/// 256-bit cryptographically secure deterministic hash for pipeline cache stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StageHash(pub [u8; 32]);

impl StageHash {
    /// Empty / zero hash sentinel value.
    pub const ZERO: Self = Self([0u8; 32]);

    /// Formats the 32-byte hash as a 64-character lowercase hexadecimal string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{byte:02x}");
        }
        s
    }

    /// Parses a 64-character hexadecimal string into a `StageHash`.
    pub fn from_hex(hex: &str) -> Result<Self, CoreError> {
        if hex.len() != 64 {
            return Err(CoreError::InvalidStageHashHex(format!(
                "expected 64 hex characters, got {}",
                hex.len()
            )));
        }

        let mut bytes = [0u8; 32];
        let (chunks, _) = hex.as_bytes().as_chunks::<2>();
        for (i, chunk) in chunks.iter().enumerate() {
            let s = std::str::from_utf8(chunk)
                .map_err(|e| CoreError::InvalidStageHashHex(format!("utf-8 decode error: {e}")))?;
            bytes[i] = u8::from_str_radix(s, 16).map_err(|e| {
                CoreError::InvalidStageHashHex(format!("hex parse error for `{s}`: {e}"))
            })?;
        }

        Ok(Self(bytes))
    }
}

impl fmt::Display for StageHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Directed Acyclic Graph (DAG) recipe defining complete non-destructive photo development.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Schema format version for backwards and forwards compatibility.
    pub schema_version: u32,
    /// Unique identifier of this recipe instance or virtual copy.
    pub id: RecipeId,
    /// Stage 1: Sensor calibration, defect repair, CFA denoising.
    pub stage1_sensor: Stage1SensorParams,
    /// Stage 2: White balance, demosaic, highlight reconstruction.
    pub stage2_demosaic: Stage2DemosaicParams,
    /// Stage 3: Richardson-Lucy capture sharpening.
    pub stage3_sharpen: Stage3SharpenParams,
    /// Stage 4: Optical geometry, CA, vignetting.
    pub stage4_optics: Stage4OpticsParams,
    /// Stage 5: ACEScg scene colorimetry and tonal mastering.
    pub stage5_grading: Stage5GradingParams,
    /// Stage 6: Post-warp geometric and parametric local masks.
    pub stage6_masks: Stage6MasksParams,
    /// Stage 7: Display Rendering Transform and display encoding.
    pub stage7_display: Stage7DisplayParams,
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: RecipeId::default(),
            stage1_sensor: Stage1SensorParams::default(),
            stage2_demosaic: Stage2DemosaicParams::default(),
            stage3_sharpen: Stage3SharpenParams::default(),
            stage4_optics: Stage4OpticsParams::default(),
            stage5_grading: Stage5GradingParams::default(),
            stage6_masks: Stage6MasksParams::default(),
            stage7_display: Stage7DisplayParams::default(),
        }
    }
}

impl Recipe {
    /// Computes deterministic cache hash for `Cache L0` (Stage 1 signed CFA buffer).
    ///
    /// Depends strictly on: `source_id` + `stage1_sensor`.
    #[must_use]
    pub fn hash_l0(&self, source_id: &str) -> StageHash {
        let mut hasher = Sha256::new();
        hasher.update(b"RRAW_CACHE_L0_V1");
        hasher.update((source_id.len() as u64).to_le_bytes());
        hasher.update(source_id.as_bytes());

        // Serialize stage1_sensor to canonical json bytes
        if let Ok(bytes) = serde_json::to_vec(&self.stage1_sensor) {
            hasher.update(&bytes);
        }

        StageHash(hasher.finalize().into())
    }

    /// Computes deterministic cache hash for `Cache L1` (Stage 3 linear camera RGB + mipmaps).
    ///
    /// Depends strictly on: `hash_l0` + `stage2_demosaic` + `stage3_sharpen`.
    #[must_use]
    pub fn hash_l1(&self, source_id: &str) -> StageHash {
        let l0 = self.hash_l0(source_id);
        let mut hasher = Sha256::new();
        hasher.update(b"RRAW_CACHE_L1_V1");
        hasher.update(l0.0);

        if let Ok(bytes) = serde_json::to_vec(&self.stage2_demosaic) {
            hasher.update(&bytes);
        }
        if let Ok(bytes) = serde_json::to_vec(&self.stage3_sharpen) {
            hasher.update(&bytes);
        }

        StageHash(hasher.finalize().into())
    }

    /// Computes deterministic cache hash for `Cache L2` (Stage 4 optics warp UV LUT).
    ///
    /// Depends strictly on: `stage4_optics`.
    #[must_use]
    pub fn hash_l2(&self) -> StageHash {
        let mut hasher = Sha256::new();
        hasher.update(b"RRAW_CACHE_L2_V1");

        if let Ok(bytes) = serde_json::to_vec(&self.stage4_optics) {
            hasher.update(&bytes);
        }

        StageHash(hasher.finalize().into())
    }

    /// Computes deterministic cache hash for the display output / viewport texture.
    ///
    /// Depends on: `hash_l1` + `hash_l2` + `stage5_grading` + `stage6_masks` + `stage7_display` + `display_headroom`.
    #[must_use]
    pub fn hash_display(&self, source_id: &str, display_headroom: f32) -> StageHash {
        let l1 = self.hash_l1(source_id);
        let l2 = self.hash_l2();

        let mut hasher = Sha256::new();
        hasher.update(b"RRAW_CACHE_DISPLAY_V1");
        hasher.update(l1.0);
        hasher.update(l2.0);

        if let Ok(bytes) = serde_json::to_vec(&self.stage5_grading) {
            hasher.update(&bytes);
        }
        if let Ok(bytes) = serde_json::to_vec(&self.stage6_masks) {
            hasher.update(&bytes);
        }
        if let Ok(bytes) = serde_json::to_vec(&self.stage7_display) {
            hasher.update(&bytes);
        }

        hasher.update(display_headroom.to_bits().to_le_bytes());

        StageHash(hasher.finalize().into())
    }

    /// Serializes the recipe into canonical formatted JSON.
    pub fn to_canonical_json(&self) -> Result<String, CoreError> {
        serde_json::to_string_pretty(self).map_err(|e| CoreError::SerializationError(e.to_string()))
    }

    /// Deserializes and validates a recipe from JSON.
    pub fn from_canonical_json(json: &str) -> Result<Self, CoreError> {
        let recipe: Self = serde_json::from_str(json)
            .map_err(|e| CoreError::DeserializationError(e.to_string()))?;

        if recipe.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(CoreError::UnsupportedSchemaVersion {
                expected: CURRENT_SCHEMA_VERSION,
                actual: recipe.schema_version,
            });
        }

        Ok(recipe)
    }
}
