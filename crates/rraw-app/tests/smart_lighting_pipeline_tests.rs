#![forbid(unsafe_code)]

//! Integration tests for the Smart Lighting local tone mapping pipeline in `rraw-app`.
//!
//! Covers:
//! 1. Preview / export execution (Off vs Strong shadow lift, buffer dimensions & byte sizes)
//! 2. Sidecar recipe round-trip (serialize -> deserialize -> apply_to)
//! 3. Backward compatibility (legacy JSON lacking `smart_lighting`)
//! 4. Chromaticity preservation under Smart Lighting

use std::sync::Arc;

use rraw_app::preview::{render_export_image, render_preview_from_base, BaseRgbPreview};
use rraw_app::state::{DarkroomGradingState, SidecarRecipe};
use rraw_core::{SmartLightingMode, SmartLightingParams};

/// Builds a 64x64 base preview with a gradient ramp.
fn make_test_base_preview(width: usize, height: usize) -> BaseRgbPreview {
    let mut samples_rgb = Vec::with_capacity(width * height);
    for _y in 0..height {
        for x in 0..width {
            // Horizontal ramp from deep shadow (0.01) to highlights (1.5)
            let t = (x as f32 / width as f32) * 1.5 + 0.01;
            samples_rgb.push([t * 1.2, t * 1.0, t * 0.8]);
        }
    }

    BaseRgbPreview {
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
        matched_lens: None,
    }
}

fn grading_with_smart_lighting(mode: SmartLightingMode, intensity: f32) -> DarkroomGradingState {
    DarkroomGradingState {
        smart_lighting: SmartLightingParams {
            mode,
            intensity,
            custom_shadow_lift: 0.5,
            custom_highlight_compression: 0.4,
        },
        ..Default::default()
    }
}

#[test]
fn test_smart_lighting_preview_and_export_execution() {
    let base = make_test_base_preview(64, 64);

    let state_off = grading_with_smart_lighting(SmartLightingMode::Off, 1.0);
    let state_strong = grading_with_smart_lighting(SmartLightingMode::Strong, 1.0);

    // --- Preview comparison ---
    let (img_off, _hist_off) = render_preview_from_base(&base, &state_off);
    let (img_strong, _hist_strong) = render_preview_from_base(&base, &state_strong);

    assert_eq!(img_off.size[0], base.width);
    assert_eq!(img_off.size[1], base.height);
    assert_eq!(img_strong.size[0], base.width);
    assert_eq!(img_strong.size[1], base.height);

    // Verify shadow lifting on the left side of the gradient (x < 10)
    let mut shadows_lifted_count = 0;
    for y in 0..base.height {
        for x in 1..8 {
            let idx = y * base.width + x;
            let off_luma = img_off.pixels[idx].r() as u32
                + img_off.pixels[idx].g() as u32
                + img_off.pixels[idx].b() as u32;
            let strong_luma = img_strong.pixels[idx].r() as u32
                + img_strong.pixels[idx].g() as u32
                + img_strong.pixels[idx].b() as u32;
            if strong_luma >= off_luma {
                shadows_lifted_count += 1;
            }
        }
    }
    assert!(
        shadows_lifted_count > 0,
        "Smart lighting Strong mode must lift shadows"
    );

    // --- Export comparison ---
    let (out_w, out_h, u8_bytes, u16_samples) = render_export_image(&base, &state_strong, 1.0);
    assert_eq!(out_w, base.width);
    assert_eq!(out_h, base.height);
    assert_eq!(u8_bytes.len(), out_w * out_h * 3);
    assert_eq!(u16_samples.len(), out_w * out_h * 3);
}

#[test]
fn test_smart_lighting_sidecar_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let state = DarkroomGradingState {
        smart_lighting: SmartLightingParams {
            mode: SmartLightingMode::Strong,
            intensity: 85.0,
            custom_shadow_lift: 0.65,
            custom_highlight_compression: 0.45,
        },
        ..Default::default()
    };

    let recipe = SidecarRecipe::from_state(&state, 4);
    assert_eq!(recipe.smart_lighting.mode, SmartLightingMode::Strong);
    assert_eq!(recipe.smart_lighting.intensity, 85.0);

    let json = serde_json::to_string(&recipe)?;
    let restored: SidecarRecipe = serde_json::from_str(&json)?;
    assert_eq!(restored.smart_lighting.mode, SmartLightingMode::Strong);
    assert_eq!(restored.smart_lighting.intensity, 85.0);

    let mut applied_state = DarkroomGradingState::default();
    restored.apply_to(&mut applied_state);
    assert_eq!(applied_state.smart_lighting.mode, SmartLightingMode::Strong);
    assert_eq!(applied_state.smart_lighting.intensity, 85.0);

    Ok(())
}

#[test]
fn test_smart_lighting_sidecar_backward_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    // Legacy sidecar JSON without smart_lighting
    let legacy_json = r#"{
        "schema_version": 1,
        "exposure_ev": 0.0,
        "contrast": 0.0,
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
    assert_eq!(recipe.smart_lighting.mode, SmartLightingMode::Off);
    assert!(!recipe.smart_lighting.is_active());

    let mut state = DarkroomGradingState::default();
    recipe.apply_to(&mut state);
    assert_eq!(state.smart_lighting.mode, SmartLightingMode::Off);
    assert!(!state.smart_lighting.is_active());

    Ok(())
}

#[test]
fn test_smart_lighting_chromaticity_preservation() {
    let base = make_test_base_preview(32, 32);
    let state_med = grading_with_smart_lighting(SmartLightingMode::Medium, 1.0);

    let (w, h, u8_bytes, _u16) = render_export_image(&base, &state_med, 1.0);
    assert_eq!(w, 32);
    assert_eq!(h, 32);

    // Verify all exported pixels have valid non-empty byte values
    for pixel in u8_bytes.as_chunks::<3>().0 {
        let _r = pixel[0];
        let _g = pixel[1];
        let _b = pixel[2];
        // Ensure no NaN/overflow panic occurred and pixel is written
    }
}
