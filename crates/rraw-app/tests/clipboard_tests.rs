#![forbid(unsafe_code)]

//! Unit tests for batch recipe clipboard model and selective parameter transfer.

use rraw_app::state::{
    ColorGradingParams, CropAspectRatio, DarkroomGradingState, DenoiseSharpenParams,
    GeometryGradingParams, GradingPreset, GradingRecipeClipboard, LocalMaskKind, LocalMaskLayer,
    OpticsGradingParams, ToneGradingParams,
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
        tone: ToneGradingParams {
            exposure_ev: 1.25,
            contrast: 0.35,
            highlights: -0.45,
            shadows: 0.50,
            blacks: -0.15,
            saturation: -0.20,
            vibrance: 0.10,
        },
        color: ColorGradingParams {
            kelvin: 6800.0,
            tint: 12.0,
            display_transform: DisplayTransform::OpenDrt,
            is_monochrome: true,
        },
        tone_curves: source_curves.clone(),
        optics: OpticsGradingParams {
            creative_vignette_amount: -45.0,
            creative_vignette_midpoint: 40.0,
            creative_vignette_feather: 60.0,
            creative_vignette_roundness: 20.0,
            distortion_correction: true,
            distortion_amount: 15.0,
            ..Default::default()
        },
        geometry: GeometryGradingParams {
            crop_enabled: true,
            crop_left: 0.1,
            crop_top: 0.1,
            crop_right: 0.9,
            crop_bottom: 0.9,
            rotation_degrees: 1.5,
            ..Default::default()
        },
        denoise_sharpen: DenoiseSharpenParams {
            wavelet_denoise_sigma: 2.5,
            color_denoise: 40.0,
            capture_sharpening_amount: 0.8,
            ..Default::default()
        },
        ..Default::default()
    };

    let clipboard = GradingRecipeClipboard::from_grading(&source);

    // Verify snapshot fields
    assert_eq!(clipboard.preset, GradingPreset::SepiaVintage);
    assert_eq!(clipboard.tone.exposure_ev, 1.25);
    assert_eq!(clipboard.tone.contrast, 0.35);
    assert_eq!(clipboard.tone.highlights, -0.45);
    assert_eq!(clipboard.tone.shadows, 0.50);
    assert_eq!(clipboard.tone.blacks, -0.15);
    assert_eq!(clipboard.tone.saturation, -0.20);
    assert_eq!(clipboard.tone.vibrance, 0.10);
    assert_eq!(clipboard.color.kelvin, 6800.0);
    assert_eq!(clipboard.color.tint, 12.0);
    assert_eq!(clipboard.color.display_transform, DisplayTransform::OpenDrt);
    assert!(clipboard.color.is_monochrome);
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
        tone: ToneGradingParams {
            exposure_ev: -0.5,
            ..Default::default()
        },
        color: ColorGradingParams {
            kelvin: 5000.0,
            display_transform: DisplayTransform::AgX,
            ..Default::default()
        },
        tone_curves: ToneCurvesRecipe::default(),
        geometry: GeometryGradingParams {
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
            ..Default::default()
        },
        local_masks: vec![target_mask.clone()],
        show_local_mask_overlay: Some(0),
        optics: OpticsGradingParams {
            distortion_correction: false,
            distortion_amount: 0.0,
            chromatic_aberration_correction: true,
            defringe_purple_amount: 25.0,
            defringe_green_amount: 15.0,
            vignette_correction: true,
            vignette_amount: 30.0,
            ..Default::default()
        },
        denoise_sharpen: DenoiseSharpenParams {
            wavelet_denoise_sigma: 1.2,
            color_denoise: 18.0,
            luma_denoise: 5.0,
            capture_sharpening_amount: 0.4,
        },
        ..Default::default()
    };

    // 3. Apply clipboard to target
    clipboard.apply_to_grading(&mut target);

    // Verify all colorimetric and tonal grading parameters are updated
    assert_eq!(target.preset, GradingPreset::SepiaVintage);
    assert_eq!(target.tone.exposure_ev, 1.25);
    assert_eq!(target.tone.contrast, 0.35);
    assert_eq!(target.tone.highlights, -0.45);
    assert_eq!(target.tone.shadows, 0.50);
    assert_eq!(target.tone.blacks, -0.15);
    assert_eq!(target.tone.saturation, -0.20);
    assert_eq!(target.tone.vibrance, 0.10);
    assert_eq!(target.color.kelvin, 6800.0);
    assert_eq!(target.color.tint, 12.0);
    assert_eq!(target.color.display_transform, DisplayTransform::OpenDrt);
    assert!(target.color.is_monochrome);
    assert_eq!(target.tone_curves, source_curves);
    assert_eq!(target.optics.creative_vignette_amount, -45.0);
    assert_eq!(target.optics.creative_vignette_midpoint, 40.0);
    assert_eq!(target.optics.creative_vignette_feather, 60.0);
    assert_eq!(target.optics.creative_vignette_roundness, 20.0);

    // Verify that photo-dependent settings were strictly preserved
    assert!(target.geometry.crop_enabled);
    assert_eq!(target.geometry.crop_aspect, CropAspectRatio::Ratio16x9);
    assert_eq!(target.geometry.crop_left, 0.05);
    assert_eq!(target.geometry.crop_top, 0.05);
    assert_eq!(target.geometry.crop_right, 0.95);
    assert_eq!(target.geometry.crop_bottom, 0.95);
    assert_eq!(target.geometry.rotation_degrees, -2.3);
    assert!(target.geometry.flip_horizontal);
    assert!(!target.geometry.flip_vertical);
    assert!(target.geometry.geometry_enabled);
    assert_eq!(target.geometry.keystone_vertical, 0.15);
    assert_eq!(target.geometry.keystone_horizontal, -0.05);
    assert_eq!(target.local_masks.len(), 1);
    assert_eq!(target.local_masks[0].id, target_mask.id);
    assert_eq!(target.show_local_mask_overlay, Some(0));
    assert!(!target.optics.distortion_correction);
    assert_eq!(target.optics.distortion_amount, 0.0);
    assert!(target.optics.chromatic_aberration_correction);
    assert_eq!(target.optics.defringe_purple_amount, 25.0);
    assert_eq!(target.optics.defringe_green_amount, 15.0);
    assert!(target.optics.vignette_correction);
    assert_eq!(target.optics.vignette_amount, 30.0);
    assert_eq!(target.denoise_sharpen.wavelet_denoise_sigma, 1.2);
    assert_eq!(target.denoise_sharpen.color_denoise, 18.0);
    assert_eq!(target.denoise_sharpen.luma_denoise, 5.0);
    assert_eq!(target.denoise_sharpen.capture_sharpening_amount, 0.4);

    Ok(())
}

