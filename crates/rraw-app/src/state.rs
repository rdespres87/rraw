#![forbid(unsafe_code)]

//! State definitions, darkroom configuration, grading presets, and sidecar serialization.

use rraw_color::DisplayTransform;
use rraw_storage::{SidecarPaths, XmpEditor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Navigation tabs in the left panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeftPanelTab {
    #[default]
    Explorer,
    Presets,
    Exif,
}

/// Active channel in the interactive tone curve editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CurveChannel {
    #[default]
    CombinedRgb,
    Red,
    Green,
    Blue,
    Luma,
}

impl CurveChannel {
    pub const ALL: [Self; 5] = [
        Self::CombinedRgb,
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Luma,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CombinedRgb => "RVB",
            Self::Red => "Rouge",
            Self::Green => "Vert",
            Self::Blue => "Bleu",
            Self::Luma => "Luminance",
        }
    }
}

/// High-level inspector navigation tabs in the right panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Develop,
    Explorer,
    Exif,
}

/// Rating filter applied to bottom filmstrip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RatingFilter {
    #[default]
    All,
    MinStars(u8),
    Unrated,
}

impl RatingFilter {
    /// Returns true if a photograph with `rating` passes the filter.
    #[must_use]
    pub fn matches(self, rating: u8) -> bool {
        match self {
            Self::All => true,
            Self::MinStars(min) => rating >= min,
            Self::Unrated => rating == 0,
        }
    }
}

/// Aspect ratio presets for the cropping tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CropAspectRatio {
    #[default]
    Free,
    Original,
    Square1x1,
    Ratio3x2,
    Ratio4x3,
    Ratio16x9,
    Ratio5x4,
    Ratio9x16,
}

impl CropAspectRatio {
    pub const ALL: [Self; 8] = [
        Self::Free,
        Self::Original,
        Self::Square1x1,
        Self::Ratio3x2,
        Self::Ratio4x3,
        Self::Ratio16x9,
        Self::Ratio5x4,
        Self::Ratio9x16,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Free => "Libre (Free)",
            Self::Original => "Original",
            Self::Square1x1 => "1:1 (Carré)",
            Self::Ratio3x2 => "3:2 (35mm)",
            Self::Ratio4x3 => "4:3 (M4/3)",
            Self::Ratio16x9 => "16:9 (Cinéma)",
            Self::Ratio5x4 => "5:4 (Grand format)",
            Self::Ratio9x16 => "9:16 (Story)",
        }
    }

    #[must_use]
    pub fn ratio(self, orig_w: f32, orig_h: f32) -> Option<f32> {
        match self {
            Self::Free => None,
            Self::Original => {
                if orig_h > 0.0 {
                    Some(orig_w / orig_h)
                } else {
                    None
                }
            }
            Self::Square1x1 => Some(1.0),
            Self::Ratio3x2 => Some(3.0 / 2.0),
            Self::Ratio4x3 => Some(4.0 / 3.0),
            Self::Ratio16x9 => Some(16.0 / 9.0),
            Self::Ratio5x4 => Some(5.0 / 4.0),
            Self::Ratio9x16 => Some(9.0 / 16.0),
        }
    }
}

/// Active drag handle for interactive viewport cropping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CropDragHandle {
    #[default]
    None,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// State of the interactive 2-parallel-lines perspective correction tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParallelToolPhase {
    #[default]
    Inactive,
    DrawingLine1,
    WaitingLine2,
    DrawingLine2,
}

/// Supported export raster formats with user-selectable compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormatChoice {
    #[default]
    Jpeg,
    Png,
    Tiff,
}

impl ExportFormatChoice {
    #[must_use]
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Tiff => "tif",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG (.jpg)",
            Self::Png => "PNG (.png)",
            Self::Tiff => "TIFF (.tif)",
        }
    }
}

/// Bit depth options for raster export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportBitDepth {
    #[default]
    Bits8,
    Bits16,
}

/// Output scaling mode for darkroom image export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportScaleMode {
    #[default]
    Original,
    Percent75,
    Percent50,
    FitLongEdge2048,
    FitLongEdge3840,
}

impl ExportScaleMode {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Original => "100% (Pleine résolution)",
            Self::Percent75 => "75%",
            Self::Percent50 => "50%",
            Self::FitLongEdge2048 => "2048 px (Format Web optimal)",
            Self::FitLongEdge3840 => "3840 px (4K UHD)",
        }
    }
}

/// Identifiers for customizable darkroom inspector sections that can be reordered by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DarkroomSection {
    ExposureTone,
    AiSemanticMasks,
    ToneCurves,
    WhiteBalanceColor,
    SoftProofing,
    Crop,
    Straighten,
    Geometry,
    DetailDenoise,
    OpticalCorrections,
    #[serde(other)]
    LegacyOther,
}

