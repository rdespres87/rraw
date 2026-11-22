#![forbid(unsafe_code)]

//! Unit and integration tests for typed, zero-allocation internationalization (i18n).

use rraw_app::i18n::{Key, Language, ALL_KEYS};
use rraw_app::state::{AppLayoutConfig, DarkroomSection, ToolFamily};
use rraw_app::RrawApp;

#[test]
fn test_all_keys_resolve_non_empty_strings_in_all_languages() {
    for &lang in &Language::ALL {
        for key in ALL_KEYS {
            let translated = key.translate(lang);
            assert!(
                !translated.trim().is_empty(),
                "Key {:?} translated to empty string in language {:?}",
                key,
                lang
            );
        }
    }
}

#[test]
fn test_language_properties() {
    assert_eq!(Language::Fr.code(), "FR");
    assert_eq!(Language::En.code(), "EN");
    assert_eq!(Language::Fr.name(), "Français");
    assert_eq!(Language::En.name(), "English");
    assert_eq!(format!("{}", Language::Fr), "FR");
    assert_eq!(format!("{}", Language::En), "EN");
}

#[test]
fn test_layout_config_language_serde_backward_compat() -> Result<(), Box<dyn std::error::Error>> {
    // When `language` is missing from JSON (e.g. legacy layout.json), it must default to Fr
    let legacy_json = r#"{"section_order":[]}"#;
    let config: AppLayoutConfig = serde_json::from_str(legacy_json)?;
    assert_eq!(config.language, Language::Fr);

    // Serialization and deserialization roundtrip with English
    let config_en = AppLayoutConfig {
        language: Language::En,
        ..AppLayoutConfig::default()
    };
    let serialized = serde_json::to_string(&config_en)?;
    let deserialized: AppLayoutConfig = serde_json::from_str(&serialized)?;
    assert_eq!(deserialized.language, Language::En);
    Ok(())
}

#[test]
fn test_darkroom_sections_and_tool_families_i18n() {
    let exp_fr = DarkroomSection::ExposureTone.title_lang(Language::Fr);
    let exp_en = DarkroomSection::ExposureTone.title_lang(Language::En);
    assert_eq!(exp_fr, "☀️ Exposition & Tonalité");
    assert_eq!(exp_en, "☀️ Exposure & Tone");
    assert_ne!(exp_fr, exp_en);

    let light_fr = ToolFamily::Light.label_lang(Language::Fr);
    let light_en = ToolFamily::Light.label_lang(Language::En);
    assert_eq!(light_fr, "☀️ Lumière");
    assert_eq!(light_en, "☀️ Light");
    assert_ne!(light_fr, light_en);
}

#[test]
fn test_rraw_app_headless_language_toggle() {
    let mut app = RrawApp::new_headless();
    assert_eq!(app.lang(), Language::Fr);
    assert_eq!(app.t(Key::TopBarExport), "💾 Exporter...");

    // Switch dynamically to English
    app.layout_config.language = Language::En;
    assert_eq!(app.lang(), Language::En);
    assert_eq!(app.t(Key::TopBarExport), "💾 Export...");

    // Switch back to French
    app.layout_config.language = Language::Fr;
    assert_eq!(app.lang(), Language::Fr);
    assert_eq!(app.t(Key::TopBarExport), "💾 Exporter...");
}

#[test]
fn test_exif_tags_and_values_multilingual_resolution() {
    use rraw_app::i18n::{translate_exif_tag, translate_exif_val};

    // French
    assert_eq!(
        translate_exif_tag("Metering Mode", Language::Fr),
        "Mode de mesure"
    );
    assert_eq!(
        translate_exif_tag("White Balance Mode", Language::Fr),
        "Balance des blancs"
    );
    assert_eq!(
        translate_exif_val("Center-weighted average", Language::Fr),
        "Pondérée centrale"
    );
    assert_eq!(
        translate_exif_val("Pattern", Language::Fr),
        "Multi-zones (Matricielle)"
    );

    // English
    assert_eq!(
        translate_exif_tag("Metering Mode", Language::En),
        "Metering Mode"
    );
    assert_eq!(
        translate_exif_val("Center-weighted average", Language::En),
        "Center-weighted average"
    );

    // Spanish
    let es_lang = Language::custom("es", "Español");
    assert_eq!(
        translate_exif_tag("Metering Mode", es_lang),
        "Modo de medición"
    );
    assert_eq!(
        translate_exif_val("Center-weighted average", es_lang),
        "Ponderada al centro"
    );
}
