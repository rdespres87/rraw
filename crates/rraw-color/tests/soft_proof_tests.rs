use glam::Vec3;
use rraw_color::soft_proof::{
    evaluate_soft_proof, SoftProofConfig, SoftProofProfile, SoftProofRenderingIntent,
};

#[test]
fn soft_proof_profile_and_intent_metadata_roundtrip() {
    for p in SoftProofProfile::ALL {
        assert!(!p.short_label().is_empty());
        assert_eq!(SoftProofProfile::from_id_str(p.id_str()), Some(p));
    }
    assert!(SoftProofProfile::FineArtMattePaper.is_print_medium());
    assert!(!SoftProofProfile::SrgbIec61966.is_print_medium());

    for i in SoftProofRenderingIntent::ALL {
        assert!(!i.label().is_empty());
        assert_eq!(SoftProofRenderingIntent::from_id_str(i.id_str()), Some(i));
    }

    let cfg = SoftProofConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.profile, SoftProofProfile::FineArtMattePaper);
}

#[test]
fn soft_proof_disabled_passes_clamped_input() {
    let input = Vec3::new(0.4, 0.5, 0.6);
    let res = evaluate_soft_proof(input, &SoftProofConfig::default());
    let diff = (res.proofed_rgb - input).abs();
    assert!(diff.max_element() < 1e-6);
}

#[test]
fn soft_proof_relative_colorimetric_and_paper_ink_profiles() {
    let base_cfg = SoftProofConfig {
        enabled: true,
        simulate_paper_and_ink: true,
        intent: SoftProofRenderingIntent::RelativeColorimetric,
        ..Default::default()
    };
    let input = Vec3::new(0.95, 0.2, 0.1);

    for profile in [
        SoftProofProfile::FineArtMattePaper,
        SoftProofProfile::GlossyBarytaPaper,
        SoftProofProfile::DisplayP3,
    ] {
        let cfg = SoftProofConfig {
            profile,
            ..base_cfg
        };
        let out = evaluate_soft_proof(input, &cfg).proofed_rgb;
        assert!(out.x.is_finite() && out.y.is_finite() && out.z.is_finite());
        assert!(
            (0.0..=1.0).contains(&out.x)
                && (0.0..=1.0).contains(&out.y)
                && (0.0..=1.0).contains(&out.z)
        );
    }

    let perceptual_cfg = SoftProofConfig {
        intent: SoftProofRenderingIntent::Perceptual,
        ..base_cfg
    };
    let gray = Vec3::splat(0.5);
    let out = evaluate_soft_proof(gray, &perceptual_cfg).proofed_rgb;
    assert!(out.x.is_finite() && out.y.is_finite() && out.z.is_finite());
}
