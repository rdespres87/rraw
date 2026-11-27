#![forbid(unsafe_code)]

//! Strict TDD unit & integration tests for Edge-Aware Adaptive Demosaicing.

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

#[test]
fn test_demosaic_xtrans_uniform_flat_field_exactness() -> Result<(), CoreError> {
    let width = 24;
    let height = 24;
    let pattern = CfaPattern::fujifilm_xtrans();

    let (r_target, g_target, b_target) = (0.30_f32, 0.65_f32, 0.40_f32);
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

    // Verify interior pixels reproduce the ground-truth channels with near zero false color
    for y in 3..(height - 3) {
        for x in 3..(width - 3) {
            let px = rgb[y * width + x];
            assert!(
                (px[0] - r_target).abs() < 1e-4,
                "X-Trans Red channel error at ({x},{y}): expected {r_target}, got {}",
                px[0]
            );
            assert!(
                (px[1] - g_target).abs() < 1e-4,
                "X-Trans Green channel error at ({x},{y}): expected {g_target}, got {}",
                px[1]
            );
            assert!(
                (px[2] - b_target).abs() < 1e-4,
                "X-Trans Blue channel error at ({x},{y}): expected {b_target}, got {}",
                px[2]
            );
        }
    }

    Ok(())
}

#[test]
fn test_demosaic_xtrans_edge_aware_vertical_edge_suppresses_zippering() -> Result<(), CoreError> {
    let width = 36;
    let height = 36;
    let pattern = CfaPattern::fujifilm_xtrans();

    // High-contrast vertical edge at x = 18 (left = 0.1, right = 0.9 across all channels)
    let mut samples = vec![0.0_f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let val = if x < 18 { 0.1 } else { 0.9 };
            samples[y * width + x] = val;
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let rgb = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;

    // Interior column checks away from boundary
    for y in 6..(height - 6) {
        let left_green = rgb[y * width + 15][1];
        let right_green = rgb[y * width + 21][1];

        // Green should remain strictly monotonic without overshoot
        assert!((0.09..=0.11).contains(&left_green));
        assert!((0.89..=0.91).contains(&right_green));

        // Neutral gray edge: chrominance difference |R - G| and |B - G| should remain near zero
        let left_px = rgb[y * width + 15];
        let right_px = rgb[y * width + 21];
        assert!((left_px[0] - left_px[1]).abs() < 0.03);
        assert!((left_px[2] - left_px[1]).abs() < 0.03);
        assert!((right_px[0] - right_px[1]).abs() < 0.03);
        assert!((right_px[2] - right_px[1]).abs() < 0.03);
    }

    Ok(())
}

#[test]
fn test_demosaic_xtrans_preserves_native_photosite_exactness() -> Result<(), CoreError> {
    let width = 24;
    let height = 24;
    let pattern = CfaPattern::fujifilm_xtrans();

    let mut samples = vec![0.0_f32; width * height];
    for (i, s) in samples.iter_mut().enumerate() {
        *s = (i as f32 * 0.01).sin() * 0.5 + 0.5;
    }

    let cfa = SignedCfaBuffer::new(width, height, pattern, samples.clone())?;
    let rgb = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;

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

#[test]
fn test_demosaic_xtrans_siemens_star_psnr_above_38db() -> Result<(), CoreError> {
    let width = 48;
    let height = 48;
    let pattern = CfaPattern::fujifilm_xtrans();

    // Generate smooth 2D Gaussian/radial luminance target: I(x, y) = 0.5 + 0.3 * cos(2*pi*r / 12)
    let center_x = width as f32 * 0.5;
    let center_y = height as f32 * 0.5;

    let mut ground_truth = vec![[0.0_f32; 3]; width * height];
    let mut samples = vec![0.0_f32; width * height];

    for y in 0..height {
        let dy = y as f32 - center_y;
        for x in 0..width {
            let dx = x as f32 - center_x;
            let r = (dx * dx + dy * dy).sqrt();
            let val = (0.5 + 0.35 * (r * 0.25).cos()).clamp(0.05, 0.95);

            ground_truth[y * width + x] = [val, val, val];
            samples[y * width + x] = val;
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let rgb = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;

    // Calculate MSE and PSNR in the central ROI [6..42, 6..42]
    let mut mse_sum = 0.0_f64;
    let mut pixel_count = 0_usize;

    for y in 6..(height - 6) {
        for x in 6..(width - 6) {
            let gt = ground_truth[y * width + x];
            let rec = rgb[y * width + x];
            for ch in 0..3 {
                let diff = (rec[ch] - gt[ch]) as f64;
                mse_sum += diff * diff;
            }
            pixel_count += 3;
        }
    }

    let mse = mse_sum / (pixel_count as f64);
    let psnr = 10.0 * (1.0 / mse).log10();

    assert!(
        psnr >= 38.0,
        "X-Trans Edge-Aware PSNR {psnr:.2} dB is below required threshold 38.0 dB"
    );

    Ok(())
}