impl DarkroomSection {
    pub const ALL: [Self; 9] = [
        Self::ExposureTone,
        Self::AiSemanticMasks,
        Self::WhiteBalanceColor,
        Self::SoftProofing,
        Self::Crop,
        Self::Straighten,
        Self::Geometry,
        Self::DetailDenoise,
        Self::OpticalCorrections,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::ExposureTone => "☀️ Exposition & Tonalité",
            Self::AiSemanticMasks => "🎭 Masques IA (CoreML)",
            Self::ToneCurves => "📈 Courbes de tonalité",
            Self::WhiteBalanceColor => "🎨 Balance des blancs & Couleur",
            Self::SoftProofing => "🖨️ Épreuvage écran & Hors-Gamut (ICC)",
            Self::Crop => "✂️ Recadrer (Crop)",
            Self::Straighten => "📐 Redresser (Rotation / Horizon)",
            Self::Geometry => "🏛️ Géométrie (Perspective)",
            Self::DetailDenoise => "🔍 Détails & Débruitage",
            Self::OpticalCorrections => "📷 Corrections optiques",
            Self::LegacyOther => "Autre",
        }
    }
}

/// Persistent user layout preferences saved to disk across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLayoutConfig {
    pub section_order: Vec<DarkroomSection>,
}

impl Default for AppLayoutConfig {
    fn default() -> Self {
        Self {
            section_order: DarkroomSection::ALL.to_vec(),
        }
    }
}

impl AppLayoutConfig {
    fn config_path() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("rraw")
                .join("layout.json")
        })
    }

    /// Loads persisted layout configuration from disk, falling back to defaults.
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(data) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(loaded) = serde_json::from_str::<Self>(&data) else {
            return Self::default();
        };

        // Filter out unknown legacy sections and ensure all valid sections are present
        let mut order: Vec<DarkroomSection> = loaded
            .section_order
            .into_iter()
            .filter(|s| *s != DarkroomSection::LegacyOther)
            .collect();
        for s in DarkroomSection::ALL {
            if !order.contains(&s) {
                order.push(s);
            }
        }
        Self {
            section_order: order,
        }
    }

    /// Persists layout configuration to disk.
    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}

/// Photographic development presets for 1-click styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GradingPreset {
    #[default]
    AsShot,
    DaylightPunch,
    PortraitSoft,
    CinematicAgX,
    MonochromeFineArt,
    LandscapeVivid,
    GoldenHourWarmth,
    MoodyMatte,
    HighKeyAiry,
    StreetContrastBW,
    VintageFilm,
    NightAstro,
    FujiClassicChrome,
    KodakPortra400,
    FujiVelvia50,
    KodakTriX400,
    IlfordHp5,
    Custom,
}

