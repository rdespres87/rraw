#![forbid(unsafe_code)]

//! Unit tests for batch recipe clipboard model and selective parameter transfer.

use rraw_app::state::{
    CropAspectRatio, DarkroomGradingState, GradingPreset, GradingRecipeClipboard, LocalMaskKind,
    LocalMaskLayer,
};
use rraw_color::DisplayTransform;
use rraw_core::curves::{CurvePoint, ToneCurvesRecipe};

#[test]
fn test_grading_recipe_clipboard_capture_and_selective_application(
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create a styled source photo grading
    let mut source_curves = ToneCurvesRecipe::default();
    source_curves
        .combined_rgb
        .add_point(CurvePoint::new(0.3, 0.4)?);
    source_curves.red.add_point(CurvePoint::new(0.6, 0.7)?);

    let source = DarkroomGradingState {
        preset: GradingPreset::SepiaVintage,
        exposure_ev: 1.25,
        contrast: 0.35,
        highlights: -0.45,
        shadows: 0.50,
        blacks: -0.15,
        saturation: -0.20,
        vibrance: 0.10,
        kelvin: 6800.0,
        tint: 12.0,
        display_transform: DisplayTransform::OpenDrt,
        is_monochrome: true,
        tone_curves: source_curves.clone(),
        creative_vignette_amount: -45.0,
        creative_vignette_midpoint: 40.0,
        creative_vignette_feather: 60.0,
        creative_vignette_roundness: 20.0,
        // Source also has photo-dependent settings that MUST NOT leak to clipboard
        crop_enabled: true,
        crop_left: 0.1,
        crop_top: 0.1,
        crop_right: 0.9,
        crop_bottom: 0.9,
        rotation_degrees: 1.5,
        distortion_correction: true,
        distortion_amount: 15.0,
        wavelet_denoise_sigma: 2.5,
        color_denoise: 40.0,
        capture_sharpening_amount: 0.8,
        ..Default::default()
    };

    let clipboard = GradingRecipeClipboard::from_grading(&source);

    // Verify snapshot fields
    assert_eq!(clipboard.preset, GradingPreset::SepiaVintage);
    assert_eq!(clipboard.exposure_ev, 1.25);
    assert_eq!(clipboard.contrast, 0.35);
    assert_eq!(clipboard.highlights, -0.45);
    assert_eq!(clipboard.shadows, 0.50);
    assert_eq!(clipboard.blacks, -0.15);
    assert_eq!(clipboard.saturation, -0.20);
    assert_eq!(clipboard.vibrance, 0.10);
    assert_eq!(clipboard.kelvin, 6800.0);
    assert_eq!(clipboard.tint, 12.0);
    assert_eq!(clipboard.display_transform, DisplayTransform::OpenDrt);
    assert!(clipboard.is_monochrome);
    assert_eq!(clipboard.tone_curves, source_curves);
    assert_eq!(clipboard.creative_vignette_amount, -45.0);

    // 2. Create a target photo grading with independent photo-dependent settings
    let target_mask = LocalMaskLayer {
        id: "target_linear_grad".to_string(),
        name: "Ciel sombre".to_string(),
        kind: LocalMaskKind::LinearGradient {
            start: [0.5, 0.0],
            end: [0.5, 0.4],
        },
        exposure_ev: -1.0,
        contrast: 0.0,
        saturation: 0.0,
        temperature_shift: 0.0,
        opacity: 0.8,
        feather: 0.5,
        invert: false,
        enabled: true,
    };

    let mut target = DarkroomGradingState {
        preset: GradingPreset::AsShot,
        exposure_ev: -0.5,
        contrast: 0.0,
        highlights: 0.0,
        shadows: 0.0,
        blacks: 0.0,
        saturation: 0.0,
        vibrance: 0.0,
        kelvin: 5000.0,
        tint: 0.0,
        display_transform: DisplayTransform::AgX,
        is_monochrome: false,
        tone_curves: ToneCurvesRecipe::default(),
        // Target photo-dependent settings that must be strictly preserved
        crop_enabled: true,
        crop_aspect: CropAspectRatio::Ratio16x9,
        crop_left: 0.05,
        crop_top: 0.05,
        crop_right: 0.95,
        crop_bottom: 0.95,
        rotation_degrees: -2.3,
        flip_horizontal: true,
        flip_vertical: false,
        geometry_enabled: true,
        keystone_vertical: 0.15,
        keystone_horizontal: -0.05,
        local_masks: vec![target_mask.clone()],
        show_local_mask_overlay: Some(0),
        distortion_correction: false,
        distortion_amount: 0.0,
        chromatic_aberration_correction: true,
        defringe_purple_amount: 25.0,
        defringe_green_amount: 15.0,
        vignette_correction: true,
        vignette_amount: 30.0,
        wavelet_denoise_sigma: 1.2,
        color_denoise: 18.0,
        luma_denoise: 5.0,
        capture_sharpening_amount: 0.4,
        ..Default::default()
    };

    // 3. Apply clipboard to target
    clipboard.apply_to_grading(&mut target);

    // Verify all colorimetric and tonal grading parameters are updated
    assert_eq!(target.preset, GradingPreset::SepiaVintage);
    assert_eq!(target.exposure_ev, 1.25);
    assert_eq!(target.contrast, 0.35);
    assert_eq!(target.highlights, -0.45);
    assert_eq!(target.shadows, 0.50);
    assert_eq!(target.blacks, -0.15);
    assert_eq!(target.saturation, -0.20);
    assert_eq!(target.vibrance, 0.10);
    assert_eq!(target.kelvin, 6800.0);
    assert_eq!(target.tint, 12.0);
    assert_eq!(target.display_transform, DisplayTransform::OpenDrt);
    assert!(target.is_monochrome);
    assert_eq!(target.tone_curves, source_curves);
    assert_eq!(target.creative_vignette_amount, -45.0);
    assert_eq!(target.creative_vignette_midpoint, 40.0);
    assert_eq!(target.creative_vignette_feather, 60.0);
    assert_eq!(target.creative_vignette_roundness, 20.0);

    // Verify that photo-dependent settings were strictly preserved
    assert!(target.crop_enabled);
    assert_eq!(target.crop_aspect, CropAspectRatio::Ratio16x9);
    assert_eq!(target.crop_left, 0.05);
    assert_eq!(target.crop_top, 0.05);
    assert_eq!(target.crop_right, 0.95);
    assert_eq!(target.crop_bottom, 0.95);
    assert_eq!(target.rotation_degrees, -2.3);
    assert!(target.flip_horizontal);
    assert!(!target.flip_vertical);
    assert!(target.geometry_enabled);
    assert_eq!(target.keystone_vertical, 0.15);
    assert_eq!(target.keystone_horizontal, -0.05);
    assert_eq!(target.local_masks.len(), 1);
    assert_eq!(target.local_masks[0].id, target_mask.id);
    assert_eq!(target.show_local_mask_overlay, Some(0));
    assert!(!target.distortion_correction);
    assert_eq!(target.distortion_amount, 0.0);
    assert!(target.chromatic_aberration_correction);
    assert_eq!(target.defringe_purple_amount, 25.0);
    assert_eq!(target.defringe_green_amount, 15.0);
    assert!(target.vignette_correction);
    assert_eq!(target.vignette_amount, 30.0);
    assert_eq!(target.wavelet_denoise_sigma, 1.2);
    assert_eq!(target.color_denoise, 18.0);
    assert_eq!(target.luma_denoise, 5.0);
    assert_eq!(target.capture_sharpening_amount, 0.4);

    Ok(())
}

