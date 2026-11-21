use rraw_color::{
    aces_cg_luminance, analyze_scene_dynamic_range, apply_smart_lighting_acescg, box_filter_2d,
    self_guided_filter, ColorError, CpuReferencePipeline, SceneClassification,
};
use rraw_core::{
    BayerOrigin, CfaPattern, CoreError, Recipe, SignedCfaBuffer, SmartLightingMode,
    SmartLightingParams,
};

#[test]
fn test_aces_cg_luminance_calculation() {
    let black = [0.0, 0.0, 0.0];
    assert_eq!(aces_cg_luminance(black), 0.0);

    let pure_green = [0.0, 1.0, 0.0];
    assert!((aces_cg_luminance(pure_green) - 0.6740818).abs() < 1e-6);

    let white = [1.0, 1.0, 1.0];
    let sum = 0.2722287 + 0.6740818 + 0.0536895;
    assert!((aces_cg_luminance(white) - sum).abs() < 1e-6);
}

#[test]
fn test_dimension_and_buffer_validation() {
    let empty: Vec<[f32; 3]> = Vec::new();
    assert!(matches!(
        analyze_scene_dynamic_range(&empty, 0, 0),
        Err(ColorError::Core(CoreError::InvalidDimensions { .. }))
    ));

    let pixels = vec![[0.18, 0.18, 0.18]; 100];
    assert!(matches!(
        analyze_scene_dynamic_range(&pixels, 10, 5),
        Err(ColorError::Core(CoreError::BufferLengthMismatch { .. }))
    ));
}

#[test]
fn test_box_filter_separable_sliding_window() {
    let width = 8;
    let height = 8;
    let input = vec![1.0_f32; width * height];
    let filtered = box_filter_2d(&input, width, height, 2);

    assert_eq!(filtered.len(), width * height);
    for val in filtered {
        // Uniform input must remain uniform after box filtering
        assert!((val - 1.0).abs() < 1e-5);
    }
}

#[test]
fn test_guided_filter_edge_preservation() {
    let width = 16;
    let height = 16;
    let mut step_image = vec![0.1_f32; width * height];
    // Right half is bright 0.9
    for y in 0..height {
        for x in (width / 2)..width {
            step_image[y * width + x] = 0.9;
        }
    }

    let smoothed = self_guided_filter(&step_image, width, height, 2, 0.04);
    assert_eq!(smoothed.len(), width * height);

    // Far left should remain close to 0.1, far right close to 0.9
    let left_sample = smoothed[8 * width + 2];
    let right_sample = smoothed[8 * width + 14];
    assert!((left_sample - 0.1).abs() < 0.08);
    assert!((right_sample - 0.9).abs() < 0.08);

    // No overshoot or ringing (bounded within [0.1, 0.9])
    for val in smoothed {
        assert!((0.09..=0.91).contains(&val));
    }
}

#[test]
fn test_scene_dynamic_range_analysis_normal_scene() -> Result<(), ColorError> {
    let width = 32;
    let height = 32;
    let mut pixels = Vec::with_capacity(width * height);
    for _y in 0..height {
        for x in 0..width {
            let lum = 0.10 + 0.16 * (x as f32 / width as f32);
            pixels.push([lum, lum, lum]);
        }
    }

    let analysis = analyze_scene_dynamic_range(&pixels, width, height)?;
    assert_eq!(analysis.scene_classification, SceneClassification::Normal);
    assert!(analysis.dynamic_range_ev > 0.5 && analysis.dynamic_range_ev < 8.0);
    assert!(analysis.geometric_mean_luminance > 0.05 && analysis.geometric_mean_luminance < 0.35);
    assert!(analysis.percentiles.p50 > analysis.percentiles.p10);
    assert!(analysis.percentiles.p90 > analysis.percentiles.p50);
    Ok(())
}

#[test]
fn test_scene_dynamic_range_analysis_backlit_scene() -> Result<(), ColorError> {
    let width = 32;
    let height = 32;
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let in_center = (8..24).contains(&x) && (8..24).contains(&y);
            let lum = if in_center { 0.03 } else { 0.60 };
            pixels.push([lum, lum, lum]);
        }
    }

    let analysis = analyze_scene_dynamic_range(&pixels, width, height)?;
    assert_eq!(analysis.scene_classification, SceneClassification::Backlit);
    assert!(analysis.recommended_shadow_lift >= 0.50);
    Ok(())
}