#[test]
fn test_grading_recipe_clipboard_summary_format() {
    let grading = DarkroomGradingState {
        preset: GradingPreset::KodakPortra400,
        tone: ToneGradingParams {
            exposure_ev: 0.65,
            ..Default::default()
        },
        color: ColorGradingParams {
            kelvin: 5600.0,
            ..Default::default()
        },
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
        geometry: GeometryGradingParams {
            crop_enabled: true,
            crop_left: 0.2,
            crop_right: 0.8,
            ..Default::default()
        },
        tone: ToneGradingParams {
            exposure_ev: -0.2,
            contrast: 0.1,
            ..Default::default()
        },
        ..Default::default()
    };
    rraw_app::state::save_photo_sidecars(&photo_path, &initial_grading, 3);

    // Verify sidecar was saved
    let (loaded, rating) = rraw_app::state::load_photo_sidecars(&photo_path);
    assert_eq!(rating, Some(3));
    let mut loaded_grading = loaded.ok_or("sidecar should be loaded")?;
    assert!(loaded_grading.geometry.crop_enabled);
    assert_eq!(loaded_grading.geometry.crop_left, 0.2);

    // Now copy a new recipe from a graded source
    let source = DarkroomGradingState {
        preset: GradingPreset::LandscapeVivid,
        tone: ToneGradingParams {
            exposure_ev: 0.85,
            contrast: 0.40,
            saturation: 0.30,
            ..Default::default()
        },
        color: ColorGradingParams {
            kelvin: 5800.0,
            tint: 8.0,
            ..Default::default()
        },
        // Source crop must NOT transfer
        geometry: GeometryGradingParams {
            crop_enabled: false,
            ..Default::default()
        },
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
    assert_eq!(reloaded.tone.exposure_ev, 0.85);
    assert_eq!(reloaded.tone.contrast, 0.40);
    assert_eq!(reloaded.tone.saturation, 0.30);
    assert_eq!(reloaded.color.kelvin, 5800.0);
    assert_eq!(reloaded.color.tint, 8.0);

    // Crop is STILL preserved from target photo!
    assert!(reloaded.geometry.crop_enabled);
    assert_eq!(reloaded.geometry.crop_left, 0.2);
    assert_eq!(reloaded.geometry.crop_right, 0.8);

    Ok(())
}
