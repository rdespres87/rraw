use super::*;

const SAMPLE_XML: &str = r#"
<lensdatabase>
  <lens>
    <maker>TestMaker</maker>
    <model>TestZoom 19-63mm f/3.1</model>
    <mount>E-mount</mount>
    <cropfactor>1.0</cropfactor>
    <calibration>
      <distortion model="ptlens" focal="24" a="0.01" b="-0.02" c="0.005" />
      <distortion model="poly3" focal="50" k1="-0.01" />
      <distortion model="poly5" focal="70" k1="0.005" k2="-0.002" />
      <distortion model="unknown" focal="85" k1="0.01" />
      <distortion model="ptlens" focal="100" a="0.5" b="0.5" c="0.0" />
      <distortion model="poly3" focal="0" k1="0.1" />
      <tca model="poly3" focal="24" vr="1.0004" vb="0.9996" br="0.0002" bb="-0.0002" />
      <tca model="linear" focal="70" kr="1.0001" kb="0.9999" />
      <tca model="linear" focal="0" kr="1.0" kb="1.0" />
      <vignetting model="pa" focal="24" aperture="2.8" distance="1000" k1="-0.4" k2="0.1" k3="-0.02" />
      <vignetting model="pa" focal="24" aperture="8.0" distance="1000" k1="-0.1" k2="0.02" k3="0.0" />
      <vignetting model="pa" focal="70" aperture="2.8" distance="1000" k1="-0.3" k2="0.05" k3="0.0" />
      <vignetting model="pa" focal="70" aperture="8.0" distance="1000" k1="-0.05" k2="0.0" k3="0.0" />
      <vignetting model="pa" focal="0" aperture="2.8" k1="-0.1" />
    </calibration>
  </lens>
</lensdatabase>
"#;

#[test]
fn parses_xml_merges_searches_and_interpolates_all_models() -> Result<(), LensfunError> {
    let empty = LensfunDatabase::new();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert!(empty.lenses().is_empty());
    assert!(empty.find_lens("").is_none());
    assert!(empty.find_lens("Missing").is_none());

    let mut db = LensfunDatabase::from_xml_str(SAMPLE_XML)?;
    assert_eq!(db.len(), 1);
    assert!(!db.is_empty());

    // Merging duplicate keeps length 1; merging distinct lens adds it
    db.merge(LensfunDatabase::from_xml_str(SAMPLE_XML)?);
    assert_eq!(db.len(), 1);
    let extra = LensfunDatabase::from_xml_str(
        r#"<lensdatabase><lens><maker>Other</maker><model>Other 35mm</model><calibration/></lens></lensdatabase>"#,
    )?;
    db.merge(extra);
    assert_eq!(db.len(), 2);

    // Exact model, full name, and substring matching + search_lenses
    assert!(db.find_lens("TestZoom 19-63mm f/3.1").is_some());
    assert!(db.find_lens("TestMaker TestZoom 19-63mm f/3.1").is_some());
    assert!(db.find_lens("TestZoom").is_some());
    assert_eq!(db.search_lenses("", 1).len(), 1);
    assert_eq!(db.search_lenses("testzoom", 5).len(), 1);

    // Interpolate below min, mid-range, and above max for focal & aperture
    if let Some(lens) = db.find_lens("TestZoom") {
        let m_low = db.interpolate_model(lens, 16.0, Some(1.4));
        let m_mid = db.interpolate_model(lens, 45.0, Some(5.6));
        let m_high = db.interpolate_model(lens, 200.0, Some(16.0));
        assert!(m_low.vignette.v1 >= 0.0);
        assert!(m_mid.vignette.v1 >= 0.0);
        assert!(m_high.vignette.v1 >= 0.0);
        assert!(m_mid.lateral_ca.scale_r > 0.99);
    }

    // Empty calibration lens interpolation returns defaults
    if let Some(other) = db.find_lens("Other 35mm") {
        let m_empty = db.interpolate_model(other, 35.0, None);
        assert_eq!(m_empty.distortion, DistortionPoly::default());
        assert_eq!(m_empty.lateral_ca, LateralTca::default());
        assert_eq!(m_empty.vignette, RadialVignette::default());
    }

    Ok(())
}

fn sample_metadata() -> PhotoMetadata {
    PhotoMetadata {
        camera_make: "TestMake".to_owned(),
        camera_model: "TestBody".to_owned(),
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: None,
        iso: None,
        width: 64,
        height: 64,
        black_levels: [0.0; 4],
        white_levels: [16383.0; 4],
        cfa_pattern: rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
        noise_profile: rraw_core::NoiseProfile::default(),
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        rating: 0,
        pick: rraw_core::PickStatus::Unflagged,
        label: rraw_core::ColorLabel::None,
        keywords: Vec::new(),
    }
}

