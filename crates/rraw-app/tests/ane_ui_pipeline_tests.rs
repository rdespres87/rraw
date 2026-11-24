//! Integration tests for Task 20.2: Interactive AI Masking & Real-Time Neural Denoising UI.

use rraw_app::grading::DenoiseSharpenParams;
use rraw_app::i18n::{Key, Language};
use rraw_app::preview::{execute_stage6_gpu, infer_preview_semantic_masks};
use rraw_app::state::{DarkroomGradingState, SemanticMaskLayer};
use rraw_core::SemanticMaskCategory;
use rraw_coreml::CoreMlHardwareCapabilities;
use rraw_gpu::GpuContext;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn test_denoise_sharpen_ane_params() {
    let params = DenoiseSharpenParams::default();
    assert!(!params.ane_neural_denoise);
    assert!((params.ane_denoise_strength - 50.0).abs() < 1e-4);
    assert!(!params.is_active());

    let active = DenoiseSharpenParams {
        ane_neural_denoise: true,
        ..Default::default()
    };
    assert!(active.is_active());

    let reset = DenoiseSharpenParams::untouched_raw();
    assert!(!reset.ane_neural_denoise);
}

#[test]
fn test_stage6_gpu_semantic_masks_execution() -> TestResult {
    let width = 64_u32;
    let height = 64_u32;
    let total = (width * height) as usize;
    let rgb_in = vec![0.5_f32; total * 3];

    // Create synthetic landscape RGB buffer
    let mut samples_rgb = Vec::with_capacity(total);
    for y in 0..height {
        let v = y as f32 / height as f32;
        for x in 0..width {
            let u = x as f32 / width as f32;
            let pixel = if v < 0.4 {
                [0.2_f32, 0.45_f32, 0.85_f32]
            } else if (u - 0.5).abs() < 0.2 && (v - 0.6).abs() < 0.2 {
                [0.72_f32, 0.52_f32, 0.42_f32]
            } else {
                [0.3_f32, 0.5_f32, 0.2_f32]
            };
            samples_rgb.push(pixel);
        }
    }

    let masks = infer_preview_semantic_masks(
        &samples_rgb,
        width as usize,
        height as usize,
        [1.0, 1.0, 1.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    );

    let mut grading = DarkroomGradingState::default();
    let mut sky_layer = SemanticMaskLayer::new(SemanticMaskCategory::Sky);
    sky_layer.exposure_ev = -1.0;
    grading.masks.semantic_masks.push(sky_layer);

    let stage6_params = grading.to_stage6_masks_params();
    assert!(!stage6_params.masks.is_empty());

    // Execute Stage 6 GPU pipeline if headless GPU is available
    if let Ok(ctx) = GpuContext::new_headless() {
        let out = execute_stage6_gpu(&ctx, &rgb_in, width, height, &stage6_params, &masks)?;
        assert_eq!(out.len(), total * 3);
        for &val in &out {
            assert!(val.is_finite());
            assert!(val >= 0.0);
        }
    }

    Ok(())
}

#[test]
fn test_i18n_ane_keys_completeness() {
    let ane_keys = [
        Key::DetailAneDenoise,
        Key::DetailAneDenoiseTooltip,
        Key::DetailAneStrength,
        Key::DetailAneBadgeActive,
        Key::DetailAneBadgeFallback,
        Key::DetailAneStatusTile,
        Key::MaskAiHardwareAne,
    ];

    for &lang in &Language::ALL {
        for &key in &ane_keys {
            let translated = key.translate(lang);
            assert!(
                !translated.trim().is_empty(),
                "Key {:?} must have non-empty translation for {:?}",
                key,
                lang
            );
        }
    }
}

#[test]
fn test_hardware_capabilities_detection_invariants() {
    let caps = CoreMlHardwareCapabilities::detect();
    assert!(!caps.device_name.is_empty());
    let _ = caps.has_ane;
}
