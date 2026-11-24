#![forbid(unsafe_code)]

//! Headless WGPU colorimetric visual regression suite on the 24-patch ColorChecker chart.
//!
//! Verifies that end-to-end GPU compute rendering (Stages 1-6 scene-referred ACEScg and
//! Stage 7 AgX display-referred EDR/SDR) reproduces every analytical ColorChecker patch
//! within CIEDE2000 Delta E 00 < 0.5, and that colorimetric regressions are reliably caught.

use std::error::Error;

use glam::Vec3;
use rraw_color::{
    colorchecker_patch_xyz, delta_e_00, delta_e_00_acescg, xyz_d60_to_acescg, COLORCHECKER_24_NAMES,
};
use rraw_core::{BayerOrigin, CfaPattern, NoiseProfile, Stage6MasksParams};
use rraw_gpu::{
    GpuContext, Stage1Params, Stage1Pipeline, Stage2Params, Stage2Pipeline, Stage3Params,
    Stage3RlPipeline, Stage4Params, Stage4WarpPipeline, Stage5Params, Stage5Pipeline, Stage6Params,
    Stage6Pipeline, Stage7Params, Stage7Pipeline,
};
use rraw_optics::{OpticalCorrectionModel, UvLutGenerator};

const PATCH_COLS: u32 = 6;
const PATCH_ROWS: u32 = 4;
const PATCH_SIZE: u32 = 32;
const CHART_WIDTH: u32 = PATCH_COLS * PATCH_SIZE; // 192
const CHART_HEIGHT: u32 = PATCH_ROWS * PATCH_SIZE; // 128

const SENSOR_BLACK_LEVEL: f32 = 512.0;
const SENSOR_WHITE_LEVEL: f32 = 16_383.0;
/// Capture exposure scale (-1 EV) providing headroom so white patch #19 (R ≈ 1.0115 in ACEScg)
/// never clips the 14-bit sensor; compensated by `wb_multipliers = [2.0, 2.0, 2.0]` in Stage 2.
const SENSOR_CAPTURE_SCALE: f32 = 0.5;
const DELTA_E_00_REGRESSION_LIMIT: f32 = 0.5;

/// Ground-truth 24-patch ColorChecker data in ACEScg and CIELAB D60 alongside 14-bit RGGB Bayer ADU.
struct ColorCheckerFixture {
    raw_bayer_u16: Vec<u16>,
    acescg_rgb_chart: Vec<f32>,
    patch_acescg: Vec<Vec3>,
    patch_lab_d60: Vec<[f32; 3]>,
}

fn build_colorchecker_fixture() -> Result<ColorCheckerFixture, Box<dyn Error>> {
    let mut patch_acescg = Vec::with_capacity(24);
    let mut patch_lab_d60 = Vec::with_capacity(24);
    for idx in 0..24 {
        let (xyz, lab) = colorchecker_patch_xyz(idx)?;
        patch_acescg.push(xyz_d60_to_acescg(xyz));
        patch_lab_d60.push(lab);
    }

    let pixel_count = (CHART_WIDTH * CHART_HEIGHT) as usize;
    let mut raw_bayer_u16 = Vec::with_capacity(pixel_count);
    let mut acescg_rgb_chart = Vec::with_capacity(pixel_count * 3);
    let span = SENSOR_WHITE_LEVEL - SENSOR_BLACK_LEVEL;

    for y in 0..CHART_HEIGHT {
        let patch_row = y / PATCH_SIZE;
        for x in 0..CHART_WIDTH {
            let patch_col = x / PATCH_SIZE;
            let patch_idx = (patch_row * PATCH_COLS + patch_col) as usize;
            let color = patch_acescg[patch_idx];

            acescg_rgb_chart.push(color.x);
            acescg_rgb_chart.push(color.y);
            acescg_rgb_chart.push(color.z);

            let channel = match (x % 2, y % 2) {
                (0, 0) => color.x,
                (1, 0) | (0, 1) => color.y,
                _ => color.z,
            };
            let adu = (SENSOR_BLACK_LEVEL + (channel * SENSOR_CAPTURE_SCALE) * span)
                .round()
                .clamp(0.0, SENSOR_WHITE_LEVEL) as u16;
            raw_bayer_u16.push(adu);
        }
    }

    Ok(ColorCheckerFixture {
        raw_bayer_u16,
        acescg_rgb_chart,
        patch_acescg,
        patch_lab_d60,
    })
}