#[test]
fn match_metadata_handles_dng_warp_lensfun_and_fallback() -> Result<(), LensfunError> {
    let db = LensfunDatabase::from_xml_str(SAMPLE_XML)?;

    // Priority 0: DNG WarpRectilinear
    let meta_dng = PhotoMetadata {
        dng_warp_rectilinear: Some(rraw_core::DngWarpRectilinearData {
            planes: 1,
            coefficients: vec![1.0, -0.02, 0.01, 0.0, 0.0, 0.0],
        }),
        ..sample_metadata()
    };
    let matched_dng = db.match_metadata(&meta_dng);
    assert_eq!(matched_dng.maker, "DNG Embedded");
    assert!(matched_dng.is_exact_match);

    // Priority 2: Lensfun DB match (model without maker prefix & with maker prefix)
    let meta_lf = PhotoMetadata {
        lens_model: Some("TestZoom 19-63mm f/3.1".to_owned()),
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(4.0),
        ..sample_metadata()
    };
    let matched_lf = db.match_metadata(&meta_lf);
    assert_eq!(matched_lf.profile_name, "TestMaker TestZoom 19-63mm f/3.1");
    assert!(matched_lf.is_exact_match);

    let db_prefixed = LensfunDatabase::from_xml_str(
        r#"<lensdatabase><lens><maker>Acme</maker><model>Acme 85mm f/1.8</model><calibration><vignetting model="pa" focal="85" aperture="1.8" k1="-0.2"/></calibration></lens></lensdatabase>"#,
    )?;
    let meta_pref = PhotoMetadata {
        lens_model: Some("Acme 85mm f/1.8".to_owned()),
        focal_length_mm: Some(85.0),
        aperture_f_number: Some(2.8),
        ..sample_metadata()
    };
    let matched_pref = db_prefixed.match_metadata(&meta_pref);
    assert_eq!(matched_pref.profile_name, "Acme 85mm f/1.8");

    // Fallback when unknown lens
    let meta_unknown = sample_metadata();
    let _ = db.match_metadata(&meta_unknown);
    let _ = LensfunDatabase::discover_system_paths();
    assert!(!LensfunDatabase::embedded().is_empty());
    assert!(!LensfunDatabase::global().is_empty());
    let _ = LensfunDatabase::from_xml_str("<lensdatabase><unclosed");
    let deg_xml = r#"<lensdatabase><lens><maker>M</maker><model>D</model><calibration><distortion model="ptlens" focal="50" a="0.5" b="0.5" c="0.0"/><distortion model="poly3" focal="35" k1="1.0"/><vignetting model="pa" focal="50" aperture="2.8" distance="500" k1="-0.2"/><vignetting model="pa" focal="50" aperture="2.8" distance="2000" k1="-0.1"/></calibration></lens></lensdatabase>"#;
    let deg_db = LensfunDatabase::from_xml_str(deg_xml)?;
    if let Some(l) = deg_db.find_lens("D") {
        let m = deg_db.interpolate_model(l, 42.0, Some(2.8));
        assert!(m.vignette.v1.is_finite());
    }
    Ok(())
}

#[test]
fn from_xml_str_handles_empty_missing_model_and_malformed_xml() -> Result<(), LensfunError> {
    let empty = LensfunDatabase::from_xml_str("<lensdatabase/>")?;
    assert!(empty.is_empty());

    let no_model = LensfunDatabase::from_xml_str(
        "<lensdatabase><lens><maker>NoModel</maker><mount>EF</mount><cropfactor>1.6</cropfactor></lens></lensdatabase>",
    )?;
    assert_eq!(no_model.len(), 1);
    assert!(no_model.lenses()[0].model.is_empty());

    let malformed = LensfunDatabase::from_xml_str("<lensdatabase><lens></lensdatabase>");
    assert!(malformed.is_err());
    Ok(())
}

