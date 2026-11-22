use rraw_app::app::RrawApp;
use rraw_app::i18n::{Key, Language};
use rraw_app::state::{DarkroomGradingState, DarkroomSection, OpticsGradingParams, SidecarRecipe};

#[test]
fn test_optical_profile_grading_defaults() {
    let grading = DarkroomGradingState::default();
    assert!(grading.optics.lens_profile_auto);
    assert_eq!(grading.optics.lens_distortion_strength, 100.0);
    assert_eq!(grading.optics.lens_vignette_strength, 100.0);
    assert_eq!(grading.optics.lens_ca_strength, 100.0);
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
        recipe.optics.lens_profile_auto,
        "Default lens_profile_auto must be true"
    );
    assert_eq!(recipe.optics.lens_distortion_strength, 100.0);
    assert_eq!(recipe.optics.lens_vignette_strength, 100.0);
    assert_eq!(recipe.optics.lens_ca_strength, 100.0);

    // Apply to grading and verify roundtrip
    let mut grading = DarkroomGradingState::default();
    recipe.apply_to(&mut grading);
    assert!(grading.optics.lens_profile_auto);
    assert_eq!(grading.optics.lens_distortion_strength, 100.0);

    // Serialize and verify fields are present
    let serialized = serde_json::to_string(&recipe)?;
    assert!(serialized.contains("lens_profile_auto"));
    assert!(serialized.contains("lens_distortion_strength"));

    Ok(())
}

