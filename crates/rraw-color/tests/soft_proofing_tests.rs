//! Color Management, ICC Output/Printer Profile Simulation, Rendering Intents, and Out-of-Gamut Detection.

use glam::Vec3;
use rraw_color::{
    evaluate_soft_proof, is_out_of_gamut, SoftProofConfig, SoftProofProfile,
    SoftProofRenderingIntent, SoftProofResult,
};

#[test]
fn test_soft_proof_profiles_neutral_axis_and_labels() {
    for profile in SoftProofProfile::ALL {
        assert!(!profile.label().is_empty());
        assert!(!profile.id_str().is_empty());
        assert_eq!(
            SoftProofProfile::from_id_str(profile.id_str()),
            Some(profile)
        );
    }
    assert_eq!(SoftProofProfile::from_id_str("unknown_profile"), None);

    for intent in SoftProofRenderingIntent::ALL {
        assert!(!intent.label().is_empty());
        assert!(!intent.id_str().is_empty());
        assert_eq!(
            SoftProofRenderingIntent::from_id_str(intent.id_str()),
            Some(intent)
        );
    }
}

#[test]
fn test_out_of_gamut_detection_across_target_spaces() {
    // 1. Mid-gray [0.18, 0.18, 0.18] in ACEScg / linear sRGB is strictly in gamut for all display and print profiles
    let mid_gray = Vec3::splat(0.18);
    for profile in SoftProofProfile::ALL {
        let (oog, delta) = is_out_of_gamut(mid_gray, profile);
        assert!(
            !oog,
            "Mid-gray should be inside gamut for {profile:?}, delta={delta}"
        );
        assert!(delta <= 1e-5);
    }

    // 2. Pure saturated wide-gamut primary (e.g., hyper-saturated cyan-green or negative component in sRGB)
    let hyper_cyan = Vec3::new(-0.18, 0.92, 0.88);
    let (oog_srgb, delta_srgb) = is_out_of_gamut(hyper_cyan, SoftProofProfile::SrgbIec61966);
    assert!(
        oog_srgb,
        "Hyper-cyan with negative red must be out of sRGB gamut"
    );
    assert!(delta_srgb > 0.10);

    // 3. Saturated color inside Display P3 / AdobeRGB or closer to boundary than Fine Art Matte Paper
    let deep_emerald = Vec3::new(0.02, 0.85, 0.12);
    let (_, delta_p3) = is_out_of_gamut(deep_emerald, SoftProofProfile::DisplayP3);
    let (oog_matte, delta_matte) =
        is_out_of_gamut(deep_emerald, SoftProofProfile::FineArtMattePaper);
    assert!(
        oog_matte && delta_matte > delta_p3,
        "Fine Art Matte Paper has a tighter gamut volume than Display P3 (delta_matte={delta_matte}, delta_p3={delta_p3})"
    );
}

#[test]
fn test_soft_proof_paper_white_and_black_ink_simulation() {
    let pure_black = Vec3::ZERO;
    let pure_white = Vec3::ONE;

    // Without paper simulation (or on sRGB display profile without paper ink), black is 0 and white is 1
    let cfg_display = SoftProofConfig {
        enabled: true,
        profile: SoftProofProfile::SrgbIec61966,
        intent: SoftProofRenderingIntent::Perceptual,
        simulate_paper_and_ink: false,
        show_gamut_warning: false,
    };
    let res_black_disp = evaluate_soft_proof(pure_black, &cfg_display);
    let res_white_disp = evaluate_soft_proof(pure_white, &cfg_display);
    assert!(res_black_disp.proofed_rgb.length() < 1e-4);
    assert!((res_white_disp.proofed_rgb - Vec3::ONE).length() < 1e-3);

    // With Fine Art Matte Paper + simulate_paper_and_ink:
    // - Black ink density lifts Dmax (pure black > 0.02)
    // - Warm cotton paper white scales Dmin (< 1.0 and slightly warm R > B)
    let cfg_matte = SoftProofConfig {
        enabled: true,
        profile: SoftProofProfile::FineArtMattePaper,
        intent: SoftProofRenderingIntent::Perceptual,
        simulate_paper_and_ink: true,
        show_gamut_warning: true,
    };
    let res_black_matte: SoftProofResult = evaluate_soft_proof(pure_black, &cfg_matte);
    let res_white_matte: SoftProofResult = evaluate_soft_proof(pure_white, &cfg_matte);

    assert!(
        res_black_matte.proofed_rgb.y > 0.025,
        "Matte paper black ink simulation must lift deep shadows to Dmax reflectance, got {:?}",
        res_black_matte.proofed_rgb
    );
    assert!(
        res_white_matte.proofed_rgb.y < 0.96,
        "Matte paper white simulation must compress peak white below 1.0, got {:?}",
        res_white_matte.proofed_rgb
    );
    assert!(
        res_white_matte.proofed_rgb.x > res_white_matte.proofed_rgb.z,
        "Cotton Fine Art Matte paper tint should be warm (R > B), got {:?}",
        res_white_matte.proofed_rgb
    );
}

#[test]
fn test_perceptual_vs_relative_colorimetric_rendering_intents() {
    let extreme_color = Vec3::new(-0.25, 1.15, 0.90);

    let cfg_perceptual = SoftProofConfig {
        enabled: true,
        profile: SoftProofProfile::SrgbIec61966,
        intent: SoftProofRenderingIntent::Perceptual,
        simulate_paper_and_ink: false,
        show_gamut_warning: true,
    };
    let cfg_relative = SoftProofConfig {
        intent: SoftProofRenderingIntent::RelativeColorimetric,
        ..cfg_perceptual
    };

    let res_perc = evaluate_soft_proof(extreme_color, &cfg_perceptual);
    let res_rel = evaluate_soft_proof(extreme_color, &cfg_relative);

    assert!(res_perc.is_out_of_gamut);
    assert!(res_rel.is_out_of_gamut);
    // Both must return valid clamped [0, 1] range outputs without NaNs
    for c in [res_perc.proofed_rgb, res_rel.proofed_rgb] {
        assert!(c.is_finite());
        assert!(c.min_element() >= 0.0 && c.max_element() <= 1.0);
    }
    // Perceptual smoothly rolls off high chroma differently from hard Relative Colorimetric clipping
    assert!(
        (res_perc.proofed_rgb - res_rel.proofed_rgb).length() > 1e-3,
        "Perceptual and Relative Colorimetric intents must differ on out-of-gamut colors"
    );
}
