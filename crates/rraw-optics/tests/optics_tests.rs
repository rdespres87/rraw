use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, NoiseProfile, NoiseProfileSource, PhotoMetadata,
    PickStatus,
};
use rraw_gpu::{GpuContext, Stage4Params, Stage4WarpPipeline};
use rraw_optics::{
    DistortionPoly, LateralTca, OpticalCorrectionModel, OpticalResolutionTier, OpticalResolver,
    OpticalStagePlan, RadialVignette, UvLutGenerator,
};

fn make_dummy_metadata(gainmap: bool, lens: Option<&str>) -> Option<PhotoMetadata> {
    let noise_profile = match NoiseProfile::new(
        [0.0001, 0.0001, 0.0001],
        [0.0004, 0.0004, 0.0004],
        NoiseProfileSource::CameraTable,
    ) {
        Ok(np) => np,
        Err(_) => return None,
    };

    Some(PhotoMetadata {
        camera_make: "Sony".to_string(),
        camera_model: "ILCE-7RM5".to_string(),
        lens_model: lens.map(ToString::to_string),
        iso: Some(100),
        aperture_f_number: Some(2.8),
        focal_length_mm: Some(50.0),
        width: 1000,
        height: 1000,
        black_levels: [512.0; 4],
        white_levels: [4096.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile,
        label: ColorLabel::None,
        rating: 0,
        pick: PickStatus::Unflagged,
        vignette_handled_by_gainmap: gainmap,
        keywords: Vec::new(),
    })
}

#[test]
fn test_optical_resolver_4_tier_selection() {
    let resolver = OpticalResolver;

    // Tier 1: Embedded opcode / GainMap
    let meta_tier1 = match make_dummy_metadata(true, Some("FE 50mm F1.2 GM")) {
        Some(m) => m,
        None => return,
    };
    let tier1 = resolver.resolve_tier(&meta_tier1);
    assert_eq!(tier1, OpticalResolutionTier::Tier1EmbeddedRawOpcodes);

    let plan1 = OpticalStagePlan::plan(&meta_tier1, tier1);
    assert!(
        !plan1.apply_radial_vignette_in_warp,
        "Double-vignette must be disabled when GainMap was applied"
    );

    // Tier 2: Lensfun XML match
    let meta_tier2 = match make_dummy_metadata(false, Some("FE 50mm F1.2 GM")) {
        Some(m) => m,
        None => return,
    };
    let tier2 = resolver.resolve_tier(&meta_tier2);
    assert_eq!(tier2, OpticalResolutionTier::Tier2LensfunXml);

    let plan2 = OpticalStagePlan::plan(&meta_tier2, tier2);
    assert!(
        plan2.apply_radial_vignette_in_warp,
        "Vignette in warp must be enabled when GainMap is false"
    );

    // Tier 4: Unknown lens fallback to manual
    let meta_tier4 = match make_dummy_metadata(false, None) {
        Some(m) => m,
        None => return,
    };
    let tier4 = resolver.resolve_tier(&meta_tier4);
    assert_eq!(tier4, OpticalResolutionTier::Tier4ManualSliders);
}

#[test]
fn test_uv_lut_generation_accuracy() {
    let model = OpticalCorrectionModel {
        distortion: DistortionPoly::pincushion(0.04),
        lateral_ca: LateralTca {
            scale_r: 1.002,
            scale_b: 0.998,
        },
        vignette: RadialVignette::poly(0.5, 0.2),
    };

    let lut = UvLutGenerator::generate(&model, 256);
    assert_eq!(lut.dimension, 256);
    assert_eq!(lut.uv_r.len(), 256 * 256 * 2);
    assert_eq!(lut.uv_g.len(), 256 * 256 * 2);
    assert_eq!(lut.uv_b.len(), 256 * 256 * 2);

    // Center (128, 128) must map exactly to (0.5, 0.5) for all color planes
    let center_idx = (128 * 256 + 128) * 2;
    let u_center_g = lut.uv_g[center_idx];
    let v_center_g = lut.uv_g[center_idx + 1];

    assert!((u_center_g - 0.5).abs() < 2.5e-3);
    assert!((v_center_g - 0.5).abs() < 2.5e-3);

    // Periphery (220, 220) must exhibit lateral CA separation: r_R > r_G > r_B
    let periphery_idx = (220 * 256 + 220) * 2;
    let u_periphery_r = lut.uv_r[periphery_idx];
    let u_periphery_g = lut.uv_g[periphery_idx];
    let u_periphery_b = lut.uv_b[periphery_idx];

    assert!(
        u_periphery_r > u_periphery_g && u_periphery_g > u_periphery_b,
        "Lateral CA must separate R > G > B at optical periphery: R={u_periphery_r}, G={u_periphery_g}, B={u_periphery_b}"
    );
}

#[test]
fn test_catmull_rom_warp_rectification_and_positive_clamp() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let pipeline_res = Stage4WarpPipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    // High contrast step image: left half 0.0, right half 2.0 (triggers ringing in unconstrained bicubic)
    let mut rgb_in = vec![0.0_f32; count * 3];
    for y in 0..height {
        for x in (width / 2)..width {
            let idx = ((y * width + x) * 3) as usize;
            rgb_in[idx] = 2.0;
            rgb_in[idx + 1] = 2.0;
            rgb_in[idx + 2] = 2.0;
        }
    }

    let model = OpticalCorrectionModel {
        distortion: DistortionPoly::barrel(-0.05),
        lateral_ca: LateralTca {
            scale_r: 1.0,
            scale_b: 1.0,
        },
        vignette: RadialVignette::poly(0.4, 0.1),
    };
    let lut = UvLutGenerator::generate(&model, 256);

    let params = Stage4Params {
        width,
        height,
        apply_radial_vignette: false, // DNG GainMap already applied
        vignette_v1: model.vignette.v1,
        vignette_v2: model.vignette.v2,
    };

    let warped_res = pipeline.execute(&ctx, &rgb_in, &lut, &params);
    assert!(warped_res.is_ok());
    let warped = match warped_res {
        Ok(res) => res,
        Err(_) => return,
    };

    assert_eq!(warped.len(), count * 3);

    // Invariant: Zero negative values allowed (positive half-space clamp before ACEScg)
    for (i, &val) in warped.iter().enumerate() {
        assert!(
            val >= 0.0,
            "Warped pixel at index {i} must be >= 0.0 (found: {val})"
        );
    }
}
