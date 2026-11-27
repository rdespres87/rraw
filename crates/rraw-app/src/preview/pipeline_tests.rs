use super::*;
use rraw_optics::{MatchedLens, OpticalCorrectionModel};

fn make_matched_lens(k1: f32, v1: f32) -> MatchedLens {
    let mut model = OpticalCorrectionModel::default();
    model.distortion.k1 = k1;
    model.vignette.v1 = v1;
    MatchedLens {
        profile_name: "Test Lens".to_string(),
        maker: "TestMaker".to_string(),
        model,
        is_exact_match: true,
    }
}

fn make_test_base(
    matched_lens: Option<MatchedLens>,
) -> Result<BaseRgbPreview, rraw_core::CoreError> {
    let cfa = rraw_core::SignedCfaBuffer::new(
        2,
        2,
        rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
        vec![0.0; 4],
    )?;
    let mut base = BaseRgbPreview::from_cfa(&cfa);
    base.matched_lens = matched_lens;
    base.focal_length_mm = Some(50.0);
    base.aperture_f_number = Some(2.8);
    Ok(base)
}

#[test]
fn profile_auto_disabled_returns_zero() -> Result<(), rraw_core::CoreError> {
    let base = make_test_base(Some(make_matched_lens(0.12, -0.35)))?;
    let mut grading = DarkroomGradingState::default();
    grading.optics.optical_tab = OpticalCorrectionTab::Profile;
    grading.optics.lens_profile_auto = false;
    assert_eq!(
        resolve_optical_distortion_and_vignette(&base, &grading),
        (0.0, 0.0)
    );
    Ok(())
}

#[test]
fn profile_matched_lens_applies_scaled_distortion_and_vignette() -> Result<(), rraw_core::CoreError>
{
    let base = make_test_base(Some(make_matched_lens(0.10, -0.40)))?;
    let mut grading = DarkroomGradingState::default();
    grading.optics.optical_tab = OpticalCorrectionTab::Profile;
    grading.optics.lens_profile_auto = true;
    grading.optics.lens_distortion_strength = 50.0;
    grading.optics.lens_vignette_strength = 75.0;
    let (k_dist, v_gain) = resolve_optical_distortion_and_vignette(&base, &grading);
    assert!((k_dist - 0.05).abs() < 1e-6);
    assert!((v_gain - (-0.30)).abs() < 1e-6);
    Ok(())
}

#[test]
fn profile_unknown_override_falls_back_to_matched_lens_and_caches_miss(
) -> Result<(), rraw_core::CoreError> {
    let base = make_test_base(Some(make_matched_lens(0.20, 0.40)))?;
    let mut grading = DarkroomGradingState::default();
    grading.optics.optical_tab = OpticalCorrectionTab::Profile;
    grading.optics.lens_profile_auto = true;
    grading.optics.lens_distortion_strength = 100.0;
    grading.optics.lens_vignette_strength = 100.0;
    grading.optics.lens_profile_override = Some("Nonexistent Lens 999mm".to_string());

    let first = resolve_optical_distortion_and_vignette(&base, &grading);
    let second = resolve_optical_distortion_and_vignette(&base, &grading);
    assert!((first.0 - 0.20).abs() < 1e-6);
    assert!((first.1 - 0.40).abs() < 1e-6);
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn manual_tab_applies_scaled_distortion_and_vignette() -> Result<(), rraw_core::CoreError> {
    let base = make_test_base(None)?;
    let mut grading = DarkroomGradingState::default();
    grading.optics.optical_tab = OpticalCorrectionTab::Manual;
    grading.optics.distortion_correction = true;
    grading.optics.distortion_amount = 50.0;
    grading.optics.vignette_correction = true;
    grading.optics.vignette_amount = -40.0;

    let (k_dist, v_gain) = resolve_optical_distortion_and_vignette(&base, &grading);
    assert!((k_dist - 0.075).abs() < 1e-6);
    assert!((v_gain - (-0.26)).abs() < 1e-6);
    Ok(())
}
