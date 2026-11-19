#![forbid(unsafe_code)]

//! Strict TDD unit & integration tests for Phase 7.1 Edge-Aware Adaptive Demosaicing.

use rraw_core::demosaic::{demosaic_cfa, DemosaicMethod};
use rraw_core::{BayerOrigin, CfaColor, CfaPattern, CoreError, SignedCfaBuffer};

#[test]
fn test_demosaic_rejects_empty_or_subminimal_dimensions() {
    assert!(demosaic_cfa(
        &SignedCfaBuffer::default(),
        DemosaicMethod::EdgeAwareAdaptive
    )
    .is_err());

    let (w, h) = (1, 1);
    let buf = SignedCfaBuffer::new(w, h, CfaPattern::Bayer(BayerOrigin::Rggb), vec![0.5]);
    assert!(buf.is_ok());
    let res = demosaic_cfa(&buf.unwrap_or_default(), DemosaicMethod::EdgeAwareAdaptive);
    assert!(matches!(res, Err(CoreError::InvalidDimensions { .. })));
}

#[test]
fn test_demosaic_uniform_flat_field_exactness() -> Result<(), CoreError> {
    let width = 16;
    let height = 16;
    let origin = BayerOrigin::Rggb;
    let pattern = CfaPattern::Bayer(origin);

    let (r_target, g_target, b_target) = (0.35_f32, 0.60_f32, 0.45_f32);
    let mut samples = vec![0.0_f32; width * height];

    for y in 0..height {
        for x in 0..width {
            let val = match pattern.color_at(x, y) {
                CfaColor::Red => r_target,
                CfaColor::Green => g_target,
                CfaColor::Blue => b_target,
            };
            samples[y * width + x] = val;
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let rgb = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;

    assert_eq!(rgb.len(), width * height);

    // Verify interior pixels reproduce the ground-truth channels with zero false color
    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let px = rgb[y * width + x];
            assert!(
                (px[0] - r_target).abs() < 1e-4,
                "Red channel error at ({x},{y}): expected {r_target}, got {}",
                px[0]
            );
            assert!(
                (px[1] - g_target).abs() < 1e-4,
                "Green channel error at ({x},{y}): expected {g_target}, got {}",
                px[1]
            );
            assert!(
                (px[2] - b_target).abs() < 1e-4,
                "Blue channel error at ({x},{y}): expected {b_target}, got {}",
                px[2]
            );
        }
    }

    Ok(())
}

#[test]
fn test_demosaic_edge_aware_vertical_edge_suppresses_zippering() -> Result<(), CoreError> {
    let width = 32;
    let height = 32;
    let origin = BayerOrigin::Rggb;
    let pattern = CfaPattern::Bayer(origin);

    // High-contrast vertical edge at x = 16 (left = 0.1, right = 0.9 across all channels)
    let mut samples = vec![0.0_f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let val = if x < 16 { 0.1 } else { 0.9 };
            samples[y * width + x] = val;
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let rgb = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;

    // On an edge-aware demosaic along a vertical line, the green channel should not oscillate
    // or produce overshoot/zipper artifacts on vertical columns
    for y in 4..(height - 4) {
        let left_green = rgb[y * width + 14][1];
        let right_green = rgb[y * width + 17][1];

        // Green should remain strictly monotonic across the edge without overshoot
        assert!((0.09..=0.11).contains(&left_green));
        assert!((0.89..=0.91).contains(&right_green));

        // Chrominance difference |R - G| and |B - G| should remain near zero (neutral gray edge)
        let left_px = rgb[y * width + 14];
        let right_px = rgb[y * width + 17];
        assert!((left_px[0] - left_px[1]).abs() < 0.02);
        assert!((left_px[2] - left_px[1]).abs() < 0.02);
        assert!((right_px[0] - right_px[1]).abs() < 0.02);
        assert!((right_px[2] - right_px[1]).abs() < 0.02);
    }

    Ok(())
}

#[test]
fn test_demosaic_edge_aware_horizontal_edge_suppresses_zippering() -> Result<(), CoreError> {
    let width = 32;
    let height = 32;
    let origin = BayerOrigin::Rggb;
    let pattern = CfaPattern::Bayer(origin);

    // High-contrast horizontal edge at y = 16 (top = 0.15, bottom = 0.85)
    let mut samples = vec![0.0_f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let val = if y < 16 { 0.15 } else { 0.85 };
            samples[y * width + x] = val;
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let rgb = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;

    for x in 4..(width - 4) {
        let top_green = rgb[14 * width + x][1];
        let bot_green = rgb[17 * width + x][1];

        assert!((0.14..=0.16).contains(&top_green));
        assert!((0.84..=0.86).contains(&bot_green));

        let top_px = rgb[14 * width + x];
        let bot_px = rgb[17 * width + x];
        assert!((top_px[0] - top_px[1]).abs() < 0.02);
        assert!((top_px[2] - top_px[1]).abs() < 0.02);
        assert!((bot_px[0] - bot_px[1]).abs() < 0.02);
        assert!((bot_px[2] - bot_px[1]).abs() < 0.02);
    }

    Ok(())
}

#[test]
fn test_demosaic_preserves_center_photosite_exactness() -> Result<(), CoreError> {
    let width = 12;
    let height = 12;
    let origin = BayerOrigin::Rggb;
    let pattern = CfaPattern::Bayer(origin);

    let mut samples = vec![0.0_f32; width * height];
    for (i, s) in samples.iter_mut().enumerate() {
        *s = (i as f32 * 0.01).sin() * 0.5 + 0.5;
    }

    let cfa = SignedCfaBuffer::new(width, height, pattern, samples.clone())?;
    let rgb = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;

    // Center photosite value MUST be exactly preserved in its native channel
    for y in 0..height {
        for x in 0..width {
            let original_val = samples[y * width + x];
            let px = rgb[y * width + x];
            match pattern.color_at(x, y) {
                CfaColor::Red => assert_eq!(px[0], original_val),
                CfaColor::Green => assert_eq!(px[1], original_val),
                CfaColor::Blue => assert_eq!(px[2], original_val),
            }
        }
    }

    Ok(())
}
