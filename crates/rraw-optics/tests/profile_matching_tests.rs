use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, CoreError, NoiseProfile, NoiseProfileSource,
    PhotoMetadata, PickStatus,
};
use rraw_optics::{LensDatabase, OpticalResolutionTier, OpticalResolver, UvLutGenerator};

fn create_test_metadata(
    make: &str,
    lens: Option<&str>,
    focal: Option<f32>,
    aperture: Option<f32>,
) -> Result<PhotoMetadata, CoreError> {
    let noise_profile = NoiseProfile::new(
        [0.0001, 0.0001, 0.0001],
        [0.0004, 0.0004, 0.0004],
        NoiseProfileSource::CameraTable,
    )?;

    Ok(PhotoMetadata {
        camera_make: make.to_owned(),
        camera_model: "TestCam".to_owned(),
        lens_model: lens.map(ToString::to_string),
        iso: Some(100),
        aperture_f_number: aperture,
        focal_length_mm: focal,
        width: 1000,
        height: 1000,
        black_levels: [512.0; 4],
        white_levels: [4096.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile,
        label: ColorLabel::None,
        rating: 0,
        pick: PickStatus::Unflagged,
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        keywords: Vec::new(),
    })
}

#[test]
fn test_lens_database_find_lens_exact_and_fuzzy() {
    // 1. Exact name match
    let p_sony = LensDatabase::find_lens("Sony FE 24-70mm F2.8 GM");
    assert!(p_sony.is_some(), "Exact Sony 24-70mm GM should match");
    assert_eq!(p_sony.map(|p| p.maker), Some("Sony"));

    // 2. Alias match
    let p_alias = LensDatabase::find_lens("SEL2470GM");
    assert!(p_alias.is_some(), "SEL2470GM alias should match");
    assert_eq!(p_alias.map(|p| p.name), Some("Sony FE 24-70mm F2.8 GM"));

    // 3. Brand omitted / fuzzy
    let p_fuzzy = LensDatabase::find_lens("FE 24-70mm F2.8 GM");
    assert!(p_fuzzy.is_some(), "Omitted brand should match");

    // 4. Case-insensitivity
    let p_lower = LensDatabase::find_lens("canon rf 50mm f1.2 l usm");
    assert!(p_lower.is_some(), "Lowercase Canon 50mm should match");
    assert_eq!(p_lower.map(|p| p.maker), Some("Canon"));

    // 5. Nikon, Fujifilm, Sigma, Tamron
    assert!(LensDatabase::find_lens("NIKKOR Z 24-70mm f/2.8 S").is_some());
    assert!(LensDatabase::find_lens("XF 16-55mm F2.8 R LM WR").is_some());
    assert!(LensDatabase::find_lens("Sigma 24-70mm Art").is_some());
    assert!(LensDatabase::find_lens("Tamron 28-75mm").is_some());

    // 6. Unknown lens returns None
    assert!(LensDatabase::find_lens("Unknown Imaginary 12-1200mm f/1.0").is_none());
}

#[test]
fn test_lens_profile_zoom_interpolation() -> Result<(), Box<dyn std::error::Error>> {
    let p = LensDatabase::find_lens("Sony FE 24-70mm F2.8 GM").ok_or("Lens must exist")?;

    // At 24mm: barrel distortion (k1 < 0)
    let m24 = p.interpolate_model(24.0, Some(2.8));
    assert!(
        m24.distortion.k1 < 0.0,
        "24mm must have barrel distortion (k1 < 0)"
    );

    // At 70mm: pincushion distortion (k1 > 0)
    let m70 = p.interpolate_model(70.0, Some(2.8));
    assert!(
        m70.distortion.k1 > 0.0,
        "70mm must have pincushion distortion (k1 > 0)"
    );

    // At 35mm and 50mm: monotonic increase of k1
    let m35 = p.interpolate_model(35.0, Some(2.8));
    let m50 = p.interpolate_model(50.0, Some(2.8));
    assert!(m24.distortion.k1 < m35.distortion.k1);
    assert!(m35.distortion.k1 < m50.distortion.k1);
    assert!(m50.distortion.k1 < m70.distortion.k1);

    // Clamping outside range: 18mm should clamp to 24mm
    let m18 = p.interpolate_model(18.0, Some(2.8));
    assert_eq!(m18.distortion.k1, m24.distortion.k1);
    Ok(())
}

#[test]
fn test_lens_profile_aperture_vignetting() -> Result<(), Box<dyn std::error::Error>> {
    let p = LensDatabase::find_lens("Sony FE 24-70mm F2.8 GM").ok_or("Lens must exist")?;

    let v_wide = p.interpolate_model(24.0, Some(2.8));
    let v_stopped = p.interpolate_model(24.0, Some(8.0));

    assert!(
        v_wide.vignette.v1 > v_stopped.vignette.v1,
        "Wide open vignetting ({}) must be stronger than stopped down ({})",
        v_wide.vignette.v1,
        v_stopped.vignette.v1
    );

    // Mid-aperture (f/4.0) should be between
    let v_mid = p.interpolate_model(24.0, Some(4.0));
    assert!(v_mid.vignette.v1 < v_wide.vignette.v1);
    assert!(v_mid.vignette.v1 > v_stopped.vignette.v1);
    Ok(())
}

#[test]
fn test_optical_resolver_resolve_with_strength() -> Result<(), CoreError> {
    let resolver = OpticalResolver;
    let meta = create_test_metadata(
        "Sony",
        Some("Sony FE 24-70mm F2.8 GM"),
        Some(24.0),
        Some(2.8),
    )?;

    let tier = resolver.resolve_tier(&meta);
    assert_eq!(tier, OpticalResolutionTier::Tier2LensfunXml);

    let m_100 = resolver.resolve_with_strength(&meta, tier, 100.0, 100.0, 100.0);
    assert!(m_100.distortion.k1 < -0.01);
    assert!(m_100.vignette.v1 > 0.3);

    // 0% strength resets aberrations to zero / identity
    let m_0 = resolver.resolve_with_strength(&meta, tier, 0.0, 0.0, 0.0);
    assert_eq!(m_0.distortion.k1, 0.0);
    assert_eq!(m_0.vignette.v1, 0.0);
    assert_eq!(m_0.lateral_ca.scale_r, 1.0);
    assert_eq!(m_0.lateral_ca.scale_b, 1.0);

    // 200% strength doubles distortion and vignetting
    let m_200 = resolver.resolve_with_strength(&meta, tier, 200.0, 200.0, 100.0);
    assert!((m_200.distortion.k1 - 2.0 * m_100.distortion.k1).abs() < 1e-5);
    assert!((m_200.vignette.v1 - 2.0 * m_100.vignette.v1).abs() < 1e-5);

    Ok(())
}

#[test]
fn test_match_metadata_fallback() -> Result<(), CoreError> {
    // 1. Unknown lens with wide focal length
    let meta_wide = create_test_metadata("VintageBrand", None, Some(20.0), Some(3.5))?;
    let matched_wide = LensDatabase::match_metadata(&meta_wide);
    assert!(!matched_wide.is_exact_match);
    assert!(
        matched_wide.model.distortion.k1 < 0.0,
        "Wide angle fallback must barrel-distort"
    );

    // 2. Unknown lens with tele focal length
    let meta_tele = create_test_metadata("VintageBrand", None, Some(135.0), Some(2.8))?;
    let matched_tele = LensDatabase::match_metadata(&meta_tele);
    assert!(!matched_tele.is_exact_match);
    assert!(
        matched_tele.model.distortion.k1 > 0.0,
        "Telephoto fallback must pincushion-distort"
    );

    // 3. Known lens gives exact match
    let meta_known = create_test_metadata(
        "Canon",
        Some("Canon RF 50mm F1.2 L USM"),
        Some(50.0),
        Some(1.2),
    )?;
    let matched_known = LensDatabase::match_metadata(&meta_known);
    assert!(matched_known.is_exact_match);
    assert_eq!(matched_known.profile_name, "Canon RF 50mm F1.2 L USM");

    Ok(())
}

#[test]
fn test_numerical_stability_of_uv_lut() -> Result<(), CoreError> {
    let meta = create_test_metadata("Sony", Some("FE 24-70mm F2.8 GM"), Some(24.0), Some(2.8))?;
    let matched = LensDatabase::match_metadata(&meta);

    let lut = UvLutGenerator::generate(&matched.model, 64);
    assert_eq!(lut.dimension, 64);

    for (i, (&ur, &vr)) in lut
        .uv_r
        .iter()
        .step_by(2)
        .zip(lut.uv_r.iter().skip(1).step_by(2))
        .enumerate()
    {
        assert!(ur.is_finite(), "ur at {i} must be finite, got {ur}");
        assert!(vr.is_finite(), "vr at {i} must be finite, got {vr}");
        assert!((0.0..=1.0).contains(&ur), "ur must be in [0, 1], got {ur}");
        assert!((0.0..=1.0).contains(&vr), "vr must be in [0, 1], got {vr}");
    }

    Ok(())
}