impl GradingPreset {
    pub const ALL: [Self; 18] = [
        Self::AsShot,
        Self::DaylightPunch,
        Self::PortraitSoft,
        Self::CinematicAgX,
        Self::MonochromeFineArt,
        Self::LandscapeVivid,
        Self::GoldenHourWarmth,
        Self::MoodyMatte,
        Self::HighKeyAiry,
        Self::StreetContrastBW,
        Self::VintageFilm,
        Self::NightAstro,
        Self::FujiClassicChrome,
        Self::KodakPortra400,
        Self::FujiVelvia50,
        Self::KodakTriX400,
        Self::IlfordHp5,
        Self::Custom,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AsShot => "Default / As Shot",
            Self::DaylightPunch => "Daylight Punch (+0.3 EV, 5600K)",
            Self::PortraitSoft => "Portrait Soft (+0.6 EV, 5200K)",
            Self::CinematicAgX => "Cinematic AgX (+0.2 EV, 6000K)",
            Self::MonochromeFineArt => "B&W Fine Art (Monochrome)",
            Self::LandscapeVivid => "Landscape Vivid (+0.35 EV, 5400K)",
            Self::GoldenHourWarmth => "Golden Hour Warmth (+0.5 EV, 6300K)",
            Self::MoodyMatte => "Moody Matte (-0.25 EV, 5100K)",
            Self::HighKeyAiry => "High-Key Airy (+0.9 EV, 5300K)",
            Self::StreetContrastBW => "Street Contrast B&W (Monochrome)",
            Self::VintageFilm => "Vintage Film (+0.15 EV, 5800K)",
            Self::NightAstro => "Night & Astro (+0.75 EV, 4200K)",
            Self::FujiClassicChrome => "Fujifilm Classic Chrome",
            Self::KodakPortra400 => "Kodak Portra 400",
            Self::FujiVelvia50 => "Fujifilm Velvia 50",
            Self::KodakTriX400 => "Kodak Tri-X 400 (N&B)",
            Self::IlfordHp5 => "Ilford HP5 Plus (N&B)",
            Self::Custom => "Custom",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::AsShot => "Réglages d'origine du boîtier sans retouche",
            Self::DaylightPunch => "Contraste dynamique & ombres relevées pour plein jour",
            Self::PortraitSoft => "Teinte douce, tons chair chauds et contraste atténué",
            Self::CinematicAgX => {
                "Rendu cinéma argentique avec compression douce des hautes lumières"
            }
            Self::MonochromeFineArt => "Noir & blanc artistique profond et modelé fin",
            Self::LandscapeVivid => "Couleurs riches, ombres débouchées et netteté paysagère",
            Self::GoldenHourWarmth => {
                "Lumière dorée du coucher de soleil (6300K) et ombres chaudes"
            }
            Self::MoodyMatte => "Ambiance cinématographique feutrée, noirs légèrement décollés",
            Self::HighKeyAiry => "Tonalité lumineuse, surexposition maîtrisée et teintes éthérées",
            Self::StreetContrastBW => "Noir & blanc urbain percutant, micro-contraste élevé",
            Self::VintageFilm => "Grain et patine argentique vintage, vignetage marqué",
            Self::NightAstro => "Ciel nocturne clarifié (4200K), débruitage ondelettes renforcé",
            Self::FujiClassicChrome => {
                "Rendu documentaire feutré, ombres denses, saturation atténuée et bleus cyan"
            }
            Self::KodakPortra400 => {
                "Couleurs pastel, tons chair flatteurs, ombres décollées et hautes lumières soyeuses"
            }
            Self::FujiVelvia50 => {
                "Diapositive mythique à saturation éclatante, noirs d'encre et ciels profonds"
            }
            Self::KodakTriX400 => {
                "Noir & blanc photojournalisme classique, contraste affirmé et noirs profonds"
            }
            Self::IlfordHp5 => {
                "Noir & blanc argentique à grande latitude, modelé doux et dégradés subtils"
            }
            Self::Custom => "Réglages personnalisés par l'utilisateur",
        }
    }

    #[must_use]
    pub const fn id_str(self) -> &'static str {
        match self {
            Self::AsShot => "AsShot",
            Self::DaylightPunch => "DaylightPunch",
            Self::PortraitSoft => "PortraitSoft",
            Self::CinematicAgX => "CinematicAgX",
            Self::MonochromeFineArt => "MonochromeFineArt",
            Self::LandscapeVivid => "LandscapeVivid",
            Self::GoldenHourWarmth => "GoldenHourWarmth",
            Self::MoodyMatte => "MoodyMatte",
            Self::HighKeyAiry => "HighKeyAiry",
            Self::StreetContrastBW => "StreetContrastBW",
            Self::VintageFilm => "VintageFilm",
            Self::NightAstro => "NightAstro",
            Self::FujiClassicChrome => "FujiClassicChrome",
            Self::KodakPortra400 => "KodakPortra400",
            Self::FujiVelvia50 => "FujiVelvia50",
            Self::KodakTriX400 => "KodakTriX400",
            Self::IlfordHp5 => "IlfordHp5",
            Self::Custom => "Custom",
        }
    }

    #[must_use]
    pub fn from_id_str(s: &str) -> Option<Self> {
        match s {
            "AsShot" => Some(Self::AsShot),
            "DaylightPunch" => Some(Self::DaylightPunch),
            "PortraitSoft" => Some(Self::PortraitSoft),
            "CinematicAgX" => Some(Self::CinematicAgX),
            "MonochromeFineArt" => Some(Self::MonochromeFineArt),
            "LandscapeVivid" => Some(Self::LandscapeVivid),
            "GoldenHourWarmth" => Some(Self::GoldenHourWarmth),
            "MoodyMatte" => Some(Self::MoodyMatte),
            "HighKeyAiry" => Some(Self::HighKeyAiry),
            "StreetContrastBW" => Some(Self::StreetContrastBW),
            "VintageFilm" => Some(Self::VintageFilm),
            "NightAstro" => Some(Self::NightAstro),
            "FujiClassicChrome" => Some(Self::FujiClassicChrome),
            "KodakPortra400" => Some(Self::KodakPortra400),
            "FujiVelvia50" => Some(Self::FujiVelvia50),
            "KodakTriX400" => Some(Self::KodakTriX400),
            "IlfordHp5" => Some(Self::IlfordHp5),
            "Custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn apply(self, grading: &mut DarkroomGradingState) {
        grading.preset = self;
        grading.tone_curves = rraw_core::curves::ToneCurvesRecipe::default();
        if self != Self::Custom {
            grading.custom_clut = None;
        }
        match self {
            Self::AsShot => {
                let p = grading.preset;
                *grading = DarkroomGradingState::default();
                grading.preset = p;
            }
            Self::DaylightPunch => {
                grading.exposure_ev = 0.3;
                grading.contrast = 15.0;
                grading.highlights = -10.0;
                grading.shadows = 10.0;
                grading.kelvin = 5600.0;
                grading.tint = 0.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 35.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::PortraitSoft => {
                grading.exposure_ev = 0.6;
                grading.contrast = -8.0;
                grading.highlights = -15.0;
                grading.shadows = 20.0;
                grading.kelvin = 5200.0;
                grading.tint = 2.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.wavelet_denoise_sigma = 1.5;
                grading.vignette_amount = 20.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::CinematicAgX => {
                grading.exposure_ev = 0.2;
                grading.contrast = 25.0;
                grading.highlights = -20.0;
                grading.shadows = -10.0;
                grading.kelvin = 6000.0;
                grading.tint = -4.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 60.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::MonochromeFineArt => {
                grading.exposure_ev = 0.0;
                grading.contrast = 35.0;
                grading.highlights = 15.0;
                grading.shadows = -15.0;
                grading.wavelet_denoise_sigma = 0.5;
                grading.capture_sharpening_amount = 0.8;
                grading.vignette_amount = 60.0;
                grading.vignette_correction = true;
                grading.is_monochrome = true;
            }
            Self::LandscapeVivid => {
                grading.exposure_ev = 0.35;
                grading.contrast = 22.0;
                grading.highlights = -25.0;
                grading.shadows = 18.0;
                grading.kelvin = 5400.0;
                grading.tint = -3.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.capture_sharpening_amount = 0.7;
                grading.vignette_amount = 30.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::GoldenHourWarmth => {
                grading.exposure_ev = 0.5;
                grading.contrast = 12.0;
                grading.highlights = -20.0;
                grading.shadows = 22.0;
                grading.kelvin = 6300.0;
                grading.tint = 5.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 40.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::MoodyMatte => {
                grading.exposure_ev = -0.25;
                grading.contrast = -15.0;
                grading.highlights = -15.0;
                grading.shadows = 32.0;
                grading.kelvin = 5100.0;
                grading.tint = -6.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 50.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::HighKeyAiry => {
                grading.exposure_ev = 0.9;
                grading.contrast = -10.0;
                grading.highlights = -30.0;
                grading.shadows = 15.0;
                grading.kelvin = 5300.0;
                grading.tint = 2.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.wavelet_denoise_sigma = 1.2;
                grading.vignette_amount = 15.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::StreetContrastBW => {
                grading.exposure_ev = 0.1;
                grading.contrast = 48.0;
                grading.highlights = 20.0;
                grading.shadows = -28.0;
                grading.capture_sharpening_amount = 1.0;
                grading.vignette_amount = 45.0;
                grading.vignette_correction = true;
                grading.is_monochrome = true;
            }
            Self::VintageFilm => {
                grading.exposure_ev = 0.15;
                grading.contrast = 8.0;
                grading.highlights = -12.0;
                grading.shadows = 18.0;
                grading.kelvin = 5800.0;
                grading.tint = 8.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 65.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::NightAstro => {
                grading.exposure_ev = 0.75;
                grading.contrast = 28.0;
                grading.highlights = -12.0;
                grading.shadows = 10.0;
                grading.kelvin = 4200.0;
                grading.tint = -5.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.wavelet_denoise_sigma = 2.2;
                grading.capture_sharpening_amount = 0.6;
                grading.vignette_amount = 40.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
            }
            Self::FujiClassicChrome => {
                grading.exposure_ev = 0.0;
                grading.contrast = 14.0;
                grading.highlights = -10.0;
                grading.shadows = 6.0;
                grading.blacks = -8.0;
                grading.saturation = -18.0;
                grading.vibrance = -6.0;
                grading.kelvin = 5400.0;
                grading.tint = -2.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 25.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.0), (0.25, 0.22), (0.75, 0.77), (1.0, 1.0)]);
                grading.tone_curves.red =
                    curve_from_points(&[(0.0, 0.0), (0.70, 0.68), (1.0, 0.98)]);
                grading.tone_curves.blue =
                    curve_from_points(&[(0.0, 0.0), (0.65, 0.67), (1.0, 1.0)]);
            }
            Self::KodakPortra400 => {
                grading.exposure_ev = 0.25;
                grading.contrast = -6.0;
                grading.highlights = -16.0;
                grading.shadows = 18.0;
                grading.blacks = 8.0;
                grading.saturation = 4.0;
                grading.vibrance = 10.0;
                grading.kelvin = 5800.0;
                grading.tint = 4.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 20.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.025), (0.5, 0.52), (1.0, 0.98)]);
                grading.tone_curves.red = curve_from_points(&[(0.0, 0.0), (0.5, 0.53), (1.0, 1.0)]);
            }
            Self::FujiVelvia50 => {
                grading.exposure_ev = -0.1;
                grading.contrast = 28.0;
                grading.highlights = 8.0;
                grading.shadows = -14.0;
                grading.blacks = -15.0;
                grading.saturation = 28.0;
                grading.vibrance = 20.0;
                grading.kelvin = 5600.0;
                grading.tint = 0.0;
                grading.display_transform = DisplayTransform::AgX;
                grading.vignette_amount = 35.0;
                grading.vignette_correction = true;
                grading.is_monochrome = false;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.0), (0.25, 0.20), (0.75, 0.80), (1.0, 1.0)]);
            }
            Self::KodakTriX400 => {
                grading.exposure_ev = 0.15;
                grading.contrast = 36.0;
                grading.highlights = 12.0;
                grading.shadows = -18.0;
                grading.blacks = -15.0;
                grading.capture_sharpening_amount = 0.8;
                grading.vignette_amount = 40.0;
                grading.vignette_correction = true;
                grading.is_monochrome = true;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.0), (0.30, 0.24), (0.70, 0.78), (1.0, 1.0)]);
            }
            Self::IlfordHp5 => {
                grading.exposure_ev = 0.2;
                grading.contrast = 10.0;
                grading.highlights = -12.0;
                grading.shadows = 16.0;
                grading.blacks = 5.0;
                grading.capture_sharpening_amount = 0.5;
                grading.vignette_amount = 25.0;
                grading.vignette_correction = true;
                grading.is_monochrome = true;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.02), (0.5, 0.51), (1.0, 0.985)]);
            }
            Self::Custom => {}
        }
    }

    #[must_use]
    pub fn detect_preset(grading: &DarkroomGradingState) -> Self {
        for preset in [
            Self::AsShot,
            Self::DaylightPunch,
            Self::PortraitSoft,
            Self::CinematicAgX,
            Self::MonochromeFineArt,
            Self::LandscapeVivid,
            Self::GoldenHourWarmth,
            Self::MoodyMatte,
            Self::HighKeyAiry,
            Self::StreetContrastBW,
            Self::VintageFilm,
            Self::NightAstro,
            Self::FujiClassicChrome,
            Self::KodakPortra400,
            Self::FujiVelvia50,
            Self::KodakTriX400,
            Self::IlfordHp5,
        ] {
            let mut test = DarkroomGradingState::default();
            preset.apply(&mut test);
            if (grading.exposure_ev - test.exposure_ev).abs() < 1e-4
                && (grading.contrast - test.contrast).abs() < 1e-4
                && (grading.highlights - test.highlights).abs() < 1e-4
                && (grading.shadows - test.shadows).abs() < 1e-4
                && (grading.blacks - test.blacks).abs() < 1e-4
                && (grading.saturation - test.saturation).abs() < 1e-4
                && (grading.vibrance - test.vibrance).abs() < 1e-4
                && (grading.kelvin - test.kelvin).abs() < 1e-4
                && (grading.tint - test.tint).abs() < 1e-4
                && grading.display_transform == test.display_transform
                && grading.is_monochrome == test.is_monochrome
                && grading.tone_curves == test.tone_curves
            {
                return preset;
            }
        }
        Self::Custom
    }
}

