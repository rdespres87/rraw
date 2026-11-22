use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, CoreError, NoiseProfile, NoiseProfileSource,
    PhotoMetadata, PickStatus,
};
use rraw_optics::lensfun::{LensfunDatabase, LensfunError};
use rraw_optics::{OpticalResolutionTier, OpticalResolver};

fn test_metadata(
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
        camera_make: "Sony".to_owned(),
        camera_model: "ILCE-7RM4".to_owned(),
        lens_model: lens.map(ToString::to_string),
        iso: Some(100),
        aperture_f_number: aperture,
        focal_length_mm: focal,
        width: 9504,
        height: 6336,
        black_levels: [512.0; 4],
        white_levels: [16383.0; 4],
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

const SAMPLE_XML: &str = r#"<lensdatabase version="1">
    <lens>
        <maker>Sony</maker>
        <model>FE 24-70mm F2.8 GM</model>
        <mount>Sony E</mount>
        <cropfactor>1.0</cropfactor>
        <calibration>
            <distortion model="ptlens" focal="24" a="0.015" b="-0.045" c="0.005" />
            <distortion model="ptlens" focal="35" a="0.008" b="-0.020" c="0.002" />
            <distortion model="ptlens" focal="70" a="0.002" b="0.015" c="-0.001" />
            <tca model="poly3" focal="24" br="-0.0002" vr="1.0008" bb="0.0002" vb="0.9992" />
            <tca model="poly3" focal="70" br="-0.0001" vr="1.0003" bb="0.0001" vb="0.9997" />
            <vignetting model="pa" focal="24" aperture="2.8" distance="10" k1="-1.6" k2="1.2" k3="-0.4" />
            <vignetting model="pa" focal="24" aperture="8.0" distance="10" k1="-0.5" k2="0.3" k3="-0.1" />
            <vignetting model="pa" focal="70" aperture="2.8" distance="10" k1="-1.1" k2="0.8" k3="-0.2" />
            <vignetting model="pa" focal="70" aperture="8.0" distance="10" k1="-0.3" k2="0.1" k3="0.0" />
        </calibration>
    </lens>
</lensdatabase>"#;

#[test]
fn test_parse_lensfun_xml_string() -> Result<(), LensfunError> {
    let db = LensfunDatabase::from_xml_str(SAMPLE_XML)?;
    assert_eq!(db.len(), 1);

    let lens = db
        .find_lens("FE 24-70mm F2.8 GM")
        .ok_or_else(|| LensfunError::InvalidParameter("Lens not found".to_owned()))?;
    assert_eq!(lens.maker, "Sony");
    assert_eq!(lens.model, "FE 24-70mm F2.8 GM");
    assert_eq!(lens.distortions.len(), 3);
    assert_eq!(lens.tcas.len(), 2);
    assert_eq!(lens.vignettings.len(), 4);

    Ok(())
}

#[test]
fn test_embedded_catalog_contains_over_1000_lenses() -> Result<(), LensfunError> {
    let db = LensfunDatabase::embedded();
    assert!(
        db.len() >= 1000,
        "Embedded database must contain at least 1,000 lenses (found {})",
        db.len()
    );

    // Test prominent lenses across major brands
    assert!(db.find_lens("FE 24-70mm F2.8 GM").is_some());
    assert!(db.find_lens("EF 24-70mm f/2.8L II USM").is_some());
    assert!(db.find_lens("NIKKOR Z 24-70mm f/2.8 S").is_some());
    assert!(db.find_lens("XF 16-55mm F2.8 R LM WR").is_some());

    Ok(())
}

#[test]
fn test_dynamic_interpolation_across_focal_and_aperture() -> Result<(), LensfunError> {
    let db = LensfunDatabase::from_xml_str(SAMPLE_XML)?;
    let lens = db
        .find_lens("FE 24-70mm F2.8 GM")
        .ok_or_else(|| LensfunError::InvalidParameter("Lens must exist".to_owned()))?;

    // At 24mm f/2.8: barrel distortion (k1 < 0) and high vignetting
    let m24_f28 = db.interpolate_model(lens, 24.0, Some(2.8));
    assert!(
        m24_f28.distortion.k1 < 0.0,
        "24mm should be barrel distortion"
    );
    assert!(m24_f28.lateral_ca.scale_r > 1.0, "Red scale > 1");
    assert!(m24_f28.lateral_ca.scale_b < 1.0, "Blue scale < 1");
    assert!(m24_f28.vignette.v1 > 0.0, "Wide open vignetting gain > 0");

    // At 70mm f/2.8: pincushion distortion (k1 > 0)
    let m70_f28 = db.interpolate_model(lens, 70.0, Some(2.8));
    assert!(
        m70_f28.distortion.k1 > 0.0,
        "70mm should be pincushion distortion"
    );
    assert!(m24_f28.distortion.k1 < m70_f28.distortion.k1);

    // Vignette stopped down (f/8) vs wide open (f/2.8) at 24mm
    let m24_f8 = db.interpolate_model(lens, 24.0, Some(8.0));
    assert!(
        m24_f8.vignette.v1 < m24_f28.vignette.v1,
        "f/8 vignette gain should be less than f/2.8"
    );

    Ok(())
}

#[test]
fn test_optical_resolver_tier2_integration() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = OpticalResolver;
    let meta = test_metadata(Some("E 16mm f/2.8"), Some(16.0), Some(2.8))?;

    let tier = resolver.resolve_tier(&meta);
    assert_eq!(tier, OpticalResolutionTier::Tier2LensfunXml);

    let model = resolver.resolve_model(&meta, tier);
    assert!(
        model.distortion.k1 != 0.0 || model.distortion.k2 != 0.0,
        "Tier 2 Lensfun XML must resolve a non-zero distortion model"
    );

    Ok(())
}
