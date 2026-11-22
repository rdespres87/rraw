#![forbid(unsafe_code)]

//! Unit and integration tests for the scalable external resource-based i18n engine in `rraw-app`.

use rraw_app::i18n::{t, Catalog, I18nEngine, Key, Language, ALL_KEYS};

#[test]
fn test_default_embedded_catalogs_completeness() {
    let engine = I18nEngine::global();
    for &lang in &Language::ALL {
        for key in ALL_KEYS {
            let translated = engine.translate(lang, key);
            assert!(
                !translated.trim().is_empty(),
                "Key {:?} must not be empty in default catalog for {:?}",
                key,
                lang
            );
        }
    }
}

#[test]
fn test_dynamic_external_catalog_loading_and_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let engine = I18nEngine::global();

    // Incomplete Spanish JSON catalog with only a few keys translated
    let es_json = r#"{
        "TopBarOpenImage": "📂 Abrir imagen...",
        "TopBarExport": "💾 Exportar...",
        "TopBarZoom": "Zoom:"
    }"#;

    let es_lang = Language::custom("es", "Español");
    let catalog = Catalog::from_json_str(es_lang, es_json, Some(&engine.catalog(Language::En)))
        .map_err(|e| format!("Failed to parse catalog: {e}"))?;

    engine.register_catalog(catalog);

    // Verify translated keys
    assert_eq!(
        engine.translate(es_lang, Key::TopBarOpenImage),
        "📂 Abrir imagen..."
    );
    assert_eq!(
        engine.translate(es_lang, Key::TopBarExport),
        "💾 Exportar..."
    );

    // Verify fallback to English for omitted keys in the Spanish catalog
    assert_eq!(engine.translate(es_lang, Key::TopBarZoomFit), "Fit");
    assert_eq!(
        engine.translate(es_lang, Key::SidebarHistogramTitle),
        "📊 RGB Histogram"
    );
    Ok(())
}

#[test]
fn test_key_indexing_and_roundtrip() {
    assert_eq!(ALL_KEYS.len(), Key::COUNT);
    for (i, &key) in ALL_KEYS.iter().enumerate() {
        assert_eq!(key.index(), i);
        assert_eq!(Key::from_index(i), Some(key));
        assert_eq!(Key::from_str_name(key.as_str()), Some(key));
    }
    assert_eq!(Key::from_index(Key::COUNT), None);
    assert_eq!(Key::from_str_name("NonExistentKey123"), None);
}

#[test]
fn test_zero_allocation_repeated_lookups() {
    let engine = I18nEngine::global();
    // Warm-up and address verification: translations must return &'static str pointers
    let str1 = engine.translate(Language::Fr, Key::TopBarExport);
    let str2 = engine.translate(Language::Fr, Key::TopBarExport);
    assert_eq!(str1, str2);
    assert_eq!(
        str1.as_ptr(),
        str2.as_ptr(),
        "Static string references must point to identical memory addresses"
    );
}

#[test]
fn test_language_custom_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let es = Language::custom("es", "Español");
    let json = serde_json::to_string(&es)?;
    assert_eq!(json, "\"es\"");

    let deserialized: Language = serde_json::from_str(&json)?;
    assert_eq!(deserialized, es);
    assert_eq!(deserialized.code(), "ES");
    Ok(())
}

#[test]
fn test_free_function_t_delegation() {
    assert_eq!(t(Language::Fr, Key::TopBarExport), "💾 Exporter...");
    assert_eq!(t(Language::En, Key::TopBarExport), "💾 Export...");
}

#[test]
fn test_directory_discovery_of_assets_i18n() -> Result<(), Box<dyn std::error::Error>> {
    let engine = I18nEngine::global();
    let loaded = engine.load_from_directory(std::path::Path::new("../../assets/i18n"))?;
    assert!(loaded >= 1, "Should discover at least es.json");

    let es_lang = Language::custom("es", "Español");
    assert_eq!(
        engine.translate(es_lang, Key::TopBarOpenImage),
        "📂 Abrir imagen..."
    );
    assert_eq!(engine.translate(es_lang, Key::DarkroomTitle), "🛠️ Revelado");
    assert_eq!(es_lang.name(), "Español");
    Ok(())
}

#[test]
fn test_i18n_character_budget_guardrails() -> Result<(), Box<dyn std::error::Error>> {
    let engine = I18nEngine::global();
    let _ = engine.load_from_directory(std::path::Path::new("../../assets/i18n"));

    let languages = engine.available_languages();
    assert!(languages.len() >= 2);

    // 1. Triple horizontal inline buttons (Straighten angle reset and 90° rotations)
    // In a 360px panel, 3 buttons in one row MUST not exceed 15 characters each
    let triple_row_keys = [
        Key::InspectorResetAngle,
        Key::InspectorRotate90Left,
        Key::InspectorRotate90Right,
    ];

    for &lang in &languages {
        for &key in &triple_row_keys {
            let text = engine.translate(lang, key);
            assert!(
                text.chars().count() <= 15,
                "Key {:?} in language {:?} exceeds 15-char triple-row budget (length: {}, text: '{}')",
                key,
                lang,
                text.chars().count(),
                text
            );
        }
    }

    // 2. Short family titles (2-row grid of 3 buttons in inspector header)
    let short_family_keys = [
        Key::FamilyAllShort,
        Key::FamilyLightShort,
        Key::FamilyColorShort,
        Key::FamilyDetailShort,
        Key::FamilyGeometryShort,
        Key::FamilyLocalMasksShort,
        Key::FamilyTimelapseShort,
    ];

    for &lang in &languages {
        for &key in &short_family_keys {
            let text = engine.translate(lang, key);
            assert!(
                text.chars().count() <= 15,
                "Family key {:?} in language {:?} exceeds 15-char budget: '{}'",
                key,
                lang,
                text
            );
        }
    }

    // 3. Compact inline action buttons (must fit without blowing panel width)
    let compact_action_keys = [
        Key::StraightenHorizonDraw,
        Key::FilmstripToolbarPasteSelection,
        Key::InspectorResetCrop,
        Key::InspectorResetGeometry,
        Key::InspectorResetTone,
        Key::InspectorResetColor,
    ];

    for &lang in &languages {
        for &key in &compact_action_keys {
            let text = engine.translate(lang, key);
            assert!(
                text.chars().count() <= 28,
                "Action button key {:?} in language {:?} exceeds 28-char budget: '{}'",
                key,
                lang,
                text
            );
        }
    }

    Ok(())
}
