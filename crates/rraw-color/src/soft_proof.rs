//! Color Management & Soft Proofing:
//! ICC Display & Printer Profile Simulation, Perceptual / Relative Colorimetric Intents,
//! Paper White / Black Ink (BPC) reflectance mapping, and Out-of-Gamut detection.

use glam::{Mat3, Vec3};
use serde::{Deserialize, Serialize};

/// Target display or printer color profile for Soft Proofing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SoftProofProfile {
    /// Fine Art Matte Cotton Paper (Archival pigment inkjet on matte rag paper, warm white, lifted Dmax).
    #[default]
    FineArtMattePaper,
    /// Glossy / Baryta Photo Paper (High Dmax pigment/chromogenic print on baryta fiber paper).
    GlossyBarytaPaper,
    /// Standard sRGB (IEC 61966-2-1, D65, narrow web/SDR gamut).
    SrgbIec61966,
    /// Apple Display P3 (DCI-P3 primaries, D65 white point).
    DisplayP3,
    /// Adobe RGB (1998) wide-gamut print preparation space.
    AdobeRgb1998,
}

impl SoftProofProfile {
    /// Ordered list of all available soft proofing profiles.
    pub const ALL: [Self; 5] = [
        Self::FineArtMattePaper,
        Self::GlossyBarytaPaper,
        Self::SrgbIec61966,
        Self::DisplayP3,
        Self::AdobeRgb1998,
    ];

    /// Human-readable French UI label for the profile.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SrgbIec61966 => "sRGB IEC61966-2.1 (Web / SDR)",
            Self::DisplayP3 => "Apple Display P3 (Écran large gamut)",
            Self::AdobeRgb1998 => "Adobe RGB (1998) (Pré-presse)",
            Self::FineArtMattePaper => "Papier Fine Art Mat (Coton / Hahnemühle)",
            Self::GlossyBarytaPaper => "Papier Photo Baryté / Brillant (Glossy)",
        }
    }

    /// Short compact badge label for toolbar / status display.
    #[must_use]
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::SrgbIec61966 => "sRGB",
            Self::DisplayP3 => "Display P3",
            Self::AdobeRgb1998 => "Adobe RGB",
            Self::FineArtMattePaper => "Fine Art Mat",
            Self::GlossyBarytaPaper => "Photo Baryté",
        }
    }

    /// Canonical identifier string for `.rraw.json` sidecar serialization.
    #[must_use]
    pub const fn id_str(self) -> &'static str {
        match self {
            Self::SrgbIec61966 => "srgb_iec61966",
            Self::DisplayP3 => "display_p3",
            Self::AdobeRgb1998 => "adobe_rgb_1998",
            Self::FineArtMattePaper => "fine_art_matte",
            Self::GlossyBarytaPaper => "glossy_baryta",
        }
    }

    /// Parses a `SoftProofProfile` from its canonical ID string.
    #[must_use]
    pub fn from_id_str(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id_str() == id)
    }

    /// Returns true if this profile represents a physical reflective print medium.
    #[must_use]
    pub const fn is_print_medium(self) -> bool {
        matches!(self, Self::FineArtMattePaper | Self::GlossyBarytaPaper)
    }
}

/// ICC rendering intent used when mapping colors into the target proof profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SoftProofRenderingIntent {
    /// Smoothly compresses out-of-gamut saturation while preserving tonal relationships.
    #[default]
    Perceptual,
    /// Preserves in-gamut coordinates exactly and clips out-of-gamut colors to the gamut boundary.
    RelativeColorimetric,
}

impl SoftProofRenderingIntent {
    /// All supported rendering intents.
    pub const ALL: [Self; 2] = [Self::Perceptual, Self::RelativeColorimetric];

    /// Human-readable French UI label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Perceptual => "Perceptuel (Compression douce)",
            Self::RelativeColorimetric => "Colorimétrique Relatif (Exact / Écrêtage)",
        }
    }

    /// Canonical identifier string for `.rraw.json` sidecar serialization.
    #[must_use]
    pub const fn id_str(self) -> &'static str {
        match self {
            Self::Perceptual => "perceptual",
            Self::RelativeColorimetric => "relative_colorimetric",
        }
    }

    /// Parses a `SoftProofRenderingIntent` from its canonical ID string.
    #[must_use]
    pub fn from_id_str(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.id_str() == id)
    }
}

/// Configuration state for Soft Proofing and Out-of-Gamut inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftProofConfig {
    /// Master switch enabling soft proofing transform on the viewport.
    pub enabled: bool,
    /// Target display or printer ICC profile to simulate.
    pub profile: SoftProofProfile,
    /// Rendering intent (Perceptual vs Relative Colorimetric).
    pub intent: SoftProofRenderingIntent,
    /// Simulates physical paper white tint and black ink density (Dmin / Dmax).
    pub simulate_paper_and_ink: bool,
    /// Overlays a high-visibility cyan/magenta warning on pixels exceeding the target gamut.
    pub show_gamut_warning: bool,
}

