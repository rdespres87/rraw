use rraw_app::app::RrawApp;
use rraw_app::i18n::{Key, Language};
use rraw_app::state::{DarkroomGradingState, DarkroomSection, SidecarRecipe};

#[test]
fn test_optical_profile_grading_defaults() {
    let grading = DarkroomGradingState::default();
    assert!(grading.lens_profile_auto);
    assert_eq!(grading.lens_distortion_strength, 100.0);
    assert_eq!(grading.lens_vignette_strength, 100.0);
    assert_eq!(grading.lens_ca_strength, 100.0);
}

#[test]
fn test_optical_profile_sidecar_backward_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    // Legacy sidecar JSON without lens_profile fields
    let legacy_json = r#"{
        "schema_version": 1,
        "exposure_ev": 0.5,
        "contrast": 10.0,
        "highlights": 0.0,
        "shadows": 0.0,
        "kelvin": 5500.0,
        "tint": 0.0,
        "display_transform": "AgX",
        "wavelet_denoise_sigma": 1.0,
        "capture_sharpening_amount": 0.5,
        "distortion_correction": true,
        "distortion_amount": 0.0,
        "chromatic_aberration_correction": true,
        "vignette_correction": true,
        "vignette_amount": 50.0,
        "is_monochrome": false,
        "rating": 0
    }"#;

    let recipe: SidecarRecipe = serde_json::from_str(legacy_json)?;

    assert!(
        recipe.lens_profile_auto,
        "Default lens_profile_auto must be true"
    );
    assert_eq!(recipe.lens_distortion_strength, 100.0);
    assert_eq!(recipe.lens_vignette_strength, 100.0);
    assert_eq!(recipe.lens_ca_strength, 100.0);

    // Apply to grading and verify roundtrip
    let mut grading = DarkroomGradingState::default();
    recipe.apply_to(&mut grading);
    assert!(grading.lens_profile_auto);
    assert_eq!(grading.lens_distortion_strength, 100.0);

    // Serialize and verify fields are present
    let serialized = serde_json::to_string(&recipe)?;
    assert!(serialized.contains("lens_profile_auto"));
    assert!(serialized.contains("lens_distortion_strength"));

    Ok(())
}

#[test]
fn test_optical_profile_reset_section() {
    let mut app = RrawApp::new_headless();
    app.grading.lens_profile_auto = false;
    app.grading.lens_distortion_strength = 45.0;
    app.grading.lens_vignette_strength = 60.0;
    app.grading.lens_ca_strength = 80.0;
    app.grading.distortion_amount = 15.0;

    app.reset_section(DarkroomSection::OpticalCorrections);

    assert!(app.grading.lens_profile_auto);
    assert_eq!(app.grading.lens_distortion_strength, 100.0);
    assert_eq!(app.grading.lens_vignette_strength, 100.0);
    assert_eq!(app.grading.lens_ca_strength, 100.0);
    assert_eq!(app.grading.distortion_amount, 0.0);
}

#[test]
fn test_optical_profile_i18n_keys() {
    let keys = [
        Key::InspectorLensProfileTitle,
        Key::InspectorLensProfileAuto,
        Key::InspectorLensProfileAutoTooltip,
        Key::InspectorLensProfileDetected,
        Key::InspectorLensProfileGeneric,
        Key::InspectorLensProfileDistortion,
        Key::InspectorLensProfileVignette,
        Key::InspectorLensProfileCa,
        Key::InspectorLensManualTitle,
    ];

    for key in keys {
        let fr = key.translate(Language::Fr);
        let en = key.translate(Language::En);
        assert!(!fr.is_empty(), "Key {:?} in French must not be empty", key);
        assert!(!en.is_empty(), "Key {:?} in English must not be empty", key);
        assert_ne!(
            fr, en,
            "Key {:?} translations should differ between FR and EN",
            key
        );
    }
}
