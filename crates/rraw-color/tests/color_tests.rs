use glam::Vec3;
use rraw_color::cct::chromaticity_to_cct;
use rraw_color::colorchecker::{colorchecker_patch_xyz, delta_e_00, COLORCHECKER_24_NAMES};
use rraw_color::drt::{apply_agx_edr, apply_opendrt};
use rraw_color::gamut_guard::apply_gamut_guard_ap1;
use rraw_color::matrix::{
    cam_wb_to_acescg_matrix_forward, cam_wb_to_acescg_matrix_inverse_colormatrix,
};

#[test]
fn test_cct_solver_from_chromaticity() -> Result<(), Box<dyn std::error::Error>> {
    // Standard CIE Illuminant D65: x = 0.3127, y = 0.3290 -> CCT ~ 6504 K
    let cct_d65 = chromaticity_to_cct(0.3127, 0.3290)?;
    assert!(
        (cct_d65 - 6504.0).abs() < 10.0,
        "D65 CCT {cct_d65} diverged from 6504 K"
    );

    // Standard CIE Illuminant A: x = 0.4476, y = 0.4074 -> CCT ~ 2856 K
    let cct_stda = chromaticity_to_cct(0.4476, 0.4074)?;
    assert!(
        (cct_stda - 2856.0).abs() < 10.0,
        "StdA CCT {cct_stda} diverged from 2856 K"
    );

    // Degenerate/non-finite inputs must fail
    assert!(chromaticity_to_cct(0.0, 0.0).is_err());
    assert!(chromaticity_to_cct(f64::NAN, 0.3).is_err());

    Ok(())
}

#[test]
fn test_colorchecker_24_patches_delta_e_under_0_01() -> Result<(), Box<dyn std::error::Error>> {
    // Verify all 24 ColorChecker patches
    assert_eq!(COLORCHECKER_24_NAMES.len(), 24);

    // Calibration matrices for a calibrated camera under D50 and D65
    let fm = glam::Mat3::from_cols(
        Vec3::new(0.68, 0.05, 0.01),
        Vec3::new(0.15, 0.75, 0.08),
        Vec3::new(0.08, 0.12, 0.78),
    );
    let m_fwd = cam_wb_to_acescg_matrix_forward(&fm)?;

    let cm = glam::Mat3::from_cols(
        Vec3::new(0.85, 0.08, 0.02),
        Vec3::new(0.12, 0.82, 0.09),
        Vec3::new(0.04, 0.08, 0.88),
    );
    let wb = [1.8_f32, 1.0_f32, 1.4_f32];
    let (scene_x, scene_y) = (0.3127, 0.3290); // D65
    let m_inv = cam_wb_to_acescg_matrix_inverse_colormatrix(&cm, wb, scene_x, scene_y)?;

    let bradford_d60_to_d50 =
        rraw_color::matrix::bradford_adaptation(0.32168, 0.33767, 0.34567, 0.35850);
    let bradford_d60_to_scene =
        rraw_color::matrix::bradford_adaptation(0.32168, 0.33767, scene_x, scene_y);

    let fm_inv = fm.inverse();
    let diag_wb = glam::Mat3::from_diagonal(Vec3::new(wb[0], wb[1], wb[2]));

    for (idx, &name) in COLORCHECKER_24_NAMES.iter().enumerate() {
        let (xyz_d60, expected_lab) = colorchecker_patch_xyz(idx)?;

        // Path A: Simulate camera white-balanced sensor values for this patch under D50 FM
        let xyz_d50 = bradford_d60_to_d50 * xyz_d60;
        let cam_wb_a = fm_inv * xyz_d50;
        let acescg_a = m_fwd * cam_wb_a;

        // Path B: Simulate camera white-balanced sensor values for this patch under ColorMatrix
        let xyz_scene = bradford_d60_to_scene * xyz_d60;
        let cam_raw_b = cm * xyz_scene;
        let cam_wb_b = diag_wb * cam_raw_b;
        let acescg_b = m_inv * cam_wb_b;

        // Apply Gamut Guard: must be transparent for all 24 patches
        let guarded_a = apply_gamut_guard_ap1(acescg_a);
        let guarded_b = apply_gamut_guard_ap1(acescg_b);

        assert!(
            (guarded_a - acescg_a).length() < 1e-6,
            "patch {name} modified by Gamut Guard on Path A"
        );
        assert!(
            (guarded_b - acescg_b).length() < 1e-6,
            "patch {name} modified by Gamut Guard on Path B"
        );

        let d_e_a = delta_e_00(guarded_a, expected_lab);
        let d_e_b = delta_e_00(guarded_b, expected_lab);

        assert!(
            d_e_a < 0.01,
            "patch {idx} ({name}) Delta E 00 {d_e_a:.5} >= 0.01 on Path A"
        );
        assert!(
            d_e_b < 0.01,
            "patch {idx} ({name}) Delta E 00 {d_e_b:.5} >= 0.01 on Path B"
        );
    }

    Ok(())
}

#[test]
fn test_gamut_guard_inner_core_strict_identity() {
    // 1. Points strictly inside 85% core of AP1 (e.g. sRGB / Display P3 gamut primaries & neutrals)
    let inside_points = [
        Vec3::new(0.18, 0.18, 0.18), // Neutral mid-gray
        Vec3::new(0.5, 0.4, 0.3),    // Typical skin tone
        Vec3::new(0.2, 0.6, 0.2),    // Foliage green
        Vec3::new(0.1, 0.2, 0.7),    // Standard blue sky
        Vec3::new(0.7, 0.15, 0.1),   // Saturated red
    ];

    for p in inside_points {
        let guarded = apply_gamut_guard_ap1(p);
        let diff = (guarded - p).length();
        assert!(
            diff < 1e-7,
            "inner core point {p:?} altered by Gamut Guard: diff = {diff}"
        );
    }

    // 2. Out-of-gamut point (extreme monochromatic laser/LED outside AP1)
    let outside_point = Vec3::new(-0.2, 1.2, -0.1);
    let guarded_outside = apply_gamut_guard_ap1(outside_point);
    assert_ne!(guarded_outside, outside_point);
    // Values must be brought into valid non-negative gamut
    assert!(guarded_outside.x >= 0.0);
    assert!(guarded_outside.y >= 0.0);
    assert!(guarded_outside.z >= 0.0);
}

#[test]
fn test_agx_edr_monotonicity_and_headroom_bounds() {
    let headrooms = [1.0_f32, 2.0_f32, 4.0_f32, 16.0_f32];

    for &h in &headrooms {
        let mut prev_y = -1.0_f32;
        let steps = 200;

        for s in 0..=steps {
            let x = (s as f32 / steps as f32) * 100.0;
            let y = apply_agx_edr(x, h);

            // Bounded by headroom
            assert!(
                y <= h + 1e-5,
                "AgX output {y} exceeds headroom {h} at x={x}"
            );
            assert!(y >= 0.0, "AgX output {y} is negative at x={x}");

            // Strictly monotonically increasing
            if s > 0 {
                assert!(
                    y > prev_y,
                    "AgX violated strict monotonicity at x={x}, y={y}, prev_y={prev_y} for H={h}"
                );
            }
            prev_y = y;
        }

        // Also verify OpenDRT satisfies boundary conditions
        let odrt_0 = apply_opendrt(0.0, h);
        let odrt_100 = apply_opendrt(100.0, h);
        assert!((odrt_0 - 0.0).abs() < 1e-5);
        assert!(odrt_100 <= h + 1e-5);
        assert!(odrt_100 > odrt_0);
    }
}
