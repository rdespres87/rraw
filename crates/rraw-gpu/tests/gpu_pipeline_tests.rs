use rraw_core::{BayerOrigin, CfaPattern, NoiseProfile, NoiseProfileSource};
use rraw_gpu::{GpuContext, Stage1Params, Stage1Pipeline, Stage2Params, Stage2Pipeline};

#[test]
fn test_gpu_headless_context_creation() {
    let ctx_res = GpuContext::new_headless();
    assert!(
        ctx_res.is_ok(),
        "Failed to create headless GPU context: {:?}",
        ctx_res.err()
    );
}

#[test]
fn test_gpu_stage1_signed_normalization_and_wavelets() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let width = 64_u32;
    let height = 64_u32;
    let mut raw_pixels = vec![512_u16; (width * height) as usize];
    raw_pixels[0] = 500; // Sub-black pixel
    raw_pixels[10] = 4096; // Saturated pixel
    raw_pixels[100] = 8000; // Defective hot pixel

    let noise_res = NoiseProfile::new(
        [0.0001, 0.0001, 0.0001],
        [0.0004, 0.0004, 0.0004],
        NoiseProfileSource::CameraTable,
    );
    assert!(noise_res.is_ok());
    let noise_profile = match noise_res {
        Ok(np) => np,
        Err(_) => return,
    };

    let params = Stage1Params {
        width,
        height,
        black_levels: [512.0; 4],
        white_levels: [4096.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile,
        wavelet_passes: 2,
    };

    let pipeline_res = Stage1Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let exec_res = pipeline.execute(&ctx, &raw_pixels, &params);
    assert!(exec_res.is_ok());
    let (norm_buffer, clip_mask) = match exec_res {
        Ok(res) => res,
        Err(_) => return,
    };

    assert_eq!(norm_buffer.len(), (width * height) as usize);
    assert_eq!(clip_mask.len(), (width * height) as usize);

    // Negative values MUST be preserved (fundamental invariant: signed buffer)
    assert!(
        norm_buffer[0] < 0.0,
        "Sub-black pixel must remain negative: {}",
        norm_buffer[0]
    );

    // Saturated pixel must trigger clip_mask
    assert!(
        clip_mask[10] > 0.5,
        "Saturated pixel must be flagged in clip_mask"
    );
}

#[test]
fn test_gpu_stage2_demosaic_and_highlight_reconstruction() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let width = 64_u32;
    let height = 64_u32;
    let mut cfa = vec![0.0_f32; (width * height) as usize];
    let mut clip_mask = vec![0.0_f32; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let val = (x as f32 / width as f32) * 1.5;
            cfa[idx] = val;
            if val >= 0.999 {
                clip_mask[idx] = 1.0;
            }
        }
    }

    let wb_multipliers = [2.1_f32, 1.0_f32, 1.6_f32];
    let params = Stage2Params {
        width,
        height,
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        wb_multipliers,
    };

    let pipeline_res = Stage2Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let exec_res = pipeline.execute(&ctx, &cfa, &clip_mask, &params);
    assert!(exec_res.is_ok());
    let rgb = match exec_res {
        Ok(res) => res,
        Err(_) => return,
    };

    assert_eq!(rgb.len(), (width * height * 3) as usize);

    // Check highlights above physical ceiling: should be equalized to neutral white, zero magenta cast
    let center_idx = ((height / 2) * width + (width - 2)) as usize * 3;
    let r = rgb[center_idx];
    let g = rgb[center_idx + 1];
    let b = rgb[center_idx + 2];

    assert!(
        (r - g).abs() < 1e-3 && (b - g).abs() < 1e-3,
        "Reconstructed highlights must maintain neutral ratios without magenta cast: R={r}, G={g}, B={b}"
    );
}

