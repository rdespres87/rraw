#![forbid(unsafe_code)]

//! Strongly-typed, scalable internationalization (i18n) engine for `rraw-app`.
//!
//! # Architecture & Guarantees
//! - **Zero Runtime Allocation**: All translations return `&'static str` literals with zero heap allocations during the render loop.
//! - **Scalable External Resource-Based Catalogs**: Translations are stored in external JSON catalogs under `assets/i18n/`.
//!   Default catalogs (FR, EN) are bundled at compile time for fully standalone operation.
//! - **Dynamic Zero-Code Language Addition**: New languages (e.g. Spanish, German, Japanese) can be added simply by dropping
//!   a JSON translation file into `assets/i18n/` or user configuration directory without modifying core Rust code.
//! - **Fallback Guarantee**: Incomplete translation files automatically fall back to English with zero runtime panics.
//! - **Reactive O(1) Switching**: UI switches language instantly by mutating `app.layout_config.language`.
//! - **Persistence**: Language selection is saved to `~/.config/rraw/layout.json` via Serde.

pub mod catalog;
pub mod engine;
pub mod exif_map;
pub mod key;
pub mod language;

pub use catalog::Catalog;
pub use engine::I18nEngine;
pub use exif_map::{exif_tag_to_key, exif_val_to_key, translate_exif_tag, translate_exif_val};
pub use key::{Key, ALL_KEYS};
pub use language::Language;

/// Global helper to translate a key for a given language.
#[inline]
#[must_use]
pub fn t(lang: Language, key: Key) -> &'static str {
    key.translate(lang)
}

/// Returns the localized human-readable label for a `SoftProofRenderingIntent`,
/// consulting custom external catalog overrides first before falling back to built-in translations.
#[must_use]
pub fn soft_proof_intent_label_lang(
    intent: rraw_color::SoftProofRenderingIntent,
    lang: Language,
) -> &'static str {
    let custom_key = match intent {
        rraw_color::SoftProofRenderingIntent::Perceptual => "SoftProofIntent.Perceptual",
        rraw_color::SoftProofRenderingIntent::RelativeColorimetric => {
            "SoftProofIntent.RelativeColorimetric"
        }
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (intent, lang) {
        (rraw_color::SoftProofRenderingIntent::Perceptual, Language::Fr) => {
            "Perceptuel (Compression douce)"
        }
        (rraw_color::SoftProofRenderingIntent::Perceptual, _) if is_es => {
            "Perceptual (Compresión suave)"
        }
        (rraw_color::SoftProofRenderingIntent::Perceptual, _) => "Perceptual (Smooth Compression)",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, Language::Fr) => {
            "Colorimétrique Relatif (Exact / Écrêtage)"
        }
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) if is_es => {
            "Colorimétrico Relativo (Exacto / Recorte)"
        }
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) => {
            "Relative Colorimetric (Exact / Clipping)"
        }
    }
}

/// Returns the short compact label for a `SoftProofRenderingIntent` (used in top bar & viewport HUD).
#[must_use]
pub fn soft_proof_intent_short_label_lang(
    intent: rraw_color::SoftProofRenderingIntent,
    lang: Language,
) -> &'static str {
    let custom_key = match intent {
        rraw_color::SoftProofRenderingIntent::Perceptual => "SoftProofIntent.Perceptual.Short",
        rraw_color::SoftProofRenderingIntent::RelativeColorimetric => {
            "SoftProofIntent.RelativeColorimetric.Short"
        }
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (intent, lang) {
        (rraw_color::SoftProofRenderingIntent::Perceptual, Language::Fr) => "Perceptuel",
        (rraw_color::SoftProofRenderingIntent::Perceptual, _) => "Perceptual",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, Language::Fr) => "Relatif",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) if is_es => "Relativo",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) => "Relative",
    }
}

