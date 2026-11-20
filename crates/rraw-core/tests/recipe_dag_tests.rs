use rraw_core::recipe::{
    CfaDenoiseMode, DemosaicAlgorithm, GeometricMask, HighlightReconstructionMode, MaskKind,
    OutputColorSpace, Recipe, ToneMapperKind, CURRENT_SCHEMA_VERSION,
};
use rraw_core::WhiteBalanceMultipliers;

#[test]
fn test_default_recipe_schema_version_and_invariants() {
    let recipe = Recipe::default();
    assert_eq!(recipe.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(recipe.stage1_sensor.hot_pixel_threshold, 4.0);
    assert_eq!(
        recipe.stage1_sensor.cfa_denoise_mode,
        CfaDenoiseMode::InteractiveWavelet { threshold: 1.0 }
    );
    assert_eq!(
        recipe.stage2_demosaic.demosaic_algorithm,
        DemosaicAlgorithm::Rcd
    );
    assert_eq!(
        recipe.stage2_demosaic.highlight_reconstruction,
        HighlightReconstructionMode::ColorRatio
    );
    assert_eq!(recipe.stage7_display.tone_mapper, ToneMapperKind::AgX);
    assert_eq!(
        recipe.stage7_display.output_color_space,
        OutputColorSpace::DisplayP3
    );
}

#[test]
fn test_canonical_json_roundtrip_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let mut recipe = Recipe::default();
    recipe.stage5_grading.exposure_ev = 1.25;
    recipe.stage4_optics.distortion_k1 = -0.015;
    recipe.stage6_masks.masks.push(GeometricMask {
        id: "gradient_sky".to_string(),
        kind: MaskKind::LinearGradient {
            start: [0.5, 0.0],
            end: [0.5, 0.4],
        },
        exposure_offset_ev: -0.75,
        opacity: 0.9,
        enabled: true,
    });

    let json_canonical = recipe.to_canonical_json()?;
    let deserialized: Recipe = Recipe::from_canonical_json(&json_canonical)?;
    assert_eq!(recipe, deserialized);

    let json_re_serialized = deserialized.to_canonical_json()?;
    assert_eq!(json_canonical, json_re_serialized);
    Ok(())
}

#[test]
fn test_stage_cache_hash_isolation_invariants() -> Result<(), Box<dyn std::error::Error>> {
    let base = Recipe::default();
    let source_id = "camera_dsc0001_raw";
    let headroom = 2.0;

    let hash_l0_base = base.hash_l0(source_id);
    let hash_l1_base = base.hash_l1(source_id);
    let hash_l2_base = base.hash_l2();
    let hash_disp_base = base.hash_display(source_id, headroom);

    // 1. Changing Stage 5 (exposure): MUST invalidate display hash, but leave L0, L1, L2 bitwise identical!
    let mut altered_exp = base.clone();
    altered_exp.stage5_grading.exposure_ev = 0.85;
    assert_eq!(altered_exp.hash_l0(source_id), hash_l0_base);
    assert_eq!(altered_exp.hash_l1(source_id), hash_l1_base);
    assert_eq!(altered_exp.hash_l2(), hash_l2_base);
    assert_ne!(
        altered_exp.hash_display(source_id, headroom),
        hash_disp_base
    );

    // 2. Changing Stage 4 (distortion / optics): MUST invalidate L2 and display, but leave L0 and L1 bitwise identical!
    let mut altered_optics = base.clone();
    altered_optics.stage4_optics.distortion_k1 = 0.035;
    assert_eq!(altered_optics.hash_l0(source_id), hash_l0_base);
    assert_eq!(altered_optics.hash_l1(source_id), hash_l1_base);
    assert_ne!(altered_optics.hash_l2(), hash_l2_base);
    assert_ne!(
        altered_optics.hash_display(source_id, headroom),
        hash_disp_base
    );

    // 3. Changing Stage 2 (white balance): MUST invalidate L1 and display, but leave L0 and L2 bitwise identical!
    let mut altered_wb = base.clone();
    altered_wb.stage2_demosaic.wb_multipliers = WhiteBalanceMultipliers::new(2.4, 1.0, 1.8)?;
    assert_eq!(altered_wb.hash_l0(source_id), hash_l0_base);
    assert_ne!(altered_wb.hash_l1(source_id), hash_l1_base);
    assert_eq!(altered_wb.hash_l2(), hash_l2_base);
    assert_ne!(altered_wb.hash_display(source_id, headroom), hash_disp_base);

    // 4. Changing Stage 1 (sensor / black level): MUST invalidate L0, L1, and display, but leave L2 bitwise identical!
    let mut altered_sensor = base.clone();
    altered_sensor.stage1_sensor.black_level_offset[0] = 4.0;
    assert_ne!(altered_sensor.hash_l0(source_id), hash_l0_base);
    assert_ne!(altered_sensor.hash_l1(source_id), hash_l1_base);
    assert_eq!(altered_sensor.hash_l2(), hash_l2_base);
    assert_ne!(
        altered_sensor.hash_display(source_id, headroom),
        hash_disp_base
    );

    // 5. Changing display headroom: MUST invalidate display hash, but leave L0, L1, L2 bitwise identical!
    assert_eq!(base.hash_l0(source_id), hash_l0_base);
    assert_eq!(base.hash_l1(source_id), hash_l1_base);
    assert_eq!(base.hash_l2(), hash_l2_base);
    assert_ne!(base.hash_display(source_id, 4.0), hash_disp_base);

    // 6. Changing raw source_id: MUST invalidate L0, L1, display, but leave L2 (optics profile) bitwise identical!
    let other_source = "camera_dsc0002_raw";
    assert_ne!(base.hash_l0(other_source), hash_l0_base);
    assert_ne!(base.hash_l1(other_source), hash_l1_base);
    assert_eq!(base.hash_l2(), hash_l2_base);
    assert_ne!(base.hash_display(other_source, headroom), hash_disp_base);

    Ok(())
}

