#![forbid(unsafe_code)]

//! Photographic development presets and 1-click styling definitions.

use rraw_color::DisplayTransform;
use serde::{Deserialize, Serialize};

use crate::i18n::{I18nEngine, Language};
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
        if let Some(custom_val) = I18nEngine::global().translate_custom(lang, self.name_i18n_key())
        {
            return custom_val;
        }
        let is_es = lang.is_spanish();
        match (self, lang) {
            (Self::AsShot, Language::Fr) => "À la prise de vue (D'origine)",
            (Self::AsShot, _) if is_es => "Al disparar (Original)",
            (Self::AsShot, _) => "As Shot (Camera Original)",
            (Self::DaylightPunch, _) => "Daylight Punch (+0.3 EV, 5600K)",
            (Self::PortraitSoft, _) => "Portrait Soft (+0.6 EV, 5200K)",
            (Self::CinematicAgX, _) => "Cinematic AgX (+0.2 EV, 6000K)",
            (Self::MonochromeFineArt, _) => "B&W Fine Art (Monochrome)",
            (Self::LandscapeVivid, _) => "Landscape Vivid (+0.35 EV, 5400K)",
            (Self::GoldenHourWarmth, _) => "Golden Hour Warmth (+0.5 EV, 6300K)",
            (Self::MoodyMatte, _) => "Moody Matte (-0.25 EV, 5100K)",
            (Self::HighKeyAiry, _) => "High-Key Airy (+0.9 EV, 5300K)",
            (Self::StreetContrastBW, _) => "Street Contrast B&W (Monochrome)",
            (Self::VintageFilm, _) => "Vintage Film (+0.15 EV, 5800K)",
            (Self::NightAstro, _) => "Night & Astro (+0.75 EV, 4200K)",
            (Self::FujiClassicChrome, _) => "Fujifilm Classic Chrome",
            (Self::KodakPortra400, _) => "Kodak Portra 400",
            (Self::FujiVelvia50, _) => "Fujifilm Velvia 50",
            (Self::KodakTriX400, Language::Fr) => "Kodak Tri-X 400 (N&B)",
            (Self::KodakTriX400, _) if is_es => "Kodak Tri-X 400 (B&N)",
            (Self::KodakTriX400, _) => "Kodak Tri-X 400 (B&W)",
            (Self::IlfordHp5, Language::Fr) => "Ilford HP5 Plus (N&B)",
            (Self::IlfordHp5, _) if is_es => "Ilford HP5 Plus (B&N)",
            (Self::IlfordHp5, _) => "Ilford HP5 Plus (B&W)",
            (Self::SepiaVintage, Language::Fr) => "Sépia Vintage (Virage chaud)",
            (Self::SepiaVintage, _) if is_es => "Sepia Vintage (Virado cálido)",
            (Self::SepiaVintage, _) => "Sepia Vintage (Warm Toning)",
            (Self::Custom, Language::Fr) => "Personnalisé",
            (Self::Custom, _) if is_es => "Personalizado",
            (Self::Custom, _) => "Custom",
        }
    }

    #[must_use]
    pub fn description(self) -> &'static str {
        self.description_lang(Language::Fr)
    }

    #[must_use]
    pub fn description_lang(self, lang: Language) -> &'static str {
        if let Some(custom_val) =
            I18nEngine::global().translate_custom(lang, self.description_i18n_key())
        {
            return custom_val;
        }
        let is_es = lang.is_spanish();
        if lang == Language::Fr {
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
                Self::MoodyMatte => {
                    "Ambiance cinématographique feutrée, noirs légèrement décollés"
                }
                Self::HighKeyAiry => {
                    "Tonalité lumineuse, surexposition maîtrisée et teintes éthérées"
                }
                Self::StreetContrastBW => "Noir & blanc urbain percutant, micro-contraste élevé",
                Self::VintageFilm => "Grain et patine argentique vintage, vignetage marqué",
                Self::NightAstro => {
                    "Ciel nocturne clarifié (4200K), débruitage ondelettes renforcé"
                }
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
                Self::SepiaVintage => {
                    "Virage au sulfure d'époque : ombres chaudes brun-chocolat & hautes lumières ambrées"
                }
                Self::Custom => "Réglages personnalisés par l'utilisateur",
            }
        } else if is_es {
            match self {
                Self::AsShot => "Ajustes originales de la cámara sin edición",
                Self::DaylightPunch => "Contraste dinámico y sombras realzadas para luz diurna",
                Self::PortraitSoft => "Tono suave, tonos de piel cálidos y contraste atenuado",
                Self::CinematicAgX => {
                    "Rendimiento cinematográfico analógico con compresión suave de altas luces"
                }
                Self::MonochromeFineArt => "Blanco y negro artístico profundo y modelado fino",
                Self::LandscapeVivid => "Colores ricos, sombras abiertas y nitidez para paisajes",
                Self::GoldenHourWarmth => "Luz dorada del atardecer (6300K) y sombras cálidas",
                Self::MoodyMatte => "Ambiente cinematográfico mate, negros ligeramente elevados",
                Self::HighKeyAiry => {
                    "Tonalidad luminosa, sobreexposición controlada y tonos etéreos"
                }
                Self::StreetContrastBW => "Blanco y negro urbano contundente, alto microcontraste",
                Self::VintageFilm => "Grano y pátina analógica vintage, viñeteado pronunciado",
                Self::NightAstro => {
                    "Cielo nocturno clarificado (4200K), reducción de ruido por ondículas"
                }
                Self::FujiClassicChrome => {
                    "Rendimiento documental suave, sombras densas, saturación atenuada"
                }
                Self::KodakPortra400 => {
                    "Colores pastel, tonos de piel favorecedores, sombras abiertas"
                }
                Self::FujiVelvia50 => {
                    "Diapositiva mítica de saturación viva, negros profundos y cielos intensos"
                }
                Self::KodakTriX400 => "Blanco y negro clásico de fotoperiodismo, contraste firme",
                Self::IlfordHp5 => "Blanco y negro analógico de gran latitud, gradaciones sutiles",
                Self::SepiaVintage => {
                    "Virado clásico al sulfuro: sombras cálidas marrón chocolate y luces ámbar"
                }
                Self::Custom => "Ajustes personalizados por el usuario",
            }
        } else {
            match self {
                Self::AsShot => "Original camera settings without adjustments",
                Self::DaylightPunch => "Dynamic contrast & boosted shadows for bright daylight",
                Self::PortraitSoft => "Soft tint, flattering skin tones and gentle contrast",
                Self::CinematicAgX => "Film cinematic look with smooth highlight compression",
                Self::MonochromeFineArt => {
                    "Deep fine-art black & white with subtle midtone tonality"
                }
                Self::LandscapeVivid => "Rich vibrant colors, opened shadows and landscape clarity",
                Self::GoldenHourWarmth => "Sunset golden hour light (6300K) and warm shadows",
                Self::MoodyMatte => "Subdued moody cinema atmosphere with lifted matte blacks",
                Self::HighKeyAiry => {
                    "Bright airy tonality, controlled exposure and ethereal highlights"
                }
                Self::StreetContrastBW => "Punchy urban street B&W with high micro-contrast",
                Self::VintageFilm => "Vintage analog film look with warm patina and vignetting",
                Self::NightAstro => "Clarified night sky (4200K) with enhanced wavelet denoise",
                Self::FujiClassicChrome => {
                    "Documentary editorial look, dense shadows and muted tones with cyan blues"
                }
                Self::KodakPortra400 => {
                    "Pastel colors, flattering skin tones, lifted shadows and smooth highlights"
                }
                Self::FujiVelvia50 => {
                    "Legendary slide film with vivid saturation, ink blacks and deep skies"
                }
                Self::KodakTriX400 => {
                    "Classic photojournalism B&W with assertive contrast and deep blacks"
                }
                Self::IlfordHp5 => {
                    "Wide latitude analog B&W with soft tonality and subtle gradients"
                }
                Self::SepiaVintage => {
                    "Antique sulfide toning: warm chocolate shadows & amber highlights"
                }
                Self::Custom => "Custom user adjustments",
            }
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
