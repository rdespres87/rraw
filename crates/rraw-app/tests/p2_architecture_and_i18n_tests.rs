#![forbid(unsafe_code)]

//! Integration tests for architectural modularization (`presets`, `sidecar`, `mask_layers`)
//! and bundle-aware dynamic internationalization (`i18n`).

use std::path::{Path, PathBuf};

use rraw_app::i18n::{Catalog, I18nEngine, Language};
use rraw_app::mask_layers::{LocalMaskLayer, PrecomputedBrushGrid, SemanticMaskLayer};
use rraw_app::presets::GradingPreset;
use rraw_app::sidecar::SidecarRecipe;
use rraw_app::state::DarkroomGradingState;

#[test]
fn test_i18n_bundle_and_multi_path_directory_discovery() {
    let binary_path = Some(Path::new("/Applications/Rraw.app/Contents/MacOS/rraw-app"));
    let workspace_path = Some(Path::new("/workspace/Rraw"));
    let config_path = Some(Path::new("/Users/test/.config/rraw/layout.json"));

    let discovered =
        I18nEngine::candidate_i18n_directories(binary_path, workspace_path, config_path);

    let expected_paths = [
        PathBuf::from("/Applications/Rraw.app/Contents/Resources/assets/i18n"),
        PathBuf::from("/Applications/Rraw.app/Contents/MacOS/assets/i18n"),
        PathBuf::from("/workspace/Rraw/assets/i18n"),
        PathBuf::from("/Users/test/.config/rraw/i18n"),
    ];

    for expected in &expected_paths {
        assert!(
            discovered.iter().any(|p| p == expected),
            "Expected discovered i18n directories to contain {expected:?}, got {discovered:?}"
        );
    }
}

#[test]
fn test_custom_catalog_dynamic_override_for_presets_and_soft_proof_intents(
) -> Result<(), Box<dyn std::error::Error>> {
    let engine = I18nEngine::global();
    let de = Language::custom("de", "Deutsch");

    let catalog_json = r#"{
        "_language_name": "Deutsch",
        "Preset.AsShot.Name": "Originalaufnahme",
        "Preset.AsShot.Desc": "Unveränderte Kameraeinstellungen",
        "SoftProofIntent.Perceptual": "Perzeptiv (Sanfte Kompression)",
        "SoftProofIntent.RelativeColorimetric": "Relativ Farbmetrisch (Exakt)"
    }"#;

    let catalog = Catalog::from_json_str(de, catalog_json, Some(&engine.catalog(Language::En)))?;
    engine.register_catalog(catalog);

    // 1. Verify custom catalog overrides preset name and description dynamically without code changes
    assert_eq!(GradingPreset::AsShot.name_lang(de), "Originalaufnahme");
    assert_eq!(
        GradingPreset::AsShot.description_lang(de),
        "Unveränderte Kameraeinstellungen"
    );

    // 2. Verify soft-proof intent labels across FR, EN, ES, and custom DE
    assert_eq!(
        rraw_app::i18n::soft_proof_intent_label_lang(
            rraw_color::SoftProofRenderingIntent::Perceptual,
            Language::Fr
        ),
        "Perceptuel (Compression douce)"
    );
    assert_eq!(
        rraw_app::i18n::soft_proof_intent_label_lang(
            rraw_color::SoftProofRenderingIntent::Perceptual,
            Language::En
        ),
        "Perceptual (Smooth Compression)"
    );
    assert_eq!(
        rraw_app::i18n::soft_proof_intent_label_lang(
            rraw_color::SoftProofRenderingIntent::Perceptual,
            de
        ),
        "Perzeptiv (Sanfte Kompression)"
    );
    assert_eq!(
        rraw_app::i18n::soft_proof_intent_label_lang(
            rraw_color::SoftProofRenderingIntent::RelativeColorimetric,
            de
        ),
        "Relativ Farbmetrisch (Exakt)"
    );

    Ok(())
}