/// Helper creating a `ChannelCurve` from static slice coordinates.
fn curve_from_points(pts: &[(f32, f32)]) -> rraw_core::curves::ChannelCurve {
    rraw_core::curves::ChannelCurve {
        points: pts
            .iter()
            .map(|&(x, y)| rraw_core::curves::CurvePoint { x, y })
            .collect(),
    }
}

/// Pre-computed 256-bin RGB and Luminance histogram for live display in scopes panel.
#[derive(Debug, Clone)]
pub struct HistogramData {
    pub r: [u32; 256],
    pub g: [u32; 256],
    pub b: [u32; 256],
    pub lum: [u32; 256],
    pub max_count: u32,
    pub shadow_clipped: u32,
    pub highlight_clipped: u32,
    /// Number of pixels exceeding the active Soft Proof target profile gamut.
    pub out_of_gamut_count: u32,
}

impl Default for HistogramData {
    fn default() -> Self {
        Self {
            r: [0; 256],
            g: [0; 256],
            b: [0; 256],
            lum: [0; 256],
            max_count: 1,
            shadow_clipped: 0,
            highlight_clipped: 0,
            out_of_gamut_count: 0,
        }
    }
}

/// Local adjustment layer driven by a CoreML AI semantic segmentation mask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMaskLayer {
    /// Target semantic class (Subject, Sky, Background, Portrait).
    pub category: rraw_core::SemanticMaskCategory,
    /// Whether this semantic mask layer is active.
    pub enabled: bool,
    /// Whether to invert the semantic mask ($1.0 - p$).
    pub invert: bool,
    /// Boundary feather / soft transition width in `[0.02, 0.5]`.
    pub feather: f32,
    /// Overall layer opacity in `[0.0, 1.0]`.
    pub opacity: f32,
    /// Local exposure offset in EV (`-3.0..=+3.0`).
    pub exposure_ev: f32,
    /// Local contrast shift (`-100.0..=+100.0`).
    pub contrast: f32,
    /// Local saturation shift (`-100.0..=+100.0`).
    pub saturation: f32,
    /// Local temperature shift (`-100.0..=+100.0`).
    pub temperature_shift: f32,
}

