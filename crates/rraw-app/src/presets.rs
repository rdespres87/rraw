#![forbid(unsafe_code)]

//! Photographic development presets and 1-click styling definitions.

use rraw_color::DisplayTransform;
use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::state::DarkroomGradingState;

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
    SepiaVintage,
    Custom,
}

impl GradingPreset {
    pub const ALL: [Self; 19] = [
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
        Self::SepiaVintage,
        Self::Custom,
    ];

    /// Dynamic JSON catalog lookup key for this preset's display name.
    #[must_use]
    pub const fn name_i18n_key(self) -> &'static str {
        match self {
            Self::AsShot => "Preset.AsShot.Name",
            Self::DaylightPunch => "Preset.DaylightPunch.Name",
            Self::PortraitSoft => "Preset.PortraitSoft.Name",
            Self::CinematicAgX => "Preset.CinematicAgX.Name",
            Self::MonochromeFineArt => "Preset.MonochromeFineArt.Name",
            Self::LandscapeVivid => "Preset.LandscapeVivid.Name",
            Self::GoldenHourWarmth => "Preset.GoldenHourWarmth.Name",
            Self::MoodyMatte => "Preset.MoodyMatte.Name",
            Self::HighKeyAiry => "Preset.HighKeyAiry.Name",
            Self::StreetContrastBW => "Preset.StreetContrastBW.Name",
            Self::VintageFilm => "Preset.VintageFilm.Name",
            Self::NightAstro => "Preset.NightAstro.Name",
            Self::FujiClassicChrome => "Preset.FujiClassicChrome.Name",
            Self::KodakPortra400 => "Preset.KodakPortra400.Name",
            Self::FujiVelvia50 => "Preset.FujiVelvia50.Name",
            Self::KodakTriX400 => "Preset.KodakTriX400.Name",
            Self::IlfordHp5 => "Preset.IlfordHp5.Name",
            Self::SepiaVintage => "Preset.SepiaVintage.Name",
            Self::Custom => "Preset.Custom.Name",
        }
    }

    /// Dynamic JSON catalog lookup key for this preset's description.
    #[must_use]
    pub const fn description_i18n_key(self) -> &'static str {
        match self {
            Self::AsShot => "Preset.AsShot.Desc",
            Self::DaylightPunch => "Preset.DaylightPunch.Desc",
            Self::PortraitSoft => "Preset.PortraitSoft.Desc",
            Self::CinematicAgX => "Preset.CinematicAgX.Desc",
            Self::MonochromeFineArt => "Preset.MonochromeFineArt.Desc",
            Self::LandscapeVivid => "Preset.LandscapeVivid.Desc",
            Self::GoldenHourWarmth => "Preset.GoldenHourWarmth.Desc",
            Self::MoodyMatte => "Preset.MoodyMatte.Desc",
            Self::HighKeyAiry => "Preset.HighKeyAiry.Desc",
            Self::StreetContrastBW => "Preset.StreetContrastBW.Desc",
            Self::VintageFilm => "Preset.VintageFilm.Desc",
            Self::NightAstro => "Preset.NightAstro.Desc",
            Self::FujiClassicChrome => "Preset.FujiClassicChrome.Desc",
            Self::KodakPortra400 => "Preset.KodakPortra400.Desc",
            Self::FujiVelvia50 => "Preset.FujiVelvia50.Desc",
            Self::KodakTriX400 => "Preset.KodakTriX400.Desc",
            Self::IlfordHp5 => "Preset.IlfordHp5.Desc",
            Self::SepiaVintage => "Preset.SepiaVintage.Desc",
            Self::Custom => "Preset.Custom.Desc",
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        self.name_lang(Language::Fr)
    }

    #[must_use]
    pub fn name_lang(self, lang: Language) -> &'static str {
        crate::i18n::tc(lang, self.name_i18n_key())
    }

    #[must_use]
    pub fn description(self) -> &'static str {
        self.description_lang(Language::Fr)
    }

    #[must_use]
    pub fn description_lang(self, lang: Language) -> &'static str {
        crate::i18n::tc(lang, self.description_i18n_key())
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
            Self::SepiaVintage => "SepiaVintage",
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
            "SepiaVintage" => Some(Self::SepiaVintage),
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
                grading.tone.exposure_ev = 0.3;
                grading.tone.contrast = 15.0;
                grading.tone.highlights = -10.0;
                grading.tone.shadows = 10.0;
                grading.color.kelvin = 5600.0;
                grading.color.tint = 0.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 35.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::PortraitSoft => {
                grading.tone.exposure_ev = 0.6;
                grading.tone.contrast = -8.0;
                grading.tone.highlights = -15.0;
                grading.tone.shadows = 20.0;
                grading.color.kelvin = 5200.0;
                grading.color.tint = 2.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.denoise_sharpen.wavelet_denoise_sigma = 1.5;
                grading.optics.vignette_amount = 20.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::CinematicAgX => {
                grading.tone.exposure_ev = 0.2;
                grading.tone.contrast = 25.0;
                grading.tone.highlights = -20.0;
                grading.tone.shadows = -10.0;
                grading.color.kelvin = 6000.0;
                grading.color.tint = -4.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 60.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::MonochromeFineArt => {
                grading.tone.exposure_ev = 0.0;
                grading.tone.contrast = 35.0;
                grading.tone.highlights = 15.0;
                grading.tone.shadows = -15.0;
                grading.denoise_sharpen.wavelet_denoise_sigma = 0.5;
                grading.denoise_sharpen.capture_sharpening_amount = 0.8;
                grading.optics.vignette_amount = 60.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = true;
            }
            Self::LandscapeVivid => {
                grading.tone.exposure_ev = 0.35;
                grading.tone.contrast = 22.0;
                grading.tone.highlights = -25.0;
                grading.tone.shadows = 18.0;
                grading.color.kelvin = 5400.0;
                grading.color.tint = -3.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.denoise_sharpen.capture_sharpening_amount = 0.7;
                grading.optics.vignette_amount = 30.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::GoldenHourWarmth => {
                grading.tone.exposure_ev = 0.5;
                grading.tone.contrast = 12.0;
                grading.tone.highlights = -20.0;
                grading.tone.shadows = 22.0;
                grading.color.kelvin = 6300.0;
                grading.color.tint = 5.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 40.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::MoodyMatte => {
                grading.tone.exposure_ev = -0.25;
                grading.tone.contrast = -15.0;
                grading.tone.highlights = -15.0;
                grading.tone.shadows = 32.0;
                grading.color.kelvin = 5100.0;
                grading.color.tint = -6.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 50.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::HighKeyAiry => {
                grading.tone.exposure_ev = 0.9;
                grading.tone.contrast = -10.0;
                grading.tone.highlights = -30.0;
                grading.tone.shadows = 15.0;
                grading.color.kelvin = 5300.0;
                grading.color.tint = 2.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.denoise_sharpen.wavelet_denoise_sigma = 1.2;
                grading.optics.vignette_amount = 15.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::StreetContrastBW => {
                grading.tone.exposure_ev = 0.1;
                grading.tone.contrast = 48.0;
                grading.tone.highlights = 20.0;
                grading.tone.shadows = -28.0;
                grading.denoise_sharpen.capture_sharpening_amount = 1.0;
                grading.optics.vignette_amount = 45.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = true;
            }
            Self::VintageFilm => {
                grading.tone.exposure_ev = 0.15;
                grading.tone.contrast = 8.0;
                grading.tone.highlights = -12.0;
                grading.tone.shadows = 18.0;
                grading.color.kelvin = 5800.0;
                grading.color.tint = 8.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 65.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::NightAstro => {
                grading.tone.exposure_ev = 0.75;
                grading.tone.contrast = 28.0;
                grading.tone.highlights = -12.0;
                grading.tone.shadows = 10.0;
                grading.color.kelvin = 4200.0;
                grading.color.tint = -5.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.denoise_sharpen.wavelet_denoise_sigma = 2.2;
                grading.denoise_sharpen.capture_sharpening_amount = 0.6;
                grading.optics.vignette_amount = 40.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
            }
            Self::FujiClassicChrome => {
                grading.tone.exposure_ev = 0.0;
                grading.tone.contrast = 14.0;
                grading.tone.highlights = -10.0;
                grading.tone.shadows = 6.0;
                grading.tone.blacks = -8.0;
                grading.tone.saturation = -18.0;
                grading.tone.vibrance = -6.0;
                grading.color.kelvin = 5400.0;
                grading.color.tint = -2.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 25.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.0), (0.25, 0.22), (0.75, 0.77), (1.0, 1.0)]);
                grading.tone_curves.red =
                    curve_from_points(&[(0.0, 0.0), (0.70, 0.68), (1.0, 0.98)]);
                grading.tone_curves.blue =
                    curve_from_points(&[(0.0, 0.0), (0.65, 0.67), (1.0, 1.0)]);
            }
            Self::KodakPortra400 => {
                grading.tone.exposure_ev = 0.25;
                grading.tone.contrast = -6.0;
                grading.tone.highlights = -16.0;
                grading.tone.shadows = 18.0;
                grading.tone.blacks = 8.0;
                grading.tone.saturation = 4.0;
                grading.tone.vibrance = 10.0;
                grading.color.kelvin = 5800.0;
                grading.color.tint = 4.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 20.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.025), (0.5, 0.52), (1.0, 0.98)]);
                grading.tone_curves.red = curve_from_points(&[(0.0, 0.0), (0.5, 0.53), (1.0, 1.0)]);
            }
            Self::FujiVelvia50 => {
                grading.tone.exposure_ev = -0.1;
                grading.tone.contrast = 28.0;
                grading.tone.highlights = 8.0;
                grading.tone.shadows = -14.0;
                grading.tone.blacks = -15.0;
                grading.tone.saturation = 28.0;
                grading.tone.vibrance = 20.0;
                grading.color.kelvin = 5600.0;
                grading.color.tint = 0.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 35.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = false;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.0), (0.25, 0.20), (0.75, 0.80), (1.0, 1.0)]);
            }
            Self::KodakTriX400 => {
                grading.tone.exposure_ev = 0.15;
                grading.tone.contrast = 36.0;
                grading.tone.highlights = 12.0;
                grading.tone.shadows = -18.0;
                grading.tone.blacks = -15.0;
                grading.denoise_sharpen.capture_sharpening_amount = 0.8;
                grading.optics.vignette_amount = 40.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = true;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.0), (0.30, 0.24), (0.70, 0.78), (1.0, 1.0)]);
            }
            Self::IlfordHp5 => {
                grading.tone.exposure_ev = 0.2;
                grading.tone.contrast = 10.0;
                grading.tone.highlights = -12.0;
                grading.tone.shadows = 16.0;
                grading.tone.blacks = 5.0;
                grading.denoise_sharpen.capture_sharpening_amount = 0.5;
                grading.optics.vignette_amount = 25.0;
                grading.optics.vignette_correction = true;
                grading.color.is_monochrome = true;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.02), (0.5, 0.51), (1.0, 0.985)]);
            }
            Self::SepiaVintage => {
                grading.tone.exposure_ev = 0.20;
                grading.tone.contrast = 22.0;
                grading.tone.highlights = -15.0;
                grading.tone.shadows = 15.0;
                grading.tone.blacks = 5.0;
                grading.tone.saturation = -35.0;
                grading.tone.vibrance = 10.0;
                grading.color.kelvin = 6800.0;
                grading.color.tint = 8.0;
                grading.color.display_transform = DisplayTransform::AgX;
                grading.optics.vignette_amount = 30.0;
                grading.optics.vignette_correction = true;
                grading.effects.creative_vignette_amount = -35.0;
                grading.color.is_monochrome = false;
                grading.tone_curves.luma =
                    curve_from_points(&[(0.0, 0.04), (0.35, 0.32), (0.75, 0.78), (1.0, 0.98)]);
                grading.tone_curves.red =
                    curve_from_points(&[(0.0, 0.05), (0.50, 0.54), (1.0, 1.0)]);
                grading.tone_curves.blue =
                    curve_from_points(&[(0.0, 0.01), (0.50, 0.44), (1.0, 0.92)]);
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
            Self::SepiaVintage,
        ] {
            let mut test = DarkroomGradingState::default();
            preset.apply(&mut test);
            if grading.tone == test.tone
                && grading.color == test.color
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
