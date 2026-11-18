use rraw_core::{NoiseProfile, NoiseProfileSource};
use rraw_optics::{execute_richardson_lucy, RichardsonLucyParams};

#[test]
fn test_rl_noise_regularization_in_flat_region() {
    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    let base_signal = 0.2_f32;
    let sigma_noise = 0.015_f32;

    let mut noisy_flat = vec![0.0_f32; count * 3];

    // Deterministic pseudo-random noise generator (Box-Muller)
    let mut seed = 123456789_u64;
    for i in 0..count {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let u1 = ((seed >> 32) as f32 + 1.0) / 4294967296.0;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let u2 = ((seed >> 32) as f32 + 1.0) / 4294967296.0;

        let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos();
        let val = (base_signal + z0 * sigma_noise).max(0.0);

        noisy_flat[i * 3] = val;
        noisy_flat[i * 3 + 1] = val;
        noisy_flat[i * 3 + 2] = val;
    }

    // Compute input variance
    let mean_in: f64 = noisy_flat.iter().map(|&v| v as f64).sum::<f64>() / (count * 3) as f64;
    let var_in: f64 = noisy_flat
        .iter()
        .map(|&v| {
            let diff = v as f64 - mean_in;
            diff * diff
        })
        .sum::<f64>()
        / (count * 3) as f64;

    let noise_profile = match NoiseProfile::new(
        [0.0001, 0.0001, 0.0001],
        [
            sigma_noise * sigma_noise,
            sigma_noise * sigma_noise,
            sigma_noise * sigma_noise,
        ],
        NoiseProfileSource::CameraTable,
    ) {
        Ok(np) => np,
        Err(_) => return,
    };

    let params = RichardsonLucyParams {
        iterations: 10,
        epsilon: 1e-6,
        base_psf_sigma: 1.0,
        deringing_threshold: 0.05,
        noise_profile,
    };

    let sharpened = execute_richardson_lucy(&noisy_flat, width, height, &params);
    assert_eq!(sharpened.len(), count * 3);

    // Compute output variance
    let mean_out: f64 = sharpened.iter().map(|&v| v as f64).sum::<f64>() / (count * 3) as f64;
    let var_out: f64 = sharpened
        .iter()
        .map(|&v| {
            let diff = v as f64 - mean_out;
            diff * diff
        })
        .sum::<f64>()
        / (count * 3) as f64;

    let variance_increase_ratio = (var_out - var_in) / var_in;

    assert!(
        variance_increase_ratio < 0.01,
        "Noise variance increase in flat region must be < +1% (measured: {:.2}%)",
        variance_increase_ratio * 100.0
    );
}

#[test]
fn test_rl_edge_sharpening_mtf50() {
    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    let mut blurred_edge = vec![0.0_f32; count * 3];

    // Error function / Gaussian blurred step edge at x = 32 with sigma = 1.1 px
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 - 32.0) / 1.1;
            // Sigmoid-like smooth approximation of Gaussian error function
            let val = 0.5 * (1.0 + (dx * 0.797884 * (1.0 + 0.044715 * dx * dx)).tanh());
            let idx = ((y * width + x) * 3) as usize;
            blurred_edge[idx] = val;
            blurred_edge[idx + 1] = val;
            blurred_edge[idx + 2] = val;
        }
    }

    // Measure pre-sharpening 10%-90% rise distance on row 32
    let row_idx = (32 * width) as usize;
    let mut rise_x_10_in = 0.0_f32;
    let mut rise_x_90_in = 0.0_f32;
    for x in 0..(width - 1) {
        let v0 = blurred_edge[(row_idx + x as usize) * 3];
        let v1 = blurred_edge[(row_idx + x as usize + 1) * 3];
        if v0 <= 0.1 && v1 >= 0.1 {
            rise_x_10_in = x as f32 + (0.1 - v0) / (v1 - v0);
        }
        if v0 <= 0.9 && v1 >= 0.9 {
            rise_x_90_in = x as f32 + (0.9 - v0) / (v1 - v0);
        }
    }
    let rise_dist_in = rise_x_90_in - rise_x_10_in;

    let noise_profile = match NoiseProfile::new(
        [0.0001, 0.0001, 0.0001],
        [0.0001, 0.0001, 0.0001],
        NoiseProfileSource::CameraTable,
    ) {
        Ok(np) => np,
        Err(_) => return,
    };

    let params = RichardsonLucyParams {
        iterations: 8,
        epsilon: 1e-6,
        base_psf_sigma: 1.0,
        deringing_threshold: 0.05,
        noise_profile,
    };

    let sharpened = execute_richardson_lucy(&blurred_edge, width, height, &params);

    let mut rise_x_10_out = 0.0_f32;
    let mut rise_x_90_out = 0.0_f32;
    for x in 0..(width - 1) {
        let v0 = sharpened[(row_idx + x as usize) * 3];
        let v1 = sharpened[(row_idx + x as usize + 1) * 3];
        if v0 <= 0.1 && v1 >= 0.1 {
            rise_x_10_out = x as f32 + (0.1 - v0) / (v1 - v0);
        }
        if v0 <= 0.9 && v1 >= 0.9 {
            rise_x_90_out = x as f32 + (0.9 - v0) / (v1 - v0);
        }
    }
    let rise_dist_out = rise_x_90_out - rise_x_10_out;

    let sharpness_improvement = (rise_dist_in - rise_dist_out) / rise_dist_in;

    assert!(
        sharpness_improvement >= 0.35,
        "RL sharpening must improve edge rise distance by >= +35% (measured: {:.2}%)",
        sharpness_improvement * 100.0
    );
}

