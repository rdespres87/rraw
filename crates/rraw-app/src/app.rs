#![forbid(unsafe_code)]

//! Interactive `eframe` / `egui` native macOS application shell for Rraw.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use byteorder::{BigEndian, ByteOrder};
use egui::Color32;
use image::ImageEncoder;
use rraw_color::{apply_agx_edr, apply_opendrt, DisplayTransform};
use rraw_core::{CfaColor, DisplayHeadroomProvider, SignedCfaBuffer, SurfaceFormat};
use rraw_decode::{PureRustRawDecoder, RawDecoder};
#[cfg(target_os = "macos")]
use rraw_macos_edr::MacOsDisplayHeadroomProvider;
#[cfg(not(target_os = "macos"))]
use rraw_macos_edr::SdrFallbackHeadroomProvider;
use rraw_storage::{SidecarPaths, XmpEditor};
use serde::{Deserialize, Serialize};

/// Navigation tabs in the left panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeftPanelTab {
    #[default]
    Explorer,
    Presets,
    Exif,
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
    WhiteBalanceColor,
    Crop,
    Straighten,
    Geometry,
    DetailDenoise,
    OpticalCorrections,
    #[serde(other)]
    LegacyOther,
}

impl DarkroomSection {
    pub const ALL: [Self; 7] = [
        Self::ExposureTone,
        Self::WhiteBalanceColor,
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
            Self::WhiteBalanceColor => "🎨 Balance des blancs & Couleur",
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
    Custom,
}

impl GradingPreset {
    pub const ALL: [Self; 13] = [
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
            "Custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn apply(self, grading: &mut DarkroomGradingState) {
        grading.preset = self;
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
        ] {
            let mut test = DarkroomGradingState::default();
            preset.apply(&mut test);
            if (grading.exposure_ev - test.exposure_ev).abs() < 1e-4
                && (grading.contrast - test.contrast).abs() < 1e-4
                && (grading.highlights - test.highlights).abs() < 1e-4
                && (grading.shadows - test.shadows).abs() < 1e-4
                && (grading.kelvin - test.kelvin).abs() < 1e-4
                && (grading.tint - test.tint).abs() < 1e-4
                && grading.display_transform == test.display_transform
                && grading.is_monochrome == test.is_monochrome
            {
                return preset;
            }
        }
        Self::Custom
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
        }
    }
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

/// Metadata and state for an opened or inspected RAW photograph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedPhotoView {
    pub path: PathBuf,
    pub camera_make: String,
    pub camera_model: String,
    pub lens_model: String,
    pub dimensions: (u32, u32),
    pub iso: u32,
    pub shutter_str: String,
    pub aperture_str: String,
    pub focal_length_str: String,
    pub date_time: String,
    pub exposure_bias: String,
    pub rating: u8,
    pub exif_tags: Vec<(String, String)>,
}

impl LoadedPhotoView {
    /// Serializes all EXIF metadata tags into a formatted JSON string for export.
    #[must_use]
    pub fn export_exif_json(&self) -> String {
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "file_path".to_string(),
            self.path.to_string_lossy().to_string(),
        );
        map.insert("camera_make".to_string(), self.camera_make.clone());
        map.insert("camera_model".to_string(), self.camera_model.clone());
        if !self.lens_model.is_empty() {
            map.insert("lens_model".to_string(), self.lens_model.clone());
        }
        map.insert(
            "dimensions".to_string(),
            format!("{}x{}", self.dimensions.0, self.dimensions.1),
        );
        map.insert("iso".to_string(), self.iso.to_string());
        map.insert("shutter_speed".to_string(), self.shutter_str.clone());
        map.insert("aperture".to_string(), self.aperture_str.clone());
        map.insert("focal_length".to_string(), self.focal_length_str.clone());
        if !self.date_time.is_empty() {
            map.insert("date_time_original".to_string(), self.date_time.clone());
        }
        if !self.exposure_bias.is_empty() {
            map.insert("exposure_bias".to_string(), self.exposure_bias.clone());
        }
        for (k, v) in &self.exif_tags {
            map.insert(k.clone(), v.clone());
        }
        serde_json::to_string_pretty(&map).unwrap_or_default()
    }
}

/// Computes a 3x3 camera-to-sRGB calibration matrix from DNG ColorMatrix and white balance.
#[must_use]
pub fn compute_cam_to_srgb_matrix(cm: Option<&[f32; 9]>, wb: &[f32; 3]) -> [f32; 9] {
    if let Some(cm) = cm {
        let a = cm[0];
        let b = cm[1];
        let c = cm[2];
        let d = cm[3];
        let e = cm[4];
        let f = cm[5];
        let g = cm[6];
        let h = cm[7];
        let i = cm[8];
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if det.abs() > 1e-6 {
            let invdet = 1.0 / det;
            let inv_cm = [
                (e * i - f * h) * invdet,
                (c * h - b * i) * invdet,
                (b * f - c * e) * invdet,
                (f * g - d * i) * invdet,
                (a * i - c * g) * invdet,
                (c * d - a * f) * invdet,
                (d * h - e * g) * invdet,
                (g * b - a * h) * invdet,
                (a * e - b * d) * invdet,
            ];

            let w_r = if wb[0] > 0.0 { 1.0 / wb[0] } else { 1.0 };
            let w_g = if wb[1] > 0.0 { 1.0 / wb[1] } else { 1.0 };
            let w_b = if wb[2] > 0.0 { 1.0 / wb[2] } else { 1.0 };

            let cam_wb_to_xyz = [
                inv_cm[0] * w_r,
                inv_cm[1] * w_g,
                inv_cm[2] * w_b,
                inv_cm[3] * w_r,
                inv_cm[4] * w_g,
                inv_cm[5] * w_b,
                inv_cm[6] * w_r,
                inv_cm[7] * w_g,
                inv_cm[8] * w_b,
            ];

            const XYZ_TO_SRGB: [f32; 9] = [
                3.240_454_2,
                -1.537_138_5,
                -0.498_531_4,
                -0.969_266,
                1.876_010_8,
                0.041_556,
                0.055_643_4,
                -0.204_025_9,
                1.057_225_2,
            ];

            let mut out = [0.0_f32; 9];
            for r in 0..3 {
                for col in 0..3 {
                    let mut sum = 0.0;
                    for k in 0..3 {
                        sum += XYZ_TO_SRGB[r * 3 + k] * cam_wb_to_xyz[k * 3 + col];
                    }
                    out[r * 3 + col] = sum;
                }
            }

            for r in 0..3 {
                let row_sum = out[r * 3] + out[r * 3 + 1] + out[r * 3 + 2];
                if row_sum.abs() > 1e-4 {
                    out[r * 3] /= row_sum;
                    out[r * 3 + 1] /= row_sum;
                    out[r * 3 + 2] /= row_sum;
                }
            }

            return out;
        }
    }

    // Default saturated sRGB primary matrix preserving row neutral sum 1.0
    [1.45, -0.35, -0.10, -0.15, 1.35, -0.20, -0.05, -0.30, 1.35]
}

/// Fast pre-demosaiced linear camera RGB buffer cached in RAM.
#[derive(Debug, Clone)]
pub struct BaseRgbPreview {
    pub width: usize,
    pub height: usize,
    pub samples_rgb: Vec<[f32; 3]>,
    pub as_shot_wb: [f32; 3],
    pub cam_to_srgb: [f32; 9],
}

impl BaseRgbPreview {
    /// Builds a base preview by block-averaging sensor CFA mosaic channels.
    #[must_use]
    pub fn from_cfa(cfa: &SignedCfaBuffer) -> Self {
        Self::from_cfa_with_color(cfa, [2.0, 1.0, 1.5], None)
    }

    /// Builds a base preview with full color calibration and white balance.
    #[must_use]
    pub fn from_cfa_with_color(
        cfa: &SignedCfaBuffer,
        as_shot_wb: [f32; 3],
        color_matrix: Option<&[f32; 9]>,
    ) -> Self {
        Self::from_cfa_with_color_and_target_width(cfa, as_shot_wb, color_matrix, 1600)
    }

    /// Builds a base preview scaled to a target bounding width (e.g. 1200 for preview, 140 for thumbnail).
    #[must_use]
    pub fn from_cfa_with_color_and_target_width(
        cfa: &SignedCfaBuffer,
        as_shot_wb: [f32; 3],
        color_matrix: Option<&[f32; 9]>,
        target_width: usize,
    ) -> Self {
        let width = cfa.width();
        let height = cfa.height();
        let cam_to_srgb = compute_cam_to_srgb_matrix(color_matrix, &as_shot_wb);
        if width == 0 || height == 0 {
            return Self {
                width: 1,
                height: 1,
                samples_rgb: vec![[0.0, 0.0, 0.0]],
                as_shot_wb,
                cam_to_srgb,
            };
        }

        let raw_step = (width / target_width.max(1)).max(1);
        let step = if !raw_step.is_multiple_of(2) {
            raw_step + 1
        } else {
            raw_step
        };

        let preview_w = (width / step).max(1);
        let preview_h = (height / step).max(1);
        let samples = cfa.samples();
        let pattern = cfa.pattern();

        // Precompute Bayer quad color channel mapping for sub-millisecond point-decimation sampling
        let colors = [
            pattern.color_at(0, 0),
            pattern.color_at(1, 0),
            pattern.color_at(0, 1),
            pattern.color_at(1, 1),
        ];

        let mut r_idx = 0;
        let mut g0_idx = 1;
        let mut g1_idx = 2;
        let mut b_idx = 3;
        let mut g_found = 0;

        for (idx, &col) in colors.iter().enumerate() {
            match col {
                CfaColor::Red => r_idx = idx,
                CfaColor::Blue => b_idx = idx,
                CfaColor::Green => {
                    if g_found == 0 {
                        g0_idx = idx;
                        g_found = 1;
                    } else {
                        g1_idx = idx;
                    }
                }
            }
        }

        let mut samples_rgb = Vec::with_capacity(preview_w * preview_h);

        for py in 0..preview_h {
            let sy = py * step;
            let next_sy = (sy + 1).min(height.saturating_sub(1));
            let row0 = sy * width;
            let row1 = next_sy * width;

            for px in 0..preview_w {
                let sx = px * step;
                let next_sx = (sx + 1).min(width.saturating_sub(1));

                let quad = [
                    samples[row0 + sx],
                    samples[row0 + next_sx],
                    samples[row1 + sx],
                    samples[row1 + next_sx],
                ];

                let r = quad[r_idx];
                let g = (quad[g0_idx] + quad[g1_idx]) * 0.5;
                let b = quad[b_idx];

                samples_rgb.push([r, g, b]);
            }
        }

        Self {
            width: preview_w,
            height: preview_h,
            samples_rgb,
            as_shot_wb,
            cam_to_srgb,
        }
    }
}

/// Generates a fast ~140px RGB thumbnail image from a RAW file applying its sidecar grading if available.
#[must_use]
pub fn generate_fast_thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let decoder = PureRustRawDecoder::new();
    let decoded = decoder.decode_file(path).ok()?;
    let (grading_opt, _) = load_photo_sidecars(path);
    let grading = grading_opt.unwrap_or_default();

    let base = BaseRgbPreview::from_cfa_with_color_and_target_width(
        &decoded.cfa_signed,
        [
            decoded.wb_coeffs[0],
            decoded.wb_coeffs[1],
            decoded.wb_coeffs[2],
        ],
        decoded.color_matrix.as_ref(),
        140,
    );

    let (preview, _) = render_preview_from_base(&base, &grading);
    Some(preview)
}