impl SemanticMaskLayer {
    /// Creates a new enabled semantic mask layer for `category` with neutral grading offsets.
    #[must_use]
    pub const fn new(category: rraw_core::SemanticMaskCategory) -> Self {
        Self {
            category,
            enabled: true,
            invert: false,
            feather: 0.35,
            opacity: 1.0,
            exposure_ev: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature_shift: 0.0,
        }
    }

    /// Returns true if this mask layer contributes no effective color or exposure change.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        !self.enabled
            || self.opacity <= 1e-4
            || (self.exposure_ev.abs() < 1e-4
                && self.contrast.abs() < 1e-3
                && self.saturation.abs() < 1e-3
                && self.temperature_shift.abs() < 1e-3)
    }

    /// Human-readable French UI label for a semantic category.
    #[must_use]
    pub const fn category_label(category: rraw_core::SemanticMaskCategory) -> &'static str {
        match category {
            rraw_core::SemanticMaskCategory::Subject => "🧍 Sujet",
            rraw_core::SemanticMaskCategory::Sky => "☁️ Ciel",
            rraw_core::SemanticMaskCategory::Background => "🏞️ Arrière-plan",
            rraw_core::SemanticMaskCategory::Portrait => "👤 Portrait (Peau)",
        }
    }
}

/// Active custom 3D CLUT state.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomClutState {
    pub name: String,
    pub path: Option<std::path::PathBuf>,
    pub lut: std::sync::Arc<rraw_color::ColorLut3d>,
    pub strength: f32,
}

