#![forbid(unsafe_code)]

//! Integration tests for the Smart Lighting UI, inspector controls, clipboard copy/paste, and i18n keys.

use rraw_app::app::RrawApp;
use rraw_app::i18n::{Key, Language};
use rraw_app::state::{DarkroomGradingState, DarkroomSection, GradingRecipeClipboard};
use rraw_core::{SmartLightingMode, SmartLightingParams};

// ---------------------------------------------------------------------------
// 1. Clipboard copy / paste round-trip (Phase 12 model)
// ---------------------------------------------------------------------------
#[test]
fn test_smart_lighting_clipboard_copy_paste() {
    let source_state = DarkroomGradingState {
        smart_lighting: SmartLightingParams {
            mode: SmartLightingMode::Strong,
            intensity: 75.0,
            custom_shadow_lift: 0.60,
            custom_highlight_compression: 0.40,
        },
        ..Default::default()
    };

    // Copy to clipboard snapshot
    let clipboard = GradingRecipeClipboard::from_grading(&source_state);
    assert_eq!(clipboard.smart_lighting.mode, SmartLightingMode::Strong);
    assert_eq!(clipboard.smart_lighting.intensity, 75.0);
    assert_eq!(clipboard.smart_lighting.custom_shadow_lift, 0.60);
    assert_eq!(clipboard.smart_lighting.custom_highlight_compression, 0.40);

    // Paste into a clean target state
    let mut target_state = DarkroomGradingState::default();
    assert_eq!(target_state.smart_lighting.mode, SmartLightingMode::Slight);

    clipboard.apply_to_grading(&mut target_state);
    assert_eq!(target_state.smart_lighting.mode, SmartLightingMode::Strong);
    assert_eq!(target_state.smart_lighting.intensity, 75.0);
    assert_eq!(target_state.smart_lighting.custom_shadow_lift, 0.60);
    assert_eq!(
        target_state.smart_lighting.custom_highlight_compression,
        0.40
    );
}

// ---------------------------------------------------------------------------
// 2. Section activation and reset in ExposureTone
// ---------------------------------------------------------------------------
#[test]
fn test_smart_lighting_section_active_and_reset() {
    let mut app = RrawApp::new_headless();

    // Initially ExposureTone is not active (all defaults)
    assert!(
        !app.is_section_active(DarkroomSection::ExposureTone),
        "ExposureTone section should not be active initially"
    );

    // Activate smart lighting
    app.grading.smart_lighting.mode = SmartLightingMode::Medium;
    assert!(
        app.is_section_active(DarkroomSection::ExposureTone),
        "ExposureTone section must be active when smart lighting is on"
    );

    // Reset section
    app.reset_section(DarkroomSection::ExposureTone);
    assert_eq!(
        app.grading.smart_lighting.mode,
        SmartLightingMode::Slight,
        "smart lighting must reset to default Slight"
    );
    assert!(
        !app.is_section_active(DarkroomSection::ExposureTone),
        "ExposureTone section must not be active after reset"
    );
}

// ---------------------------------------------------------------------------
// 3. i18n key completeness – all 10 keys in both Fr and En
// ---------------------------------------------------------------------------
#[test]
fn test_smart_lighting_i18n_keys_completeness() {
    let keys = [
        Key::InspectorSmartLightingTitle,
        Key::InspectorSmartLightingTooltip,
        Key::InspectorSmartLightingModeOff,
        Key::InspectorSmartLightingModeSlight,
        Key::InspectorSmartLightingModeMedium,
        Key::InspectorSmartLightingModeStrong,
        Key::InspectorSmartLightingModeCustom,
        Key::InspectorSmartLightingIntensity,
        Key::InspectorSmartLightingShadowLift,
        Key::InspectorSmartLightingHighlightComp,
    ];

    let languages = [Language::En, Language::Fr];

    for key in &keys {
        for lang in &languages {
            let translated = key.translate(*lang);
            assert!(
                !translated.is_empty(),
                "key {:?} must have a non-empty translation in {:?}",
                key,
                lang
            );
        }
    }

    // Spot check FR title
    let fr_title = Key::InspectorSmartLightingTitle.translate(Language::Fr);
    assert!(fr_title.contains("Éclairage intelligent"));

    // Spot check EN title
    let en_title = Key::InspectorSmartLightingTitle.translate(Language::En);
    assert!(en_title.contains("Smart Lighting"));

    // Mode labels must be distinct in each language
    let fr_modes = [
        Key::InspectorSmartLightingModeOff.translate(Language::Fr),
        Key::InspectorSmartLightingModeSlight.translate(Language::Fr),
        Key::InspectorSmartLightingModeMedium.translate(Language::Fr),
        Key::InspectorSmartLightingModeStrong.translate(Language::Fr),
        Key::InspectorSmartLightingModeCustom.translate(Language::Fr),
    ];
    for i in 0..fr_modes.len() {
        for j in (i + 1)..fr_modes.len() {
            assert_ne!(
                fr_modes[i], fr_modes[j],
                "FR mode labels at {i} and {j} must differ"
            );
        }
    }
}