#[test]
fn test_recipe_schema_version_mismatch_fails() {
    let bad_json = r#"{
        "schema_version": 999,
        "id": "bad",
        "stage1_sensor": {
            "black_level_offset": [0.0, 0.0, 0.0, 0.0],
            "white_level_override": null,
            "hot_pixel_threshold": 4.0,
            "cfa_denoise_mode": { "mode": "disabled" }
        },
        "stage2_demosaic": {
            "wb_multipliers": { "r": 1.0, "g": 1.0, "b": 1.0 },
            "demosaic_algorithm": "rcd",
            "highlight_reconstruction": "color_ratio"
        },
        "stage3_sharpen": {
            "iterations": 5,
            "amount": 0.5,
            "psf_radius_center": 0.8,
            "psf_radius_corner": 1.2,
            "dering_amount": 0.2,
            "enabled": true
        },
        "stage4_optics": {
            "distortion_k1": 0.0,
            "distortion_k2": 0.0,
            "lateral_ca_red": 0.0,
            "lateral_ca_blue": 0.0,
            "vignette_amount": 0.0,
            "vignette_radius": 1.0,
            "lens_profile_name": null
        },
        "stage5_grading": {
            "exposure_ev": 0.0,
            "contrast": 0.0,
            "highlights": 0.0,
            "shadows": 0.0,
            "whites": 0.0,
            "blacks": 0.0,
            "saturation": 0.0,
            "vibrance": 0.0,
            "laplacian_local_contrast": 0.0
        },
        "stage6_masks": { "masks": [] },
        "stage7_display": {
            "tone_mapper": "ag_x",
            "look": "default",
            "output_color_space": "display_p3",
            "dither_enabled": true
        }
    }"#;

    let res = Recipe::from_canonical_json(bad_json);
    assert!(res.is_err());
    let err = res.err();
    assert!(matches!(
        err,
        Some(rraw_core::CoreError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 999
        })
    ));
}

#[test]
fn test_stage_hash_hex_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let hash = rraw_core::recipe::StageHash([42u8; 32]);
    let hex = hash.to_hex();
    assert_eq!(hex.len(), 64);
    assert_eq!(hex, "2a".repeat(32));

    let parsed = rraw_core::recipe::StageHash::from_hex(&hex)?;
    assert_eq!(parsed, hash);

    assert!(rraw_core::recipe::StageHash::from_hex("tooshort").is_err());
    assert!(rraw_core::recipe::StageHash::from_hex(&"zz".repeat(32)).is_err());
    Ok(())
}

#[test]
fn test_tone_curves_recipe_invariants_and_serialization() -> Result<(), Box<dyn std::error::Error>>
{
    use rraw_core::curves::{CurvePoint, ToneCurvesRecipe};

    let mut curves = ToneCurvesRecipe::default();
    assert!(curves.is_identity());

    let pt1 = CurvePoint::new(0.25, 0.20)?;
    let pt2 = CurvePoint::new(0.75, 0.85)?;
    curves.combined_rgb.add_point(pt1);
    curves.combined_rgb.add_point(pt2);
    assert!(!curves.is_identity());
    assert_eq!(curves.combined_rgb.points.len(), 4);

    let mut recipe = Recipe::default();
    recipe.stage5_grading.tone_curves = curves.clone();

    let json = recipe.to_canonical_json()?;
    let deserialized: Recipe = Recipe::from_canonical_json(&json)?;
    assert_eq!(deserialized.stage5_grading.tone_curves, curves);
    Ok(())
}

