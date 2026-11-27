use rraw_optics::lensfun::LensfunDatabase;

#[test]
fn lensfun_parses_ptlens_tca_vignette_and_cropfactor_xml() -> Result<(), Box<dyn std::error::Error>>
{
    let xml = r#"<lensdatabase><lens><maker>TestMaker</maker><model>Test 50mm f/1.8</model><mount>TestMount</mount><cropfactor>1.5</cropfactor><calibration><distortion model="ptlens" focal="50" a="0.01" b="-0.02" c="0.005"/><distortion model="poly3" focal="35" k1="-0.01"/><tca model="poly3" focal="50" vr="1.0002" vb="0.9998"/><vignetting model="pa" focal="50" aperture="2.8" distance="10" k1="-0.5" k2="0.1" k3="-0.02"/></calibration></lens></lensdatabase>"#;
    let db = LensfunDatabase::from_xml_str(xml)?;
    let lens = db
        .find_lens("Test 50mm")
        .ok_or("expected synthetic Test 50mm lens")?;
    let model = db.interpolate_model(lens, 50.0, Some(2.8));
    assert!(model.distortion.k1.is_finite());
    assert!(model.vignette.v1.is_finite());
    Ok(())
}

#[test]
fn lensfun_load_with_system_and_discover_paths() {
    let _paths = LensfunDatabase::discover_system_paths();
    let db = LensfunDatabase::load_with_system();
    assert!(!db.lenses().is_empty());
}
