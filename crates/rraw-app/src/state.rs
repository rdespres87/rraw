#![forbid(unsafe_code)]

//! State definitions, darkroom configuration, and re-exports of grading presets, mask layers, and sidecars.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use crate::grading::*;
use crate::i18n::{Key, Language};
pub use crate::mask_layers::*;
pub use crate::presets::*;
pub use crate::sidecar::*;

/// Navigation tabs in the left panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    pub fn label(self) -> &'static str {
        self.label_lang(Language::Fr)
    }

    #[must_use]
    pub fn label_lang(self, lang: Language) -> &'static str {
        match self {
            Self::CombinedRgb => Key::CurveChannelRgb.translate(lang),
            Self::Red => Key::CurveChannelRed.translate(lang),
            Self::Green => Key::CurveChannelGreen.translate(lang),
            Self::Blue => Key::CurveChannelBlue.translate(lang),
            Self::Luma => Key::CurveChannelLuma.translate(lang),
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
    pub fn label(self) -> &'static str {
        self.label_lang(Language::Fr)
    }

    #[must_use]
    pub fn label_lang(self, lang: Language) -> &'static str {
        match self {
            Self::Free => Key::CropRatioFree.translate(lang),
            Self::Original => Key::CropRatioOriginal.translate(lang),
            Self::Square1x1 => Key::CropRatioSquare.translate(lang),
            Self::Ratio3x2 => Key::CropRatio3x2.translate(lang),
            Self::Ratio4x3 => Key::CropRatio4x3.translate(lang),
            Self::Ratio16x9 => Key::CropRatio16x9.translate(lang),
            Self::Ratio5x4 => Key::CropRatio5x4.translate(lang),
            Self::Ratio9x16 => Key::CropRatio9x16.translate(lang),
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

/// Export target scope selected in the export dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportTargetMode {
    #[default]
    CurrentPhoto,
    BatchSelection,
    TimelapseSequence,
}

/// Supported export raster formats with user-selectable compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormatChoice {
    #[default]
    Jpeg,
    Png,
    Tiff,
    Avif,
}

impl ExportFormatChoice {
    #[must_use]
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Tiff => "tif",
            Self::Avif => "avif",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG (.jpg)",
            Self::Png => "PNG (.png)",
            Self::Tiff => "TIFF (.tif)",
            Self::Avif => "AVIF (.avif)",
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
        self.label_lang(Language::Fr)
    }

    /// Language-aware display label for export scale mode.
    #[must_use]
    pub fn label_lang(&self, lang: Language) -> &'static str {
        match self {
            Self::Original => Key::ExportScaleOriginal.translate(lang),
            Self::Percent75 => "75%",
            Self::Percent50 => "50%",
            Self::FitLongEdge2048 => Key::ExportScale2048.translate(lang),
            Self::FitLongEdge3840 => "3840 px (4K UHD)",
        }
    }
}

/// Tool family categorization for organized darkroom palette grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ToolFamily {
    #[default]
    All,
    Light,
    Color,
    Detail,
    Geometry,
    LocalMasks,
    Timelapse,
}

impl ToolFamily {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Light,
        Self::Color,
        Self::Detail,
        Self::Geometry,
        Self::LocalMasks,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        self.label_lang(Language::Fr)
    }

    #[must_use]
    pub fn label_lang(self, lang: Language) -> &'static str {
        match self {
            Self::All => Key::FamilyAll.translate(lang),
            Self::Light => Key::FamilyLight.translate(lang),
            Self::Color => Key::FamilyColor.translate(lang),
            Self::Detail => Key::FamilyDetail.translate(lang),
            Self::Geometry => Key::FamilyGeometry.translate(lang),
            Self::LocalMasks => Key::FamilyLocalMasks.translate(lang),
            Self::Timelapse => Key::FamilyTimelapse.translate(lang),
        }
    }

    #[must_use]
    pub fn short_label(self) -> &'static str {
        self.short_label_lang(Language::Fr)
    }

    #[must_use]
    pub fn short_label_lang(self, lang: Language) -> &'static str {
        match self {
            Self::All => Key::FamilyAllShort.translate(lang),
            Self::Light => Key::FamilyLightShort.translate(lang),
            Self::Color => Key::FamilyColorShort.translate(lang),
            Self::Detail => Key::FamilyDetailShort.translate(lang),
            Self::Geometry => Key::FamilyGeometryShort.translate(lang),
            Self::LocalMasks => Key::FamilyLocalMasksShort.translate(lang),
            Self::Timelapse => Key::FamilyTimelapseShort.translate(lang),
        }
    }
}

/// Selected sub-tab / mode in the Optical Corrections inspector section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OpticalCorrectionTab {
    #[default]
    Profile,
    Manual,
}