#[test]
fn test_semantic_ai_mask_serialization_and_hash_invariance(
) -> Result<(), Box<dyn std::error::Error>> {
    use rraw_core::recipe::SemanticMaskCategory;

    let base = Recipe::default();
    let mut recipe = base.clone();
    for (idx, category) in SemanticMaskCategory::ALL.iter().copied().enumerate() {
        assert_eq!(category.channel_index(), idx);
        recipe.stage6_masks.masks.push(GeometricMask {
            id: format!("ai_{idx}"),
            kind: MaskKind::SemanticAi {
                category,
                invert: idx % 2 == 1,
                feather: 0.35,
            },
            exposure_offset_ev: 0.5,
            opacity: 0.85,
            enabled: true,
        });
    }

    let json = recipe.to_canonical_json()?;
    let roundtrip = Recipe::from_canonical_json(&json)?;
    assert_eq!(roundtrip, recipe);

    // Stage 6 AI mask changes display hash but keeps L0, L1, L2 hashes unchanged
    let src = "raw_01";
    assert_eq!(recipe.hash_l0(src), base.hash_l0(src));
    assert_eq!(recipe.hash_l1(src), base.hash_l1(src));
    assert_eq!(recipe.hash_l2(), base.hash_l2());
    assert_ne!(recipe.hash_display(src, 2.0), base.hash_display(src, 2.0));
    Ok(())
}

#[test]
fn test_color_tone_recipe_clipboard_isolation_and_application(
) -> Result<(), Box<dyn std::error::Error>> {
    use rraw_core::curves::{CurvePoint, ToneCurvesRecipe};
    use rraw_core::recipe::{ColorToneRecipeClipboard, Stage5GradingParams};

    let mut source_stage5 = Stage5GradingParams {
        exposure_ev: 0.75,
        contrast: 0.25,
        highlights: -0.40,
        shadows: 0.30,
        whites: 0.15,
        blacks: -0.10,
        saturation: 0.20,
        vibrance: 0.10,
        laplacian_local_contrast: 0.85,
        tone_curves: ToneCurvesRecipe::default(),
    };
    source_stage5
        .tone_curves
        .combined_rgb
        .add_point(CurvePoint::new(0.5, 0.6)?);

    let clipboard = ColorToneRecipeClipboard::from_stage5(&source_stage5);
    assert_eq!(clipboard.exposure_ev, 0.75);
    assert_eq!(clipboard.contrast, 0.25);
    assert_eq!(clipboard.highlights, -0.40);
    assert_eq!(clipboard.shadows, 0.30);
    assert_eq!(clipboard.whites, 0.15);
    assert_eq!(clipboard.blacks, -0.10);
    assert_eq!(clipboard.saturation, 0.20);
    assert_eq!(clipboard.vibrance, 0.10);
    assert_eq!(clipboard.tone_curves, source_stage5.tone_curves);

    // Target has independent settings, including laplacian local contrast
    let mut target_stage5 = Stage5GradingParams {
        exposure_ev: -1.0,
        contrast: -0.5,
        highlights: 0.0,
        shadows: 0.0,
        whites: 0.0,
        blacks: 0.0,
        saturation: -0.5,
        vibrance: -0.5,
        laplacian_local_contrast: 0.12,
        tone_curves: ToneCurvesRecipe::default(),
    };

    clipboard.apply_to_stage5(&mut target_stage5);

    // Color and tone parameters are copied
    assert_eq!(target_stage5.exposure_ev, 0.75);
    assert_eq!(target_stage5.contrast, 0.25);
    assert_eq!(target_stage5.highlights, -0.40);
    assert_eq!(target_stage5.shadows, 0.30);
    assert_eq!(target_stage5.whites, 0.15);
    assert_eq!(target_stage5.blacks, -0.10);
    assert_eq!(target_stage5.saturation, 0.20);
    assert_eq!(target_stage5.vibrance, 0.10);
    assert_eq!(target_stage5.tone_curves, source_stage5.tone_curves);

    // Photo-specific laplacian local contrast is strictly preserved
    assert_eq!(target_stage5.laplacian_local_contrast, 0.12);

    Ok(())
}