/// Evaluates darkroom grading, optical distortion, vignetting, and histogram from `BaseRgbPreview`.
///
/// Executes in < 1 ms over contiguous RAM without re-sampling the raw sensor matrix.
#[must_use]
pub fn render_preview_from_base(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let preview_w = base.width;
    let preview_h = base.height;

    let exposure_gain = 2.0_f32.powf(grading.exposure_ev);

    let kelvin_ratio = (grading.kelvin / 5500.0).clamp(0.4, 2.5);
    let r_temp_scale = kelvin_ratio.powf(0.85);
    let b_temp_scale = (1.0 / kelvin_ratio).powf(0.85);
    let tint_scale = 1.0 - (grading.tint / 100.0);

    let eff_wb_r = base.as_shot_wb[0] * r_temp_scale;
    let eff_wb_g = base.as_shot_wb[1] * tint_scale;
    let eff_wb_b = base.as_shot_wb[2] * b_temp_scale;

    let mat = base.cam_to_srgb;

    let cx = preview_w as f32 * 0.5;
    let cy = preview_h as f32 * 0.5;
    let max_radius_sq = (cx * cx + cy * cy).max(1.0);

    let k_dist = if grading.distortion_correction {
        -0.06 + (grading.distortion_amount / 100.0) * 0.15
    } else if grading.distortion_amount.abs() > 0.001 {
        (grading.distortion_amount / 100.0) * 0.15
    } else {
        0.0
    };

    let v_gain = if grading.vignette_correction {
        0.35 + (grading.vignette_amount / 100.0) * 0.65
    } else if grading.vignette_amount > 0.01 {
        (grading.vignette_amount / 100.0) * 0.65
    } else {
        0.0
    };

    let rot_deg = grading.rotation_degrees;
    let rot_rad = rot_deg.to_radians();
    let cos_rot = rot_rad.cos();
    let sin_rot = rot_rad.sin();

    let flip_x = if grading.flip_horizontal { -1.0 } else { 1.0 };
    let flip_y = if grading.flip_vertical { -1.0 } else { 1.0 };

    let (keystone_v, keystone_h) = if grading.geometry_enabled {
        (
            grading.keystone_vertical / 100.0,
            grading.keystone_horizontal / 100.0,
        )
    } else {
        (0.0, 0.0)
    };

    let auto_zoom = if grading.geometry_enabled && grading.auto_crop_geometry {
        let abs_rot = rot_deg.abs().to_radians();
        let rot_zoom = abs_rot.cos() + abs_rot.sin() * (cy / cx.max(1.0));
        let k_zoom = 1.0
            + (grading
                .keystone_vertical
                .abs()
                .max(grading.keystone_horizontal.abs())
                / 100.0)
                * 0.5;
        (rot_zoom * k_zoom).max(1.0)
    } else {
        1.0
    };

    let mut color_pixels = Vec::with_capacity(preview_w * preview_h);
    let mut hist_r = [0_u32; 256];
    let mut hist_g = [0_u32; 256];
    let mut hist_b = [0_u32; 256];
    let mut hist_lum = [0_u32; 256];
    let mut shadow_clipped = 0_u32;
    let mut highlight_clipped = 0_u32;

    for py in 0..preview_h {
        let raw_dy = (py as f32 - cy) * flip_y;

        for px in 0..preview_w {
            let raw_dx = (px as f32 - cx) * flip_x;

            // 1. Rotation around optical center
            let rx = raw_dx * cos_rot + raw_dy * sin_rot;
            let ry = -raw_dx * sin_rot + raw_dy * cos_rot;

            // 2. Keystone perspective correction & auto-crop zoom
            let v_scale = (1.0 + keystone_v * (ry / cy)).max(0.1);
            let kx = (rx / v_scale) / auto_zoom;
            let h_scale = (1.0 + keystone_h * (kx / cx)).max(0.1);
            let ky = (ry / h_scale) / auto_zoom;

            let sample_idx = if k_dist.abs() > 1e-5 {
                let r2 = (kx * kx + ky * ky) / max_radius_sq;
                let warp = 1.0 + k_dist * r2;
                let warped_x =
                    ((cx + kx * warp).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let warped_y =
                    ((cy + ky * warp).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                warped_y * preview_w + warped_x
            } else {
                let sx = ((cx + kx).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let sy = ((cy + ky).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                sy * preview_w + sx
            };

            let [mean_r, mean_g, mean_b] = base.samples_rgb[sample_idx];

            let v_mult = if v_gain > 1e-4 {
                let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };

            let r_wb = mean_r * eff_wb_r;
            let g_wb = mean_g * eff_wb_g;
            let b_wb = mean_b * eff_wb_b;

            // Apply camera color calibration matrix to eliminate CFA spectral cross-talk
            let r_linear = (mat[0] * r_wb + mat[1] * g_wb + mat[2] * b_wb).max(0.0);
            let g_linear = (mat[3] * r_wb + mat[4] * g_wb + mat[5] * b_wb).max(0.0);
            let b_linear = (mat[6] * r_wb + mat[7] * g_wb + mat[8] * b_wb).max(0.0);

            let r_scaled = (r_linear * exposure_gain * v_mult).max(0.0);
            let g_scaled = (g_linear * exposure_gain * v_mult).max(0.0);
            let b_scaled = (b_linear * exposure_gain * v_mult).max(0.0);

            // Responsive localized Shadows and Highlights tonal adjustment
            let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
            let y_norm = lum / (lum + 0.18);
            let shadow_weight = (1.0 - y_norm).powi(2);
            let highlight_weight = y_norm.powi(2);
            let shadow_shift = (grading.shadows / 100.0) * 2.5 * shadow_weight;
            let highlight_shift = (grading.highlights / 100.0) * 2.5 * highlight_weight;
            let blacks_weight = (1.0 - y_norm).powi(4);
            let blacks_shift = (grading.blacks / 100.0) * 1.5 * blacks_weight;
            let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);

            let r_toned = (r_scaled * tone_gain).max(0.0);
            let g_toned = (g_scaled * tone_gain).max(0.0);
            let b_toned = (b_scaled * tone_gain).max(0.0);

            // Apply Display Rendering Transform (AgX or OpenDRT)
            let (r_drt, g_drt, b_drt) = match grading.display_transform {
                DisplayTransform::AgX => (
                    apply_agx_edr(r_toned, 1.0),
                    apply_agx_edr(g_toned, 1.0),
                    apply_agx_edr(b_toned, 1.0),
                ),
                DisplayTransform::OpenDrt => (
                    apply_opendrt(r_toned, 1.0),
                    apply_opendrt(g_toned, 1.0),
                    apply_opendrt(b_toned, 1.0),
                ),
            };

            let factor = 1.0 + (grading.contrast / 100.0).clamp(-0.8, 2.0);
            let r_c = ((r_drt - 0.18) * factor + 0.18).max(0.0);
            let g_c = ((g_drt - 0.18) * factor + 0.18).max(0.0);
            let b_c = ((b_drt - 0.18) * factor + 0.18).max(0.0);

            // Perceptual Vibrance & Saturation enhancement
            let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
            let max_val = r_c.max(g_c).max(b_c);
            let min_val = r_c.min(g_c).min(b_c);
            let current_sat = if max_val > 1e-4 {
                (max_val - min_val) / max_val
            } else {
                0.0
            };
            let base_vibrance = 0.10_f32;
            let user_vibrance = grading.vibrance / 100.0;
            let user_sat = grading.saturation / 100.0;
            let vib_weight = (1.0 - current_sat).clamp(0.0, 1.0);
            let eff_sat =
                ((1.0 + user_sat) * (1.0 + (base_vibrance + user_vibrance) * vib_weight)).max(0.0);

            let r_s = (lum_c + (r_c - lum_c) * eff_sat).max(0.0);
            let g_s = (lum_c + (g_c - lum_c) * eff_sat).max(0.0);
            let b_s = (lum_c + (b_c - lum_c) * eff_sat).max(0.0);

            let to_u8 = |x: f32| -> u8 {
                let clamped = x.clamp(0.0, 1.0);
                let toe = if clamped < 0.35 {
                    clamped.powf(1.08)
                } else {
                    clamped
                };
                let mapped = toe.powf(1.0 / 2.2);
                (mapped * 255.0).clamp(0.0, 255.0) as u8
            };

            let (u8_r, u8_g, u8_b) = if grading.is_monochrome {
                let lum_val =
                    ((to_u8(r_s) as u32 * 54 + to_u8(g_s) as u32 * 183 + to_u8(b_s) as u32 * 19)
                        >> 8) as u8;
                (lum_val, lum_val, lum_val)
            } else {
                (to_u8(r_s), to_u8(g_s), to_u8(b_s))
            };

            hist_r[u8_r as usize] += 1;
            hist_g[u8_g as usize] += 1;
            hist_b[u8_b as usize] += 1;

            let lum = (((u8_r as u32 * 54) + (u8_g as u32 * 183) + (u8_b as u32 * 19)) >> 8)
                .min(255) as usize;
            hist_lum[lum] += 1;

            if lum == 0 {
                shadow_clipped += 1;
            }
            if lum == 255 {
                highlight_clipped += 1;
            }

            color_pixels.push(Color32::from_rgb(u8_r, u8_g, u8_b));
        }
    }

    let max_count = hist_lum[1..255].iter().copied().max().unwrap_or(1).max(1);

    let hist_data = HistogramData {
        r: hist_r,
        g: hist_g,
        b: hist_b,
        lum: hist_lum,
        max_count,
        shadow_clipped,
        highlight_clipped,
    };

    (
        egui::ColorImage::new([preview_w, preview_h], color_pixels),
        hist_data,
    )
}

/// Renders the fully developed photographic raster for export, applying all darkroom grades,
/// cropping boundaries, geometric corrections, color transform, and scaling.
#[must_use]
pub fn render_export_image(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
    scale: f32,
) -> (usize, usize, Vec<u8>, Vec<u16>) {
    let preview_w = base.width;
    let preview_h = base.height;

    let (crop_x0, crop_y0, crop_x1, crop_y1) = if grading.crop_enabled {
        let x0 = (grading.crop_left.clamp(0.0, 0.95) * preview_w as f32).round() as usize;
        let y0 = (grading.crop_top.clamp(0.0, 0.95) * preview_h as f32).round() as usize;
        let x1 = (grading.crop_right.clamp(0.05, 1.0) * preview_w as f32)
            .round()
            .max(x0 as f32 + 1.0) as usize;
        let y1 = (grading.crop_bottom.clamp(0.05, 1.0) * preview_h as f32)
            .round()
            .max(y0 as f32 + 1.0) as usize;
        (
            x0.min(preview_w.saturating_sub(1)),
            y0.min(preview_h.saturating_sub(1)),
            x1.min(preview_w),
            y1.min(preview_h),
        )
    } else {
        (0, 0, preview_w, preview_h)
    };

    let cropped_w = (crop_x1 - crop_x0).max(1);
    let cropped_h = (crop_y1 - crop_y0).max(1);

    let out_w = ((cropped_w as f32 * scale).round() as usize).max(1);
    let out_h = ((cropped_h as f32 * scale).round() as usize).max(1);

    let exposure_gain = 2.0_f32.powf(grading.exposure_ev);

    let kelvin_ratio = (grading.kelvin / 5500.0).clamp(0.4, 2.5);
    let r_temp_scale = kelvin_ratio.powf(0.85);
    let b_temp_scale = (1.0 / kelvin_ratio).powf(0.85);
    let tint_scale = 1.0 - (grading.tint / 100.0);

    let eff_wb_r = base.as_shot_wb[0] * r_temp_scale;
    let eff_wb_g = base.as_shot_wb[1] * tint_scale;
    let eff_wb_b = base.as_shot_wb[2] * b_temp_scale;

    let mat = base.cam_to_srgb;

    let cx = preview_w as f32 * 0.5;
    let cy = preview_h as f32 * 0.5;
    let max_radius_sq = (cx * cx + cy * cy).max(1.0);

    let k_dist = if grading.distortion_correction {
        -0.06 + (grading.distortion_amount / 100.0) * 0.15
    } else if grading.distortion_amount.abs() > 0.001 {
        (grading.distortion_amount / 100.0) * 0.15
    } else {
        0.0
    };

    let v_gain = if grading.vignette_correction {
        0.35 + (grading.vignette_amount / 100.0) * 0.65
    } else if grading.vignette_amount > 0.01 {
        (grading.vignette_amount / 100.0) * 0.65
    } else {
        0.0
    };

    let rot_deg = grading.rotation_degrees;
    let rot_rad = rot_deg.to_radians();
    let cos_rot = rot_rad.cos();
    let sin_rot = rot_rad.sin();

    let flip_x = if grading.flip_horizontal { -1.0 } else { 1.0 };
    let flip_y = if grading.flip_vertical { -1.0 } else { 1.0 };

    let (keystone_v, keystone_h) = if grading.geometry_enabled {
        (
            grading.keystone_vertical / 100.0,
            grading.keystone_horizontal / 100.0,
        )
    } else {
        (0.0, 0.0)
    };

    let auto_zoom = if grading.geometry_enabled && grading.auto_crop_geometry {
        let abs_rot = rot_deg.abs().to_radians();
        let rot_zoom = abs_rot.cos() + abs_rot.sin() * (cy / cx.max(1.0));
        let k_zoom = 1.0
            + (grading
                .keystone_vertical
                .abs()
                .max(grading.keystone_horizontal.abs())
                / 100.0)
                * 0.5;
        (rot_zoom * k_zoom).max(1.0)
    } else {
        1.0
    };

    let mut u8_bytes = Vec::with_capacity(out_w * out_h * 3);
    let mut u16_samples = Vec::with_capacity(out_w * out_h * 3);

    for oy in 0..out_h {
        let orig_y = crop_y0 as f32 + (oy as f32 / out_h as f32) * cropped_h as f32;
        let raw_dy = (orig_y - cy) * flip_y;

        for ox in 0..out_w {
            let orig_x = crop_x0 as f32 + (ox as f32 / out_w as f32) * cropped_w as f32;
            let raw_dx = (orig_x - cx) * flip_x;

            let rx = raw_dx * cos_rot + raw_dy * sin_rot;
            let ry = -raw_dx * sin_rot + raw_dy * cos_rot;

            let v_scale = (1.0 + keystone_v * (ry / cy)).max(0.1);
            let kx = (rx / v_scale) / auto_zoom;
            let h_scale = (1.0 + keystone_h * (kx / cx)).max(0.1);
            let ky = (ry / h_scale) / auto_zoom;

            let sample_idx = if k_dist.abs() > 1e-5 {
                let r2 = (kx * kx + ky * ky) / max_radius_sq;
                let warp = 1.0 + k_dist * r2;
                let warped_x =
                    ((cx + kx * warp).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let warped_y =
                    ((cy + ky * warp).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                warped_y * preview_w + warped_x
            } else {
                let sx = ((cx + kx).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let sy = ((cy + ky).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                sy * preview_w + sx
            };

            let [mean_r, mean_g, mean_b] = base.samples_rgb[sample_idx];

            let v_mult = if v_gain > 1e-4 {
                let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };

            let r_wb = mean_r * eff_wb_r;
            let g_wb = mean_g * eff_wb_g;
            let b_wb = mean_b * eff_wb_b;

            let r_linear = (mat[0] * r_wb + mat[1] * g_wb + mat[2] * b_wb).max(0.0);
            let g_linear = (mat[3] * r_wb + mat[4] * g_wb + mat[5] * b_wb).max(0.0);
            let b_linear = (mat[6] * r_wb + mat[7] * g_wb + mat[8] * b_wb).max(0.0);

            let r_scaled = (r_linear * exposure_gain * v_mult).max(0.0);
            let g_scaled = (g_linear * exposure_gain * v_mult).max(0.0);
            let b_scaled = (b_linear * exposure_gain * v_mult).max(0.0);

            let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
            let y_norm = lum / (lum + 0.18);
            let shadow_weight = (1.0 - y_norm).powi(2);
            let highlight_weight = y_norm.powi(2);
            let shadow_shift = (grading.shadows / 100.0) * 2.5 * shadow_weight;
            let highlight_shift = (grading.highlights / 100.0) * 2.5 * highlight_weight;
            let blacks_weight = (1.0 - y_norm).powi(4);
            let blacks_shift = (grading.blacks / 100.0) * 1.5 * blacks_weight;
            let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);

            let r_toned = (r_scaled * tone_gain).max(0.0);
            let g_toned = (g_scaled * tone_gain).max(0.0);
            let b_toned = (b_scaled * tone_gain).max(0.0);

            let (r_drt, g_drt, b_drt) = match grading.display_transform {
                DisplayTransform::AgX => (
                    apply_agx_edr(r_toned, 1.0),
                    apply_agx_edr(g_toned, 1.0),
                    apply_agx_edr(b_toned, 1.0),
                ),
                DisplayTransform::OpenDrt => (
                    apply_opendrt(r_toned, 1.0),
                    apply_opendrt(g_toned, 1.0),
                    apply_opendrt(b_toned, 1.0),
                ),
            };

            let factor = 1.0 + (grading.contrast / 100.0).clamp(-0.8, 2.0);
            let r_c = ((r_drt - 0.18) * factor + 0.18).max(0.0);
            let g_c = ((g_drt - 0.18) * factor + 0.18).max(0.0);
            let b_c = ((b_drt - 0.18) * factor + 0.18).max(0.0);

            let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
            let max_val = r_c.max(g_c).max(b_c);
            let min_val = r_c.min(g_c).min(b_c);
            let current_sat = if max_val > 1e-4 {
                (max_val - min_val) / max_val
            } else {
                0.0
            };
            let base_vibrance = 0.10_f32;
            let user_vibrance = grading.vibrance / 100.0;
            let user_sat = grading.saturation / 100.0;
            let vib_weight = (1.0 - current_sat).clamp(0.0, 1.0);
            let eff_sat =
                ((1.0 + user_sat) * (1.0 + (base_vibrance + user_vibrance) * vib_weight)).max(0.0);

            let r_s = (lum_c + (r_c - lum_c) * eff_sat).max(0.0);
            let g_s = (lum_c + (g_c - lum_c) * eff_sat).max(0.0);
            let b_s = (lum_c + (b_c - lum_c) * eff_sat).max(0.0);

            let map_f32 = |x: f32| -> f32 {
                let clamped = x.clamp(0.0, 1.0);
                let toe = if clamped < 0.35 {
                    clamped.powf(1.08)
                } else {
                    clamped
                };
                toe.powf(1.0 / 2.2)
            };

            let (f_r, f_g, f_b) = if grading.is_monochrome {
                let lum_val = map_f32(0.2126 * r_s + 0.7152 * g_s + 0.0722 * b_s);
                (lum_val, lum_val, lum_val)
            } else {
                (map_f32(r_s), map_f32(g_s), map_f32(b_s))
            };

            u8_bytes.push((f_r * 255.0).clamp(0.0, 255.0) as u8);
            u8_bytes.push((f_g * 255.0).clamp(0.0, 255.0) as u8);
            u8_bytes.push((f_b * 255.0).clamp(0.0, 255.0) as u8);

            u16_samples.push((f_r * 65535.0).clamp(0.0, 65535.0).round() as u16);
            u16_samples.push((f_g * 65535.0).clamp(0.0, 65535.0).round() as u16);
            u16_samples.push((f_b * 65535.0).clamp(0.0, 65535.0).round() as u16);
        }
    }

    (out_w, out_h, u8_bytes, u16_samples)
}

/// Convenience helper evaluating preview and histogram directly from CFA.
#[must_use]
pub fn generate_preview_image(
    cfa: &SignedCfaBuffer,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let base = BaseRgbPreview::from_cfa(cfa);
    render_preview_from_base(&base, grading)
}

/// Scans a filesystem directory for supported RAW and raster image files.
#[must_use]
pub fn scan_directory_for_images(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };

    const EXTENSIONS: &[&str] = &[
        "dng", "arw", "cr2", "cr3", "nef", "raf", "rw2", "orf", "pef", "jpg", "jpeg", "png", "tif",
        "tiff",
    ];

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_ascii_lowercase();
                if EXTENSIONS.contains(&ext_lower.as_str()) {
                    entries.push(path);
                }
            }
        }
    }

    entries.sort();
    entries
}

/// Cached photo entry kept in memory for instantaneous (< 0.1 ms) switching between photos.
#[derive(Debug, Clone)]
pub struct CachedPhotoEntry {
    pub path: PathBuf,
    pub photo_view: LoadedPhotoView,
    pub base_preview: BaseRgbPreview,
    pub cfa: SignedCfaBuffer,
}

/// Payload sent from the background decoding thread to the main UI thread.
pub struct BackgroundDecodePayload {
    pub path: PathBuf,
    pub photo_view: LoadedPhotoView,
    pub base_preview: BaseRgbPreview,
    pub cfa: SignedCfaBuffer,
    pub is_prefetch: bool,
}

/// Decodes a RAW image file from disk and precomputes its base RGB preview.
fn decode_photo_file_payload(
    target_path: &Path,
    is_prefetch: bool,
) -> Option<BackgroundDecodePayload> {
    let decoder = PureRustRawDecoder::new();
    let decoded = decoder.decode_file(target_path).ok()?;
    let meta = decoded.metadata;
    let make = meta.camera_make.clone();
    let model = meta.camera_model.clone();
    let make_model = format!("{make} {model}");
    let camera_str = make_model.trim().to_string();
    let camera = if camera_str.is_empty() {
        "Digital RAW Camera".to_string()
    } else {
        camera_str
    };

    let iso = meta.iso.unwrap_or(100);
    let aperture_str = meta
        .aperture_f_number
        .map(|f| format!("f/{f:.1}"))
        .unwrap_or_else(|| "f/2.8".to_string());
    let focal_str = meta
        .focal_length_mm
        .map(|f| format!("{f:.1} mm"))
        .unwrap_or_default();
    let lens_str = meta.lens_model.clone().unwrap_or_default();

    let shutter_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Shutter Speed")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "1/125 s".to_string());

    let date_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Date/Time Original")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let bias_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Exposure Bias")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let photo_view = LoadedPhotoView {
        path: target_path.to_path_buf(),
        camera_make: make,
        camera_model: camera,
        lens_model: lens_str,
        dimensions: (meta.width as u32, meta.height as u32),
        iso,
        shutter_str,
        aperture_str,
        focal_length_str: focal_str,
        date_time: date_str,
        exposure_bias: bias_str,
        rating: meta.rating,
        exif_tags: decoded.exif_tags,
    };

    let base = BaseRgbPreview::from_cfa_with_color(
        &decoded.cfa_signed,
        [
            decoded.wb_coeffs[0],
            decoded.wb_coeffs[1],
            decoded.wb_coeffs[2],
        ],
        decoded.color_matrix.as_ref(),
    );

    Some(BackgroundDecodePayload {
        path: target_path.to_path_buf(),
        photo_view,
        base_preview: base,
        cfa: decoded.cfa_signed,
        is_prefetch,
    })
}

/// Native interactive application struct implementing `eframe::App`.
pub struct RrawApp {
    /// Active macOS EDR headroom provider or SDR fallback.
    headroom_provider: Box<dyn DisplayHeadroomProvider>,
    /// Darkroom development parameters.
    pub grading: DarkroomGradingState,
    /// Currently active development preset.
    pub current_preset: GradingPreset,
    /// Persistent layout order for right inspector sections.
    pub layout_config: AppLayoutConfig,
    /// Currently loaded photograph metadata, if any.
    pub loaded_photo: Option<LoadedPhotoView>,
    /// Decoded signed CFA buffer in RAM.
    pub cached_cfa: Option<SignedCfaBuffer>,
    /// Pre-demosaiced linear RGB base preview for zero-latency slider scrubbing.
    pub base_preview: Option<BaseRgbPreview>,
    /// MRU in-memory cache of previously decoded photos for instant switching.
    pub photo_cache: Vec<CachedPhotoEntry>,
    /// Cached GPU texture handle for rendering in egui viewport.
    pub preview_texture: Option<egui::TextureHandle>,
    /// Pre-computed live RGB histogram data.
    pub histogram: HistogramData,
    /// Flag signaling that preview texture must be re-rendered.
    pub needs_preview_update: bool,
    /// Channel sender for dispatching background decode jobs.
    pub decode_sender: Sender<BackgroundDecodePayload>,
    /// Channel receiver for receiving background decode results on main UI thread.
    pub decode_receiver: Receiver<BackgroundDecodePayload>,
    /// True while a background decode task is in progress.
    pub is_decoding: bool,
    /// Currently selected directory in file explorer.
    pub current_directory: Option<PathBuf>,
    /// Image files discovered in `current_directory`.
    pub directory_entries: Vec<PathBuf>,
    /// Active inspector navigation tab.
    pub inspector_tab: InspectorTab,
    /// Active navigation tab in left panel.
    pub left_panel_tab: LeftPanelTab,
    /// Whether the leveling line tool is active.
    pub horizon_tool_active: bool,
    /// Drag start point for leveling line tool.
    pub horizon_start_point: Option<egui::Pos2>,
    /// Last computed fit-to-viewport scale.
    pub last_fit_scale: f32,
    /// Active interactive handle being dragged for cropping.
    pub crop_drag_handle: CropDragHandle,
    /// Viewport zoom scale: 0.0 = Fit to viewport, 1.0 = 100% 1:1 pixel mapping, etc.
    pub zoom_level: f32,
    /// Viewport pan offset in logical points.
    pub pan_offset: egui::Vec2,
    /// Status message displayed on the bottom bar.
    pub status_message: String,
    /// Surface format invariant: always Rgba16Float.
    pub surface_format: SurfaceFormat,
    /// Per-photo darkroom grading state persisted by photo path.
    pub photo_gradings: HashMap<PathBuf, DarkroomGradingState>,
    /// Per-photo star ratings persisted by photo path (0 to 5).
    pub photo_ratings: HashMap<PathBuf, u8>,
    /// Active rating filter applied to bottom filmstrip.
    pub rating_filter: RatingFilter,
    /// Loaded GPU textures for filmstrip thumbnails.
    pub thumbnail_textures: HashMap<PathBuf, egui::TextureHandle>,
    /// Background sender for asynchronous thumbnail generation.
    pub thumbnail_sender: Sender<(PathBuf, egui::ColorImage)>,
    /// Background receiver for completed thumbnails.
    pub thumbnail_receiver: Receiver<(PathBuf, egui::ColorImage)>,
    /// Request sender to background thumbnail worker queue.
    pub thumbnail_req_tx: Sender<PathBuf>,
    /// Set of thumbnail paths currently being generated in background.
    pub thumbnail_pending: HashSet<PathBuf>,
    /// Whether the export modal dialog is open.
    pub show_export_dialog: bool,
    /// Selected output format for photo export.
    pub export_format: ExportFormatChoice,
    /// JPEG quality parameter (1-100).
    pub export_jpeg_quality: u8,
    /// Bit depth for raster export (8-bit or 16-bit).
    pub export_bit_depth: ExportBitDepth,
    /// Scaling mode applied during export.
    pub export_scale_mode: ExportScaleMode,
    /// Custom export destination folder, if chosen by user.
    pub export_custom_dir: Option<PathBuf>,
    /// Status message from latest export operation: (message, is_success).
    pub export_status: Option<(String, bool)>,
    /// Phase of the interactive 2-parallel-lines perspective correction tool.
    pub parallel_tool_phase: ParallelToolPhase,
    /// First user-drawn reference line for parallel alignment.
    pub parallel_line1: Option<(egui::Pos2, egui::Pos2)>,
    /// Second user-drawn reference line for parallel alignment.
    pub parallel_line2: Option<(egui::Pos2, egui::Pos2)>,
    /// Pointer start coordinate when dragging a parallel reference line.
    pub parallel_drag_start: Option<egui::Pos2>,
    /// Whether the crop frame is actively being manipulated in the central viewport.
    pub is_crop_editing: bool,
    /// Set of photos selected for batch operations in the filmstrip.
    pub selected_photos: HashSet<PathBuf>,
    /// Whether to create a subfolder in the photo directory upon export.
    pub export_use_subfolder: bool,
    /// Name of the subfolder for exported files.
    pub export_subfolder: String,
    /// Suffix appended to the exported filename.
    pub export_suffix: String,
    /// Whether export applies to all selected photos in batch or current photo.
    pub export_batch_selection: bool,
    /// Last selected index in filmstrip for Shift-range selection.
    pub last_selected_index: Option<usize>,
    /// Search query for filtering EXIF metadata tags table.
    pub exif_search_query: String,
}

impl RrawApp {
    /// Initializes a new interactive application instance.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(target_os = "macos")]
        let headroom_provider: Box<dyn DisplayHeadroomProvider> =
            Box::new(MacOsDisplayHeadroomProvider);
        #[cfg(not(target_os = "macos"))]
        let headroom_provider: Box<dyn DisplayHeadroomProvider> =
            Box::new(SdrFallbackHeadroomProvider);

        let (decode_sender, decode_receiver) = channel();
        let (thumbnail_sender, thumbnail_receiver) = channel();
        let (thumbnail_req_tx, thumbnail_req_rx) = channel::<PathBuf>();

        // Dedicated background thumbnail generator workers (pooled to avoid CPU saturation)
        let req_rx_arc = std::sync::Arc::new(std::sync::Mutex::new(thumbnail_req_rx));
        for _ in 0..2 {
            let rx = std::sync::Arc::clone(&req_rx_arc);
            let tx = thumbnail_sender.clone();
            std::thread::spawn(move || loop {
                let path = {
                    let Ok(guard) = rx.lock() else { break };
                    match guard.recv() {
                        Ok(p) => p,
                        Err(_) => break,
                    }
                };
                if let Some(img) = generate_fast_thumbnail(&path) {
                    let _ = tx.send((path, img));
                }
            });
        }

        // Default folder to user's Desktop as requested
        let default_dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join("Desktop"))
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let default_entries = scan_directory_for_images(&default_dir);

        let mut app = Self {
            headroom_provider,
            grading: DarkroomGradingState::default(),
            current_preset: GradingPreset::AsShot,
            layout_config: AppLayoutConfig::load(),
            loaded_photo: None,
            cached_cfa: None,
            base_preview: None,
            photo_cache: Vec::new(),
            preview_texture: None,
            histogram: HistogramData::default(),
            needs_preview_update: false,
            decode_sender,
            decode_receiver,
            is_decoding: false,
            current_directory: Some(default_dir),
            directory_entries: default_entries,
            inspector_tab: InspectorTab::Develop,
            left_panel_tab: LeftPanelTab::Explorer,
            horizon_tool_active: false,
            horizon_start_point: None,
            last_fit_scale: 1.0,
            crop_drag_handle: CropDragHandle::None,
            zoom_level: 0.0,
            pan_offset: egui::Vec2::ZERO,
            status_message: "Ready | 120 Hz Metal EDR Surface | Format: Rgba16Float".to_string(),
            surface_format: SurfaceFormat::Rgba16Float,
            photo_gradings: HashMap::new(),
            photo_ratings: HashMap::new(),
            rating_filter: RatingFilter::All,
            thumbnail_textures: HashMap::new(),
            thumbnail_sender,
            thumbnail_receiver,
            thumbnail_req_tx,
            thumbnail_pending: HashSet::new(),
            show_export_dialog: false,
            export_format: ExportFormatChoice::Jpeg,
            export_jpeg_quality: 92,
            export_bit_depth: ExportBitDepth::Bits8,
            export_scale_mode: ExportScaleMode::Original,
            export_custom_dir: None,
            export_status: None,
            parallel_tool_phase: ParallelToolPhase::Inactive,
            parallel_line1: None,
            parallel_line2: None,
            parallel_drag_start: None,
            is_crop_editing: false,
            selected_photos: HashSet::new(),
            export_use_subfolder: true,
            export_subfolder: "Export".to_string(),
            export_suffix: "_rraw".to_string(),
            export_batch_selection: false,
            last_selected_index: None,
            exif_search_query: String::new(),
        };

        // Open the first photo in default directory if available
        if let Some(first) = app.directory_entries.first().cloned() {
            app.open_raw_file(&first);
        }

        app
    }

    /// Queries the current active display headroom (e.g. 1.0 on SDR, up to 4.0 or 16.0 on EDR).
    #[must_use]
    pub fn current_headroom(&self) -> f32 {
        self.headroom_provider.current_headroom().active_headroom()
    }

    /// Updates the main viewport texture and histogram from the active base preview.
    pub fn update_preview_textures(&mut self, ctx: &egui::Context) {
        if let Some(base) = &self.base_preview {
            let (preview, hist) = render_preview_from_base(base, &self.grading);
            if let Some(tex) = &mut self.preview_texture {
                tex.set(preview, egui::TextureOptions::LINEAR);
            } else {
                self.preview_texture =
                    Some(ctx.load_texture("raw_preview", preview, egui::TextureOptions::LINEAR));
            }
            self.histogram = hist;
        }
        self.needs_preview_update = false;
    }

    /// Persists current photo grading and rating to both `.rraw.json` and `.xmp` sidecars.
    pub fn save_current_photo_sidecar(&self) {
        if let Some(photo) = &self.loaded_photo {
            let rating = self
                .photo_ratings
                .get(&photo.path)
                .copied()
                .unwrap_or(photo.rating);
            save_photo_sidecars(&photo.path, &self.grading, rating);
        }
    }

    fn write_exported_image(
        &self,
        target_path: &Path,
        out_w: usize,
        out_h: usize,
        u8_bytes: &[u8],
        u16_samples: &[u16],
    ) -> Result<(), String> {
        match self.export_format {
            ExportFormatChoice::Jpeg => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                let quality = self.export_jpeg_quality.clamp(1, 100);
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality)
                    .write_image(
                        u8_bytes,
                        out_w as u32,
                        out_h as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| format!("Erreur d'encodage JPEG: {e}"))?;
            }
            ExportFormatChoice::Png => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                match self.export_bit_depth {
                    ExportBitDepth::Bits16 => {
                        let mut encoder =
                            png::Encoder::new(&mut writer, out_w as u32, out_h as u32);
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Sixteen);
                        let mut png_writer = encoder
                            .write_header()
                            .map_err(|e| format!("Erreur header PNG 16: {e}"))?;
                        let mut raw_u16_bytes = vec![0_u8; out_w * out_h * 3 * 2];
                        for (i, &sample) in u16_samples.iter().enumerate() {
                            BigEndian::write_u16(&mut raw_u16_bytes[i * 2..i * 2 + 2], sample);
                        }
                        png_writer
                            .write_image_data(&raw_u16_bytes)
                            .map_err(|e| format!("Erreur d'écriture PNG 16: {e}"))?;
                    }
                    ExportBitDepth::Bits8 => {
                        let mut encoder =
                            png::Encoder::new(&mut writer, out_w as u32, out_h as u32);
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Eight);
                        let mut png_writer = encoder
                            .write_header()
                            .map_err(|e| format!("Erreur header PNG 8: {e}"))?;
                        png_writer
                            .write_image_data(u8_bytes)
                            .map_err(|e| format!("Erreur d'écriture PNG 8: {e}"))?;
                    }
                }
            }
            ExportFormatChoice::Tiff => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                match self.export_bit_depth {
                    ExportBitDepth::Bits16 => {
                        image::codecs::tiff::TiffEncoder::new(&mut writer)
                            .write_image(
                                bytemuck::cast_slice(u16_samples),
                                out_w as u32,
                                out_h as u32,
                                image::ExtendedColorType::Rgb16,
                            )
                            .map_err(|e| format!("Erreur d'encodage TIFF 16: {e}"))?;
                    }
                    ExportBitDepth::Bits8 => {
                        image::codecs::tiff::TiffEncoder::new(&mut writer)
                            .write_image(
                                u8_bytes,
                                out_w as u32,
                                out_h as u32,
                                image::ExtendedColorType::Rgb8,
                            )
                            .map_err(|e| format!("Erreur d'encodage TIFF 8: {e}"))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Executes photographic export of the active or selected images with all darkroom corrections,
    /// cropping, scaling, and the user-selected format & compression level.
    pub fn execute_export(&mut self) -> Result<PathBuf, String> {
        let photos_to_export: Vec<PathBuf> = if self.export_batch_selection {
            if self.selected_photos.is_empty() {
                if let Some(loaded) = &self.loaded_photo {
                    vec![loaded.path.clone()]
                } else {
                    return Err("Aucune photo sélectionnée pour l'export par lot.".to_string());
                }
            } else {
                let mut sorted: Vec<PathBuf> = self.selected_photos.iter().cloned().collect();
                sorted.sort();
                sorted
            }
        } else if let Some(loaded) = &self.loaded_photo {
            vec![loaded.path.clone()]
        } else {
            return Err("Aucune photo chargée à exporter.".to_string());
        };

        let mut last_exported_path = None;
        let mut count = 0;

        for photo_path in &photos_to_export {
            let is_loaded = self.loaded_photo.as_ref().map(|p| &p.path) == Some(photo_path);
            let (base_preview, grading) = if is_loaded {
                let base = match &self.cached_cfa {
                    Some(cfa) => {
                        let target_width = match self.export_scale_mode {
                            ExportScaleMode::Original => cfa.width().saturating_div(2).max(1200),
                            ExportScaleMode::Percent75 => (cfa.width() * 3 / 8).max(900),
                            ExportScaleMode::Percent50 => (cfa.width() / 4).max(600),
                            ExportScaleMode::FitLongEdge2048 => 2048,
                            ExportScaleMode::FitLongEdge3840 => 3840,
                        };
                        let as_shot_wb = self
                            .base_preview
                            .as_ref()
                            .map(|b| b.as_shot_wb)
                            .unwrap_or([2.0, 1.0, 1.5]);
                        BaseRgbPreview::from_cfa_with_color_and_target_width(
                            cfa,
                            as_shot_wb,
                            None,
                            target_width,
                        )
                    }
                    None => match &self.base_preview {
                        Some(b) => b.clone(),
                        None => continue,
                    },
                };
                (base, self.grading.clone())
            } else if let Some(cached) = self.photo_cache.iter().find(|e| &e.path == photo_path) {
                let grading = self
                    .photo_gradings
                    .get(photo_path)
                    .cloned()
                    .unwrap_or_else(|| load_photo_sidecars(photo_path).0.unwrap_or_default());
                (cached.base_preview.clone(), grading)
            } else if let Some(payload) = decode_photo_file_payload(photo_path, false) {
                let grading = self
                    .photo_gradings
                    .get(photo_path)
                    .cloned()
                    .unwrap_or_else(|| load_photo_sidecars(photo_path).0.unwrap_or_default());
                (payload.base_preview, grading)
            } else {
                continue;
            };

            let (out_w, out_h, u8_bytes, u16_samples) =
                render_export_image(&base_preview, &grading, 1.0);

            let base_dest = self.export_custom_dir.clone().unwrap_or_else(|| {
                photo_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
            });

            let final_dest =
                if self.export_use_subfolder && !self.export_subfolder.trim().is_empty() {
                    base_dest.join(self.export_subfolder.trim())
                } else {
                    base_dest
                };

            let _ = std::fs::create_dir_all(&final_dest);

            let stem = photo_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let ext = self.export_format.extension();
            let suffix = &self.export_suffix;
            let target_path = final_dest.join(format!("{stem}{suffix}.{ext}"));

            self.write_exported_image(&target_path, out_w, out_h, &u8_bytes, &u16_samples)?;
            last_exported_path = Some(target_path);
            count += 1;
        }

        if count == 0 {
            return Err("Aucune photo n'a pu être exportée.".to_string());
        }

        self.save_current_photo_sidecar();

        last_exported_path.ok_or_else(|| "Erreur lors de l'export".to_string())
    }

    /// Renders the export configuration dialog window.
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_export_dialog {
            return;
        }

        let mut is_open = self.show_export_dialog;
        let mut trigger_export = false;
        let mut close_dialog = false;

        egui::Window::new("📤 Exporter")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading("Cible d'export");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.export_batch_selection, false, "Photo courante");
                    let sel_count = self.selected_photos.len();
                    let batch_label = format!(
                        "Sélection par lot ({} photo{})",
                        sel_count,
                        if sel_count > 1 { "s" } else { "" }
                    );
                    ui.radio_value(&mut self.export_batch_selection, true, batch_label);
                });

                if !self.export_batch_selection {
                    if let Some(photo) = &self.loaded_photo {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Photo active :").strong());
                            if let Some(name) = photo.path.file_name().and_then(|n| n.to_str()) {
                                ui.label(
                                    egui::RichText::new(name).color(Color32::from_rgb(100, 180, 255)),
                                );
                            }
                        });
                        let crop_text = if self.grading.crop_enabled {
                            " (Recadrage actif appliqué)"
                        } else {
                            ""
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "Définition capteur : {} × {} px{crop_text}",
                                photo.dimensions.0, photo.dimensions.1
                            ))
                            .small()
                            .color(Color32::LIGHT_GRAY),
                        );
                    }
                } else if self.selected_photos.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "⚠️ Aucune photo cochée dans les vignettes. Seule la photo active sera exportée.",
                        )
                        .small()
                        .color(Color32::from_rgb(255, 180, 50)),
                    );
                }

                ui.separator();
                ui.heading("Format de sortie & Compression");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Jpeg, "JPEG");
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Png, "PNG");
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Tiff, "TIFF");
                });

                ui.add_space(4.0);
                match self.export_format {
                    ExportFormatChoice::Jpeg => {
                        ui.horizontal(|ui| {
                            ui.label("Qualité JPEG :");
                            ui.add(
                                egui::Slider::new(&mut self.export_jpeg_quality, 1..=100)
                                    .suffix("%"),
                            );
                        });
                        let quality_desc = if self.export_jpeg_quality >= 92 {
                            "Qualité maximale (Recommandé pour tirage / archivage)"
                        } else if self.export_jpeg_quality >= 80 {
                            "Qualité équilibrée (Idéal pour le web et les réseaux)"
                        } else {
                            "Forte compression (Taille de fichier minimale)"
                        };
                        ui.label(egui::RichText::new(quality_desc).small().color(Color32::GRAY));
                    }
                    ExportFormatChoice::Png => {
                        ui.horizontal(|ui| {
                            ui.label("Profondeur :");
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits8,
                                "8-bit sRGB (Standard)",
                            );
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits16,
                                "16-bit (Master sans perte)",
                            );
                        });
                    }
                    ExportFormatChoice::Tiff => {
                        ui.horizontal(|ui| {
                            ui.label("Profondeur :");
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits8,
                                "8-bit",
                            );
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits16,
                                "16-bit (Studio)",
                            );
                        });
                    }
                }

                ui.separator();
                ui.heading("Redimensionnement");
                egui::ComboBox::from_id_salt("export_scale_combo")
                    .selected_text(self.export_scale_mode.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Original,
                            ExportScaleMode::Original.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Percent75,
                            ExportScaleMode::Percent75.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Percent50,
                            ExportScaleMode::Percent50.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::FitLongEdge2048,
                            ExportScaleMode::FitLongEdge2048.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::FitLongEdge3840,
                            ExportScaleMode::FitLongEdge3840.label(),
                        );
                    });

                ui.separator();
                ui.heading("Dossier de destination & Nom");
                ui.horizontal(|ui| {
                    let dest_label = if let Some(dir) = &self.export_custom_dir {
                        dir.to_string_lossy().to_string()
                    } else if let Some(photo) = &self.loaded_photo {
                        photo
                            .path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Dossier source".to_string())
                    } else {
                        "Dossier source".to_string()
                    };
                    ui.label(egui::RichText::new(dest_label).small().monospace());
                    if ui.button("📁 Parcourir...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.export_custom_dir = Some(folder);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.export_use_subfolder, "Créer un sous-dossier :");
                    if self.export_use_subfolder {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export_subfolder)
                                .desired_width(120.0),
                        );
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Suffixe de fichier :");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.export_suffix).desired_width(120.0),
                    );
                });

                if let Some((msg, success)) = &self.export_status {
                    ui.add_space(6.0);
                    let col = if *success {
                        Color32::from_rgb(100, 220, 120)
                    } else {
                        Color32::from_rgb(255, 100, 100)
                    };
                    ui.label(egui::RichText::new(msg).color(col));
                }

                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() {
                        close_dialog = true;
                    }
                    let export_btn_text = if self.export_batch_selection {
                        "🚀 Exporter le lot"
                    } else {
                        "🚀 Exporter l'image"
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(export_btn_text).strong(),
                            )
                            .fill(Color32::from_rgb(30, 110, 210)),
                        )
                        .clicked()
                    {
                        trigger_export = true;
                    }
                });
            });

        if close_dialog {
            is_open = false;
        }
        self.show_export_dialog = is_open;

        if trigger_export {
            match self.execute_export() {
                Ok(path) => {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
                    let status_txt = if self.export_batch_selection {
                        format!(
                            "✅ Export par lot réussi ({} fichier(s))",
                            self.selected_photos.len().max(1)
                        )
                    } else {
                        format!("✅ Export réussi : {file_name}")
                    };
                    self.status_message = status_txt.clone();
                    self.export_status = Some((status_txt, true));
                    // Automatically close dialog on success as requested by user
                    self.show_export_dialog = false;
                }
                Err(err) => {
                    self.export_status = Some((format!("❌ Échec : {err}"), false));
                }
            }
        }
    }

    /// Dispatches a thumbnail generation request to background worker pool if not already loaded or pending.
    pub fn request_thumbnail(&mut self, path: PathBuf) {
        if self.thumbnail_textures.contains_key(&path)
            || !self.thumbnail_pending.insert(path.clone())
        {
            return;
        }

        let _ = self.thumbnail_req_tx.send(path);
    }

    /// Changes the active directory in the file explorer and preloads sidecars.
    pub fn open_directory(&mut self, dir: &Path) {
        let abs_dir = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(dir))
                .unwrap_or_else(|_| dir.to_path_buf())
        };

        self.current_directory = Some(abs_dir.clone());
        self.directory_entries = scan_directory_for_images(&abs_dir);

        // Preload sidecars (.rraw.json & .xmp) without stalling CPU
        let entries = self.directory_entries.clone();
        for path in &entries {
            let (grading_opt, rating_opt) = load_photo_sidecars(path);
            if let Some(g) = grading_opt {
                self.photo_gradings.insert(path.clone(), g);
            }
            if let Some(r) = rating_opt {
                self.photo_ratings.insert(path.clone(), r);
            }
        }

        self.status_message = format!(
            "Directory: {} ({} photos found)",
            abs_dir.display(),
            self.directory_entries.len()
        );
    }

    /// Triggers background prefetching of adjacent photos (+1, +2, -1, -2) for instant switching.
    pub fn trigger_prefetch(&self, around_path: &Path) {
        if let Some(pos) = self.directory_entries.iter().position(|p| p == around_path) {
            let mut candidates = Vec::new();
            if pos + 1 < self.directory_entries.len() {
                candidates.push(self.directory_entries[pos + 1].clone());
            }
            if pos + 2 < self.directory_entries.len() {
                candidates.push(self.directory_entries[pos + 2].clone());
            }
            if pos > 0 {
                candidates.push(self.directory_entries[pos - 1].clone());
            }
            if pos > 1 {
                candidates.push(self.directory_entries[pos - 2].clone());
            }

            for path in candidates {
                if !self.photo_cache.iter().any(|e| e.path == path) {
                    let tx = self.decode_sender.clone();
                    std::thread::spawn(move || {
                        if let Some(payload) = decode_photo_file_payload(&path, true) {
                            let _ = tx.send(payload);
                        }
                    });
                }
            }
        }
    }

    /// Returns the number of photos in `directory_entries` with rating >= `min_stars`.
    pub fn count_photos_with_min_rating(&self, min_stars: u8) -> usize {
        self.directory_entries
            .iter()
            .filter(|p| {
                let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                rating >= min_stars
            })
            .count()
    }

    /// Returns the number of photos in `directory_entries` with 0 stars (unrated).
    pub fn count_photos_unrated(&self) -> usize {
        self.directory_entries
            .iter()
            .filter(|p| {
                let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                rating == 0
            })
            .count()
    }

    /// Sets the rating (0 to 5) of the currently loaded photo and persists to `.rraw.json` and `.xmp`.
    pub fn set_current_photo_rating(&mut self, rating: u8) {
        if let Some(photo) = &mut self.loaded_photo {
            photo.rating = rating;
            self.photo_ratings.insert(photo.path.clone(), rating);
            if let Some(entry) = self.photo_cache.iter_mut().find(|e| e.path == photo.path) {
                entry.photo_view.rating = rating;
            }

            // Save immediately to adjacent <photo>.rraw.json and <photo>.xmp
            save_photo_sidecars(&photo.path, &self.grading, rating);

            let star_text = if rating > 0 {
                "★".repeat(rating as usize)
            } else {
                "Unrated".to_string()
            };
            self.status_message = format!(
                "Rated {}: {} (Saved to .rraw.json & .xmp)",
                photo
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("photo"),
                star_text
            );
        }
    }

    /// Navigates relative to the current photo in the filtered filmstrip.
    pub fn navigate_relative(&mut self, delta: isize) {
        self.navigate_relative_with_shift(delta, false);
    }

    /// Navigates relative to current photo with optional Shift-range selection extension.
    pub fn navigate_relative_with_shift(&mut self, delta: isize, shift: bool) {
        let filtered: Vec<PathBuf> = self
            .directory_entries
            .iter()
            .filter(|p| {
                let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                self.rating_filter.matches(rating)
            })
            .cloned()
            .collect();

        if filtered.is_empty() {
            return;
        }

        let current_path = self.loaded_photo.as_ref().map(|p| &p.path);
        let current_idx = current_path
            .and_then(|cp| filtered.iter().position(|p| p == cp))
            .unwrap_or(0);

        let new_idx = (current_idx as isize + delta).rem_euclid(filtered.len() as isize) as usize;
        let target = filtered[new_idx].clone();

        if shift {
            let anchor = self.last_selected_index.unwrap_or(current_idx);
            let min_i = anchor.min(new_idx);
            let max_i = anchor.max(new_idx);
            for i in min_i..=max_i {
                if let Some(p) = filtered.get(i) {
                    self.selected_photos.insert(p.clone());
                }
            }
        } else {
            self.selected_photos.clear();
            self.selected_photos.insert(target.clone());
            self.last_selected_index = Some(new_idx);
        }

        self.open_raw_file(&target);
    }

    /// Decodes and loads a RAW file at `path`, utilizing in-memory MRU cache or async worker thread.
    pub fn open_raw_file(&mut self, path: &Path) {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        };

        // Automatically populate filmstrip with all photos in parent folder
        if let Some(parent) = abs_path.parent() {
            if !parent.as_os_str().is_empty() && self.current_directory.as_deref() != Some(parent) {
                self.open_directory(parent);
            }
        }

        // Save current photo's grading and sidecars in background before switching
        if let Some(loaded) = &self.loaded_photo {
            self.photo_gradings
                .insert(loaded.path.clone(), self.grading.clone());
            let rating = self
                .photo_ratings
                .get(&loaded.path)
                .copied()
                .unwrap_or(loaded.rating);
            let path_to_save = loaded.path.clone();
            let grading_to_save = self.grading.clone();
            std::thread::spawn(move || {
                save_photo_sidecars(&path_to_save, &grading_to_save, rating);
            });
        }

        // Reset zoom & pan to Fit on new photo open
        self.zoom_level = 0.0;
        self.pan_offset = egui::Vec2::ZERO;

        // Check sidecars on disk first
        let (disk_grading, disk_rating) = load_photo_sidecars(&abs_path);
        if let Some(r) = disk_rating {
            self.photo_ratings.insert(abs_path.clone(), r);
        }
        if let Some(g) = disk_grading.as_ref() {
            self.photo_gradings.insert(abs_path.clone(), g.clone());
        }

        // Check if photo is already cached in memory for instantaneous 0 ms switching
        if let Some(pos) = self
            .photo_cache
            .iter()
            .position(|entry| entry.path == abs_path)
        {
            let mut entry = self.photo_cache.remove(pos);
            if let Some(&rating) = self.photo_ratings.get(&abs_path) {
                entry.photo_view.rating = rating;
            }
            self.loaded_photo = Some(entry.photo_view.clone());
            self.base_preview = Some(entry.base_preview.clone());
            self.cached_cfa = Some(entry.cfa.clone());

            // Restore per-photo grading state (preferring disk sidecar if present)
            if let Some(g) = disk_grading {
                self.grading = g;
                self.current_preset = self.grading.preset;
            } else if let Some(saved) = self.photo_gradings.get(&abs_path) {
                self.grading = saved.clone();
                self.current_preset = self.grading.preset;
            } else {
                self.grading = DarkroomGradingState::default();
                self.current_preset = GradingPreset::AsShot;
            }

            self.status_message = format!("Loaded (cached) {}", entry.photo_view.camera_model);
            self.needs_preview_update = true;
            self.is_decoding = false;
            self.photo_cache.insert(0, entry);
            self.trigger_prefetch(&abs_path);
            return;
        }

        let file_name = abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("RAW")
            .to_string();

        self.status_message = format!("Decoding {}...", file_name);
        self.is_decoding = true;

        // Instant visual feedback: if thumbnail texture is already in memory, display it immediately!
        if let Some(thumb_tex) = self.thumbnail_textures.get(&abs_path) {
            self.preview_texture = Some(thumb_tex.clone());
        }

        let tx = self.decode_sender.clone();
        let target_path = abs_path;

        std::thread::spawn(move || {
            if let Some(payload) = decode_photo_file_payload(&target_path, false) {
                let _ = tx.send(payload);
            }
        });
    }

    /// Renders the folder explorer navigation in the left panel with breadcrumbs and `..` parent navigation.
    pub fn render_explorer_tab(&mut self, ui: &mut egui::Ui) {
        let dir_opt = self.current_directory.clone();
        let mut dir_to_open = None;

        ui.vertical(|ui| {
            if let Some(dir) = dir_opt {
                // Action row: Browse button & photo count
                ui.horizontal(|ui| {
                    if ui
                        .button("📂 Parcourir...")
                        .on_hover_text("Choisir un dossier avec le sélecteur natif macOS")
                        .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            dir_to_open = Some(folder);
                        }
                    }
                    let count = self.directory_entries.len();
                    ui.label(
                        egui::RichText::new(format!(
                            "{count} photo{}",
                            if count > 1 { "s" } else { "" }
                        ))
                        .small()
                        .color(Color32::from_rgb(100, 180, 255)),
                    );
                });

                ui.add_space(4.0);

                // Interactive Breadcrumb path navigation
                ui.horizontal_wrapped(|ui| {
                    let mut accum = PathBuf::new();
                    let components: Vec<_> = dir.components().collect();
                    for (i, comp) in components.iter().enumerate() {
                        accum.push(comp);
                        let name = comp.as_os_str().to_str().unwrap_or("/");
                        let display_name = if i == 0 && name == "/" { "🏠" } else { name };
                        let target = accum.clone();
                        if ui
                            .small_button(display_name)
                            .on_hover_text(target.display().to_string())
                            .clicked()
                        {
                            dir_to_open = Some(target);
                        }
                        if i + 1 < components.len() {
                            ui.label(egui::RichText::new("/").color(Color32::DARK_GRAY));
                        }
                    }
                });

                ui.add_space(6.0);
                ui.separator();

                // "📁 .." parent directory navigation entry at the top of the list
                if let Some(parent) = dir.parent() {
                    if !parent.as_os_str().is_empty() {
                        let btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new("📁 .. (Dossier parent)")
                                    .strong()
                                    .color(Color32::from_rgb(140, 190, 255)),
                            )
                            .frame(false),
                        );
                        if btn
                            .on_hover_text(format!("Remonter vers {}", parent.display()))
                            .clicked()
                        {
                            dir_to_open = Some(parent.to_path_buf());
                        }
                    }
                }

                // Subdirectories list
                let subdirs: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                    .map(|e| e.path())
                    .collect();

                if !subdirs.is_empty() {
                    ui.add_space(2.0);
                    for subdir in subdirs {
                        let name = subdir
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("folder");
                        let btn = ui.add(egui::Button::new(format!("📁 {name}")).frame(false));
                        if btn.clicked() {
                            dir_to_open = Some(subdir);
                        }
                    }
                }
            } else {
                ui.label("Aucun dossier ouvert.");
                if ui.button("📂 Parcourir un dossier...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        dir_to_open = Some(folder);
                    }
                }
            }
        });

        if let Some(d) = dir_to_open {
            self.open_directory(&d);
            if let Some(first) = self.directory_entries.first().cloned() {
                self.open_raw_file(&first);
            }
        }
    }

    /// Moves a darkroom section up or down in the persistent layout order.
    pub fn move_section(&mut self, idx: usize, up: bool) {
        if up && idx > 0 {
            self.layout_config.section_order.swap(idx, idx - 1);
            self.layout_config.save();
        } else if !up && idx + 1 < self.layout_config.section_order.len() {
            self.layout_config.section_order.swap(idx, idx + 1);
            self.layout_config.save();
        }
    }

    /// Renders a single collapsible section in the darkroom inspector.
    fn render_darkroom_section(&mut self, ui: &mut egui::Ui, section: DarkroomSection, idx: usize) {
        let is_first = idx == 0;
        let is_last = idx + 1 == self.layout_config.section_order.len();

        ui.horizontal(|ui| {
            ui.strong(section.title());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !is_last && ui.small_button("▼").on_hover_text("Move down").clicked() {
                    self.move_section(idx, false);
                }
                if !is_first && ui.small_button("▲").on_hover_text("Move up").clicked() {
                    self.move_section(idx, true);
                }
            });
        });

        match section {
            DarkroomSection::ExposureTone => {
                ui.label("Exposure (EV)");
                let r1 = ui.add(
                    egui::Slider::new(&mut self.grading.exposure_ev, -5.0..=5.0).show_value(true),
                );
                if r1.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Contrast");
                let r2 = ui.add(
                    egui::Slider::new(&mut self.grading.contrast, -100.0..=100.0).show_value(true),
                );
                if r2.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Highlights");
                let r3 = ui.add(
                    egui::Slider::new(&mut self.grading.highlights, -100.0..=100.0)
                        .show_value(true),
                );
                if r3.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Shadows");
                let r4 = ui.add(
                    egui::Slider::new(&mut self.grading.shadows, -100.0..=100.0).show_value(true),
                );
                if r4.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Noirs / Blacks");
                let r5 = ui.add(
                    egui::Slider::new(&mut self.grading.blacks, -100.0..=100.0).show_value(true),
                );
                if r5.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button("Reset Tones").clicked() {
                    self.grading.exposure_ev = 0.0;
                    self.grading.contrast = 0.0;
                    self.grading.highlights = 0.0;
                    self.grading.shadows = 0.0;
                    self.grading.blacks = 0.0;
                    self.grading.preset = GradingPreset::detect_preset(&self.grading);
                    self.current_preset = self.grading.preset;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::WhiteBalanceColor => {
                ui.label("Temperature (Kelvin)");
                let r_k = ui.add(
                    egui::Slider::new(&mut self.grading.kelvin, 2000.0..=12000.0).show_value(true),
                );
                if r_k.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Tint");
                let r_t = ui
                    .add(egui::Slider::new(&mut self.grading.tint, -50.0..=50.0).show_value(true));
                if r_t.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Vibrance");
                let r_vib = ui.add(
                    egui::Slider::new(&mut self.grading.vibrance, -100.0..=100.0).show_value(true),
                );
                if r_vib.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Saturation");
                let r_sat = ui.add(
                    egui::Slider::new(&mut self.grading.saturation, -100.0..=100.0)
                        .show_value(true),
                );
                if r_sat.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.horizontal(|ui| {
                    ui.label("Transform:");
                    egui::ComboBox::from_id_salt("drt_select")
                        .selected_text(format!("{:?}", self.grading.display_transform))
                        .show_ui(ui, |ui| {
                            let c1 = ui.selectable_value(
                                &mut self.grading.display_transform,
                                DisplayTransform::AgX,
                                "AgX (Default)",
                            );
                            let c2 = ui.selectable_value(
                                &mut self.grading.display_transform,
                                DisplayTransform::OpenDrt,
                                "OpenDRT",
                            );
                            if c1.changed() || c2.changed() {
                                self.grading.preset = GradingPreset::Custom;
                                self.current_preset = GradingPreset::Custom;
                                self.needs_preview_update = true;
                                ui.ctx().request_repaint();
                            }
                        });
                });

                let c_mono = ui.checkbox(&mut self.grading.is_monochrome, "Monochrome (B&W)");
                if c_mono.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button("Reset Color").clicked() {
                    self.grading.kelvin = 5500.0;
                    self.grading.tint = 0.0;
                    self.grading.vibrance = 0.0;
                    self.grading.saturation = 0.0;
                    self.grading.is_monochrome = false;
                    self.grading.preset = GradingPreset::detect_preset(&self.grading);
                    self.current_preset = self.grading.preset;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::Crop => {
                let c_crop = ui.checkbox(&mut self.grading.crop_enabled, "Activer le recadrage");
                if c_crop.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if self.grading.crop_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Format / Ratio :");
                        egui::ComboBox::from_id_salt("crop_ratio_select")
                            .selected_text(self.grading.crop_aspect.label())
                            .show_ui(ui, |ui| {
                                for ratio in CropAspectRatio::ALL {
                                    let is_sel = self.grading.crop_aspect == ratio;
                                    if ui.selectable_label(is_sel, ratio.label()).clicked() {
                                        self.grading.crop_aspect = ratio;
                                        let (orig_w, orig_h) = if let Some(photo) =
                                            &self.loaded_photo
                                        {
                                            (photo.dimensions.0 as f32, photo.dimensions.1 as f32)
                                        } else {
                                            (3.0, 2.0)
                                        };
                                        if let Some(target_r) = ratio.ratio(orig_w, orig_h) {
                                            let sensor_aspect = orig_w / orig_h.max(1.0);
                                            let norm_aspect = target_r / sensor_aspect;
                                            let (crop_w, crop_h) = if norm_aspect <= 1.0 {
                                                (norm_aspect, 1.0)
                                            } else {
                                                (1.0, 1.0 / norm_aspect)
                                            };
                                            self.grading.crop_left =
                                                ((1.0 - crop_w) * 0.5).clamp(0.0, 0.45);
                                            self.grading.crop_right =
                                                (self.grading.crop_left + crop_w).clamp(0.55, 1.0);
                                            self.grading.crop_top =
                                                ((1.0 - crop_h) * 0.5).clamp(0.0, 0.45);
                                            self.grading.crop_bottom =
                                                (self.grading.crop_top + crop_h).clamp(0.55, 1.0);
                                        }
                                        self.is_crop_editing = true;
                                        ui.ctx().request_repaint();
                                    }
                                }
                            });
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if self.is_crop_editing {
                            if ui
                                .button(
                                    egui::RichText::new("✅ Valider le recadrage")
                                        .strong()
                                        .color(Color32::from_rgb(100, 220, 120)),
                                )
                                .clicked()
                            {
                                self.is_crop_editing = false;
                                self.needs_preview_update = true;
                                self.save_current_photo_sidecar();
                                ui.ctx().request_repaint();
                            }
                        } else if ui
                            .button(
                                egui::RichText::new("✂️ Ajuster le recadrage (Souris)").strong(),
                            )
                            .clicked()
                        {
                            self.is_crop_editing = true;
                            ui.ctx().request_repaint();
                        }

                        if ui.button("↺ Réinitialiser").clicked() {
                            self.grading.crop_left = 0.0;
                            self.grading.crop_top = 0.0;
                            self.grading.crop_right = 1.0;
                            self.grading.crop_bottom = 1.0;
                            self.grading.crop_aspect = CropAspectRatio::Free;
                            self.is_crop_editing = false;
                            self.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }
                    });

                    let c_grid = ui.checkbox(
                        &mut self.grading.crop_show_grid,
                        "Afficher la grille des tiers",
                    );
                    if c_grid.changed() {
                        ui.ctx().request_repaint();
                    }

                    ui.label(
                        egui::RichText::new(
                            "💡 Ajustez le cadre à la souris sur la photo puis cliquez sur Valider.",
                        )
                        .small()
                        .color(Color32::GRAY),
                    );
                }
            }
            DarkroomSection::Straighten => {
                ui.horizontal(|ui| {
                    ui.label("Rotation:");
                    let r_rot = ui.add(
                        egui::Slider::new(&mut self.grading.rotation_degrees, -45.0..=45.0)
                            .step_by(0.1)
                            .suffix("°")
                            .show_value(true),
                    );
                    if r_rot.changed() {
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                    if ui.button("↺ 0°").clicked() {
                        self.grading.rotation_degrees = 0.0;
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                });

                let horizon_btn_text = if self.horizon_tool_active {
                    "📍 Horizon actif (Tracez sur la photo)"
                } else {
                    "📐 Tracer une ligne d'horizon"
                };
                let horizon_btn = ui.selectable_label(self.horizon_tool_active, horizon_btn_text);
                if horizon_btn.clicked() {
                    self.horizon_tool_active = !self.horizon_tool_active;
                    self.horizon_start_point = None;
                    if self.horizon_tool_active {
                        self.status_message =
                            "Glissez sur la photo pour aligner l'horizon".to_string();
                    }
                }

                ui.horizontal(|ui| {
                    if ui.button("↺ 90° CCW").clicked() {
                        self.grading.rotation_degrees =
                            (self.grading.rotation_degrees - 90.0).clamp(-45.0, 45.0);
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                    if ui.button("↻ 90° CW").clicked() {
                        self.grading.rotation_degrees =
                            (self.grading.rotation_degrees + 90.0).clamp(-45.0, 45.0);
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                    let c_fx = ui.checkbox(&mut self.grading.flip_horizontal, "↔ Miroir H");
                    let c_fy = ui.checkbox(&mut self.grading.flip_vertical, "↕ Miroir V");
                    if c_fx.changed() || c_fy.changed() {
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                });
            }
            DarkroomSection::Geometry => {
                let c_geom =
                    ui.checkbox(&mut self.grading.geometry_enabled, "Activer la géométrie");
                if c_geom.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if self.grading.geometry_enabled {
                    let is_parallel_active =
                        self.parallel_tool_phase != ParallelToolPhase::Inactive;
                    let parallel_btn_label = if is_parallel_active {
                        "📐 2 Lignes Parallèles (Actif - Glissez sur la photo)"
                    } else {
                        "📐 Tracer 2 lignes parallèles (Perspective)"
                    };
                    let btn = ui.selectable_label(is_parallel_active, parallel_btn_label);
                    if btn.clicked() {
                        if is_parallel_active {
                            self.parallel_tool_phase = ParallelToolPhase::Inactive;
                            self.parallel_line1 = None;
                            self.parallel_line2 = None;
                            self.parallel_drag_start = None;
                        } else {
                            self.parallel_tool_phase = ParallelToolPhase::DrawingLine1;
                            self.parallel_line1 = None;
                            self.parallel_line2 = None;
                            self.parallel_drag_start = None;
                            self.status_message =
                                "Glissez sur la photo pour tracer la 1ère référence verticale"
                                    .to_string();
                        }
                    }

                    ui.label("Keystone Vertical (Perspective verticale)");
                    let r_kv = ui.add(
                        egui::Slider::new(&mut self.grading.keystone_vertical, -50.0..=50.0)
                            .step_by(0.5)
                            .show_value(true),
                    );
                    if r_kv.changed() {
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    ui.label("Keystone Horizontal (Perspective horizontale)");
                    let r_kh = ui.add(
                        egui::Slider::new(&mut self.grading.keystone_horizontal, -50.0..=50.0)
                            .step_by(0.5)
                            .show_value(true),
                    );
                    if r_kh.changed() {
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    let c_ac = ui.checkbox(
                        &mut self.grading.auto_crop_geometry,
                        "Recadrage automatique anti-bords",
                    );
                    if c_ac.changed() {
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    if ui.button("Réinitialiser Géométrie").clicked() {
                        self.grading.keystone_vertical = 0.0;
                        self.grading.keystone_horizontal = 0.0;
                        self.grading.rotation_degrees = 0.0;
                        self.grading.flip_horizontal = false;
                        self.grading.flip_vertical = false;
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                }
            }
            DarkroomSection::DetailDenoise => {
                ui.label("Wavelet Denoise (σ)");
                let r_w = ui.add(
                    egui::Slider::new(&mut self.grading.wavelet_denoise_sigma, 0.0..=5.0)
                        .show_value(true),
                );
                if r_w.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Richardson-Lucy Sharpening");
                let r_s = ui.add(
                    egui::Slider::new(&mut self.grading.capture_sharpening_amount, 0.0..=2.0)
                        .show_value(true),
                );
                if r_s.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::OpticalCorrections => {
                let c_dist = ui.checkbox(
                    &mut self.grading.distortion_correction,
                    "Geometric Distortion (Lensfun/MakerNote)",
                );
                if c_dist.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Manual Distortion");
                let r_da = ui.add(
                    egui::Slider::new(&mut self.grading.distortion_amount, -50.0..=50.0)
                        .show_value(true),
                );
                if r_da.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.separator();

                let c_ca = ui.checkbox(
                    &mut self.grading.chromatic_aberration_correction,
                    "Lateral Chromatic Aberration",
                );
                if c_ca.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.separator();

                let c_vig = ui.checkbox(&mut self.grading.vignette_correction, "Radial Vignetting");
                if c_vig.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Vignette Correction Strength");
                let r_va = ui.add(
                    egui::Slider::new(&mut self.grading.vignette_amount, 0.0..=100.0)
                        .show_value(true),
                );
                if r_va.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button("Reset Optics").clicked() {
                    self.grading.distortion_correction = true;
                    self.grading.distortion_amount = 0.0;
                    self.grading.vignette_correction = true;
                    self.grading.vignette_amount = 50.0;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::LegacyOther => {}
        }

        if self.needs_preview_update {
            if let Some(photo) = &self.loaded_photo {
                self.photo_gradings
                    .insert(photo.path.clone(), self.grading.clone());
                let rating = self
                    .photo_ratings
                    .get(&photo.path)
                    .copied()
                    .unwrap_or(photo.rating);
                save_photo_sidecars(&photo.path, &self.grading, rating);
            }
        }

        ui.separator();
    }

    /// Renders the presets dropdown list and quick controls in the left panel.
    pub fn render_presets_section(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Style :").strong().size(12.0));
                egui::ComboBox::from_id_salt("left_panel_preset_combo")
                    .selected_text(self.current_preset.name())
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for preset in GradingPreset::ALL {
                            if preset == GradingPreset::Custom {
                                continue;
                            }
                            let is_sel = self.current_preset == preset;
                            if ui.selectable_label(is_sel, preset.name()).clicked() {
                                self.current_preset = preset;
                                preset.apply(&mut self.grading);
                                self.needs_preview_update = true;
                                self.save_current_photo_sidecar();
                                ui.ctx().request_repaint();
                            }
                        }
                    });

                if ui
                    .small_button("↺ As Shot")
                    .on_hover_text("Réinitialiser aux réglages boîtier d'origine")
                    .clicked()
                {
                    GradingPreset::AsShot.apply(&mut self.grading);
                    self.current_preset = GradingPreset::AsShot;
                    self.needs_preview_update = true;
                    self.save_current_photo_sidecar();
                    ui.ctx().request_repaint();
                }
            });

            if self.current_preset != GradingPreset::AsShot
                && self.current_preset != GradingPreset::Custom
            {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(self.current_preset.description())
                        .small()
                        .italics()
                        .color(Color32::from_rgb(180, 190, 205)),
                );
            }
        });
    }

    /// Renders the presets selection tab (delegates to dropdown section).
    pub fn render_presets_tab(&mut self, ui: &mut egui::Ui) {
        self.render_presets_section(ui);
    }

    /// Renders the complete, clear EXIF table in the left panel.
    pub fn render_exif_panel(&mut self, ui: &mut egui::Ui) {
        let Some(photo) = self.loaded_photo.clone() else {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Aucune photo RAW chargée.")
                    .small()
                    .italics()
                    .color(Color32::GRAY),
            );
            return;
        };

        ui.vertical(|ui| {
            // Summary Table in clean striped grid
            egui::Grid::new("exif_left_panel_grid")
                .striped(true)
                .min_col_width(110.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Appareil :").strong().size(12.0));
                    ui.label(egui::RichText::new(&photo.camera_model).size(12.0));
                    ui.end_row();

                    ui.label(egui::RichText::new("Marque :").strong().size(12.0));
                    ui.label(egui::RichText::new(&photo.camera_make).size(12.0));
                    ui.end_row();

                    if !photo.lens_model.is_empty() {
                        ui.label(egui::RichText::new("Objectif :").strong().size(12.0));
                        ui.label(egui::RichText::new(&photo.lens_model).size(12.0));
                        ui.end_row();
                    }

                    ui.label(egui::RichText::new("Dimensions :").strong().size(12.0));
                    ui.label(
                        egui::RichText::new(format!(
                            "{} × {} ({:.1} MP)",
                            photo.dimensions.0,
                            photo.dimensions.1,
                            (photo.dimensions.0 as f32 * photo.dimensions.1 as f32) / 1_000_000.0
                        ))
                        .size(12.0),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Vitesse :").strong().size(12.0));
                    ui.label(
                        egui::RichText::new(&photo.shutter_str)
                            .size(12.0)
                            .color(Color32::from_rgb(100, 200, 255)),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Ouverture :").strong().size(12.0));
                    ui.label(
                        egui::RichText::new(&photo.aperture_str)
                            .size(12.0)
                            .color(Color32::from_rgb(255, 180, 100)),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Sensibilité :").strong().size(12.0));
                    ui.label(
                        egui::RichText::new(format!("ISO {}", photo.iso))
                            .size(12.0)
                            .color(Color32::from_rgb(120, 225, 140)),
                    );
                    ui.end_row();

                    if !photo.focal_length_str.is_empty() {
                        ui.label(egui::RichText::new("Focale :").strong().size(12.0));
                        ui.label(egui::RichText::new(&photo.focal_length_str).size(12.0));
                        ui.end_row();
                    }

                    if !photo.exposure_bias.is_empty() {
                        ui.label(
                            egui::RichText::new("Biais d'exposition :")
                                .strong()
                                .size(12.0),
                        );
                        ui.label(egui::RichText::new(&photo.exposure_bias).size(12.0));
                        ui.end_row();
                    }

                    if !photo.date_time.is_empty() {
                        ui.label(egui::RichText::new("Date de capture :").strong().size(12.0));
                        ui.label(egui::RichText::new(&photo.date_time).size(12.0));
                        ui.end_row();
                    }
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.small_button("📋 Copier EXIF").clicked() {
                    ui.ctx().copy_text(photo.export_exif_json());
                    self.status_message = "Métadonnées EXIF copiées au presse-papier !".to_string();
                }
                if ui.small_button("📥 Exporter (.exif.json)").clicked() {
                    let out_path = photo.path.with_extension("exif.json");
                    let json = photo.export_exif_json();
                    if std::fs::write(&out_path, json).is_ok() {
                        self.status_message = format!("EXIF exporté vers {}", out_path.display());
                    }
                }
            });

            // Detailed all EXIF tags list
            if !photo.exif_tags.is_empty() {
                ui.add_space(6.0);
                egui::CollapsingHeader::new(
                    egui::RichText::new(format!(
                        "🔍 Tous les tags EXIF détaillés ({})",
                        photo.exif_tags.len()
                    ))
                    .strong()
                    .size(12.0)
                    .color(Color32::from_rgb(200, 215, 235)),
                )
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🔎").size(11.0));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.exif_search_query)
                                .hint_text("Filtrer les tags...")
                                .desired_width(140.0),
                        );
                        if !self.exif_search_query.is_empty() && ui.small_button("✖").clicked() {
                            self.exif_search_query.clear();
                        }
                    });

                    ui.add_space(4.0);
                    let query = self.exif_search_query.to_lowercase();
                    let filtered_tags: Vec<&(String, String)> = photo
                        .exif_tags
                        .iter()
                        .filter(|(tag, val)| {
                            query.is_empty()
                                || tag.to_lowercase().contains(&query)
                                || val.to_lowercase().contains(&query)
                        })
                        .collect();

                    if filtered_tags.is_empty() {
                        ui.label(
                            egui::RichText::new("Aucun tag correspondant.")
                                .small()
                                .italics()
                                .color(Color32::GRAY),
                        );
                    } else {
                        egui::Grid::new("exif_all_tags_grid")
                            .striped(true)
                            .min_col_width(110.0)
                            .show(ui, |ui| {
                                for (tag, val) in filtered_tags {
                                    ui.label(egui::RichText::new(tag).strong().size(11.0));
                                    ui.label(egui::RichText::new(val).size(11.0));
                                    ui.end_row();
                                }
                            });
                    }
                });
            }
        });
    }

    /// Renders an at-a-glance EXIF summary card for backwards compatibility.
    pub fn render_exif_compact_card(&mut self, ui: &mut egui::Ui) {
        self.render_exif_panel(ui);
    }

    /// Renders an embellished live RGB / Luminance histogram waveform widget inside `ui`.
    pub fn render_histogram_widget(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("📊 Waveform RGB").strong().size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.histogram.highlight_clipped > 0 {
                        ui.label(
                            egui::RichText::new(format!(
                                "⬜ HL ({})",
                                self.histogram.highlight_clipped
                            ))
                            .small()
                            .color(Color32::from_rgb(255, 90, 90)),
                        );
                    }
                    if self.histogram.shadow_clipped > 0 {
                        ui.label(
                            egui::RichText::new(format!(
                                "⬛ Noirs ({})",
                                self.histogram.shadow_clipped
                            ))
                            .small()
                            .color(Color32::from_rgb(80, 160, 255)),
                        );
                    }
                });
            });

            let hist_height = 84.0;
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), hist_height),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(rect);

            // Deep OLED rounded background card with subtle border
            painter.rect_filled(rect, 4.0, Color32::from_rgb(12, 13, 16));
            painter.rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0, Color32::from_rgb(38, 42, 50)),
                egui::StrokeKind::Outside,
            );

            // 4 Tonal zones: Noirs (0-25%), Ombres (25-50%), Exposition (50-75%), Hautes Lum. (75-100%)
            for quartile in [0.25_f32, 0.50, 0.75] {
                let x = rect.left() + quartile * rect.width();
                painter.line_segment(
                    [
                        egui::pos2(x, rect.top() + 2.0),
                        egui::pos2(x, rect.bottom() - 14.0),
                    ],
                    egui::Stroke::new(1.0, Color32::from_rgb(28, 30, 36)),
                );
            }

            // Sqrt-scaled height for beautiful smooth curves
            let max_c = (self.histogram.max_count as f32).sqrt().max(1.0);

            let channel_points = |bins: &[u32; 256]| -> Vec<egui::Pos2> {
                bins.iter()
                    .enumerate()
                    .map(|(i, &count)| {
                        let x = rect.left() + (i as f32 / 255.0) * rect.width();
                        let height_ratio = ((count as f32).sqrt() / max_c).clamp(0.0, 1.0);
                        let y = rect.bottom() - 14.0 - height_ratio * (rect.height() - 18.0);
                        egui::pos2(x, y)
                    })
                    .collect()
            };

            // Luminance backdrop filled shape
            let mut lum_polygon = vec![egui::pos2(rect.left(), rect.bottom() - 14.0)];
            lum_polygon.extend(channel_points(&self.histogram.lum));
            lum_polygon.push(egui::pos2(rect.right(), rect.bottom() - 14.0));
            painter.add(egui::Shape::convex_polygon(
                lum_polygon,
                Color32::from_rgba_unmultiplied(210, 220, 235, 38),
                egui::Stroke::NONE,
            ));

            // Translucent glowing RGB curves
            painter.add(egui::Shape::line(
                channel_points(&self.histogram.r),
                egui::Stroke::new(1.3, Color32::from_rgba_unmultiplied(255, 75, 75, 200)),
            ));
            painter.add(egui::Shape::line(
                channel_points(&self.histogram.g),
                egui::Stroke::new(1.3, Color32::from_rgba_unmultiplied(75, 230, 85, 200)),
            ));
            painter.add(egui::Shape::line(
                channel_points(&self.histogram.b),
                egui::Stroke::new(1.3, Color32::from_rgba_unmultiplied(75, 160, 255, 200)),
            ));

            // Zone labels at the bottom of the card
            let label_col = Color32::from_rgb(85, 90, 105);
            let label_font = egui::FontId::proportional(8.5);
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.125, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Noirs",
                label_font.clone(),
                label_col,
            );
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.375, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Ombres",
                label_font.clone(),
                label_col,
            );
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.625, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Exposition",
                label_font.clone(),
                label_col,
            );
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.875, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Hautes Lum.",
                label_font,
                label_col,
            );

            if response.hovered() {
                if let Some(pos) = response.hover_pos() {
                    let pct = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let bin_idx = (pct * 255.0).round() as usize;
                    let lum_v = self.histogram.lum[bin_idx.min(255)];
                    painter.line_segment(
                        [
                            egui::pos2(pos.x, rect.top()),
                            egui::pos2(pos.x, rect.bottom() - 14.0),
                        ],
                        egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120)),
                    );
                    let tip = format!("Niveau: {} | Pixels: {}", bin_idx, lum_v);
                    painter.text(
                        egui::pos2(
                            pos.x.clamp(rect.left() + 45.0, rect.right() - 45.0),
                            rect.top() + 6.0,
                        ),
                        egui::Align2::CENTER_TOP,
                        tip,
                        egui::FontId::monospace(9.5),
                        Color32::WHITE,
                    );
                }
            }
        });
    }

    /// Renders a concise EXIF card in the reorderable inspector sections.
    pub fn render_exif_card(&mut self, ui: &mut egui::Ui) {
        if let Some(photo) = &self.loaded_photo {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&photo.camera_model).strong());
                if !photo.lens_model.is_empty() {
                    ui.label(
                        egui::RichText::new(&photo.lens_model)
                            .small()
                            .color(Color32::LIGHT_GRAY),
                    );
                }
                let fl_str = if !photo.focal_length_str.is_empty() {
                    format!(" | {}", photo.focal_length_str)
                } else {
                    String::new()
                };
                ui.label(format!(
                    "{} | {} | ISO {}{fl_str}",
                    photo.shutter_str, photo.aperture_str, photo.iso
                ));
                if !photo.exposure_bias.is_empty() {
                    ui.label(format!("Bias: {}", photo.exposure_bias));
                }
                if !photo.date_time.is_empty() {
                    ui.label(
                        egui::RichText::new(&photo.date_time)
                            .small()
                            .color(Color32::GRAY),
                    );
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Full EXIF Tab 📋").clicked() {
                        self.left_panel_tab = LeftPanelTab::Exif;
                    }
                    if ui.button("Copy JSON").clicked() {
                        ui.ctx().copy_text(photo.export_exif_json());
                        self.status_message = "EXIF metadata copied to clipboard!".to_string();
                    }
                });
            });
        } else {
            ui.label(
                egui::RichText::new("No photo loaded.")
                    .italics()
                    .color(Color32::GRAY),
            );
        }
    }

    /// Renders the complete, detailed EXIF and metadata inspector with export controls.
    pub fn render_exif_full_panel(&mut self, ui: &mut egui::Ui) {
        let Some(photo) = self.loaded_photo.clone() else {
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new("No RAW photograph loaded.")
                    .italics()
                    .color(Color32::GRAY),
            );
            return;
        };

        ui.vertical(|ui| {
            ui.heading("EXIF & Photo Metadata");
            ui.label(
                egui::RichText::new(photo.path.display().to_string())
                    .small()
                    .color(Color32::GRAY),
            );
            ui.add_space(6.0);

            // Export Actions
            ui.horizontal(|ui| {
                if ui
                    .button("📥 Export EXIF (JSON)")
                    .on_hover_text("Save complete EXIF to .exif.json next to RAW file")
                    .clicked()
                {
                    let out_path = photo.path.with_extension("exif.json");
                    let json = photo.export_exif_json();
                    if std::fs::write(&out_path, json).is_ok() {
                        self.status_message = format!("Exported EXIF to {}", out_path.display());
                    } else {
                        self.status_message = "Failed to write EXIF file.".to_string();
                    }
                }

                if ui
                    .button("📋 Copy to Clipboard")
                    .on_hover_text("Copy EXIF JSON to system clipboard")
                    .clicked()
                {
                    ui.ctx().copy_text(photo.export_exif_json());
                    self.status_message = "EXIF JSON copied to clipboard!".to_string();
                }
            });

            ui.separator();

            // Summary Table
            egui::Grid::new("exif_summary_grid")
                .striped(true)
                .min_col_width(140.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Camera Make:").strong().size(13.0));
                    ui.label(egui::RichText::new(&photo.camera_make).size(13.0));
                    ui.end_row();

                    ui.label(egui::RichText::new("Camera Model:").strong().size(13.0));
                    ui.label(egui::RichText::new(&photo.camera_model).size(13.0));
                    ui.end_row();

                    if !photo.lens_model.is_empty() {
                        ui.label(egui::RichText::new("Lens:").strong().size(13.0));
                        ui.label(egui::RichText::new(&photo.lens_model).size(13.0));
                        ui.end_row();
                    }

                    ui.label(egui::RichText::new("Dimensions:").strong().size(13.0));
                    ui.label(
                        egui::RichText::new(format!(
                            "{} x {} ({:.1} MP)",
                            photo.dimensions.0,
                            photo.dimensions.1,
                            (photo.dimensions.0 as f32 * photo.dimensions.1 as f32) / 1_000_000.0
                        ))
                        .size(13.0),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Shutter Speed:").strong().size(13.0));
                    ui.label(egui::RichText::new(&photo.shutter_str).size(13.0));
                    ui.end_row();

                    ui.label(egui::RichText::new("Aperture:").strong().size(13.0));
                    ui.label(egui::RichText::new(&photo.aperture_str).size(13.0));
                    ui.end_row();

                    ui.label(egui::RichText::new("ISO Sensitivity:").strong().size(13.0));
                    ui.label(egui::RichText::new(photo.iso.to_string()).size(13.0));
                    ui.end_row();

                    if !photo.focal_length_str.is_empty() {
                        ui.label(egui::RichText::new("Focal Length:").strong().size(13.0));
                        ui.label(egui::RichText::new(&photo.focal_length_str).size(13.0));
                        ui.end_row();
                    }

                    if !photo.exposure_bias.is_empty() {
                        ui.label(egui::RichText::new("Exposure Bias:").strong().size(13.0));
                        ui.label(egui::RichText::new(&photo.exposure_bias).size(13.0));
                        ui.end_row();
                    }

                    if !photo.date_time.is_empty() {
                        ui.label(egui::RichText::new("Capture Date:").strong().size(13.0));
                        ui.label(egui::RichText::new(&photo.date_time).size(13.0));
                        ui.end_row();
                    }
                });

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("All Decoded EXIF Tags:")
                    .strong()
                    .size(14.0),
            );
            ui.separator();

            egui::Grid::new("exif_all_tags_grid")
                .striped(true)
                .min_col_width(140.0)
                .show(ui, |ui| {
                    for (tag, val) in &photo.exif_tags {
                        ui.label(egui::RichText::new(tag).strong().size(13.0));
                        ui.label(egui::RichText::new(val).size(13.0));
                        ui.end_row();
                    }
                });
        });
    }
}