/// Averages the interior 8x8 core of patch `patch_idx` (offsets 12..20) to exclude patch-border filter bleed.
fn sample_patch_interior_mean(rgb: &[f32], width: u32, patch_idx: usize) -> Vec3 {
    let patch_col = (patch_idx as u32) % PATCH_COLS;
    let patch_row = (patch_idx as u32) / PATCH_COLS;
    let x0 = patch_col * PATCH_SIZE + 12;
    let y0 = patch_row * PATCH_SIZE + 12;

    let mut sum = Vec3::ZERO;
    let mut samples = 0.0_f32;
    for dy in 0..8 {
        for dx in 0..8 {
            let idx = (((y0 + dy) * width + (x0 + dx)) * 3) as usize;
            sum += Vec3::new(rgb[idx], rgb[idx + 1], rgb[idx + 2]);
            samples += 1.0;
        }
    }
    sum / samples
}

/// Analytical CPU reference matching `stage7_display_scopes.wgsl::agx_drt` (`rraw_color::apply_agx_edr`).
fn agx_drt_cpu(x: f32, headroom: f32) -> f32 {
    rraw_color::apply_agx_edr(x, headroom)
}

#[test]
fn test_headless_wgpu_colorchecker_24_patches_full_pipeline_delta_e_under_0_5(
) -> Result<(), Box<dyn Error>> {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "NOTICE: Headless GPU adapter unavailable ({e}); skipping visual regression test."
            );
            return Ok(());
        }
    };

    let fixture = build_colorchecker_fixture()?;
    let stage1 = Stage1Pipeline::new(&ctx)?;
    let stage2 = Stage2Pipeline::new(&ctx)?;
    let stage3 = Stage3RlPipeline::new(&ctx)?;
    let stage4 = Stage4WarpPipeline::new(&ctx)?;
    let stage5 = Stage5Pipeline::new(&ctx)?;
    let stage6 = Stage6Pipeline::new(&ctx)?;

    let s1_params = Stage1Params {
        width: CHART_WIDTH,
        height: CHART_HEIGHT,
        black_levels: [SENSOR_BLACK_LEVEL; 4],
        white_levels: [SENSOR_WHITE_LEVEL; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile: NoiseProfile::default(),
        wavelet_passes: 0,
    };
    let (norm_cfa, clip_mask) = stage1.execute(&ctx, &fixture.raw_bayer_u16, &s1_params)?;

    let s2_params = Stage2Params {
        width: CHART_WIDTH,
        height: CHART_HEIGHT,
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        wb_multipliers: [
            1.0 / SENSOR_CAPTURE_SCALE,
            1.0 / SENSOR_CAPTURE_SCALE,
            1.0 / SENSOR_CAPTURE_SCALE,
        ],
    };
    let s2_rgb = stage2.execute(&ctx, &norm_cfa, &clip_mask, &s2_params)?;

    let s3_params = Stage3Params {
        width: CHART_WIDTH,
        height: CHART_HEIGHT,
        iterations: 0,
        base_psf_sigma: 1.0,
        epsilon: 1e-4,
        deringing_threshold: 0.1,
        shot_r: 0.0,
        shot_g: 0.0,
        shot_b: 0.0,
        read_r: 0.0,
        read_g: 0.0,
        read_b: 0.0,
    };
    let s3_rgb = stage3.execute(&ctx, &s2_rgb, &s3_params)?;

    let identity_lut = UvLutGenerator::generate(&OpticalCorrectionModel::default(), 16);
    let s4_params = Stage4Params {
        width: CHART_WIDTH,
        height: CHART_HEIGHT,
        apply_radial_vignette: false,
        vignette_v1: 0.0,
        vignette_v2: 0.0,
    };
    let s4_rgb = stage4.execute(&ctx, &s3_rgb, &identity_lut, &s4_params)?;

    let s5_params = Stage5Params::new(CHART_WIDTH, CHART_HEIGHT, 0, 3, 0.0);
    let s5_rgb = stage5.execute(&ctx, &s4_rgb, &s5_params)?;

    let s6_params = Stage6Params {
        width: CHART_WIDTH,
        height: CHART_HEIGHT,
        masks: Stage6MasksParams::default(),
    };
    let s6_rgb = stage6.execute(&ctx, &s5_rgb, &s6_params)?;

    assert_eq!(s6_rgb.len(), (CHART_WIDTH * CHART_HEIGHT * 3) as usize);

    let mut max_de = 0.0_f32;
    let mut sum_de = 0.0_f32;
    for (patch_idx, &expected_lab) in fixture.patch_lab_d60.iter().enumerate() {
        let measured = sample_patch_interior_mean(&s6_rgb, CHART_WIDTH, patch_idx);
        let de = delta_e_00(measured, expected_lab);
        assert!(
            de.is_finite() && de < DELTA_E_00_REGRESSION_LIMIT,
            "Headless WGPU Stages 1-6 ColorChecker patch #{} ({}) exceeded Delta E 00 limit: {:.4} >= {:.2} (measured={:?})",
            patch_idx + 1,
            COLORCHECKER_24_NAMES[patch_idx],
            de,
            DELTA_E_00_REGRESSION_LIMIT,
            measured
        );
        max_de = max_de.max(de);
        sum_de += de;
    }

    let mean_de = sum_de / 24.0;
    assert!(
        max_de < DELTA_E_00_REGRESSION_LIMIT && mean_de < DELTA_E_00_REGRESSION_LIMIT,
        "Aggregate ColorChecker Delta E 00 regression: max={max_de:.4}, mean={mean_de:.4}"
    );
    Ok(())
}