#[test]
fn interpolate_single_point_and_exact_boundaries() -> Result<(), LensfunError> {
    let single_xml = r#"<lensdatabase><lens>
        <maker>T</maker><model>Single</model>
        <calibration>
            <distortion model="poly3" focal="35" k1="-0.02" />
            <tca model="linear" focal="35" kr="1.001" kb="0.999" />
            <vignetting model="pa" focal="35" aperture="2.8" k1="-0.3" />
        </calibration>
    </lens></lensdatabase>"#;
    let db = LensfunDatabase::from_xml_str(single_xml)?;
    let lens = db.find_lens("Single").ok_or_else(|| {
        LensfunError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing lens",
        ))
    })?;
    let m_low = db.interpolate_model(lens, 10.0, None);
    let m_high = db.interpolate_model(lens, 200.0, Some(11.0));
    assert_eq!(m_low.distortion, m_high.distortion);
    assert_eq!(m_low.lateral_ca, m_high.lateral_ca);
    assert_eq!(m_low.vignette, m_high.vignette);

    let two_xml = r#"<lensdatabase><lens>
        <maker>T</maker><model>Two</model>
        <calibration>
            <distortion model="poly5" focal="20" k1="-0.04" k2="0.0" />
            <distortion model="poly5" focal="60" k1="-0.02" k2="0.0" />
            <tca model="linear" focal="20" kr="1.002" kb="0.998" />
            <tca model="linear" focal="60" kr="1.000" kb="1.000" />
        </calibration>
    </lens></lensdatabase>"#;
    let db2 = LensfunDatabase::from_xml_str(two_xml)?;
    let lens2 = db2.find_lens("Two").ok_or_else(|| {
        LensfunError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing lens",
        ))
    })?;
    let m_mid = db2.interpolate_model(lens2, 40.0, None);
    assert!((m_mid.distortion.k1 - (-0.03)).abs() < 1e-5);
    assert!((m_mid.lateral_ca.scale_r - 1.001).abs() < 1e-5);
    Ok(())
}

#[test]
fn search_lenses_respects_limit_and_case_insensitivity() -> Result<(), LensfunError> {
    let xml = r#"<lensdatabase>
        <lens><maker>Canon</maker><model>EF 50mm</model><calibration/></lens>
        <lens><maker>Canon</maker><model>EF 85mm</model><calibration/></lens>
        <lens><maker>Nikon</maker><model>AF 50mm</model><calibration/></lens>
    </lensdatabase>"#;
    let db = LensfunDatabase::from_xml_str(xml)?;
    assert!(db.search_lenses("canon", 1).len() <= 1);
    assert_eq!(db.search_lenses("CANON", 10).len(), 2);
    assert!(db.search_lenses("nonexistent_xyz", 5).is_empty());
    Ok(())
}

#[test]
fn match_metadata_prefers_curated_and_handles_missing_lens() {
    let db = LensfunDatabase::new();
    let meta_none = sample_metadata();
    let fallback = db.match_metadata(&meta_none);
    assert!(!fallback.is_exact_match);

    let curated_name = "Sony FE 24-70mm F2.8 GM";
    let meta_curated = PhotoMetadata {
        lens_model: Some(curated_name.to_owned()),
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(2.8),
        ..sample_metadata()
    };
    let matched = db.match_metadata(&meta_curated);
    assert!(matched.is_exact_match);
    assert_eq!(matched.profile_name, curated_name);
}

#[test]
fn parse_ptlens_distortion_poly3_tca_and_vignetting_pa() -> Result<(), LensfunError> {
    let xml = r#"<lensdatabase><lens>
        <maker>T</maker><model>Test 24mm</model>
        <calibration>
            <distortion model="ptlens" focal="24" a="0.01" b="-0.02" c="0.005" />
            <distortion model="poly3" focal="0" k1="0.1" />
            <tca model="poly3" focal="24" vr="1.001" vb="0.999" />
            <vignetting model="pa" focal="24" aperture="2.8" k1="-0.4" k2="0.1" k3="-0.02" />
        </calibration>
    </lens></lensdatabase>"#;
    let db = LensfunDatabase::from_xml_str(xml)?;
    let lens = db.find_lens("Test 24mm").ok_or_else(|| {
        LensfunError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "lens Test 24mm not found",
        ))
    })?;
    let model = db.interpolate_model(lens, 24.0, Some(2.8));
    assert!(model.distortion.k1 != 0.0);
    assert!(model.lateral_ca != Default::default());
    assert!(model.vignette != Default::default());
    Ok(())
}

#[test]
fn from_reader_and_load_with_system_succeed() -> Result<(), Box<dyn std::error::Error>> {
    let xml = r#"<lensdatabase><lens><maker>T</maker><model>Reader Lens</model><calibration><distortion model="ptlens" focal="35" a="0.01" b="0" c="0" /></calibration></lens></lensdatabase>"#;
    let db = LensfunDatabase::from_reader(std::io::Cursor::new(xml.as_bytes()))?;
    assert!(db.find_lens("Reader Lens").is_some());
    let sys_db = LensfunDatabase::load_with_system();
    assert!(!sys_db.is_empty());
    Ok(())
}