impl Default for SoftProofConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: SoftProofProfile::FineArtMattePaper,
            intent: SoftProofRenderingIntent::Perceptual,
            simulate_paper_and_ink: true,
            show_gamut_warning: false,
        }
    }
}

/// Evaluation result for a single pixel under soft proofing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoftProofResult {
    /// Simulated linear RGB pixel mapped back to working sRGB display primaries.
    pub proofed_rgb: Vec3,
    /// True if the input pixel exceeded the target profile's color gamut volume.
    pub is_out_of_gamut: bool,
    /// Magnitude of the gamut excursion (`0.0` when inside gamut).
    pub gamut_excursion: f32,
}

/// Matrix converting linear sRGB (Rec. 709 / D65) to Display P3 (D65).
const MAT_SRGB_TO_P3: Mat3 = Mat3::from_cols(
    Vec3::new(0.822_462, 0.033_194, 0.017_083),
    Vec3::new(0.177_538, 0.966_806, 0.072_397),
    Vec3::new(0.000_000, 0.000_000, 0.910_520),
);

/// Matrix converting Display P3 (D65) back to linear sRGB (D65).
const MAT_P3_TO_SRGB: Mat3 = Mat3::from_cols(
    Vec3::new(1.224_94, -0.042_057, -0.019_638),
    Vec3::new(-0.224_94, 1.042_057, -0.078_636),
    Vec3::new(0.000_000, 0.000_000, 1.098_274),
);

/// Matrix converting linear sRGB (D65) to Adobe RGB (1998, D65).
const MAT_SRGB_TO_ADOBERGB: Mat3 = Mat3::from_cols(
    Vec3::new(0.715_146, 0.000_000, 0.000_000),
    Vec3::new(0.284_854, 1.000_000, 0.041_169),
    Vec3::new(0.000_000, 0.000_000, 0.958_831),
);

/// Matrix converting Adobe RGB (1998, D65) back to linear sRGB (D65).
const MAT_ADOBERGB_TO_SRGB: Mat3 = Mat3::from_cols(
    Vec3::new(1.398_316, 0.000_000, 0.000_000),
    Vec3::new(-0.398_316, 1.000_000, -0.042_937),
    Vec3::new(0.000_000, 0.000_000, 1.042_937),
);

/// Rec. 709 relative luminance weights.
const LUMA_709: Vec3 = Vec3::new(0.2126, 0.7152, 0.0722);

/// Returns the `(forward_matrix, inverse_matrix, max_chroma_limit)` for `profile`.
///
/// Print media have a physically constrained gamut hull (pigment ink absorption limits peak saturation
/// compared to emissive RGB displays).
#[must_use]
const fn profile_transform_params(profile: SoftProofProfile) -> (Mat3, Mat3, f32) {
    match profile {
        SoftProofProfile::SrgbIec61966 => (Mat3::IDENTITY, Mat3::IDENTITY, 1.0),
        SoftProofProfile::DisplayP3 => (MAT_SRGB_TO_P3, MAT_P3_TO_SRGB, 1.0),
        SoftProofProfile::AdobeRgb1998 => (MAT_SRGB_TO_ADOBERGB, MAT_ADOBERGB_TO_SRGB, 1.0),
        // Fine Art Matte cotton paper has a compressed pigment gamut (~74% of sRGB chroma envelope)
        SoftProofProfile::FineArtMattePaper => (Mat3::IDENTITY, Mat3::IDENTITY, 0.74),
        // Glossy Baryta paper preserves deeper blacks and wider pigment saturation (~88% of sRGB envelope)
        SoftProofProfile::GlossyBarytaPaper => (Mat3::IDENTITY, Mat3::IDENTITY, 0.88),
    }
}

/// Checks whether a linear sRGB-referenced color `rgb` lies outside the gamut of `profile`.
///
/// Returns `(is_out_of_gamut, excursion_magnitude)`.
#[must_use]
pub fn is_out_of_gamut(rgb: Vec3, profile: SoftProofProfile) -> (bool, f32) {
    let safe_rgb = Vec3::new(
        if rgb.x.is_finite() { rgb.x } else { 0.0 },
        if rgb.y.is_finite() { rgb.y } else { 0.0 },
        if rgb.z.is_finite() { rgb.z } else { 0.0 },
    );
    let (fwd, _, max_chroma_limit) = profile_transform_params(profile);
    let target = fwd * safe_rgb;

    // 1. Primary boundary excursion (< 0.0 or > 1.0 in target space)
    let under = (-target.min_element()).max(0.0);
    let over = (target.max_element() - 1.0).max(0.0);
    let mut excursion = under.max(over);

    // 2. Reflective pigment saturation hull limit for print profiles
    if max_chroma_limit < 0.999 {
        let luma = LUMA_709.dot(target.clamp(Vec3::ZERO, Vec3::ONE));
        let max_c = target.max_element();
        let min_c = target.min_element();
        let span = (max_c - min_c).max(0.0);
        // Normalized chroma relative to tonal envelope
        let allowed_span = max_chroma_limit * (1.0 - (2.0 * luma - 1.0).abs() * 0.35).max(0.25);
        if span > allowed_span {
            excursion = excursion.max(span - allowed_span);
        }
    }

    (excursion > 1e-3, excursion)
}