#[test]
fn test_optical_profile_reset_section() {
    let mut app = RrawApp::new_headless();
    app.grading.optics.lens_profile_auto = false;
    app.grading.optics.lens_distortion_strength = 45.0;
    app.grading.optics.lens_vignette_strength = 60.0;
    app.grading.optics.lens_ca_strength = 80.0;
    app.grading.optics.distortion_amount = 15.0;

    app.reset_section(DarkroomSection::OpticalCorrections);

    assert!(app.grading.optics.lens_profile_auto);
    assert_eq!(app.grading.optics.lens_distortion_strength, 100.0);
    assert_eq!(app.grading.optics.lens_vignette_strength, 100.0);
    assert_eq!(app.grading.optics.lens_ca_strength, 100.0);
    assert_eq!(app.grading.optics.distortion_amount, 0.0);
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
        Key::InspectorLensTabProfile,
        Key::InspectorLensTabManual,
        Key::InspectorParamDefringeThreshold,
        Key::InspectorDefringePreviewMask,
        Key::InspectorDefringePreviewMaskTooltip,
        Key::InspectorLensProfileDisabledNotice,
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

#[test]
fn test_optical_tab_and_defringe_threshold_defaults_and_reset() {
    use rraw_app::state::OpticalCorrectionTab;

    let mut app = RrawApp::new_headless();
    assert_eq!(
        app.grading.optics.optical_tab,
        OpticalCorrectionTab::Profile
    );
    assert!((app.grading.optics.defringe_threshold - 0.08).abs() < 1e-4);
    assert!(!app.grading.optics.defringe_preview_mask);

    app.grading.optics.optical_tab = OpticalCorrectionTab::Manual;
    app.grading.optics.defringe_threshold = 0.18;
    app.grading.optics.defringe_preview_mask = true;
    app.grading.optics.defringe_purple_amount = 55.0;

    app.reset_section(DarkroomSection::OpticalCorrections);

    assert_eq!(
        app.grading.optics.optical_tab,
        OpticalCorrectionTab::Profile
    );
    assert!((app.grading.optics.defringe_threshold - 0.08).abs() < 1e-4);
    assert!(!app.grading.optics.defringe_preview_mask);
    assert_eq!(app.grading.optics.defringe_purple_amount, 0.0);
}

#[test]
fn test_optical_sidecar_roundtrip_with_tabs_and_threshold() -> Result<(), Box<dyn std::error::Error>>
{
    use rraw_app::state::OpticalCorrectionTab;

    let grading = DarkroomGradingState {
        optics: OpticsGradingParams {
            optical_tab: OpticalCorrectionTab::Manual,
            defringe_threshold: 0.14,
            defringe_preview_mask: true,
            defringe_purple_amount: 40.0,
            defringe_green_amount: 25.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let recipe = SidecarRecipe::from_state(&grading, 5);
    let serialized = serde_json::to_string(&recipe)?;

    assert!(serialized.contains(r#""optical_tab":"Manual""#));
    assert!(serialized.contains("defringe_threshold"));
    assert!(serialized.contains("defringe_preview_mask"));

    let deserialized: SidecarRecipe = serde_json::from_str(&serialized)?;
    assert_eq!(
        deserialized.optics.optical_tab,
        OpticalCorrectionTab::Manual
    );
    assert!((deserialized.optics.defringe_threshold - 0.14).abs() < 1e-4);
    assert!(deserialized.optics.defringe_preview_mask);

    let mut restored = DarkroomGradingState::default();
    deserialized.apply_to(&mut restored);
    assert_eq!(restored.optics.optical_tab, OpticalCorrectionTab::Manual);
    assert!((restored.optics.defringe_threshold - 0.14).abs() < 1e-4);
    assert!(restored.optics.defringe_preview_mask);

    Ok(())
}

#[test]
fn test_optical_mode_isolation_profile_vs_manual() {
    use rraw_app::preview::{render_preview_from_base, BaseRgbPreview};
    use rraw_app::state::OpticalCorrectionTab;
    use rraw_optics::db::MatchedLens;
    use rraw_optics::model::{DistortionPoly, LateralTca, OpticalCorrectionModel, RadialVignette};
    use std::sync::Arc;

    let width = 64;
    let height = 64;
    let mut samples_rgb = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let r = x as f32 / width as f32;
            let g = y as f32 / height as f32;
            samples_rgb.push([r, g, 0.5]);
        }
    }

    let fake_matched = MatchedLens {
        profile_name: "Test Distortion Lens".to_string(),
        maker: "TestMaker".to_string(),
        model: OpticalCorrectionModel {
            distortion: DistortionPoly {
                k1: 0.08,
                k2: 0.0,
                k3: 0.0,
            },
            lateral_ca: LateralTca::default(),
            vignette: RadialVignette::default(),
        },
        is_exact_match: true,
    };

    let base = BaseRgbPreview {
        width,
        height,
        samples_rgb,
        as_shot_wb: [1.0, 1.0, 1.0],
        cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
            width,
            height,
            subject: vec![0.0; width * height],
            sky: vec![0.0; width * height],
            background: vec![0.0; width * height],
            portrait: vec![0.0; width * height],
        }),
        exif_orientation: 1,
        matched_lens: Some(fake_matched),
    };

    // 1. Identity baseline (no profile, manual disabled)
    let grading_ident = DarkroomGradingState {
        optics: OpticsGradingParams {
            optical_tab: OpticalCorrectionTab::Manual,
            distortion_correction: false,
            distortion_amount: 0.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let (p_ident, _) = render_preview_from_base(&base, &grading_ident);

    // 2. Profile mode: profile distortion must be active
    let grading_prof = DarkroomGradingState {
        optics: OpticsGradingParams {
            optical_tab: OpticalCorrectionTab::Profile,
            lens_profile_auto: true,
            lens_distortion_strength: 100.0,
            distortion_correction: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let (p_prof, _) = render_preview_from_base(&base, &grading_prof);

    // 3. Manual mode: profile distortion must NOT be applied (no ghost profile)
    let grading_man_clean = DarkroomGradingState {
        optics: OpticsGradingParams {
            optical_tab: OpticalCorrectionTab::Manual,
            lens_profile_auto: true,
            distortion_correction: false,
            distortion_amount: 0.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let (p_man_clean, _) = render_preview_from_base(&base, &grading_man_clean);

    // Pixel at corner (0, 0) must differ between Profile and Identity
    let c_ident = p_ident.pixels[0];
    let c_prof = p_prof.pixels[0];
    assert_ne!(
        c_ident, c_prof,
        "Profile mode must distort pixels relative to identity"
    );

    // Manual mode without manual distortion must match identity exactly (profile isolated!)
    let c_man = p_man_clean.pixels[0];
    assert_eq!(
        c_man, c_ident,
        "Manual mode without distortion must match identity (no ghost profile)"
    );
}

#[test]
fn test_untouched_raw_reference_view_all_corrections_disabled() {
    use rraw_app::state::{DarkroomGradingState, OpticalCorrectionTab};

    let untouched = DarkroomGradingState::untouched_raw();

    // Verify optical corrections are 100% disabled
    assert_eq!(untouched.optics.optical_tab, OpticalCorrectionTab::Manual);
    assert!(!untouched.optics.lens_profile_auto);
    assert_eq!(untouched.optics.lens_distortion_strength, 0.0);
    assert_eq!(untouched.optics.lens_vignette_strength, 0.0);
    assert_eq!(untouched.optics.lens_ca_strength, 0.0);
    assert!(!untouched.optics.distortion_correction);
    assert_eq!(untouched.optics.distortion_amount, 0.0);
    assert!(!untouched.optics.chromatic_aberration_correction);
    assert_eq!(untouched.optics.defringe_purple_amount, 0.0);
    assert_eq!(untouched.optics.defringe_green_amount, 0.0);
    assert!(!untouched.optics.defringe_preview_mask);
    assert!(!untouched.optics.vignette_correction);
    assert_eq!(untouched.optics.vignette_amount, 0.0);
    assert_eq!(untouched.optics.creative_vignette_amount, 0.0);

    // Verify geometry is 100% disabled
    assert!(!untouched.geometry.geometry_enabled);
    assert!(!untouched.geometry.auto_crop_geometry);
    assert_eq!(untouched.geometry.rotation_degrees, 0.0);
    assert_eq!(untouched.geometry.keystone_vertical, 0.0);
    assert_eq!(untouched.geometry.keystone_horizontal, 0.0);

    // Verify smart lighting is disabled
    assert_eq!(
        untouched.smart_lighting.mode,
        rraw_core::SmartLightingMode::Off
    );
    assert_eq!(untouched.smart_lighting.intensity, 0.0);

    // Verify denoise and sharpening are zeroed
    assert_eq!(untouched.denoise_sharpen.wavelet_denoise_sigma, 0.0);
    assert_eq!(untouched.denoise_sharpen.color_denoise, 0.0);
    assert_eq!(untouched.denoise_sharpen.capture_sharpening_amount, 0.0);
}
