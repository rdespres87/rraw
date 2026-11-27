use rraw_core::curves::{ChannelCurve, CurvePoint};
use rraw_core::recipe::{
    ColorToneRecipeClipboard, Recipe, Stage5GradingParams, StageHash, CURRENT_SCHEMA_VERSION,
};
use rraw_core::CoreError;

#[test]
fn curve_point_validates_bounds_and_updates_or_removes_interior_points(
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(
        CurvePoint::new(-0.1, 0.5),
        Err(CoreError::InvalidPhysicalValue {
            field: "curve_point_x",
            ..
        })
    ));
    assert!(matches!(
        CurvePoint::new(0.5, f32::NAN),
        Err(CoreError::InvalidPhysicalValue {
            field: "curve_point_y",
            ..
        })
    ));

    let mut curve = ChannelCurve::new_linear();
    curve.add_point(CurvePoint::new(0.5, 0.6)?);
    curve.add_point(CurvePoint::new(0.49999, 0.65)?);
    assert_eq!(curve.points.len(), 3);
    assert!((curve.points[1].y - 0.65).abs() < 1e-6);

    assert!(!curve.remove_point(0));
    assert!(!curve.remove_point(2));
    assert!(curve.remove_point(1));
    assert!(!curve.remove_point(1));
    assert_eq!(curve.points.len(), 2);
    Ok(())
}

#[test]
fn stage_hash_hex_roundtrip_and_validation() -> Result<(), Box<dyn std::error::Error>> {
    let hash = StageHash([0xAB; 32]);
    let hex = hash.to_hex();
    assert_eq!(hex, hash.to_string());
    assert_eq!(StageHash::from_hex(&hex)?, hash);

    assert!(matches!(
        StageHash::from_hex("abcd"),
        Err(CoreError::InvalidStageHashHex(_))
    ));
    assert!(matches!(
        StageHash::from_hex(&"zz".repeat(32)),
        Err(CoreError::InvalidStageHashHex(_))
    ));
    Ok(())
}

#[test]
fn recipe_stage_hashes_and_canonical_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut r = Recipe::default();
    assert_ne!(r.hash_l0("a"), r.hash_l0("b"));
    assert_ne!(r.hash_l1("a"), StageHash::ZERO);
    assert_ne!(r.hash_l2(), StageHash::ZERO);
    assert_ne!(r.hash_display("a", 1.0), r.hash_display("a", 4.0));

    let json = r.to_canonical_json()?;
    assert_eq!(Recipe::from_canonical_json(&json)?, r);

    r.schema_version = CURRENT_SCHEMA_VERSION + 1;
    let bad_ver = serde_json::to_string(&r)?;
    assert!(matches!(
        Recipe::from_canonical_json(&bad_ver),
        Err(CoreError::UnsupportedSchemaVersion { .. })
    ));

    assert!(matches!(
        Recipe::from_canonical_json("{invalid"),
        Err(CoreError::DeserializationError(_))
    ));
    Ok(())
}

#[test]
fn color_tone_clipboard_roundtrip_preserves_non_tonal_fields(
) -> Result<(), Box<dyn std::error::Error>> {
    let src = Stage5GradingParams {
        exposure_ev: 1.25,
        contrast: 0.2,
        ..Default::default()
    };
    let mut dst = Stage5GradingParams::default();
    let clipboard = ColorToneRecipeClipboard::from_stage5(&src);
    clipboard.apply_to_stage5(&mut dst);
    assert!((dst.exposure_ev - 1.25).abs() < 1e-6);
    assert!((dst.contrast - 0.2).abs() < 1e-6);
    Ok(())
}