/// Evaluates the Soft Proofing pipeline on a linear sRGB-referenced pixel `rgb`.
///
/// Applies target profile gamut mapping according to `config.intent`, optional paper white
/// & black ink reflectance compression (`simulate_paper_and_ink`), and out-of-gamut detection.
#[must_use]
pub fn evaluate_soft_proof(rgb: Vec3, config: &SoftProofConfig) -> SoftProofResult {
    let safe_rgb = Vec3::new(
        if rgb.x.is_finite() { rgb.x } else { 0.0 },
        if rgb.y.is_finite() { rgb.y } else { 0.0 },
        if rgb.z.is_finite() { rgb.z } else { 0.0 },
    );

    let (oog, excursion) = is_out_of_gamut(safe_rgb, config.profile);
    if !config.enabled {
        return SoftProofResult {
            proofed_rgb: safe_rgb.clamp(Vec3::ZERO, Vec3::ONE),
            is_out_of_gamut: oog,
            gamut_excursion: excursion,
        };
    }

    let (fwd, inv, max_chroma_limit) = profile_transform_params(config.profile);
    let target = fwd * safe_rgb;
    let luma = LUMA_709.dot(target).clamp(0.0, 1.0);
    let chroma = target - Vec3::splat(luma);

    // 1. Apply Rendering Intent in target space
    let mapped_target = match config.intent {
        SoftProofRenderingIntent::Perceptual => {
            // Soft knee compression on chroma + highlight roll-off toward achromatic axis
            let chroma_len = chroma.length();
            let compressed_chroma = if chroma_len > 1e-5 {
                let knee = 0.55 * max_chroma_limit;
                let max_c = 0.85 * max_chroma_limit;
                let scaled_len = if chroma_len > knee {
                    let excess = chroma_len - knee;
                    let span = (max_c - knee).max(1e-4);
                    knee + span * (excess / (excess + span))
                } else {
                    chroma_len
                };
                chroma * (scaled_len / chroma_len)
            } else {
                Vec3::ZERO
            };
            (Vec3::splat(luma) + compressed_chroma).clamp(Vec3::ZERO, Vec3::ONE)
        }
        SoftProofRenderingIntent::RelativeColorimetric => {
            // Exact reproduction inside gamut, hard chroma clip to max_chroma_limit outside
            let clamped = target.clamp(Vec3::ZERO, Vec3::ONE);
            if max_chroma_limit < 0.999 {
                let c_luma = LUMA_709.dot(clamped);
                let c_diff = clamped - Vec3::splat(c_luma);
                (Vec3::splat(c_luma) + c_diff * max_chroma_limit).clamp(Vec3::ZERO, Vec3::ONE)
            } else {
                clamped
            }
        }
    };

    // Convert back to display working sRGB
    let mut display_rgb = (inv * mapped_target).clamp(Vec3::ZERO, Vec3::ONE);

    // 2. Optional Paper White & Black Ink Density Simulation (Dmin / Dmax)
    if config.simulate_paper_and_ink {
        let (black_floor, paper_white) = match config.profile {
            // Archival Cotton Matte Rag: warm cream white (Dmin) and lifted matte black ink (Dmax ~ 1.6)
            SoftProofProfile::FineArtMattePaper => (
                Vec3::new(0.036, 0.035, 0.033),
                Vec3::new(0.955, 0.942, 0.905),
            ),
            // Glossy / Baryta Fiber Paper: neutral-warm bright white and deep Dmax (~2.2)
            SoftProofProfile::GlossyBarytaPaper => (
                Vec3::new(0.014, 0.014, 0.015),
                Vec3::new(0.975, 0.970, 0.955),
            ),
            // Standard proofing paper simulation on display profiles
            SoftProofProfile::SrgbIec61966
            | SoftProofProfile::DisplayP3
            | SoftProofProfile::AdobeRgb1998 => (
                Vec3::new(0.010, 0.010, 0.010),
                Vec3::new(0.980, 0.980, 0.975),
            ),
        };
        display_rgb = black_floor + display_rgb * (paper_white - black_floor);
    }

    SoftProofResult {
        proofed_rgb: display_rgb.clamp(Vec3::ZERO, Vec3::ONE),
        is_out_of_gamut: oog,
        gamut_excursion: excursion,
    }
}