/// Returns the localized human-readable label for a `SoftProofProfile`,
/// consulting custom external catalog overrides first before falling back to built-in translations.
#[must_use]
pub fn soft_proof_profile_label_lang(
    profile: rraw_color::SoftProofProfile,
    lang: Language,
) -> &'static str {
    let custom_key = match profile {
        rraw_color::SoftProofProfile::FineArtMattePaper => "SoftProofProfile.FineArtMattePaper",
        rraw_color::SoftProofProfile::GlossyBarytaPaper => "SoftProofProfile.GlossyBarytaPaper",
        rraw_color::SoftProofProfile::SrgbIec61966 => "SoftProofProfile.SrgbIec61966",
        rraw_color::SoftProofProfile::DisplayP3 => "SoftProofProfile.DisplayP3",
        rraw_color::SoftProofProfile::AdobeRgb1998 => "SoftProofProfile.AdobeRgb1998",
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (profile, lang) {
        (rraw_color::SoftProofProfile::SrgbIec61966, _) => "sRGB IEC61966-2.1 (Web / SDR)",
        (rraw_color::SoftProofProfile::DisplayP3, Language::Fr) => {
            "Apple Display P3 (Écran large gamut)"
        }
        (rraw_color::SoftProofProfile::DisplayP3, _) if is_es => {
            "Apple Display P3 (Pantalla amplia gama)"
        }
        (rraw_color::SoftProofProfile::DisplayP3, _) => "Apple Display P3 (Wide-Gamut Display)",
        (rraw_color::SoftProofProfile::AdobeRgb1998, Language::Fr) => {
            "Adobe RGB (1998) (Pré-presse)"
        }
        (rraw_color::SoftProofProfile::AdobeRgb1998, _) if is_es => {
            "Adobe RGB (1998) (Preimpresión)"
        }
        (rraw_color::SoftProofProfile::AdobeRgb1998, _) => "Adobe RGB (1998) (Prepress)",
        (rraw_color::SoftProofProfile::FineArtMattePaper, Language::Fr) => {
            "Papier Fine Art Mat (Coton / Hahnemühle)"
        }
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) if is_es => {
            "Papel Fine Art Mate (Algodón / Hahnemühle)"
        }
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) => {
            "Fine Art Matte Paper (Cotton / Hahnemühle)"
        }
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, Language::Fr) => {
            "Papier Photo Baryté / Brillant (Glossy)"
        }
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) if is_es => {
            "Papel Fotográfico Baritado / Brillante (Glossy)"
        }
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) => {
            "Glossy / Baryta Photo Paper (Glossy)"
        }
    }
}

/// Returns the short compact badge label for a `SoftProofProfile`.
#[must_use]
pub fn soft_proof_profile_short_label_lang(
    profile: rraw_color::SoftProofProfile,
    lang: Language,
) -> &'static str {
    let custom_key = match profile {
        rraw_color::SoftProofProfile::FineArtMattePaper => {
            "SoftProofProfile.FineArtMattePaper.Short"
        }
        rraw_color::SoftProofProfile::GlossyBarytaPaper => {
            "SoftProofProfile.GlossyBarytaPaper.Short"
        }
        rraw_color::SoftProofProfile::SrgbIec61966 => "SoftProofProfile.SrgbIec61966.Short",
        rraw_color::SoftProofProfile::DisplayP3 => "SoftProofProfile.DisplayP3.Short",
        rraw_color::SoftProofProfile::AdobeRgb1998 => "SoftProofProfile.AdobeRgb1998.Short",
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (profile, lang) {
        (rraw_color::SoftProofProfile::SrgbIec61966, _) => "sRGB",
        (rraw_color::SoftProofProfile::DisplayP3, _) => "Display P3",
        (rraw_color::SoftProofProfile::AdobeRgb1998, _) => "Adobe RGB",
        (rraw_color::SoftProofProfile::FineArtMattePaper, Language::Fr) => "Fine Art Mat",
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) if is_es => "Fine Art Mate",
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) => "Fine Art Matte",
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, Language::Fr) => "Photo Baryté",
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) if is_es => "Foto Baritado",
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) => "Glossy Baryta",
    }
}

/// Formats the localized viewport Soft Proofing HUD banner string.
#[must_use]
pub fn format_soft_proofing_hud_text(
    soft_proof: &rraw_color::SoftProofConfig,
    out_of_gamut_count: u32,
    lang: Language,
) -> String {
    let engine = I18nEngine::global();
    let is_es = lang.is_spanish();

    let prefix = engine
        .translate_custom(lang, "SoftProofHud.Prefix")
        .unwrap_or(if lang == Language::Fr {
            "🖨️ ÉPREUVAGE"
        } else if is_es {
            "🖨️ PRUEBA DE COLOR"
        } else {
            "🖨️ SOFT PROOF"
        });

    let profile_str = soft_proof_profile_label_lang(soft_proof.profile, lang);
    let intent_str = soft_proof_intent_short_label_lang(soft_proof.intent, lang);

    let paper_str = if soft_proof.simulate_paper_and_ink {
        let paper_label = engine
            .translate_custom(lang, "SoftProofHud.PaperInk")
            .unwrap_or(if lang == Language::Fr {
                "Papier & Encre (Dmin/Dmax)"
            } else if is_es {
                "Papel y Tinta (Dmin/Dmax)"
            } else {
                "Paper & Ink (Dmin/Dmax)"
            });
        format!(" | {paper_label}")
    } else {
        String::new()
    };

    let oog_label = engine
        .translate_custom(lang, "SoftProofHud.OutOfGamut")
        .unwrap_or(if lang == Language::Fr {
            "Hors-Gamut"
        } else if is_es {
            "Fuera de gama"
        } else {
            "Out-of-Gamut"
        });

    format!(
        "{prefix} : {profile_str} ({intent_str}{paper_str}) | {oog_label}: {out_of_gamut_count} px"
    )
}