#[test]
fn test_headless_wgpu_colorchecker_stage7_agx_drt_matches_cpu_reference_delta_e_under_0_5(
) -> Result<(), Box<dyn Error>> {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("NOTICE: Headless GPU adapter unavailable ({e}); skipping Stage 7 visual regression test.");
            return Ok(());
        }
    };

    let fixture = build_colorchecker_fixture()?;
    let stage7 = Stage7Pipeline::new(&ctx)?;

    for headroom in [1.0_f32, 4.0_f32] {
        let params = Stage7Params {
            width: CHART_WIDTH,
            height: CHART_HEIGHT,
            headroom,
        };
        let (display_rgb, histogram, has_clipping) =
            stage7.execute(&ctx, &fixture.acescg_rgb_chart, &params)?;

        assert_eq!(display_rgb.len(), (CHART_WIDTH * CHART_HEIGHT * 3) as usize);
        let histogram_total: u32 = histogram.iter().copied().sum();
        assert_eq!(
            histogram_total,
            CHART_WIDTH * CHART_HEIGHT,
            "GPU histogram pixel count must match chart dimensions for headroom={headroom}"
        );
        let expected_clipping = fixture
            .patch_acescg
            .iter()
            .any(|c| c.x > headroom || c.y > headroom || c.z > headroom);
        assert_eq!(
            has_clipping, expected_clipping,
            "Clipping alert mismatch at headroom={headroom} (expected={expected_clipping})"
        );

        for (patch_idx, &ref_acescg) in fixture.patch_acescg.iter().enumerate() {
            let gpu_mean = sample_patch_interior_mean(&display_rgb, CHART_WIDTH, patch_idx);
            let cpu_ref = Vec3::new(
                agx_drt_cpu(ref_acescg.x, headroom),
                agx_drt_cpu(ref_acescg.y, headroom),
                agx_drt_cpu(ref_acescg.z, headroom),
            );
            let de = delta_e_00_acescg(gpu_mean, cpu_ref);
            assert!(
                de.is_finite() && de < DELTA_E_00_REGRESSION_LIMIT,
                "Stage 7 AgX DRT (headroom={headroom}) patch #{} ({}) diverged from CPU reference: Delta E 00 = {:.4} >= {:.2}",
                patch_idx + 1,
                COLORCHECKER_24_NAMES[patch_idx],
                de,
                DELTA_E_00_REGRESSION_LIMIT
            );
        }
    }

    Ok(())
}

#[test]
fn test_headless_wgpu_colorchecker_detects_colorimetric_drift() -> Result<(), Box<dyn Error>> {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "NOTICE: Headless GPU adapter unavailable ({e}); skipping drift sensitivity test."
            );
            return Ok(());
        }
    };

    let fixture = build_colorchecker_fixture()?;
    let stage1 = Stage1Pipeline::new(&ctx)?;
    let stage2 = Stage2Pipeline::new(&ctx)?;

    let s1_params = Stage1Params {
        width: CHART_WIDTH,
        height: CHART_HEIGHT,
        black_levels: [SENSOR_BLACK_LEVEL; 4],
        white_levels: [SENSOR_WHITE_LEVEL; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile: NoiseProfile::default(),
        wavelet_passes: 0,
    };
    let (norm_cfa, clip_mask) = stage1.execute(&ctx, &fixture.raw_bayer_u16, &s1_params)?;

    // Deliberate white-balance shift simulating an unintended shader/calibration regression
    let base_gain = 1.0 / SENSOR_CAPTURE_SCALE;
    let s2_params = Stage2Params {
        width: CHART_WIDTH,
        height: CHART_HEIGHT,
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        wb_multipliers: [1.35 * base_gain, 1.0 * base_gain, 0.75 * base_gain],
    };
    let drifted_rgb = stage2.execute(&ctx, &norm_cfa, &clip_mask, &s2_params)?;

    let max_de = fixture
        .patch_lab_d60
        .iter()
        .enumerate()
        .map(|(patch_idx, &expected_lab)| {
            let measured = sample_patch_interior_mean(&drifted_rgb, CHART_WIDTH, patch_idx);
            delta_e_00(measured, expected_lab)
        })
        .fold(0.0_f32, f32::max);

    assert!(
        max_de > 5.0,
        "ColorChecker regression oracle must detect intentional WB drift (measured max Delta E 00 = {max_de:.4})"
    );
    Ok(())
}