/// Darkroom grading parameters bound to reactive egui sliders.
#[derive(Debug, Clone, PartialEq)]
pub struct DarkroomGradingState {
    pub preset: GradingPreset,
    pub exposure_ev: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub kelvin: f32,
    pub tint: f32,
    pub display_transform: DisplayTransform,
    pub wavelet_denoise_sigma: f32,
    pub capture_sharpening_amount: f32,
    pub distortion_correction: bool,
    pub distortion_amount: f32,
    pub chromatic_aberration_correction: bool,
    pub vignette_correction: bool,
    pub vignette_amount: f32,
    pub is_monochrome: bool,

    // Crop settings
    pub crop_enabled: bool,
    pub crop_aspect: CropAspectRatio,
    pub crop_left: f32,
    pub crop_top: f32,
    pub crop_right: f32,
    pub crop_bottom: f32,
    pub crop_show_grid: bool,

    // Straighten settings
    pub rotation_degrees: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,

    // Geometry / Perspective settings
    pub geometry_enabled: bool,
    pub keystone_vertical: f32,
    pub keystone_horizontal: f32,
    pub auto_crop_geometry: bool,

    // Tone curves
    pub tone_curves: rraw_core::curves::ToneCurvesRecipe,

    // 3D CLUT (.cube)
    pub custom_clut: Option<CustomClutState>,

    // CoreML AI Semantic Segmentation Masks (Stage 6)
    pub semantic_masks: Vec<SemanticMaskLayer>,
    /// Optional active semantic category overlay displayed in ruby-magenta over the viewport.
    pub show_semantic_mask_overlay: Option<rraw_core::SemanticMaskCategory>,

    // Color Management & Soft Proofing (Task 6.4)
    pub soft_proof: rraw_color::SoftProofConfig,
}

impl DarkroomGradingState {
    /// Returns a mutable reference to the semantic mask layer for `category`, creating it if absent.
    pub fn upsert_semantic_mask(
        &mut self,
        category: rraw_core::SemanticMaskCategory,
    ) -> &mut SemanticMaskLayer {
        if let Some(pos) = self
            .semantic_masks
            .iter()
            .position(|m| m.category == category)
        {
            &mut self.semantic_masks[pos]
        } else {
            self.semantic_masks.push(SemanticMaskLayer::new(category));
            let last_idx = self.semantic_masks.len() - 1;
            &mut self.semantic_masks[last_idx]
        }
    }

    /// Removes the semantic mask layer for `category` if present.
    pub fn remove_semantic_mask(&mut self, category: rraw_core::SemanticMaskCategory) {
        self.semantic_masks.retain(|m| m.category != category);
        if self.show_semantic_mask_overlay == Some(category) {
            self.show_semantic_mask_overlay = None;
        }
    }

    /// Converts all configured semantic mask layers into `rraw_core::Stage6MasksParams` for GPU Stage 6.
    #[must_use]
    pub fn to_stage6_masks_params(&self) -> rraw_core::Stage6MasksParams {
        let masks = self
            .semantic_masks
            .iter()
            .map(|layer| rraw_core::GeometricMask {
                id: format!("ai_{}", layer.category.id_str()),
                kind: rraw_core::MaskKind::SemanticAi {
                    category: layer.category,
                    invert: layer.invert,
                    feather: layer.feather,
                },
                exposure_offset_ev: layer.exposure_ev,
                opacity: layer.opacity,
                enabled: layer.enabled,
            })
            .collect();
        rraw_core::Stage6MasksParams { masks }
    }
}

impl Default for DarkroomGradingState {
    fn default() -> Self {
        Self {
            preset: GradingPreset::AsShot,
            exposure_ev: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            blacks: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            kelvin: 5500.0,
            tint: 0.0,
            display_transform: DisplayTransform::AgX,
            wavelet_denoise_sigma: 1.0,
            capture_sharpening_amount: 0.5,
            distortion_correction: true,
            distortion_amount: 0.0,
            chromatic_aberration_correction: true,
            vignette_correction: true,
            vignette_amount: 50.0,
            is_monochrome: false,
            crop_enabled: false,
            crop_aspect: CropAspectRatio::Free,
            crop_left: 0.0,
            crop_top: 0.0,
            crop_right: 1.0,
            crop_bottom: 1.0,
            crop_show_grid: true,
            rotation_degrees: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            geometry_enabled: true,
            keystone_vertical: 0.0,
            keystone_horizontal: 0.0,
            auto_crop_geometry: true,
            tone_curves: rraw_core::curves::ToneCurvesRecipe::default(),
            custom_clut: None,
            semantic_masks: Vec::new(),
            show_semantic_mask_overlay: None,
            soft_proof: rraw_color::SoftProofConfig::default(),
        }
    }
}