/// Identifiers for customizable darkroom inspector sections that can be reordered by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DarkroomSection {
    ExposureTone,
    AiSemanticMasks,
    ToneCurves,
    WhiteBalanceColor,
    SoftProofing,
    Timelapse,
    Crop,
    Straighten,
    Geometry,
    DetailDenoise,
    OpticalCorrections,
    Effects,
    #[serde(other)]
    LegacyOther,
}

impl DarkroomSection {
    pub const ALL: [Self; 12] = [
        Self::ExposureTone,
        Self::ToneCurves,
        Self::WhiteBalanceColor,
        Self::SoftProofing,
        Self::DetailDenoise,
        Self::Crop,
        Self::Straighten,
        Self::Geometry,
        Self::OpticalCorrections,
        Self::Effects,
        Self::AiSemanticMasks,
        Self::Timelapse,
    ];

    #[must_use]
    pub const fn family(self) -> ToolFamily {
        match self {
            Self::ExposureTone | Self::ToneCurves | Self::Effects => ToolFamily::Light,
            Self::WhiteBalanceColor | Self::SoftProofing => ToolFamily::Color,
            Self::DetailDenoise => ToolFamily::Detail,
            Self::Crop | Self::Straighten | Self::Geometry | Self::OpticalCorrections => {
                ToolFamily::Geometry
            }
            Self::AiSemanticMasks => ToolFamily::LocalMasks,
            Self::Timelapse => ToolFamily::Timelapse,
            Self::LegacyOther => ToolFamily::All,
        }
    }

    #[must_use]
    pub fn title(self) -> &'static str {
        self.title_lang(Language::Fr)
    }

    #[must_use]
    pub fn title_lang(self, lang: Language) -> &'static str {
        match self {
            Self::ExposureTone => Key::SectionExposureTone.translate(lang),
            Self::AiSemanticMasks => Key::SectionAiSemanticMasks.translate(lang),
            Self::ToneCurves => Key::SectionToneCurves.translate(lang),
            Self::WhiteBalanceColor => Key::SectionWhiteBalanceColor.translate(lang),
            Self::SoftProofing => Key::SectionSoftProofing.translate(lang),
            Self::Timelapse => Key::SectionTimelapse.translate(lang),
            Self::Crop => Key::SectionCrop.translate(lang),
            Self::Straighten => Key::SectionStraighten.translate(lang),
            Self::Geometry => Key::SectionGeometry.translate(lang),
            Self::DetailDenoise => Key::SectionDetailDenoise.translate(lang),
            Self::OpticalCorrections => Key::SectionOpticalCorrections.translate(lang),
            Self::Effects => Key::SectionEffects.translate(lang),
            Self::LegacyOther => "Autre",
        }
    }
}

/// Persistent user layout preferences saved to disk across sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppLayoutConfig {
    pub section_order: Vec<DarkroomSection>,
    #[serde(default)]
    pub active_tool_family: ToolFamily,
    #[serde(default)]
    pub active_tools_only: bool,
    /// Whether the bottom filmstrip is shown (toggled with `F9`).
    #[serde(default = "default_true")]
    pub filmstrip_visible: bool,
    /// Whether the translucent shooting-parameter HUD is painted over the canvas (`I`).
    #[serde(default = "default_true")]
    pub show_canvas_hud: bool,
    /// Active application UI language.
    #[serde(default)]
    pub language: Language,
    /// Last opened directory path, persisted across application restarts.
    #[serde(default)]
    pub last_open_directory: Option<PathBuf>,
    /// Last opened photo file path, persisted across application restarts.
    #[serde(default)]
    pub last_open_photo: Option<PathBuf>,
    /// User preference for the raw-only filmstrip filter, persisted across sessions.
    /// If `None`, defaults adaptively to `raw_count > 0`.
    #[serde(default)]
    pub raw_only_filter: Option<bool>,
    /// Active contextual tab in the left sidebar panel (toggled with `Cmd+1/2/3`).
    #[serde(default)]
    pub left_panel_tab: LeftPanelTab,
    /// Whether live WGPU compute preview acceleration is enabled (automatically falls back to CPU/Rayon when unavailable or for CPU-only modes).
    #[serde(default = "default_true")]
    pub gpu_preview_enabled: bool,
}

const fn default_true() -> bool {
    true
}

impl Default for AppLayoutConfig {
    fn default() -> Self {
        Self {
            section_order: DarkroomSection::ALL.to_vec(),
            active_tool_family: ToolFamily::All,
            active_tools_only: false,
            filmstrip_visible: true,
            show_canvas_hud: true,
            language: Language::Fr,
            last_open_directory: None,
            last_open_photo: None,
            raw_only_filter: None,
            left_panel_tab: LeftPanelTab::Explorer,
            gpu_preview_enabled: true,
        }
    }
}