#[test]
fn test_scene_dynamic_range_analysis_high_dynamic_range() -> Result<(), ColorError> {
    let width = 32;
    let height = 32;
    let mut pixels = Vec::with_capacity(width * height);
    for i in 0..(width * height) {
        let t = i as f32 / (width * height) as f32;
        // From 0.0001 (deep shadow) to 16.0 (extreme specular) > 17 EV
        let lum = 0.0001 * (16.0 / 0.0001_f32).powf(t);
        pixels.push([lum, lum, lum]);
    }

    let analysis = analyze_scene_dynamic_range(&pixels, width, height)?;
    assert_eq!(
        analysis.scene_classification,
        SceneClassification::HighDynamicRange
    );
    assert!(analysis.dynamic_range_ev > 10.5);
    Ok(())
}

#[test]
fn test_chromaticity_preservation_under_smart_lighting() -> Result<(), ColorError> {
    let width = 32;
    let height = 32;
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let factor = (x + y + 1) as f32 / 64.0 * 0.5;
            // Distinct non-neutral chromaticity: R:G:B = 3:2:1
            pixels.push([0.3 * factor, 0.2 * factor, 0.1 * factor]);
        }
    }

    let mut processed = pixels.clone();
    let params = SmartLightingParams {
        mode: SmartLightingMode::Medium,
        intensity: 1.0,
        custom_shadow_lift: 0.0,
        custom_highlight_compression: 0.0,
    };

    let result = apply_smart_lighting_acescg(&mut processed, width, height, &params)?;
    assert!(result.is_some());

    // Verify chromaticity ratios are strictly preserved for all non-zero pixels
    for (orig, modif) in pixels.iter().zip(processed.iter()) {
        if orig[1] > 1e-4 && modif[1] > 1e-4 {
            let orig_rg = orig[0] / orig[1];
            let modif_rg = modif[0] / modif[1];
            assert!(
                (orig_rg - modif_rg).abs() < 1e-4,
                "R/G chromaticity ratio violated: expected {orig_rg}, got {modif_rg}"
            );

            let orig_bg = orig[2] / orig[1];
            let modif_bg = modif[2] / modif[1];
            assert!(
                (orig_bg - modif_bg).abs() < 1e-4,
                "B/G chromaticity ratio violated: expected {orig_bg}, got {modif_bg}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_smart_lighting_shadow_lift_increases_deep_shadows() -> Result<(), ColorError> {
    let width = 16;
    let height = 16;
    let mut dark_pixels = vec![[0.02, 0.02, 0.02]; width * height];

    let params = SmartLightingParams {
        mode: SmartLightingMode::Strong,
        intensity: 1.0,
        custom_shadow_lift: 0.0,
        custom_highlight_compression: 0.0,
    };

    apply_smart_lighting_acescg(&mut dark_pixels, width, height, &params)?;

    // Deep shadows should be lifted above initial 0.02
    for px in dark_pixels {
        assert!(
            px[0] > 0.02,
            "Shadow lift failed: expected > 0.02, got {}",
            px[0]
        );
    }
    Ok(())
}

#[test]
fn test_smart_lighting_inactive_mode_is_noop() -> Result<(), ColorError> {
    let width = 8;
    let height = 8;
    let original = vec![[0.2, 0.2, 0.2]; width * height];
    let mut test_pixels = original.clone();

    let off_params = SmartLightingParams {
        mode: SmartLightingMode::Off,
        intensity: 1.0,
        custom_shadow_lift: 0.0,
        custom_highlight_compression: 0.0,
    };

    let res = apply_smart_lighting_acescg(&mut test_pixels, width, height, &off_params)?;
    assert!(res.is_none());
    assert_eq!(original, test_pixels);
    Ok(())
}

#[test]
fn test_cpu_reference_pipeline_with_smart_lighting() -> Result<(), ColorError> {
    let width = 16;
    let height = 16;
    let samples = vec![0.08_f32; width * height];
    let cfa = SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)?;

    let mut recipe = Recipe::default();
    recipe.stage5_grading.smart_lighting = SmartLightingParams {
        mode: SmartLightingMode::Medium,
        intensity: 1.0,
        custom_shadow_lift: 0.0,
        custom_highlight_compression: 0.0,
    };

    let pipeline = CpuReferencePipeline::new();
    let img = pipeline.execute(&cfa, &recipe, 1.0)?;

    assert_eq!(img.width(), width);
    assert_eq!(img.height(), height);

    // PNG export should work cleanly
    let png_bytes = img.to_png_16bit_bytes()?;
    assert!(!png_bytes.is_empty());
    Ok(())
}