const fn default_true_bool() -> bool {
    true
}

const fn default_one_f32() -> f32 {
    1.0
}

/// Serialized recipe saved into adjacent `<photo>.rraw.json` sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarRecipe {
    pub schema_version: u32,
    #[serde(default)]
    pub preset: Option<String>,
    pub exposure_ev: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    #[serde(default)]
    pub blacks: f32,
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub vibrance: f32,
    pub kelvin: f32,
    pub tint: f32,
    pub display_transform: String,
    pub wavelet_denoise_sigma: f32,
    pub capture_sharpening_amount: f32,
    pub distortion_correction: bool,
    pub distortion_amount: f32,
    pub chromatic_aberration_correction: bool,
    pub vignette_correction: bool,
    pub vignette_amount: f32,
    pub is_monochrome: bool,
    pub rating: u8,
    #[serde(default)]
    pub crop_enabled: bool,
    #[serde(default)]
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
    #[serde(default)]
    pub tone_curves: rraw_core::curves::ToneCurvesRecipe,
    #[serde(default)]
    pub custom_lut_path: Option<String>,
    #[serde(default = "default_one_f32")]
    pub custom_lut_strength: f32,
    #[serde(default)]
    pub semantic_masks: Vec<SemanticMaskLayer>,
    #[serde(default)]
    pub soft_proof: rraw_color::SoftProofConfig,
}

impl SidecarRecipe {
    #[must_use]
    pub fn from_state(grading: &DarkroomGradingState, rating: u8) -> Self {
        Self {
            schema_version: 1,
            preset: Some(grading.preset.id_str().to_string()),
            exposure_ev: grading.exposure_ev,
            contrast: grading.contrast,
            highlights: grading.highlights,
            shadows: grading.shadows,
            blacks: grading.blacks,
            saturation: grading.saturation,
            vibrance: grading.vibrance,
            kelvin: grading.kelvin,
            tint: grading.tint,
            display_transform: match grading.display_transform {
                DisplayTransform::AgX => "AgX".to_string(),
                DisplayTransform::OpenDrt => "OpenDrt".to_string(),
            },
            wavelet_denoise_sigma: grading.wavelet_denoise_sigma,
            capture_sharpening_amount: grading.capture_sharpening_amount,
            distortion_correction: grading.distortion_correction,
            distortion_amount: grading.distortion_amount,
            chromatic_aberration_correction: grading.chromatic_aberration_correction,
            vignette_correction: grading.vignette_correction,
            vignette_amount: grading.vignette_amount,
            is_monochrome: grading.is_monochrome,
            rating,
            crop_enabled: grading.crop_enabled,
            crop_aspect: Some(format!("{:?}", grading.crop_aspect)),
            crop_left: grading.crop_left,
            crop_top: grading.crop_top,
            crop_right: grading.crop_right,
            crop_bottom: grading.crop_bottom,
            rotation_degrees: grading.rotation_degrees,
            flip_horizontal: grading.flip_horizontal,
            flip_vertical: grading.flip_vertical,
            geometry_enabled: grading.geometry_enabled,
            keystone_vertical: grading.keystone_vertical,
            keystone_horizontal: grading.keystone_horizontal,
            tone_curves: grading.tone_curves.clone(),
            custom_lut_path: grading
                .custom_clut
                .as_ref()
                .and_then(|c| c.path.as_ref().map(|p| p.to_string_lossy().to_string())),
            custom_lut_strength: grading.custom_clut.as_ref().map_or(1.0, |c| c.strength),
            semantic_masks: grading.semantic_masks.clone(),
            soft_proof: grading.soft_proof,
        }
    }

