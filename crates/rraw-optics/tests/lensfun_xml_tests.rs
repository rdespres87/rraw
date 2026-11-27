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

#[test]
fn lensfun_xml_child_tags_and_model_variants() -> Result<(), Box<dyn std::error::Error>> {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<lensdatabase>
  <lens>
    <maker>Canon</maker>
    <model>EF 50mm f/1.4</model>
    <mount>Canon EF</mount>
    <cropfactor>1.0</cropfactor>
    <calibration>
      <distortion model="poly5" focal="50" k1="-0.01" k2="0.002"/>
      <distortion model="poly3" focal="35" k1="1.0"/>
      <distortion model="ptlens" focal="24" a="0.5" b="0.5" c="0.0"/>
      <distortion model="unknown" focal="18"/>
      <distortion model="poly3" focal="0" k1="0.1"/>
      <tca model="poly3" focal="50" kr="1.0002" kb="0.9998" br="0.0001" bb="-0.0001"/>
      <tca model="linear" focal="0" vr="1.0"/>
      <vignetting model="pa" focal="50" aperture="1.4" distance="5.0" k1="-0.3" k2="0.05" k3="-0.01"/>
      <vignetting model="pa" focal="0" aperture="2.8" k1="-0.2"/>
    </calibration>
  </lens>
</lensdatabase>"#;
    let db = rraw_optics::LensfunDatabase::from_xml_str(xml)?;
    assert_eq!(db.lenses().len(), 1);
    let lens = &db.lenses()[0];
    assert_eq!(lens.maker, "Canon");
    assert_eq!(lens.model, "EF 50mm f/1.4");
    assert_eq!(lens.mounts, ["Canon EF"]);
    assert_eq!(lens.crop_factor, Some(1.0));
    assert_eq!(lens.distortions.len(), 4);
    assert_eq!(lens.tcas.len(), 1);
    assert_eq!(lens.vignettings.len(), 1);
    Ok(())
}

#[test]
fn lensfun_from_reader_merge_and_interpolation_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let xml_a = r#"<lensdatabase>
  <lens>
    <maker>Sony</maker>
    <model>FE 24-70mm F2.8 GM</model>
    <calibration>
      <distortion model="poly3" focal="24" k1="-0.02"/>
      <distortion model="poly3" focal="70" k1="0.01"/>
      <tca model="linear" focal="24" vr="1.001" vb="0.999"/>
      <tca model="linear" focal="70" vr="0.999" vb="1.001"/>
      <vignetting model="pa" focal="24" aperture="2.8" k1="-0.4" k2="0.1" k3="0.0"/>
      <vignetting model="pa" focal="70" aperture="5.6" k1="-0.2" k2="0.05" k3="0.0"/>
    </calibration>
  </lens>
</lensdatabase>"#;

    let xml_b = r#"<lensdatabase>
  <lens>
    <maker>Sigma</maker>
    <model>35mm F1.4 DG HSM</model>
    <calibration>
      <distortion model="poly5" focal="35" k1="-0.01" k2="0.001"/>
    </calibration>
  </lens>
</lensdatabase>"#;

    let temp_dir = std::env::temp_dir().join(format!("rraw_lf_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir)?;
    let xml_path = temp_dir.join("sony.xml");
    std::fs::write(&xml_path, xml_a)?;

    let file = std::fs::File::open(&xml_path)?;
    let mut db = rraw_optics::LensfunDatabase::from_reader(std::io::BufReader::new(file))?;
    let db_b = rraw_optics::LensfunDatabase::from_xml_str(xml_b)?;
    db.merge(db_b);
    let _ = std::fs::remove_dir_all(&temp_dir);

    assert_eq!(db.lenses().len(), 2);
    assert!(db.search_lenses("", 1).len() <= 1);
    assert!(!db.search_lenses("Sony", 5).is_empty());

    assert!(db.find_lens("").is_none());
    assert!(db.find_lens("FE 24-70mm F2.8 GM").is_some());
    assert!(db.find_lens("Sony FE 24-70mm F2.8 GM").is_some());
    assert!(db.find_lens("24-70mm").is_some());
    Ok(())
}