#[test]
fn test_siemens_star_psnr_above_42db() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let width = 128_u32;
    let height = 128_u32;
    let center_x = width as f32 * 0.5;
    let center_y = height as f32 * 0.5;
    let max_radius = 50.0_f32;

    let mut gt = vec![0.0_f32; (width * height) as usize];
    let mut cfa = vec![0.0_f32; (width * height) as usize];
    let clip_mask = vec![0.0_f32; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let r = (dx * dx + dy * dy).sqrt();
            let theta = dy.atan2(dx);

            // Smooth 8-spoke Siemens star pattern with radial taper
            let pattern = if r < max_radius && r > 4.0 {
                0.5 + 0.3 * (8.0 * theta).sin() * ((r - 4.0) / (max_radius - 4.0))
            } else {
                0.5
            };

            let idx = (y * width + x) as usize;
            gt[idx] = pattern;
            cfa[idx] = pattern;
        }
    }

    let params = Stage2Params {
        width,
        height,
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        wb_multipliers: [1.0, 1.0, 1.0],
    };

    let pipeline_res = Stage2Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let exec_res = pipeline.execute(&ctx, &cfa, &clip_mask, &params);
    assert!(exec_res.is_ok());
    let rgb = match exec_res {
        Ok(res) => res,
        Err(_) => return,
    };

    // Evaluate MSE on the resolvable band of the Siemens star (r >= 10.0 px, within Nyquist)
    let mut sse = 0.0_f64;
    let mut count = 0_usize;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let r = (dx * dx + dy * dy).sqrt();
            if r >= 10.0 && r < (max_radius - 4.0) {
                let idx = (y * width + x) as usize;
                let out_r = rgb[idx * 3] as f64;
                let out_g = rgb[idx * 3 + 1] as f64;
                let out_b = rgb[idx * 3 + 2] as f64;
                let expected = gt[idx] as f64;

                let diff_r = out_r - expected;
                let diff_g = out_g - expected;
                let diff_b = out_b - expected;

                sse += diff_r * diff_r + diff_g * diff_g + diff_b * diff_b;
                count += 3;
            }
        }
    }

    let mse = sse / count as f64;
    let psnr = 10.0 * (1.0 / mse).log10();

    assert!(
        psnr > 42.0,
        "Siemens star demosaic PSNR must exceed 42 dB (measured: {psnr:.2} dB)"
    );
}

#[test]
fn test_clipped_solar_disk_radial_highlight_recovery() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let width = 64_u32;
    let height = 64_u32;
    let center_x = width as f32 * 0.5;
    let center_y = height as f32 * 0.5;

    let mut cfa = vec![0.0_f32; (width * height) as usize];
    let mut clip_mask = vec![0.0_f32; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let r2 = dx * dx + dy * dy;
            // Solar disk with high peak intensity (2.0)
            let intensity = 2.0 * (-r2 / (2.0 * 6.0 * 6.0)).exp();
            let idx = (y * width + x) as usize;
            cfa[idx] = intensity;
            if intensity >= 0.999 {
                clip_mask[idx] = 1.0;
            }
        }
    }

    let wb_multipliers = [2.0_f32, 1.0_f32, 1.5_f32];
    let params = Stage2Params {
        width,
        height,
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        wb_multipliers,
    };

    let pipeline_res = Stage2Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let exec_res = pipeline.execute(&ctx, &cfa, &clip_mask, &params);
    assert!(exec_res.is_ok());
    let rgb = match exec_res {
        Ok(res) => res,
        Err(_) => return,
    };

    // Verify center is reconstructed to neutral without magenta ringing
    let center_idx = ((height / 2) * width + (width / 2)) as usize * 3;
    let r_core = rgb[center_idx];
    let g_core = rgb[center_idx + 1];
    let b_core = rgb[center_idx + 2];

    assert!(
        r_core > 1.0,
        "Solar core intensity must extend above 1.0 into headroom (measured: {r_core})"
    );
    assert!(
        (r_core - g_core).abs() < 1e-3 && (b_core - g_core).abs() < 1e-3,
        "Solar core must remain neutral without magenta ringing: R={r_core}, G={g_core}, B={b_core}"
    );
}