impl AppLayoutConfig {
    #[must_use]
    pub fn config_path() -> Option<PathBuf> {
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
            active_tool_family: loaded.active_tool_family,
            active_tools_only: loaded.active_tools_only,
            filmstrip_visible: loaded.filmstrip_visible,
            show_canvas_hud: loaded.show_canvas_hud,
            language: loaded.language,
            last_open_directory: loaded.last_open_directory,
            last_open_photo: loaded.last_open_photo,
            raw_only_filter: loaded.raw_only_filter,
            left_panel_tab: loaded.left_panel_tab,
            gpu_preview_enabled: loaded.gpu_preview_enabled,
        }
    }

    /// Persists layout configuration to disk atomically via write-temp-then-rename.
    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Err(err) = atomic_write_str(&path, &json) {
                eprintln!(
                    "rraw: failed to persist layout configuration to {}: {err}",
                    path.display()
                );
            }
        }
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
    pub tone: ToneGradingParams,
    pub color: ColorGradingParams,
    pub denoise_sharpen: DenoiseSharpenParams,
    pub optics: OpticsGradingParams,
    pub effects: EffectsGradingParams,
    pub geometry: GeometryGradingParams,
    pub tone_curves: rraw_core::curves::ToneCurvesRecipe,
    pub custom_clut: Option<CustomClutState>,
    pub masks: MaskGradingState,
    pub soft_proof: rraw_color::SoftProofConfig,
    pub smart_lighting: rraw_core::SmartLightingParams,
}

impl DarkroomGradingState {
    /// Creates a completely untouched raw sensor grading state with all corrections disabled.
    ///
    /// Used for the "Avant" / Original reference view in before-and-after split comparisons.
    #[must_use]
    pub fn untouched_raw() -> Self {
        Self {
            preset: GradingPreset::AsShot,
            tone: ToneGradingParams::untouched_raw(),
            color: ColorGradingParams::untouched_raw(),
            denoise_sharpen: DenoiseSharpenParams::untouched_raw(),
            optics: OpticsGradingParams::untouched_raw(),
            effects: EffectsGradingParams::untouched_raw(),
            geometry: GeometryGradingParams::untouched_raw(),
            tone_curves: rraw_core::curves::ToneCurvesRecipe::default(),
            custom_clut: None,
            masks: MaskGradingState::default(),
            soft_proof: rraw_color::SoftProofConfig::default(),
            smart_lighting: rraw_core::SmartLightingParams {
                mode: rraw_core::SmartLightingMode::Off,
                intensity: 0.0,
                custom_shadow_lift: 0.0,
                custom_highlight_compression: 0.0,
            },
        }
    }

    /// Returns a mutable reference to the semantic mask layer for `category`, creating it if absent.
    pub fn upsert_semantic_mask(
        &mut self,
        category: rraw_core::SemanticMaskCategory,
    ) -> &mut SemanticMaskLayer {
        self.masks.upsert_semantic_mask(category)
    }

    /// Removes the semantic mask layer for `category` if present.
    pub fn remove_semantic_mask(&mut self, category: rraw_core::SemanticMaskCategory) {
        self.masks.remove_semantic_mask(category);
    }

    /// Converts all configured local and semantic mask layers into `rraw_core::Stage6MasksParams` for GPU Stage 6.
    #[must_use]
    pub fn to_stage6_masks_params(&self) -> rraw_core::Stage6MasksParams {
        let mut masks = Vec::new();
        for mask in &self.masks.local_masks {
            if !mask.enabled {
                continue;
            }
            match &mask.kind {
                LocalMaskKind::LinearGradient { start, end } => {
                    masks.push(rraw_core::GeometricMask {
                        id: mask.id.clone(),
                        kind: rraw_core::MaskKind::LinearGradient {
                            start: *start,
                            end: *end,
                        },
                        exposure_offset_ev: mask.exposure_ev,
                        temperature_shift: mask.temperature_shift,
                        invert: mask.invert,
                        feather: mask.feather.clamp(0.05, 1.0),
                        opacity: mask.opacity,
                        enabled: mask.enabled,
                    });
                }
                LocalMaskKind::RadialGradient {
                    center,
                    radius,
                    rotation_deg,
                } => {
                    masks.push(rraw_core::GeometricMask {
                        id: mask.id.clone(),
                        kind: rraw_core::MaskKind::RadialGradient {
                            center: *center,
                            radius: *radius,
                            rotation_deg: *rotation_deg,
                        },
                        exposure_offset_ev: mask.exposure_ev,
                        temperature_shift: mask.temperature_shift,
                        invert: mask.invert,
                        feather: mask.feather.clamp(0.05, 1.0),
                        opacity: mask.opacity,
                        enabled: mask.enabled,
                    });
                }
                LocalMaskKind::FreehandBrush { dabs, .. } => {
                    for (dab_idx, dab) in dabs.iter().enumerate() {
                        if dab.erase {
                            continue;
                        }
                        masks.push(rraw_core::GeometricMask {
                            id: format!("{}_{dab_idx}", mask.id),
                            kind: rraw_core::MaskKind::RadialGradient {
                                center: dab.pos,
                                radius: [dab.radius, dab.radius],
                                rotation_deg: 0.0,
                            },
                            exposure_offset_ev: mask.exposure_ev,
                            temperature_shift: mask.temperature_shift,
                            invert: mask.invert,
                            feather: mask.feather.clamp(0.05, 1.0),
                            opacity: (mask.opacity * dab.flow).clamp(0.0, 1.0),
                            enabled: mask.enabled,
                        });
                    }
                }
            }
        }
        for layer in &self.masks.semantic_masks {
            masks.push(rraw_core::GeometricMask {
                id: format!("ai_{}", layer.category.id_str()),
                kind: rraw_core::MaskKind::SemanticAi {
                    category: layer.category,
                    invert: layer.invert,
                    feather: layer.feather,
                },
                exposure_offset_ev: layer.exposure_ev,
                temperature_shift: layer.temperature_shift,
                invert: layer.invert,
                feather: layer.feather,
                opacity: layer.opacity,
                enabled: layer.enabled,
            });
        }
        rraw_core::Stage6MasksParams { masks }
    }
}