    pub fn apply_to(&self, grading: &mut DarkroomGradingState) {
        grading.exposure_ev = self.exposure_ev;
        grading.contrast = self.contrast;
        grading.highlights = self.highlights;
        grading.shadows = self.shadows;
        grading.blacks = self.blacks;
        grading.saturation = self.saturation;
        grading.vibrance = self.vibrance;
        grading.kelvin = self.kelvin;
        grading.tint = self.tint;
        grading.display_transform = if self.display_transform == "OpenDrt" {
            DisplayTransform::OpenDrt
        } else {
            DisplayTransform::AgX
        };
        grading.wavelet_denoise_sigma = self.wavelet_denoise_sigma;
        grading.capture_sharpening_amount = self.capture_sharpening_amount;
        grading.distortion_correction = self.distortion_correction;
        grading.distortion_amount = self.distortion_amount;
        grading.chromatic_aberration_correction = self.chromatic_aberration_correction;
        grading.vignette_correction = self.vignette_correction;
        grading.vignette_amount = self.vignette_amount;
        grading.is_monochrome = self.is_monochrome;
        grading.tone_curves = self.tone_curves.clone();
        grading.semantic_masks = self.semantic_masks.clone();
        grading.show_semantic_mask_overlay = None;
        grading.soft_proof = self.soft_proof;
        grading.preset = if let Some(p_str) = &self.preset {
            GradingPreset::from_id_str(p_str)
                .unwrap_or_else(|| GradingPreset::detect_preset(grading))
        } else {
            GradingPreset::detect_preset(grading)
        };
        grading.crop_enabled = self.crop_enabled;
        grading.crop_aspect = match self.crop_aspect.as_deref() {
            Some("Square1x1") => CropAspectRatio::Square1x1,
            Some("Ratio3x2") => CropAspectRatio::Ratio3x2,
            Some("Ratio4x3") => CropAspectRatio::Ratio4x3,
            Some("Ratio16x9") => CropAspectRatio::Ratio16x9,
            Some("Ratio5x4") => CropAspectRatio::Ratio5x4,
            Some("Ratio9x16") => CropAspectRatio::Ratio9x16,
            Some("Original") => CropAspectRatio::Original,
            _ => CropAspectRatio::Free,
        };
        grading.crop_left = self.crop_left;
        grading.crop_top = self.crop_top;
        grading.crop_right = if self.crop_right <= 0.01 {
            1.0
        } else {
            self.crop_right
        };
        grading.crop_bottom = if self.crop_bottom <= 0.01 {
            1.0
        } else {
            self.crop_bottom
        };
        grading.rotation_degrees = self.rotation_degrees;
        grading.flip_horizontal = self.flip_horizontal;
        grading.flip_vertical = self.flip_vertical;
        grading.geometry_enabled = self.geometry_enabled;
        grading.keystone_vertical = self.keystone_vertical;
        grading.keystone_horizontal = self.keystone_horizontal;

        if let Some(path_str) = &self.custom_lut_path {
            let path = std::path::PathBuf::from(path_str);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(lut) = rraw_color::ColorLut3d::from_cube_str(&content) {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Custom LUT")
                            .to_string();
                        grading.custom_clut = Some(CustomClutState {
                            name,
                            path: Some(path),
                            lut: std::sync::Arc::new(lut),
                            strength: self.custom_lut_strength.clamp(0.0, 1.0),
                        });
                    }
                }
            }
        } else {
            grading.custom_clut = None;
        }
    }
}

/// Standard XMP template preserving XML namespaces.
fn default_xmp_template() -> String {
    r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/">
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
        .to_string()
}

/// Saves non-destructive development parameters to `<photo>.rraw.json` and standard metadata to `<photo>.xmp`.
pub fn save_photo_sidecars(photo_path: &Path, grading: &DarkroomGradingState, rating: u8) {
    let sidecars = SidecarPaths::from_raw_path(photo_path);

    // 1. Write <photo>.rraw.json recipe
    let recipe = SidecarRecipe::from_state(grading, rating);
    if let Ok(json_str) = serde_json::to_string_pretty(&recipe) {
        let _ = std::fs::write(&sidecars.recipe_json, json_str);
    }

    // 2. Write <photo>.xmp sidecar
    let initial_xml = if sidecars.xmp_sidecar.exists() {
        std::fs::read_to_string(&sidecars.xmp_sidecar).unwrap_or_else(|_| default_xmp_template())
    } else {
        default_xmp_template()
    };

    if let Ok(mut editor) = XmpEditor::parse_str(&initial_xml) {
        editor.set_rating(rating);
        if let Ok(xmp_out) = editor.to_xml_string() {
            let _ = std::fs::write(&sidecars.xmp_sidecar, xmp_out);
        }
    }
}

/// Attempts to load non-destructive settings from adjacent `<photo>.rraw.json` and `<photo>.xmp`.
#[must_use]
pub fn load_photo_sidecars(photo_path: &Path) -> (Option<DarkroomGradingState>, Option<u8>) {
    let sidecars = SidecarPaths::from_raw_path(photo_path);
    let mut grading = None;
    let mut rating = None;

    // 1. Check <photo>.rraw.json
    if sidecars.recipe_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&sidecars.recipe_json) {
            if let Ok(recipe) = serde_json::from_str::<SidecarRecipe>(&content) {
                let mut state = DarkroomGradingState::default();
                recipe.apply_to(&mut state);
                rating = Some(recipe.rating);
                grading = Some(state);
            }
        }
    }

    // 2. Check <photo>.xmp (overrides/complements rating)
    if sidecars.xmp_sidecar.exists() {
        if let Ok(content) = std::fs::read_to_string(&sidecars.xmp_sidecar) {
            if let Ok(editor) = XmpEditor::parse_str(&content) {
                if let Some(r) = editor.rating() {
                    rating = Some(r);
                }
            }
        }
    }

    (grading, rating)
}