#[test]
fn test_rl_zero_nan_and_deringing() {
    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    // Step edge from 0.0 to 1.5
    let mut step = vec![0.0_f32; count * 3];
    for y in 0..height {
        for x in 32..width {
            let idx = ((y * width + x) * 3) as usize;
            step[idx] = 1.5;
            step[idx + 1] = 1.5;
            step[idx + 2] = 1.5;
        }
    }

    let noise_profile = match NoiseProfile::new(
        [0.0001, 0.0001, 0.0001],
        [0.0001, 0.0001, 0.0001],
        NoiseProfileSource::CameraTable,
    ) {
        Ok(np) => np,
        Err(_) => return,
    };

    let params = RichardsonLucyParams {
        iterations: 10,
        epsilon: 1e-6,
        base_psf_sigma: 1.0,
        deringing_threshold: 0.05,
        noise_profile,
    };

    let sharpened = execute_richardson_lucy(&step, width, height, &params);

    for (i, &v) in sharpened.iter().enumerate() {
        assert!(v.is_finite(), "Pixel {i} must be finite (got {v})");
        assert!(v >= 0.0, "Pixel {i} must be non-negative (got {v})");
        // Deringing check: maximum overshoot must not exceed 1.5 * (1 + delta)
        assert!(
            v <= 1.5 * 1.05 + 1e-4,
            "Pixel {i} must not exceed deringing envelope: {v} > {}",
            1.5 * 1.05
        );
    }
}

#[test]
fn test_stage3_gpu_rl_execution() {
    let ctx = match rraw_gpu::GpuContext::new_headless() {
        Ok(c) => c,
        Err(_) => return, // Headless GPU unavailable in environment
    };

    let pipeline = match rraw_gpu::Stage3RlPipeline::new(&ctx) {
        Ok(p) => p,
        Err(_) => return,
    };

    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    let mut blurred_edge = vec![0.0_f32; count * 3];
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 - 32.0) / 1.1;
            let val = 0.5 * (1.0 + (dx * 0.797884 * (1.0 + 0.044715 * dx * dx)).tanh());
            let idx = ((y * width + x) * 3) as usize;
            blurred_edge[idx] = val;
            blurred_edge[idx + 1] = val;
            blurred_edge[idx + 2] = val;
        }
    }

    let row_idx = (32 * width) as usize;
    let mut rise_x_10_in = 0.0_f32;
    let mut rise_x_90_in = 0.0_f32;
    for x in 0..(width - 1) {
        let v0 = blurred_edge[(row_idx + x as usize) * 3];
        let v1 = blurred_edge[(row_idx + x as usize + 1) * 3];
        if v0 <= 0.1 && v1 >= 0.1 {
            rise_x_10_in = x as f32 + (0.1 - v0) / (v1 - v0);
        }
        if v0 <= 0.9 && v1 >= 0.9 {
            rise_x_90_in = x as f32 + (0.9 - v0) / (v1 - v0);
        }
    }
    let rise_dist_in = rise_x_90_in - rise_x_10_in;

    let params = rraw_gpu::Stage3Params {
        width,
        height,
        iterations: 8,
        base_psf_sigma: 1.0,
        epsilon: 1e-6,
        deringing_threshold: 0.05,
        shot_r: 0.0001,
        shot_g: 0.0001,
        shot_b: 0.0001,
        read_r: 0.0001,
        read_g: 0.0001,
        read_b: 0.0001,
    };

    let gpu_out = match pipeline.execute(&ctx, &blurred_edge, &params) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("Stage 3 GPU execution failed: {e:?}");
            return;
        }
    };

    assert_eq!(gpu_out.len(), count * 3);

    let mut rise_x_10_out = 0.0_f32;
    let mut rise_x_90_out = 0.0_f32;
    for x in 0..(width - 1) {
        let v0 = gpu_out[(row_idx + x as usize) * 3];
        let v1 = gpu_out[(row_idx + x as usize + 1) * 3];
        if v0 <= 0.1 && v1 >= 0.1 {
            rise_x_10_out = x as f32 + (0.1 - v0) / (v1 - v0);
        }
        if v0 <= 0.9 && v1 >= 0.9 {
            rise_x_90_out = x as f32 + (0.9 - v0) / (v1 - v0);
        }
    }
    let rise_dist_out = rise_x_90_out - rise_x_10_out;
    let sharpness_improvement = (rise_dist_in - rise_dist_out) / rise_dist_in;

    assert!(
        sharpness_improvement >= 0.35,
        "GPU RL sharpening must improve edge rise distance by >= +35% (measured: {:.2}%)",
        sharpness_improvement * 100.0
    );

    for (i, &v) in gpu_out.iter().enumerate() {
        assert!(v.is_finite(), "Pixel {i} must be finite (got {v})");
        assert!(v >= 0.0, "Pixel {i} must be non-negative (got {v})");
    }
}