impl eframe::App for RrawApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Keyboard navigation and multi-selection
        let shift = ui.input(|i| i.modifiers.shift);
        let nav_prev =
            ui.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp));
        let nav_next = ui
            .input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageDown));
        if nav_prev {
            self.navigate_relative_with_shift(-1, shift);
        } else if nav_next {
            self.navigate_relative_with_shift(1, shift);
        }

        // Classical keyboard selection shortcuts: Cmd/Ctrl+A, Cmd/Ctrl+D, Escape, Space
        ui.input(|i| {
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::A) {
                for entry in &self.directory_entries {
                    self.selected_photos.insert(entry.clone());
                }
            }
            if i.key_pressed(egui::Key::Escape)
                || ((i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::D))
            {
                self.selected_photos.clear();
            }
            if i.key_pressed(egui::Key::Space) {
                if let Some(photo) = &self.loaded_photo {
                    if self.selected_photos.contains(&photo.path) {
                        self.selected_photos.remove(&photo.path);
                    } else {
                        self.selected_photos.insert(photo.path.clone());
                    }
                }
            }
        });

        for digit in 0..=5 {
            let key = match digit {
                0 => egui::Key::Num0,
                1 => egui::Key::Num1,
                2 => egui::Key::Num2,
                3 => egui::Key::Num3,
                4 => egui::Key::Num4,
                5 => egui::Key::Num5,
                _ => unreachable!(),
            };
            if ui.input(|i| i.key_pressed(key)) {
                self.set_current_photo_rating(digit);
            }
        }

        // Collect results from background decoding worker without blocking the UI
        while let Ok(payload) = self.decode_receiver.try_recv() {
            if payload.is_prefetch {
                if self.photo_cache.len() >= 30 {
                    self.photo_cache.pop();
                }
                if !self.photo_cache.iter().any(|e| e.path == payload.path) {
                    self.photo_cache.insert(
                        0,
                        CachedPhotoEntry {
                            path: payload.path,
                            photo_view: payload.photo_view,
                            base_preview: payload.base_preview,
                            cfa: payload.cfa,
                        },
                    );
                }
                continue;
            }

            let file_name = payload
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("photo")
                .to_string();

            self.status_message = format!(
                "Loaded {} | {}x{} | ISO {} | {}",
                file_name,
                payload.photo_view.dimensions.0,
                payload.photo_view.dimensions.1,
                payload.photo_view.iso,
                payload.photo_view.camera_model
            );

            // Record rating if extracted from EXIF and not already tracked
            if payload.photo_view.rating > 0 && !self.photo_ratings.contains_key(&payload.path) {
                self.photo_ratings
                    .insert(payload.path.clone(), payload.photo_view.rating);
            }

            let mut view = payload.photo_view.clone();
            if let Some(&rating) = self.photo_ratings.get(&payload.path) {
                view.rating = rating;
            }

            self.loaded_photo = Some(view);
            self.base_preview = Some(payload.base_preview.clone());
            self.cached_cfa = Some(payload.cfa.clone());

            // Restore per-photo grading state
            if let Some(saved) = self.photo_gradings.get(&payload.path) {
                self.grading = saved.clone();
                self.current_preset = self.grading.preset;
            } else {
                self.grading = DarkroomGradingState::default();
                self.current_preset = GradingPreset::AsShot;
            }

            if self.photo_cache.len() >= 30 {
                self.photo_cache.pop();
            }
            self.photo_cache.insert(
                0,
                CachedPhotoEntry {
                    path: payload.path.clone(),
                    photo_view: payload.photo_view,
                    base_preview: payload.base_preview,
                    cfa: payload.cfa,
                },
            );

            self.is_decoding = false;
            self.needs_preview_update = true;

            // Trigger background prefetching for adjacent photos
            self.trigger_prefetch(&payload.path);
        }

        // Drain completed thumbnail images from background workers
        while let Ok((path, img)) = self.thumbnail_receiver.try_recv() {
            self.thumbnail_pending.remove(&path);
            let tex = ui.ctx().load_texture(
                format!("thumb_{}", path.display()),
                img,
                egui::TextureOptions::LINEAR,
            );
            self.thumbnail_textures.insert(path, tex);
        }

        if !self.thumbnail_pending.is_empty() {
            ui.ctx().request_repaint();
        }

        // Sub-millisecond zero-latency preview texture update
        if self.needs_preview_update {
            self.update_preview_textures(ui.ctx());
        }

        if self.is_decoding {
            ui.ctx().request_repaint();
        }

        let headroom = self.current_headroom();

        // 1. Top Control Bar
        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(22, 24, 28)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("RRAW");
                    ui.label(
                        egui::RichText::new("120Hz EDR Darkroom")
                            .color(egui::Color32::from_rgb(100, 180, 255)),
                    );

                    ui.separator();

                    if ui.button("📂 Open RAW...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                "RAW Images",
                                &["arw", "cr2", "cr3", "dng", "nef", "raf", "rw2", "orf"],
                            )
                            .pick_file()
                        {
                            self.open_raw_file(&path);
                        }
                    }

                    if ui.button("📁 Open Folder...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.open_directory(&folder);
                            if let Some(first_file) = self.directory_entries.first().cloned() {
                                self.open_raw_file(&first_file);
                            }
                        }
                    }

                    if ui.button("💾 Export...").clicked() {
                        self.show_export_dialog = true;
                        self.export_status = None;
                    }

                    ui.separator();

                    // Live EDR Headroom Badge
                    let headroom_label = if headroom > 1.05 {
                        egui::RichText::new(format!(
                            "⚡ EDR Headroom: {:.1}x (Apple XDR Active)",
                            headroom
                        ))
                        .color(egui::Color32::from_rgb(255, 200, 50))
                    } else {
                        egui::RichText::new(format!("📺 SDR Display: {:.1}x", headroom))
                            .color(egui::Color32::LIGHT_GRAY)
                    };
                    ui.label(headroom_label);

                    if let Some(photo) = &self.loaded_photo {
                        ui.separator();
                        if let Some(file_name) = photo.path.file_name().and_then(|n| n.to_str()) {
                            ui.label(egui::RichText::new(file_name).strong());
                        }
                        let fl_info = if !photo.focal_length_str.is_empty() {
                            format!(" | {}", photo.focal_length_str)
                        } else {
                            String::new()
                        };
                        ui.label(format!(
                            "{}x{} | ISO {} | {} | {}{fl_info} | {}",
                            photo.dimensions.0,
                            photo.dimensions.1,
                            photo.iso,
                            photo.shutter_str,
                            photo.aperture_str,
                            photo.camera_model
                        ));

                        ui.separator();
                        let current_r = photo.rating;
                        let mut new_top_r = None;
                        ui.horizontal(|ui| {
                            for s in 1..=5 {
                                let is_lit = s <= current_r;
                                let star_char = if is_lit { "★" } else { "☆" };
                                let star_col = if is_lit {
                                    Color32::from_rgb(255, 200, 50)
                                } else {
                                    Color32::from_rgb(100, 105, 115)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(star_char).color(star_col).size(14.0),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                let tip = format!(
                                    "Noter {} étoile{} (0 pour effacer)",
                                    s,
                                    if s > 1 { "s" } else { "" }
                                );
                                let resp = resp.on_hover_text(tip);
                                if resp.clicked() {
                                    let target_r = if current_r == s { 0 } else { s };
                                    new_top_r = Some(target_r);
                                }
                            }
                        });
                        if let Some(r) = new_top_r {
                            self.set_current_photo_rating(r);
                        }
                    }

                    ui.separator();
                    ui.label(egui::RichText::new("Zoom:").size(12.0));
                    if ui.selectable_label(self.zoom_level <= 0.0, "Fit").clicked() {
                        self.zoom_level = 0.0;
                        self.pan_offset = egui::Vec2::ZERO;
                    }
                    if ui
                        .selectable_label((self.zoom_level - 1.0).abs() < 0.01, "1:1")
                        .clicked()
                    {
                        self.zoom_level = 1.0;
                        self.pan_offset = egui::Vec2::ZERO;
                    }
                    let fit_s = self.last_fit_scale.max(0.001);
                    if ui.small_button("➖").clicked() {
                        let cur_s = if self.zoom_level <= 0.0 {
                            fit_s
                        } else {
                            self.zoom_level
                        };
                        let new_s = (cur_s / 1.25).max(fit_s);
                        if (new_s - fit_s).abs() / fit_s < 0.015 || new_s <= fit_s {
                            self.zoom_level = 0.0;
                            self.pan_offset = egui::Vec2::ZERO;
                        } else {
                            self.zoom_level = new_s;
                        }
                    }
                    let zoom_pct_str = if self.zoom_level <= 0.0 {
                        "Fit (100%)".to_string()
                    } else {
                        format!("{}%", ((self.zoom_level / fit_s) * 100.0).round() as u32)
                    };
                    ui.label(egui::RichText::new(zoom_pct_str).monospace().strong());
                    if ui.small_button("➕").clicked() {
                        let cur_s = if self.zoom_level <= 0.0 {
                            fit_s
                        } else {
                            self.zoom_level
                        };
                        let new_s = (cur_s * 1.25).min(fit_s * 20.0);
                        self.zoom_level = new_s;
                    }
                    if ui
                        .button("↺ Centrer")
                        .on_hover_text("Réinitialiser le panoramique")
                        .clicked()
                    {
                        self.pan_offset = egui::Vec2::ZERO;
                    }
                });
            });

        // 2. Bottom Status Bar (At the very bottom of the window)
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(18, 19, 22)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.is_decoding {
                        ui.spinner();
                    }
                    ui.label(&self.status_message);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            "Surface: Rgba16Float | Color: ExtendedLinearDisplayP3 | 120 Hz Metal",
                        );
                    });
                });
            });

        // 3. Bottom Filmstrip Panel (Horizontal thumbnail strip)
        egui::Panel::bottom("filmstrip_panel")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(egui::Color32::from_rgb(22, 24, 28))
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                // Filmstrip Toolbar: Rating Filter Chips & Quick Navigation
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Filter:").strong().size(12.0));

                    let all_label = format!("All ({})", self.directory_entries.len());
                    if ui
                        .selectable_label(self.rating_filter == RatingFilter::All, all_label)
                        .clicked()
                    {
                        self.rating_filter = RatingFilter::All;
                    }

                    for stars in 1..=5 {
                        let count = self.count_photos_with_min_rating(stars);
                        let label = format!("★ {}+ ({})", stars, count);
                        if ui
                            .selectable_label(
                                self.rating_filter == RatingFilter::MinStars(stars),
                                label,
                            )
                            .clicked()
                        {
                            self.rating_filter = RatingFilter::MinStars(stars);
                        }
                    }

                    let unrated_count = self.count_photos_unrated();
                    let unrated_label = format!("0★ ({})", unrated_count);
                    if ui
                        .selectable_label(
                            self.rating_filter == RatingFilter::Unrated,
                            unrated_label,
                        )
                        .clicked()
                    {
                        self.rating_filter = RatingFilter::Unrated;
                    }

                    ui.separator();
                    let sel_count = self.selected_photos.len();
                    let sel_text = if sel_count > 0 {
                        format!(
                            "☑ {} sélectionnée{}",
                            sel_count,
                            if sel_count > 1 { "s" } else { "" }
                        )
                    } else {
                        "0 sélection".to_string()
                    };
                    ui.label(
                        egui::RichText::new(sel_text)
                            .small()
                            .color(if sel_count > 0 {
                                Color32::from_rgb(100, 190, 255)
                            } else {
                                Color32::GRAY
                            }),
                    );

                    if ui
                        .small_button("Tout cocher")
                        .on_hover_text("Sélectionner toutes les photos pour l'export par lot")
                        .clicked()
                    {
                        for entry in &self.directory_entries {
                            self.selected_photos.insert(entry.clone());
                        }
                    }
                    if sel_count > 0 && ui.small_button("Tout décocher").clicked() {
                        self.selected_photos.clear();
                    }
                    if let Some(photo) = &self.loaded_photo {
                        if !self.selected_photos.contains(&photo.path)
                            && ui
                                .small_button("+ Cocher active")
                                .on_hover_text("Ajouter la photo courante à la sélection d'export")
                                .clicked()
                        {
                            self.selected_photos.insert(photo.path.clone());
                        }
                    }

                    if sel_count > 0 {
                        let export_btn = egui::Button::new(
                            egui::RichText::new(format!("📤 Exporter sélection ({sel_count})"))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(30, 120, 225));
                        if ui
                            .add(export_btn)
                            .on_hover_text(
                                "Ouvrir la fenêtre d'export pour les photos sélectionnées",
                            )
                            .clicked()
                        {
                            self.export_batch_selection = true;
                            self.show_export_dialog = true;
                            self.export_status = None;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("▶")
                            .on_hover_text("Next Photo (Right Arrow)")
                            .clicked()
                        {
                            self.navigate_relative(1);
                        }
                        if ui
                            .button("◀")
                            .on_hover_text("Previous Photo (Left Arrow)")
                            .clicked()
                        {
                            self.navigate_relative(-1);
                        }
                        ui.label(
                            egui::RichText::new("Keys: ◀/▶ (Switch) | 0-5 (Rate)")
                                .small()
                                .color(Color32::GRAY),
                        );
                    });
                });

                ui.add_space(4.0);

                // Horizontal Filmstrip scroll area with visible scrollbar positioned safely below cards
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .max_height(134.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let active_path =
                                    self.loaded_photo.as_ref().map(|p| p.path.clone());
                                let mut file_to_open = None;
                                let mut star_change: Option<(PathBuf, u8)> = None;
                                let mut thumbs_to_request = Vec::new();
                                let entries = self.directory_entries.clone();

                                for (entry_idx, entry) in entries.iter().enumerate() {
                                    let rating =
                                        self.photo_ratings.get(entry).copied().unwrap_or(0);
                                    if !self.rating_filter.matches(rating) {
                                        continue;
                                    }

                                    let is_selected = active_path.as_ref() == Some(entry);
                                    let is_multi_selected = self.selected_photos.contains(entry);
                                    let is_cached =
                                        self.photo_cache.iter().any(|e| &e.path == entry);

                                    let file_name = entry
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("photo");

                                    let ext = entry
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("RAW")
                                        .to_uppercase();

                                    let border_color = if is_selected {
                                        Color32::from_rgb(80, 160, 255)
                                    } else if is_multi_selected {
                                        Color32::from_rgb(50, 140, 230)
                                    } else {
                                        Color32::from_rgb(45, 48, 55)
                                    };
                                    let border_width = if is_selected || is_multi_selected {
                                        2.0
                                    } else {
                                        1.0
                                    };
                                    let bg_color = if is_selected {
                                        Color32::from_rgb(28, 36, 48)
                                    } else if is_multi_selected {
                                        Color32::from_rgb(22, 32, 44)
                                    } else {
                                        Color32::from_rgb(20, 22, 26)
                                    };

                                    let card_size = egui::vec2(120.0, 104.0);
                                    let (rect, response) =
                                        ui.allocate_exact_size(card_size, egui::Sense::click());
                                    let painter = ui.painter_at(rect);

                                    painter.rect_filled(rect, 6.0, bg_color);
                                    painter.rect_stroke(
                                        rect,
                                        6.0,
                                        egui::Stroke::new(border_width, border_color),
                                        egui::StrokeKind::Outside,
                                    );

                                    // Top miniature thumbnail frame
                                    let thumb_rect = egui::Rect::from_min_size(
                                        rect.min + egui::vec2(5.0, 4.0),
                                        egui::vec2(110.0, 62.0),
                                    );

                                    if let Some(tex) = self.thumbnail_textures.get(entry) {
                                        painter.image(
                                            tex.id(),
                                            thumb_rect,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            Color32::WHITE,
                                        );
                                        painter.rect_stroke(
                                            thumb_rect,
                                            2.0,
                                            egui::Stroke::new(1.0, Color32::from_rgb(45, 50, 60)),
                                            egui::StrokeKind::Outside,
                                        );
                                    } else {
                                        thumbs_to_request.push(entry.clone());
                                        painter.rect_filled(
                                            thumb_rect,
                                            2.0,
                                            Color32::from_rgb(14, 16, 20),
                                        );
                                        painter.rect_stroke(
                                            thumb_rect,
                                            2.0,
                                            egui::Stroke::new(1.0, Color32::from_rgb(35, 38, 45)),
                                            egui::StrokeKind::Outside,
                                        );
                                        painter.text(
                                            thumb_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            format!("📷 [{}]", ext),
                                            egui::FontId::monospace(10.0),
                                            Color32::from_rgb(110, 120, 135),
                                        );
                                    }

                                    // Format badge tag inside thumbnail (top-left)
                                    let badge_rect = egui::Rect::from_min_size(
                                        thumb_rect.min + egui::vec2(3.0, 3.0),
                                        egui::vec2(30.0, 13.0),
                                    );
                                    painter.rect_filled(
                                        badge_rect,
                                        2.0,
                                        Color32::from_rgba_premultiplied(0, 0, 0, 180),
                                    );
                                    painter.text(
                                        badge_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        &ext,
                                        egui::FontId::monospace(9.0),
                                        Color32::from_rgb(200, 210, 230),
                                    );

                                    // Cache indicator (top-right of thumbnail, offset from checkbox)
                                    if is_cached {
                                        painter.text(
                                            egui::pos2(
                                                thumb_rect.max.x - 26.0,
                                                thumb_rect.min.y + 3.0,
                                            ),
                                            egui::Align2::RIGHT_TOP,
                                            "●",
                                            egui::FontId::proportional(11.0),
                                            Color32::from_rgb(80, 220, 120),
                                        );
                                    }

                                    // Selection checkbox (top-right of thumbnail)
                                    let chk_rect = egui::Rect::from_min_size(
                                        egui::pos2(thumb_rect.max.x - 22.0, thumb_rect.min.y + 3.0),
                                        egui::vec2(18.0, 18.0),
                                    );
                                    let chk_id = ui.auto_id_with(("chk_thumb", entry));
                                    let chk_resp =
                                        ui.interact(chk_rect, chk_id, egui::Sense::click());
                                    if chk_resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    let mut toggle_select = chk_resp.clicked();

                                    if is_multi_selected {
                                        painter.circle_filled(
                                            chk_rect.center(),
                                            9.0,
                                            Color32::from_rgb(30, 130, 240),
                                        );
                                        painter.text(
                                            chk_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "✓",
                                            egui::FontId::proportional(12.0),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        let circle_col = if chk_resp.hovered() {
                                            Color32::from_rgba_premultiplied(50, 130, 230, 220)
                                        } else {
                                            Color32::from_rgba_premultiplied(0, 0, 0, 140)
                                        };
                                        painter.circle_filled(chk_rect.center(), 8.0, circle_col);
                                        painter.circle_stroke(
                                            chk_rect.center(),
                                            8.0,
                                            egui::Stroke::new(
                                                1.2,
                                                Color32::from_rgb(160, 175, 200),
                                            ),
                                        );
                                    }

                                    // Middle row: File name
                                    let truncated_name = if file_name.len() > 14 {
                                        format!("{}…", &file_name[..13])
                                    } else {
                                        file_name.to_string()
                                    };
                                    let name_color = if is_selected {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(200, 205, 215)
                                    };
                                    painter.text(
                                        egui::pos2(rect.center().x, rect.min.y + 70.0),
                                        egui::Align2::CENTER_TOP,
                                        truncated_name,
                                        egui::FontId::proportional(10.5),
                                        name_color,
                                    );

                                    // Bottom row: 5 clickable star glyphs completely above the bottom edge
                                    let star_y = rect.min.y + 90.0;
                                    let star_spacing = 15.0;
                                    let star_start_x = rect.center().x - (2.0 * star_spacing);

                                    let mut hovered_star = None;
                                    for s in 1..=5 {
                                        let star_pos = egui::pos2(
                                            star_start_x + (s as f32 - 1.0) * star_spacing,
                                            star_y,
                                        );
                                        let star_rect = egui::Rect::from_center_size(
                                            star_pos,
                                            egui::vec2(14.0, 14.0),
                                        );
                                        let star_id = ui.auto_id_with(("filmstrip_star", entry, s));
                                        let star_resp =
                                            ui.interact(star_rect, star_id, egui::Sense::click());
                                        if star_resp.hovered() {
                                            hovered_star = Some(s);
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if star_resp.clicked() {
                                            let new_r = if rating == s { 0 } else { s };
                                            star_change = Some((entry.clone(), new_r));
                                        }
                                    }

                                    for s in 1..=5 {
                                        let star_pos = egui::pos2(
                                            star_start_x + (s as f32 - 1.0) * star_spacing,
                                            star_y,
                                        );
                                        let is_lit = if let Some(h) = hovered_star {
                                            s <= h
                                        } else {
                                            s <= rating
                                        };
                                        let (star_char, star_col) = if is_lit {
                                            ("★", Color32::from_rgb(255, 205, 50))
                                        } else {
                                            ("☆", Color32::from_rgb(75, 80, 95))
                                        };

                                        painter.text(
                                            star_pos,
                                            egui::Align2::CENTER_CENTER,
                                            star_char,
                                            egui::FontId::proportional(13.0),
                                            star_col,
                                        );
                                    }

                                    // Classical mouse selection: Click, Cmd/Ctrl+Click, Shift+Click
                                    if response.clicked()
                                        && star_change.is_none()
                                        && !chk_resp.clicked()
                                    {
                                        if ui.input(|i| i.modifiers.shift) {
                                            let anchor = self.last_selected_index.unwrap_or(0);
                                            let min_i = anchor.min(entry_idx);
                                            let max_i = anchor.max(entry_idx);
                                            for i in min_i..=max_i {
                                                if let Some(p) = entries.get(i) {
                                                    self.selected_photos.insert(p.clone());
                                                }
                                            }
                                            file_to_open = Some(entry.clone());
                                        } else if ui
                                            .input(|i| i.modifiers.command || i.modifiers.ctrl)
                                        {
                                            toggle_select = true;
                                            self.last_selected_index = Some(entry_idx);
                                        } else {
                                            self.selected_photos.clear();
                                            self.selected_photos.insert(entry.clone());
                                            self.last_selected_index = Some(entry_idx);
                                            file_to_open = Some(entry.clone());
                                        }
                                    }

                                    if toggle_select {
                                        if self.selected_photos.contains(entry) {
                                            self.selected_photos.remove(entry);
                                        } else {
                                            self.selected_photos.insert(entry.clone());
                                        }
                                        self.last_selected_index = Some(entry_idx);
                                    }
                                }

                                if let Some((path, new_r)) = star_change {
                                    self.photo_ratings.insert(path.clone(), new_r);
                                    if let Some(photo) = &mut self.loaded_photo {
                                        if photo.path == path {
                                            photo.rating = new_r;
                                        }
                                    }
                                    if let Some(entry) =
                                        self.photo_cache.iter_mut().find(|e| e.path == path)
                                    {
                                        entry.photo_view.rating = new_r;
                                    }

                                    // Immediately persist rating to <photo>.rraw.json and <photo>.xmp
                                    let current_grading =
                                        self.photo_gradings.get(&path).cloned().unwrap_or_default();
                                    save_photo_sidecars(&path, &current_grading, new_r);

                                    let star_text = if new_r > 0 {
                                        "★".repeat(new_r as usize)
                                    } else {
                                        "Unrated".to_string()
                                    };
                                    self.status_message = format!(
                                        "Rated {}: {}",
                                        path.file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("photo"),
                                        star_text
                                    );
                                }

                                for path in thumbs_to_request {
                                    self.request_thumbnail(path);
                                }

                                if let Some(path) = file_to_open {
                                    self.open_raw_file(&path);
                                }
                            });

                            // Dedicated vertical padding so the scrollbar track sits entirely below cards
                            ui.add_space(14.0);
                        });
                    });
            });

        // 4. Left Panel: Presets (Dropdown) + EXIF at a glance + Folder Explorer + Live Waveform Histogram at Bottom
        egui::Panel::left("left_panel")
            .resizable(true)
            .default_size(320.0)
            .min_size(260.0)
            .max_size(520.0)
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(22, 24, 28)),
            )
            .show(ui, |ui| {
                ui.add_space(4.0);

                let available_h = ui.available_height();
                let hist_area_h = 135.0;
                let content_h = (available_h - hist_area_h - 12.0).max(120.0);

                ui.allocate_ui(egui::vec2(ui.available_width(), content_h), |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // 1. Explorateur de dossiers (EN HAUT)
                            egui::CollapsingHeader::new(
                                egui::RichText::new("📁 Explorateur de dossiers")
                                    .strong()
                                    .size(13.0)
                                    .color(Color32::from_rgb(230, 235, 245)),
                            )
                            .default_open(true)
                            .show(ui, |ui| {
                                self.render_explorer_tab(ui);
                            });

                            ui.separator();

                            // 2. Préréglages avec liste déroulante (PUIS LES PRESETS)
                            egui::CollapsingHeader::new(
                                egui::RichText::new("🎨 Styles & Préréglages")
                                    .strong()
                                    .size(13.0)
                                    .color(Color32::from_rgb(230, 235, 245)),
                            )
                            .default_open(true)
                            .show(ui, |ui| {
                                self.render_presets_section(ui);
                            });

                            ui.separator();

                            // 3. Données EXIF bien présentées (PUIS LES EXIFS)
                            egui::CollapsingHeader::new(
                                egui::RichText::new("📋 Données EXIF")
                                    .strong()
                                    .size(13.0)
                                    .color(Color32::from_rgb(230, 235, 245)),
                            )
                            .default_open(true)
                            .show(ui, |ui| {
                                self.render_exif_panel(ui);
                            });
                        });
                });

                ui.separator();
                self.render_histogram_widget(ui);
            });

        // 5. Right Darkroom Inspector Panel with Reorderable Sections
        let order = self.layout_config.section_order.clone();

        egui::Panel::right("darkroom_panel")
            .resizable(true)
            .default_size(360.0)
            .min_size(320.0)
            .max_size(600.0)
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(26, 28, 33)),
            )
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.heading("🛠️ Réglages de développement");
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (idx, section) in order.iter().enumerate() {
                            self.render_darkroom_section(ui, *section, idx);
                        }

                        ui.add_space(8.0);
                        if ui.button("Reset Panel Layout").clicked() {
                            self.layout_config = AppLayoutConfig::default();
                            self.layout_config.save();
                        }
                    });
            });

        // 6. Central Viewport Area with Interactive Zoom & Pan (Clean & Unobstructed)
        if self.needs_preview_update {
            self.update_preview_textures(ui.ctx());
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(ui.style()).fill(egui::Color32::from_rgb(16, 17, 20)),
            )
            .show(ui, |ui| {
                let available_rect = ui.available_rect_before_wrap();
                let painter = ui.painter().with_clip_rect(available_rect);

                if let Some(texture) = &self.preview_texture {
                    let margin = 16.0;
                    let max_w = (available_rect.width() - margin * 2.0).max(10.0);
                    let max_h = (available_rect.height() - margin * 2.0).max(10.0);

                    let img_size = texture.size_vec2();
                    let is_cropped_view = self.grading.crop_enabled && !self.is_crop_editing;
                    let crop_w = (self.grading.crop_right - self.grading.crop_left).clamp(0.05, 1.0);
                    let crop_h = (self.grading.crop_bottom - self.grading.crop_top).clamp(0.05, 1.0);
                    let effective_img_size = if is_cropped_view {
                        egui::vec2(img_size.x * crop_w, img_size.y * crop_h)
                    } else {
                        img_size
                    };

                    let fit_scale = (max_w / effective_img_size.x).min(max_h / effective_img_size.y);
                    self.last_fit_scale = fit_scale;

                    let current_scale = if self.zoom_level <= 0.0 {
                        fit_scale
                    } else {
                        self.zoom_level
                    };

                    let display_size = effective_img_size * current_scale;
                    let center = available_rect.center() + self.pan_offset;
                    let preview_rect = egui::Rect::from_center_size(center, display_size);

                    let resp = ui.allocate_rect(available_rect, egui::Sense::click_and_drag());
                    let is_tool_active = self.horizon_tool_active
                        || self.parallel_tool_phase != ParallelToolPhase::Inactive;

                    // Clamping pan offset to ensure image never moves out of viewport
                    if self.zoom_level <= 0.0 {
                        self.pan_offset = egui::Vec2::ZERO;
                    } else {
                        let max_pan_x =
                            ((display_size.x - available_rect.width() + margin * 2.0) * 0.5)
                                .max(0.0);
                        let max_pan_y =
                            ((display_size.y - available_rect.height() + margin * 2.0) * 0.5)
                                .max(0.0);
                        self.pan_offset.x = self.pan_offset.x.clamp(-max_pan_x, max_pan_x);
                        self.pan_offset.y = self.pan_offset.y.clamp(-max_pan_y, max_pan_y);
                    }

                    if resp.dragged()
                        && !is_tool_active
                        && self.crop_drag_handle == CropDragHandle::None
                        && self.zoom_level > 0.0
                    {
                        self.pan_offset += resp.drag_delta();
                        let max_pan_x =
                            ((display_size.x - available_rect.width() + margin * 2.0) * 0.5)
                                .max(0.0);
                        let max_pan_y =
                            ((display_size.y - available_rect.height() + margin * 2.0) * 0.5)
                                .max(0.0);
                        self.pan_offset.x = self.pan_offset.x.clamp(-max_pan_x, max_pan_x);
                        self.pan_offset.y = self.pan_offset.y.clamp(-max_pan_y, max_pan_y);
                    }

                    if resp.double_clicked() && !self.is_crop_editing {
                        if self.zoom_level <= 0.0 {
                            self.zoom_level = 1.0;
                            self.pan_offset = egui::Vec2::ZERO;
                        } else {
                            self.zoom_level = 0.0;
                            self.pan_offset = egui::Vec2::ZERO;
                        }
                    }

                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.0 && resp.hovered() && !is_tool_active && !self.is_crop_editing {
                        let zoom_factor = (scroll * 0.003).exp();
                        let min_scale = fit_scale;
                        let max_scale = (fit_scale * 20.0).max(10.0);
                        let new_scale = (current_scale * zoom_factor).clamp(min_scale, max_scale);

                        if (new_scale - fit_scale).abs() / fit_scale < 0.015
                            || new_scale <= fit_scale
                        {
                            self.zoom_level = 0.0;
                            self.pan_offset = egui::Vec2::ZERO;
                        } else {
                            if let Some(mouse_pos) = resp.hover_pos() {
                                let old_offset = mouse_pos - center;
                                let scale_change = new_scale / current_scale;
                                self.pan_offset -= old_offset * (scale_change - 1.0);
                            }
                            self.zoom_level = new_scale;
                        }
                    }

                    // Draw image with dark background frame and subtle shadow
                    painter.rect_filled(
                        preview_rect.expand(2.0),
                        4.0,
                        egui::Color32::from_rgb(10, 10, 12),
                    );

                    let uv = if is_cropped_view {
                        egui::Rect::from_min_max(
                            egui::pos2(self.grading.crop_left, self.grading.crop_top),
                            egui::pos2(self.grading.crop_right, self.grading.crop_bottom),
                        )
                    } else {
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
                    };

                    painter.image(
                        texture.id(),
                        preview_rect,
                        uv,
                        egui::Color32::WHITE,
                    );
                    painter.rect_stroke(
                        preview_rect,
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 65, 75)),
                        egui::StrokeKind::Outside,
                    );

                    // Interactive mouse cropping with locked aspect ratio and dimmed mask
                    if self.grading.crop_enabled && self.is_crop_editing {
                        let pw = preview_rect.width();
                        let ph = preview_rect.height();

                        let crop_screen_min = egui::pos2(
                            preview_rect.min.x + self.grading.crop_left * pw,
                            preview_rect.min.y + self.grading.crop_top * ph,
                        );
                        let crop_screen_max = egui::pos2(
                            preview_rect.min.x + self.grading.crop_right * pw,
                            preview_rect.min.y + self.grading.crop_bottom * ph,
                        );
                        let crop_screen_rect =
                            egui::Rect::from_min_max(crop_screen_min, crop_screen_max);

                        // 1. Dimmed outer mask outside crop box
                        let mask_color = Color32::from_rgba_unmultiplied(0, 0, 0, 170);
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                preview_rect.min,
                                egui::pos2(preview_rect.max.x, crop_screen_rect.min.y),
                            ),
                            0.0,
                            mask_color,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(preview_rect.min.x, crop_screen_rect.max.y),
                                preview_rect.max,
                            ),
                            0.0,
                            mask_color,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(preview_rect.min.x, crop_screen_rect.min.y),
                                egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
                            ),
                            0.0,
                            mask_color,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
                                egui::pos2(preview_rect.max.x, crop_screen_rect.max.y),
                            ),
                            0.0,
                            mask_color,
                        );

                        // 2. Bright border around crop box
                        painter.rect_stroke(
                            crop_screen_rect,
                            0.0,
                            egui::Stroke::new(1.5, Color32::WHITE),
                            egui::StrokeKind::Outside,
                        );

                        // 3. Rule-of-thirds composition grid
                        if self.grading.crop_show_grid {
                            let grid_stroke = egui::Stroke::new(
                                1.0,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 110),
                            );
                            let cw = crop_screen_rect.width();
                            let ch = crop_screen_rect.height();
                            painter.line_segment(
                                [
                                    egui::pos2(crop_screen_rect.min.x + cw / 3.0, crop_screen_rect.min.y),
                                    egui::pos2(crop_screen_rect.min.x + cw / 3.0, crop_screen_rect.max.y),
                                ],
                                grid_stroke,
                            );
                            painter.line_segment(
                                [
                                    egui::pos2(crop_screen_rect.min.x + 2.0 * cw / 3.0, crop_screen_rect.min.y),
                                    egui::pos2(crop_screen_rect.min.x + 2.0 * cw / 3.0, crop_screen_rect.max.y),
                                ],
                                grid_stroke,
                            );
                            painter.line_segment(
                                [
                                    egui::pos2(crop_screen_rect.min.x, crop_screen_rect.min.y + ch / 3.0),
                                    egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y + ch / 3.0),
                                ],
                                grid_stroke,
                            );
                            painter.line_segment(
                                [
                                    egui::pos2(crop_screen_rect.min.x, crop_screen_rect.min.y + 2.0 * ch / 3.0),
                                    egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y + 2.0 * ch / 3.0),
                                ],
                                grid_stroke,
                            );
                        }

                        // 4. Corner L-brackets & Edge handles
                        let bracket_len = 16.0;
                        let bracket_stroke = egui::Stroke::new(3.0, Color32::WHITE);
                        painter.line_segment(
                            [
                                crop_screen_rect.min,
                                egui::pos2(crop_screen_rect.min.x + bracket_len, crop_screen_rect.min.y),
                            ],
                            bracket_stroke,
                        );
                        painter.line_segment(
                            [
                                crop_screen_rect.min,
                                egui::pos2(crop_screen_rect.min.x, crop_screen_rect.min.y + bracket_len),
                            ],
                            bracket_stroke,
                        );
                        painter.line_segment(
                            [
                                egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
                                egui::pos2(crop_screen_rect.max.x - bracket_len, crop_screen_rect.min.y),
                            ],
                            bracket_stroke,
                        );
                        painter.line_segment(
                            [
                                egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
                                egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y + bracket_len),
                            ],
                            bracket_stroke,
                        );
                        painter.line_segment(
                            [
                                egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
                                egui::pos2(crop_screen_rect.min.x + bracket_len, crop_screen_rect.max.y),
                            ],
                            bracket_stroke,
                        );
                        painter.line_segment(
                            [
                                egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
                                egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y - bracket_len),
                            ],
                            bracket_stroke,
                        );
                        painter.line_segment(
                            [
                                crop_screen_rect.max,
                                egui::pos2(crop_screen_rect.max.x - bracket_len, crop_screen_rect.max.y),
                            ],
                            bracket_stroke,
                        );
                        painter.line_segment(
                            [
                                crop_screen_rect.max,
                                egui::pos2(crop_screen_rect.max.x, crop_screen_rect.max.y - bracket_len),
                            ],
                            bracket_stroke,
                        );

                        // 5. Mouse Hit-Testing & Interactive Resizing
                        if let Some(mouse_pos) = resp.hover_pos() {
                            let hit_dist = 14.0;
                            let mut hovered_handle = CropDragHandle::None;

                            if (mouse_pos - crop_screen_rect.left_top()).length() <= hit_dist {
                                hovered_handle = CropDragHandle::TopLeft;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                            } else if (mouse_pos - crop_screen_rect.right_top()).length() <= hit_dist {
                                hovered_handle = CropDragHandle::TopRight;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNeSw);
                            } else if (mouse_pos - crop_screen_rect.left_bottom()).length() <= hit_dist {
                                hovered_handle = CropDragHandle::BottomLeft;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNeSw);
                            } else if (mouse_pos - crop_screen_rect.right_bottom()).length() <= hit_dist {
                                hovered_handle = CropDragHandle::BottomRight;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                            } else if (mouse_pos.y - crop_screen_rect.top()).abs() <= hit_dist
                                && mouse_pos.x >= crop_screen_rect.left()
                                && mouse_pos.x <= crop_screen_rect.right()
                            {
                                hovered_handle = CropDragHandle::Top;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                            } else if (mouse_pos.y - crop_screen_rect.bottom()).abs() <= hit_dist
                                && mouse_pos.x >= crop_screen_rect.left()
                                && mouse_pos.x <= crop_screen_rect.right()
                            {
                                hovered_handle = CropDragHandle::Bottom;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                            } else if (mouse_pos.x - crop_screen_rect.left()).abs() <= hit_dist
                                && mouse_pos.y >= crop_screen_rect.top()
                                && mouse_pos.y <= crop_screen_rect.bottom()
                            {
                                hovered_handle = CropDragHandle::Left;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                            } else if (mouse_pos.x - crop_screen_rect.right()).abs() <= hit_dist
                                && mouse_pos.y >= crop_screen_rect.top()
                                && mouse_pos.y <= crop_screen_rect.bottom()
                            {
                                hovered_handle = CropDragHandle::Right;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                            } else if crop_screen_rect.contains(mouse_pos) {
                                hovered_handle = CropDragHandle::Center;
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }

                            if resp.drag_started() && hovered_handle != CropDragHandle::None {
                                self.crop_drag_handle = hovered_handle;
                            }
                        }

                        if resp.dragged() && self.crop_drag_handle != CropDragHandle::None {
                            let delta = resp.drag_delta();
                            let dx = delta.x / pw;
                            let dy = delta.y / ph;
                            let img_aspect = pw / ph.max(1.0);
                            let target_aspect = self.grading.crop_aspect.ratio(pw, ph);

                            match self.crop_drag_handle {
                                CropDragHandle::Center => {
                                    let w = self.grading.crop_right - self.grading.crop_left;
                                    let h = self.grading.crop_bottom - self.grading.crop_top;
                                    let new_left = (self.grading.crop_left + dx).clamp(0.0, 1.0 - w);
                                    let new_top = (self.grading.crop_top + dy).clamp(0.0, 1.0 - h);
                                    self.grading.crop_left = new_left;
                                    self.grading.crop_right = new_left + w;
                                    self.grading.crop_top = new_top;
                                    self.grading.crop_bottom = new_top + h;
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                }
                                CropDragHandle::BottomRight => {
                                    let new_right = (self.grading.crop_right + dx)
                                        .clamp(self.grading.crop_left + 0.05, 1.0);
                                    if let Some(ratio) = target_aspect {
                                        let w_norm = new_right - self.grading.crop_left;
                                        let h_norm = w_norm * (img_aspect / ratio);
                                        if self.grading.crop_top + h_norm <= 1.0 {
                                            self.grading.crop_bottom = self.grading.crop_top + h_norm;
                                            self.grading.crop_right = new_right;
                                        }
                                    } else {
                                        self.grading.crop_right = new_right;
                                        self.grading.crop_bottom = (self.grading.crop_bottom + dy)
                                            .clamp(self.grading.crop_top + 0.05, 1.0);
                                    }
                                }
                                CropDragHandle::BottomLeft => {
                                    let new_left = (self.grading.crop_left + dx)
                                        .clamp(0.0, self.grading.crop_right - 0.05);
                                    if let Some(ratio) = target_aspect {
                                        let w_norm = self.grading.crop_right - new_left;
                                        let h_norm = w_norm * (img_aspect / ratio);
                                        if self.grading.crop_top + h_norm <= 1.0 {
                                            self.grading.crop_bottom = self.grading.crop_top + h_norm;
                                            self.grading.crop_left = new_left;
                                        }
                                    } else {
                                        self.grading.crop_left = new_left;
                                        self.grading.crop_bottom = (self.grading.crop_bottom + dy)
                                            .clamp(self.grading.crop_top + 0.05, 1.0);
                                    }
                                }
                                CropDragHandle::TopRight => {
                                    let new_right = (self.grading.crop_right + dx)
                                        .clamp(self.grading.crop_left + 0.05, 1.0);
                                    if let Some(ratio) = target_aspect {
                                        let w_norm = new_right - self.grading.crop_left;
                                        let h_norm = w_norm * (img_aspect / ratio);
                                        if self.grading.crop_bottom >= h_norm {
                                            self.grading.crop_top = self.grading.crop_bottom - h_norm;
                                            self.grading.crop_right = new_right;
                                        }
                                    } else {
                                        self.grading.crop_right = new_right;
                                        self.grading.crop_top = (self.grading.crop_top + dy)
                                            .clamp(0.0, self.grading.crop_bottom - 0.05);
                                    }
                                }
                                CropDragHandle::TopLeft => {
                                    let new_left = (self.grading.crop_left + dx)
                                        .clamp(0.0, self.grading.crop_right - 0.05);
                                    if let Some(ratio) = target_aspect {
                                        let w_norm = self.grading.crop_right - new_left;
                                        let h_norm = w_norm * (img_aspect / ratio);
                                        if self.grading.crop_bottom >= h_norm {
                                            self.grading.crop_top = self.grading.crop_bottom - h_norm;
                                            self.grading.crop_left = new_left;
                                        }
                                    } else {
                                        self.grading.crop_left = new_left;
                                        self.grading.crop_top = (self.grading.crop_top + dy)
                                            .clamp(0.0, self.grading.crop_bottom - 0.05);
                                    }
                                }
                                CropDragHandle::Right => {
                                    self.grading.crop_right = (self.grading.crop_right + dx)
                                        .clamp(self.grading.crop_left + 0.05, 1.0);
                                    if let Some(ratio) = target_aspect {
                                        let w_norm = self.grading.crop_right - self.grading.crop_left;
                                        let h_norm = (w_norm * (img_aspect / ratio)).min(1.0);
                                        let cy = (self.grading.crop_top + self.grading.crop_bottom) * 0.5;
                                        self.grading.crop_top = (cy - h_norm * 0.5).clamp(0.0, 1.0 - h_norm);
                                        self.grading.crop_bottom = self.grading.crop_top + h_norm;
                                    }
                                }
                                CropDragHandle::Left => {
                                    self.grading.crop_left = (self.grading.crop_left + dx)
                                        .clamp(0.0, self.grading.crop_right - 0.05);
                                    if let Some(ratio) = target_aspect {
                                        let w_norm = self.grading.crop_right - self.grading.crop_left;
                                        let h_norm = (w_norm * (img_aspect / ratio)).min(1.0);
                                        let cy = (self.grading.crop_top + self.grading.crop_bottom) * 0.5;
                                        self.grading.crop_top = (cy - h_norm * 0.5).clamp(0.0, 1.0 - h_norm);
                                        self.grading.crop_bottom = self.grading.crop_top + h_norm;
                                    }
                                }
                                CropDragHandle::Top => {
                                    self.grading.crop_top = (self.grading.crop_top + dy)
                                        .clamp(0.0, self.grading.crop_bottom - 0.05);
                                    if let Some(ratio) = target_aspect {
                                        let h_norm = self.grading.crop_bottom - self.grading.crop_top;
                                        let w_norm = (h_norm * (ratio / img_aspect)).min(1.0);
                                        let cx = (self.grading.crop_left + self.grading.crop_right) * 0.5;
                                        self.grading.crop_left = (cx - w_norm * 0.5).clamp(0.0, 1.0 - w_norm);
                                        self.grading.crop_right = self.grading.crop_left + w_norm;
                                    }
                                }
                                CropDragHandle::Bottom => {
                                    self.grading.crop_bottom = (self.grading.crop_bottom + dy)
                                        .clamp(self.grading.crop_top + 0.05, 1.0);
                                    if let Some(ratio) = target_aspect {
                                        let h_norm = self.grading.crop_bottom - self.grading.crop_top;
                                        let w_norm = (h_norm * (ratio / img_aspect)).min(1.0);
                                        let cx = (self.grading.crop_left + self.grading.crop_right) * 0.5;
                                        self.grading.crop_left = (cx - w_norm * 0.5).clamp(0.0, 1.0 - w_norm);
                                        self.grading.crop_right = self.grading.crop_left + w_norm;
                                    }
                                }
                                CropDragHandle::None => {}
                            }
                            ui.ctx().request_repaint();
                        }

                        if resp.drag_stopped() {
                            self.crop_drag_handle = CropDragHandle::None;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }

                        // Validate crop on Enter key or double-click inside crop box
                        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let dbl_click_crop = resp.double_clicked()
                            && resp
                                .interact_pointer_pos()
                                .is_some_and(|p| crop_screen_rect.contains(p));

                        if enter_pressed || dbl_click_crop {
                            self.is_crop_editing = false;
                            self.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }

                        // Floating validation pill at top of viewport
                        let banner_rect = egui::Rect::from_min_size(
                            available_rect.left_top() + egui::vec2(16.0, 16.0),
                            egui::vec2(380.0, 32.0),
                        );
                        painter.rect_filled(
                            banner_rect,
                            6.0,
                            Color32::from_rgba_unmultiplied(16, 20, 28, 230),
                        );
                        painter.rect_stroke(
                            banner_rect,
                            6.0,
                            egui::Stroke::new(1.0, Color32::from_rgb(60, 120, 200)),
                            egui::StrokeKind::Outside,
                        );
                        painter.text(
                            banner_rect.center() - egui::vec2(35.0, 0.0),
                            egui::Align2::CENTER_CENTER,
                            "✂️ Ajustez le cadre (Entrée pour valider)",
                            egui::FontId::proportional(12.0),
                            Color32::WHITE,
                        );

                        let val_btn_rect = egui::Rect::from_min_size(
                            egui::pos2(banner_rect.max.x - 74.0, banner_rect.min.y + 4.0),
                            egui::vec2(68.0, 24.0),
                        );
                        let val_btn_id = ui.auto_id_with("crop_validate_btn");
                        let val_resp = ui.interact(val_btn_rect, val_btn_id, egui::Sense::click());
                        if val_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        painter.rect_filled(
                            val_btn_rect,
                            4.0,
                            if val_resp.hovered() {
                                Color32::from_rgb(45, 150, 80)
                            } else {
                                Color32::from_rgb(35, 125, 65)
                            },
                        );
                        painter.text(
                            val_btn_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "✓ Valider",
                            egui::FontId::proportional(11.0),
                            Color32::WHITE,
                        );
                        if val_resp.clicked() {
                            self.is_crop_editing = false;
                            self.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }
                    }

                    // 2-parallel-lines perspective correction tool interaction
                    if self.parallel_tool_phase != ParallelToolPhase::Inactive {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);

                        let instruction = match self.parallel_tool_phase {
                            ParallelToolPhase::DrawingLine1 => {
                                "Glissez pour tracer la 1ère référence verticale (Ligne 1)"
                            }
                            ParallelToolPhase::WaitingLine2 => {
                                "Ligne 1 validée. Glissez maintenant pour tracer la 2ème référence (Ligne 2)"
                            }
                            ParallelToolPhase::DrawingLine2 => {
                                "Tracez la 2ème référence verticale..."
                            }
                            _ => "",
                        };
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                available_rect.left_top() + egui::vec2(16.0, 16.0),
                                egui::vec2(480.0, 32.0),
                            ),
                            6.0,
                            Color32::from_rgba_unmultiplied(16, 20, 28, 230),
                        );
                        painter.text(
                            available_rect.left_top() + egui::vec2(28.0, 32.0),
                            egui::Align2::LEFT_CENTER,
                            format!("📐 {instruction}"),
                            egui::FontId::proportional(14.0),
                            Color32::from_rgb(0, 240, 255),
                        );

                        if let Some((p1, p2)) = self.parallel_line1 {
                            painter.line_segment(
                                [p1, p2],
                                egui::Stroke::new(2.5, Color32::from_rgb(50, 230, 100)),
                            );
                            painter.circle_filled(p1, 5.0, Color32::from_rgb(50, 230, 100));
                            painter.circle_filled(p2, 5.0, Color32::from_rgb(50, 230, 100));
                            painter.text(
                                (p1 + p2.to_vec2()) * 0.5 + egui::vec2(10.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                "Ligne 1",
                                egui::FontId::proportional(13.0),
                                Color32::WHITE,
                            );
                        }

                        if resp.drag_started() {
                            self.parallel_drag_start = resp.interact_pointer_pos();
                        }

                        if resp.dragged() {
                            if let (Some(p1), Some(p2)) =
                                (self.parallel_drag_start, resp.interact_pointer_pos())
                            {
                                let stroke_col = match self.parallel_tool_phase {
                                    ParallelToolPhase::DrawingLine1 => {
                                        Color32::from_rgb(0, 240, 255)
                                    }
                                    _ => Color32::from_rgb(255, 220, 50),
                                };
                                painter.line_segment([p1, p2], egui::Stroke::new(2.5, stroke_col));
                                painter.circle_filled(p1, 5.0, stroke_col);
                                painter.circle_filled(p2, 5.0, stroke_col);
                            }
                        }

                        if resp.drag_stopped() {
                            if let (Some(p1), Some(p2)) =
                                (self.parallel_drag_start, resp.interact_pointer_pos())
                            {
                                let dist_sq = (p2.x - p1.x).powi(2) + (p2.y - p1.y).powi(2);
                                if dist_sq > 64.0 {
                                    match self.parallel_tool_phase {
                                        ParallelToolPhase::DrawingLine1 => {
                                            self.parallel_line1 = Some((p1, p2));
                                            self.parallel_tool_phase =
                                                ParallelToolPhase::WaitingLine2;
                                        }
                                        ParallelToolPhase::WaitingLine2
                                        | ParallelToolPhase::DrawingLine2 => {
                                            self.parallel_line2 = Some((p1, p2));
                                            if let Some((l1_a, l1_b)) = self.parallel_line1 {
                                                let (a1, b1) = if l1_b.y < l1_a.y {
                                                    (l1_a, l1_b)
                                                } else {
                                                    (l1_b, l1_a)
                                                };
                                                let (a2, b2) = if p2.y < p1.y {
                                                    (p1, p2)
                                                } else {
                                                    (p2, p1)
                                                };

                                                let dx1 = b1.x - a1.x;
                                                let dy1 = a1.y - b1.y;
                                                let dx2 = b2.x - a2.x;
                                                let dy2 = a2.y - b2.y;

                                                let slope1 = dx1 / dy1.max(1.0);
                                                let slope2 = dx2 / dy2.max(1.0);

                                                let (left_slope, right_slope) = if a1.x < a2.x {
                                                    (slope1, slope2)
                                                } else {
                                                    (slope2, slope1)
                                                };

                                                let convergence = right_slope - left_slope;
                                                let avg_tilt = (slope1 + slope2) * 0.5;

                                                let keystone =
                                                    (-convergence * 65.0).clamp(-50.0, 50.0);
                                                let rotation =
                                                    (-avg_tilt * 57.29578).clamp(-45.0, 45.0);

                                                self.grading.keystone_vertical = keystone;
                                                self.grading.rotation_degrees = rotation;
                                                self.grading.preset = GradingPreset::Custom;
                                                self.current_preset = GradingPreset::Custom;
                                                self.needs_preview_update = true;
                                                self.save_current_photo_sidecar();
                                                self.status_message = format!(
                                                    "Perspective redressée : Keystone V {:.1}, Rotation {:.1}°",
                                                    keystone, rotation
                                                );
                                            }
                                            self.parallel_tool_phase = ParallelToolPhase::Inactive;
                                            self.parallel_line1 = None;
                                            self.parallel_line2 = None;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            self.parallel_drag_start = None;
                        }
                    }

                    // Leveling horizon line interaction
                    if self.horizon_tool_active {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                        if resp.drag_started() {
                            self.horizon_start_point = resp.interact_pointer_pos();
                        }
                        if resp.dragged() {
                            if let (Some(p1), Some(p2)) =
                                (self.horizon_start_point, resp.interact_pointer_pos())
                            {
                                let dx = p2.x - p1.x;
                                let dy = p2.y - p1.y;
                                let angle = -dy.atan2(dx).to_degrees();
                                painter.line_segment(
                                    [p1, p2],
                                    egui::Stroke::new(2.5, Color32::from_rgb(0, 240, 255)),
                                );
                                painter.circle_filled(p1, 4.0, Color32::from_rgb(0, 240, 255));
                                painter.circle_filled(p2, 4.0, Color32::from_rgb(0, 240, 255));
                                painter.text(
                                    (p1 + p2.to_vec2()) * 0.5 + egui::vec2(0.0, -18.0),
                                    egui::Align2::CENTER_CENTER,
                                    format!("{:.1}°", angle),
                                    egui::FontId::proportional(14.0),
                                    Color32::WHITE,
                                );
                            }
                        }
                        if resp.drag_stopped() {
                            if let (Some(p1), Some(p2)) =
                                (self.horizon_start_point, resp.interact_pointer_pos())
                            {
                                let dx = p2.x - p1.x;
                                let dy = p2.y - p1.y;
                                if (dx * dx + dy * dy) > 36.0 {
                                    let angle = -dy.atan2(dx).to_degrees();
                                    self.grading.rotation_degrees = angle.clamp(-45.0, 45.0);
                                    self.grading.preset = GradingPreset::Custom;
                                    self.current_preset = GradingPreset::Custom;
                                    self.needs_preview_update = true;
                                    self.save_current_photo_sidecar();
                                }
                            }
                            self.horizon_start_point = None;
                            self.horizon_tool_active = false;
                        }
                    }
                } else if self.is_decoding {
                    painter.text(
                        available_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Decoding RAW file in background...",
                        egui::FontId::proportional(18.0),
                        egui::Color32::from_rgb(100, 180, 255),
                    );
                } else {
                    // Empty state prompt
                    painter.text(
                        available_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No RAW image loaded.\nClick 'Open RAW...' or pick a file in Library & Files.",
                        egui::FontId::proportional(16.0),
                        egui::Color32::from_rgb(140, 145, 155),
                    );
                }
            });

        // 7. Render interactive Export modal dialog if active
        self.render_export_dialog(ui.ctx());
    }
}