impl Default for DarkroomGradingState {
    fn default() -> Self {
        Self {
            preset: GradingPreset::AsShot,
            tone: ToneGradingParams::default(),
            color: ColorGradingParams::default(),
            denoise_sharpen: DenoiseSharpenParams::default(),
            optics: OpticsGradingParams::default(),
            effects: EffectsGradingParams::default(),
            geometry: GeometryGradingParams::default(),
            tone_curves: rraw_core::curves::ToneCurvesRecipe::default(),
            custom_clut: None,
            masks: MaskGradingState::default(),
            soft_proof: rraw_color::SoftProofConfig::default(),
            smart_lighting: rraw_core::SmartLightingParams {
                mode: rraw_core::SmartLightingMode::Slight,
                intensity: 1.0,
                custom_shadow_lift: 0.0,
                custom_highlight_compression: 0.0,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GradingRecipeClipboard {
    pub preset: GradingPreset,
    pub tone: ToneGradingParams,
    pub color: ColorGradingParams,
    pub tone_curves: rraw_core::curves::ToneCurvesRecipe,
    pub custom_clut: Option<CustomClutState>,
    pub creative_vignette_amount: f32,
    pub creative_vignette_midpoint: f32,
    pub creative_vignette_feather: f32,
    pub creative_vignette_roundness: f32,
    pub smart_lighting: rraw_core::SmartLightingParams,
}

impl GradingRecipeClipboard {
    /// Creates a new snapshot from the provided `DarkroomGradingState`.
    #[must_use]
    pub fn from_grading(grading: &DarkroomGradingState) -> Self {
        Self {
            preset: grading.preset,
            tone: grading.tone.clone(),
            color: grading.color.clone(),
            tone_curves: grading.tone_curves.clone(),
            custom_clut: grading.custom_clut.clone(),
            creative_vignette_amount: grading.effects.creative_vignette_amount,
            creative_vignette_midpoint: grading.effects.creative_vignette_midpoint,
            creative_vignette_feather: grading.effects.creative_vignette_feather,
            creative_vignette_roundness: grading.effects.creative_vignette_roundness,
            smart_lighting: grading.smart_lighting.clone(),
        }
    }

    /// Applies the color and tone recipe to target `DarkroomGradingState`,
    /// leaving photo-dependent geometry, crop, masks, optical corrections, and denoise intact.
    pub fn apply_to_grading(&self, grading: &mut DarkroomGradingState) {
        grading.preset = self.preset;
        grading.tone = self.tone.clone();
        grading.color = self.color.clone();
        grading.tone_curves = self.tone_curves.clone();
        grading.custom_clut = self.custom_clut.clone();
        grading.effects.creative_vignette_amount = self.creative_vignette_amount;
        grading.effects.creative_vignette_midpoint = self.creative_vignette_midpoint;
        grading.effects.creative_vignette_feather = self.creative_vignette_feather;
        grading.effects.creative_vignette_roundness = self.creative_vignette_roundness;
        grading.smart_lighting = self.smart_lighting.clone();
    }

    /// Formats a concise human-readable summary of the clipboard recipe.
    #[must_use]
    pub fn summary(&self) -> String {
        let preset_name = self.preset.name();
        let exp_sign = if self.tone.exposure_ev >= 0.0 {
            "+"
        } else {
            ""
        };
        format!(
            "{} · {}{:0.2} EV · {:0.0}K",
            preset_name, exp_sign, self.tone.exposure_ev, self.color.kelvin
        )
    }
}