#[test]
fn test_modular_presets_sidecar_and_mask_layers_submodules_and_reexports() {
    let mut state = DarkroomGradingState::default();
    GradingPreset::FujiClassicChrome.apply(&mut state);
    assert_eq!(state.preset, GradingPreset::FujiClassicChrome);

    let mut brush = LocalMaskLayer::new_brush("Test Brush", 0.12, 0.85);
    assert!(brush.add_brush_dab_forced(0.5, 0.5));
    let grid = PrecomputedBrushGrid::from_layer(&brush);
    assert!(grid.is_some());

    state.local_masks.push(brush);
    state
        .semantic_masks
        .push(SemanticMaskLayer::new(rraw_core::SemanticMaskCategory::Sky));

    let recipe = SidecarRecipe::from_state(&state, 4);
    assert_eq!(recipe.rating, 4);

    let mut restored = DarkroomGradingState::default();
    recipe.apply_to(&mut restored);
    assert_eq!(restored.preset, GradingPreset::FujiClassicChrome);
    assert_eq!(restored.local_masks.len(), 1);
    assert_eq!(restored.semantic_masks.len(), 1);
}

#[test]
fn test_soft_proof_profiles_and_viewport_hud_multilingual_localization(
) -> Result<(), Box<dyn std::error::Error>> {
    let engine = I18nEngine::global();
    let es = Language::custom("es", "Español");
    let de = Language::custom("de", "Deutsch");

    let de_json = r#"{
        "_language_name": "Deutsch",
        "SoftProofProfile.FineArtMattePaper": "Fine Art Mattpapier (Baumwolle / Hahnemühle)",
        "SoftProofIntent.Perceptual.Short": "Perzeptiv",
        "SoftProofHud.Prefix": "🖨️ SOFTPROOF",
        "SoftProofHud.PaperInk": "Papier & Tinte (Dmin/Dmax)",
        "SoftProofHud.OutOfGamut": "Außerhalb Gamut"
    }"#;
    let catalog = Catalog::from_json_str(de, de_json, Some(&engine.catalog(Language::En)))?;
    engine.register_catalog(catalog);

    let cfg = rraw_color::SoftProofConfig {
        enabled: true,
        profile: rraw_color::SoftProofProfile::FineArtMattePaper,
        intent: rraw_color::SoftProofRenderingIntent::Perceptual,
        simulate_paper_and_ink: true,
        show_gamut_warning: true,
    };

    // 1. Profile full and short labels
    assert_eq!(
        rraw_app::i18n::soft_proof_profile_label_lang(cfg.profile, Language::Fr),
        "Papier Fine Art Mat (Coton / Hahnemühle)"
    );
    assert_eq!(
        rraw_app::i18n::soft_proof_profile_label_lang(cfg.profile, Language::En),
        "Fine Art Matte Paper (Cotton / Hahnemühle)"
    );
    assert_eq!(
        rraw_app::i18n::soft_proof_profile_label_lang(cfg.profile, es),
        "Papel Fine Art Mate (Algodón / Hahnemühle)"
    );
    assert_eq!(
        rraw_app::i18n::soft_proof_profile_label_lang(cfg.profile, de),
        "Fine Art Mattpapier (Baumwolle / Hahnemühle)"
    );
    assert_eq!(
        rraw_app::i18n::soft_proof_profile_short_label_lang(cfg.profile, Language::En),
        "Fine Art Matte"
    );

    // 2. Viewport Soft Proofing HUD banner (exact string from user screenshot)
    let hud_fr = rraw_app::i18n::format_soft_proofing_hud_text(&cfg, 3227, Language::Fr);
    assert_eq!(
        hud_fr,
        "🖨️ ÉPREUVAGE : Papier Fine Art Mat (Coton / Hahnemühle) (Perceptuel | Papier & Encre (Dmin/Dmax)) | Hors-Gamut: 3227 px"
    );

    let hud_en = rraw_app::i18n::format_soft_proofing_hud_text(&cfg, 3227, Language::En);
    assert_eq!(
        hud_en,
        "🖨️ SOFT PROOF : Fine Art Matte Paper (Cotton / Hahnemühle) (Perceptual | Paper & Ink (Dmin/Dmax)) | Out-of-Gamut: 3227 px"
    );

    let hud_es = rraw_app::i18n::format_soft_proofing_hud_text(&cfg, 3227, es);
    assert_eq!(
        hud_es,
        "🖨️ PRUEBA DE COLOR : Papel Fine Art Mate (Algodón / Hahnemühle) (Perceptual | Papel y Tinta (Dmin/Dmax)) | Fuera de gama: 3227 px"
    );

    let hud_de = rraw_app::i18n::format_soft_proofing_hud_text(&cfg, 3227, de);
    assert_eq!(
        hud_de,
        "🖨️ SOFTPROOF : Fine Art Mattpapier (Baumwolle / Hahnemühle) (Perzeptiv | Papier & Tinte (Dmin/Dmax)) | Außerhalb Gamut: 3227 px"
    );

    Ok(())
}