#[test]
fn global_database_is_non_empty() {
    assert!(!LensfunDatabase::global().is_empty());
}

#[test]
fn merge_and_find_lens_handle_duplicates_full_name_and_empty_queries() -> Result<(), LensfunError> {
    let mut base = LensfunDatabase::from_xml_str(
        "<lensdatabase><lens><maker>Canon</maker><model>RF 50mm</model><calibration/></lens></lensdatabase>",
    )?;
    let extra = LensfunDatabase::from_xml_str(
        "<lensdatabase><lens><maker>Canon</maker><model>RF 50mm</model><calibration/></lens><lens><maker>Nikon</maker><model>Z 35mm</model><calibration/></lens></lensdatabase>",
    )?;
    base.merge(extra);
    assert_eq!(base.len(), 2);
    assert!(base.find_lens("").is_none());
    assert!(base.find_lens("Canon RF 50mm").is_some());
    assert!(base.find_lens("Z 35mm").is_some());
    assert!(base.find_lens("nonexistent_lens_xyz").is_none());
    assert_eq!(base.search_lenses("", 5).len(), 2);
    Ok(())
}

#[test]
fn vignetting_multi_focal_interpolation_and_error_display() -> Result<(), LensfunError> {
    let xml = r#"<lensdatabase>
        <camera><maker>CamMaker</maker><model>CamModel</model><mount>M</mount><cropfactor>1.5</cropfactor></camera>
        <lens><maker>LM</maker><model>LM1</model><mount>M</mount><calibration>
            <vignetting model="pa" focal="24" aperture="2.8" k1="-0.4" k2="0.1" k3="-0.02" />
            <vignetting model="pa" focal="24" aperture="8.0" k1="-0.1" k2="0.02" k3="0.0" />
            <vignetting model="pa" focal="50" aperture="2.8" k1="-0.3" k2="0.08" k3="-0.01" />
            <vignetting model="pa" focal="50" aperture="8.0" k1="-0.08" k2="0.01" k3="0.0" />
            <vignetting model="pa" focal="70" aperture="2.8" k1="-0.25" k2="0.05" k3="0.0" />
            <vignetting model="pa" focal="70" aperture="8.0" k1="-0.05" k2="0.0" k3="0.0" />
            <distortion model="poly5" focal="24" k1="-0.02" k2="0.004" />
            <distortion model="poly5" focal="50" k1="-0.01" k2="0.002" />
            <distortion model="poly5" focal="70" k1="0.005" k2="-0.001" />
            <tca model="linear" focal="24" kr="1.002" kb="0.998" />
            <tca model="linear" focal="50" kr="1.001" kb="0.999" />
            <tca model="linear" focal="70" kr="1.0005" kb="0.9995" />
        </calibration></lens>
    </lensdatabase>"#;
    let db = LensfunDatabase::from_xml_str(xml)?;
    let lens = db.find_lens("LM1").ok_or_else(|| {
        LensfunError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing LM1",
        ))
    })?;
    let r1 = db.interpolate_model(lens, 60.0, Some(5.6));
    let r2 = db.interpolate_model(lens, 20.0, Some(1.4));
    let r3 = db.interpolate_model(lens, 100.0, Some(16.0));
    for r in [&r1, &r2, &r3] {
        assert!(r.vignette.v1.is_finite() && r.vignette.v1 != 0.0);
        assert!(r.distortion.k1.is_finite() && r.distortion.k1 != 0.0);
    }
    let err = LensfunError::Io(std::io::Error::other("io err"));
    assert!(err.to_string().contains("io err"));
    Ok(())
}

#[test]
fn load_with_system_discovers_xml_in_env_dir() -> Result<(), Box<dyn std::error::Error>> {
    struct EnvAndDirGuard {
        dir: std::path::PathBuf,
    }
    impl Drop for EnvAndDirGuard {
        fn drop(&mut self) {
            std::env::remove_var("RRAW_LENSFUN_PATH");
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    let dir = std::env::temp_dir().join(format!("rraw_lf_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let _guard = EnvAndDirGuard { dir: dir.clone() };
    let xml_path = dir.join("custom.xml");
    std::fs::write(
        &xml_path,
        "<lensdatabase><lens><maker>EnvMaker</maker><model>EnvLens 42mm</model><calibration/></lens></lensdatabase>",
    )?;
    std::env::set_var("RRAW_LENSFUN_PATH", &dir);
    let paths = LensfunDatabase::discover_system_paths();
    assert!(paths.iter().any(|p| p == &dir));
    let sys_db = LensfunDatabase::load_with_system();
    assert!(sys_db.find_lens("EnvLens 42mm").is_some());
    Ok(())
}