#[test]
fn test_grading_recipe_clipboard_summary_format() {
    let grading = DarkroomGradingState {
        preset: GradingPreset::KodakPortra400,
        exposure_ev: 0.65,
        kelvin: 5600.0,
        ..Default::default()
    };
    let clipboard = GradingRecipeClipboard::from_grading(&grading);
    let summary = clipboard.summary();
    assert!(summary.contains("Kodak Portra 400"));
    assert!(summary.contains("+0.65 EV"));
    assert!(summary.contains("5600K"));
}

#[test]
fn test_clipboard_sidecar_roundtrip_preservation() -> Result<(), Box<dyn std::error::Error>> {
    struct TempDirGuard(std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let temp_dir_path = std::env::temp_dir().join(format!(
        "rraw_test_clipboard_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir_path)?;
    let _guard = TempDirGuard(temp_dir_path.clone());

    let photo_path = temp_dir_path.join("DSC_0042.RAW");
    std::fs::write(&photo_path, b"fake raw bytes")?;

    // Create initial grading with a crop on disk
    let initial_grading = DarkroomGradingState {
        crop_enabled: true,
        crop_left: 0.2,
        crop_right: 0.8,
        exposure_ev: -0.2,
        contrast: 0.1,
        ..Default::default()
    };
    rraw_app::state::save_photo_sidecars(&photo_path, &initial_grading, 3);

    // Verify sidecar was saved
    let (loaded, rating) = rraw_app::state::load_photo_sidecars(&photo_path);
    assert_eq!(rating, Some(3));
    let mut loaded_grading = loaded.ok_or("sidecar should be loaded")?;
    assert!(loaded_grading.crop_enabled);
    assert_eq!(loaded_grading.crop_left, 0.2);

    // Now copy a new recipe from a graded source
    let source = DarkroomGradingState {
        preset: GradingPreset::LandscapeVivid,
        exposure_ev: 0.85,
        contrast: 0.40,
        saturation: 0.30,
        kelvin: 5800.0,
        tint: 8.0,
        // Source crop must NOT transfer
        crop_enabled: false,
        ..Default::default()
    };
    let clipboard = GradingRecipeClipboard::from_grading(&source);

    // Apply clipboard to loaded grading and save back
    clipboard.apply_to_grading(&mut loaded_grading);
    rraw_app::state::save_photo_sidecars(&photo_path, &loaded_grading, 4);

    // Re-load from disk and assert color/tone were updated, but crop was preserved
    let (reloaded_opt, new_rating) = rraw_app::state::load_photo_sidecars(&photo_path);
    assert_eq!(new_rating, Some(4));
    let reloaded = reloaded_opt.ok_or("reloaded sidecar should exist")?;

    assert_eq!(reloaded.preset, GradingPreset::LandscapeVivid);
    assert_eq!(reloaded.exposure_ev, 0.85);
    assert_eq!(reloaded.contrast, 0.40);
    assert_eq!(reloaded.saturation, 0.30);
    assert_eq!(reloaded.kelvin, 5800.0);
    assert_eq!(reloaded.tint, 8.0);

    // Crop is STILL preserved from target photo!
    assert!(reloaded.crop_enabled);
    assert_eq!(reloaded.crop_left, 0.2);
    assert_eq!(reloaded.crop_right, 0.8);

    Ok(())
}
