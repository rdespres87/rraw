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
        dng_warp_rectilinear: None,
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

#[test]
fn test_dng_warp_rectilinear_opcode_conversion_and_tier_1_resolution() {
    let resolver = OpticalResolver;

    // Synthetic 1-plane DNG WarpRectilinear coefficients: [k0, k1, k2, k3]
    let coeffs_1plane = vec![1.0, 0.02, -0.005, 0.001];
    let model_1plane = OpticalCorrectionModel::from_dng_warp_rectilinear(1, &coeffs_1plane);
    assert!((model_1plane.distortion.k1 - 0.02).abs() < 1e-5);
    assert!((model_1plane.distortion.k2 - -0.005).abs() < 1e-5);
    assert!((model_1plane.distortion.k3 - 0.001).abs() < 1e-5);
    assert_eq!(model_1plane.lateral_ca.scale_r, 1.0);
    assert_eq!(model_1plane.lateral_ca.scale_b, 1.0);

    // Synthetic 3-plane DNG WarpRectilinear coefficients (12 coeffs total: R, G, B)
    // Plane 0 (Red): k0=1.002, k1=0.021
    // Plane 1 (Green): k0=1.000, k1=0.020, k2=-0.005, k3=0.001
    // Plane 2 (Blue): k0=0.998, k1=0.019
    let coeffs_3planes = vec![
        1.002, 0.021, 0.0, 0.0, // Plane 0: Red
        1.000, 0.020, -0.005, 0.001, // Plane 1: Green
        0.998, 0.019, 0.0, 0.0, // Plane 2: Blue
    ];
    let model_3planes = OpticalCorrectionModel::from_dng_warp_rectilinear(3, &coeffs_3planes);
    assert!((model_3planes.distortion.k1 - 0.020).abs() < 1e-5);
    assert!((model_3planes.lateral_ca.scale_r - 1.002).abs() < 1e-5);
    assert!((model_3planes.lateral_ca.scale_b - 0.998).abs() < 1e-5);

    // Verify metadata priority in OpticalResolver & LensfunDatabase
    let mut meta = match make_dummy_metadata(false, Some("Unknown Custom Lens")) {
        Some(m) => m,
        None => return,
    };
    meta.dng_warp_rectilinear = Some(rraw_core::DngWarpRectilinearData {
        planes: 3,
        coefficients: coeffs_3planes,
    });

    let tier = resolver.resolve_tier(&meta);
    assert_eq!(tier, OpticalResolutionTier::Tier1EmbeddedRawOpcodes);

    let matched = rraw_optics::LensfunDatabase::global().match_metadata(&meta);
    assert_eq!(matched.maker, "DNG Embedded");
    assert!(matched.is_exact_match);
    assert!(matched.profile_name.contains("DNG Opcode"));
    assert!((matched.model.distortion.k1 - 0.020).abs() < 1e-5);
}

#[test]
fn test_dng_warp_rectilinear_uv_lut_generation() {
    let coeffs = vec![
        1.001, 0.01, 0.0, 0.0, 1.000, 0.01, 0.0, 0.0, 0.999, 0.01, 0.0, 0.0,
    ];
    let lut = UvLutGenerator::generate_from_dng_warp_rectilinear(3, &coeffs, 128);
    assert_eq!(lut.dimension, 128);
    assert_eq!(lut.uv_r.len(), 128 * 128 * 2);
    assert_eq!(lut.uv_g.len(), 128 * 128 * 2);
    assert_eq!(lut.uv_b.len(), 128 * 128 * 2);

    // Center coordinates at normalized (0.5, 0.5) must be preserved
    let center_idx = (64 * 128 + 64) * 2;
    assert!((lut.uv_g[center_idx] - 0.5).abs() < 0.02);
    assert!((lut.uv_g[center_idx + 1] - 0.5).abs() < 0.02);
}

#[test]
fn test_lensfun_search_lenses() {
    let db = rraw_optics::LensfunDatabase::global();
    let results = db.search_lenses("Sigma 18-35mm", 5);
    assert!(!results.is_empty(), "Database must find Sigma 18-35mm");
    assert!(results[0].to_lowercase().contains("sigma"));
    assert!(results[0].contains("18-35mm"));
}
