use rraw_gpu::{
    expand_roi_with_halo, GpuContext, LruVramCache, RoiWindow, Stage5Params, Stage5Pipeline,
    Stage7Params, Stage7Pipeline,
};

#[test]
fn test_lru_vram_cache_budget_and_eviction() {
    // 10 MiB budget
    let budget_bytes = 10 * 1024 * 1024_u64;
    let mut cache = LruVramCache::new(budget_bytes);

    let photo_size_bytes = 3 * 1024 * 1024_u64; // 3 MiB per photo

    for i in 1..=5 {
        let photo_id = format!("photo_{i}");
        cache.insert_mock_photo(&photo_id, photo_size_bytes);
    }

    assert!(
        cache.current_vram_usage() <= budget_bytes,
        "VRAM cache usage must not exceed budget: {} > {}",
        cache.current_vram_usage(),
        budget_bytes
    );

    // Photos 1 and 2 should have been evicted to make room for photos 3, 4, 5 (3 * 3 = 9 MiB <= 10 MiB)
    assert!(!cache.contains("photo_1"));
    assert!(!cache.contains("photo_2"));
    assert!(cache.contains("photo_3"));
    assert!(cache.contains("photo_4"));
    assert!(cache.contains("photo_5"));
}

#[test]
fn test_roi_scrubbing_with_halo() {
    let image_width = 200_u32;
    let image_height = 200_u32;

    let roi = RoiWindow {
        x: 50,
        y: 50,
        width: 100,
        height: 100,
    };

    let halo_px = 32_u32;
    let expanded = expand_roi_with_halo(&roi, halo_px, image_width, image_height);

    assert_eq!(expanded.x, 18);
    assert_eq!(expanded.y, 18);
    assert_eq!(expanded.width, 164);
    assert_eq!(expanded.height, 164);

    // Corner test: clamped to (0, 0)
    let corner_roi = RoiWindow {
        x: 10,
        y: 10,
        width: 50,
        height: 50,
    };
    let corner_expanded = expand_roi_with_halo(&corner_roi, halo_px, image_width, image_height);
    assert_eq!(corner_expanded.x, 0);
    assert_eq!(corner_expanded.y, 0);
    assert_eq!(corner_expanded.width, 92);
    assert_eq!(corner_expanded.height, 92);
}

#[test]
fn test_scale_invariance_rmse_between_levels() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let pipeline_res = Stage5Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;
    let mut rgb_in = vec![0.5_f32; count * 3];

    // Introduce high-contrast gradient
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 3) as usize;
            let val = (x as f32 / width as f32) * 0.8 + 0.1;
            rgb_in[idx] = val;
            rgb_in[idx + 1] = val;
            rgb_in[idx + 2] = val;
        }
    }

    let params_l0 = Stage5Params {
        width,
        height,
        mipmap_level: 0,
        base_octaves: 6,
        local_contrast_strength: 0.2,
        tone_curves_lut: None,
        clut_3d: None,
        clut_3d_strength: 1.0,
        smart_lighting_shadow_lift: 0.0,
        smart_lighting_highlight_compression: 0.0,
    };

    let params_l2 = Stage5Params {
        width,
        height,
        mipmap_level: 2,
        base_octaves: 6,
        local_contrast_strength: 0.2,
        tone_curves_lut: None,
        clut_3d: None,
        clut_3d_strength: 1.0,
        smart_lighting_shadow_lift: 0.0,
        smart_lighting_highlight_compression: 0.0,
    };

    let res_l0 = pipeline.execute(&ctx, &rgb_in, &params_l0);
    assert!(res_l0.is_ok());
    let out_l0 = match res_l0 {
        Ok(res) => res,
        Err(_) => return,
    };

    let res_l2 = pipeline.execute(&ctx, &rgb_in, &params_l2);
    assert!(res_l2.is_ok());
    let out_l2 = match res_l2 {
        Ok(res) => res,
        Err(_) => return,
    };

    assert_eq!(out_l0.len(), count * 3);
    assert_eq!(out_l2.len(), count * 3);

    // Compute RMSE between L0 and L2
    let mut sse = 0.0_f64;
    for i in 0..(count * 3) {
        let diff = (out_l0[i] - out_l2[i]) as f64;
        sse += diff * diff;
    }
    let rmse = (sse / (count * 3) as f64).sqrt();

    assert!(
        rmse < 0.005,
        "Scale-invariance RMSE between L=0 and L=2 must be < 0.5% (measured: {:.4}%)",
        rmse * 100.0
    );
}

#[test]
fn test_gpu_histogram_and_clipping_reduction() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let pipeline_res = Stage7Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;
    let mut rgb_in = vec![0.5_f32; count * 3];

    // Ramp with some clipped values above 1.0 (into headroom H = 2.0)
    for i in 0..count {
        let val = (i as f32 / count as f32) * 2.5; // goes up to 2.5 (clipped above H=2.0)
        rgb_in[i * 3] = val;
        rgb_in[i * 3 + 1] = val;
        rgb_in[i * 3 + 2] = val;
    }

    let params = Stage7Params {
        width,
        height,
        headroom: 2.0,
    };

    let res7 = pipeline.execute(&ctx, &rgb_in, &params);
    assert!(res7.is_ok());
    let (display_rgb, histogram, has_clipping) = match res7 {
        Ok(res) => res,
        Err(_) => return,
    };

    assert_eq!(display_rgb.len(), count * 3);
    assert_eq!(histogram.len(), 256);
    assert!(
        has_clipping,
        "Clipping alert should be triggered for values above headroom H=2.0"
    );

    let total_hist_samples: u32 = histogram.iter().sum();
    assert_eq!(total_hist_samples, count as u32);
}

#[test]
fn test_stage5_gpu_tone_curves_matches_cpu_reference() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return;
        }
    };

    let pipeline_res = Stage5Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return,
    };

    let width = 32_u32;
    let height = 32_u32;
    let count = (width * height) as usize;
    let mut rgb_in = vec![0.0_f32; count * 3];

    for i in 0..count {
        let t = i as f32 / count as f32;
        rgb_in[i * 3] = t;
        rgb_in[i * 3 + 1] = t * 0.8;
        rgb_in[i * 3 + 2] = (1.0 - t).max(0.0);
    }

    // Create an S-curve contrast recipe
    let mut recipe = rraw_core::curves::ToneCurvesRecipe::default();
    if let Ok(p1) = rraw_core::curves::CurvePoint::new(0.25, 0.15) {
        recipe.combined_rgb.add_point(p1);
    }
    if let Ok(p2) = rraw_core::curves::CurvePoint::new(0.75, 0.85) {
        recipe.combined_rgb.add_point(p2);
    }
    let lut = rraw_color::curves::generate_1d_lut_1024(&recipe);

    // Compute CPU reference
    let mut cpu_out = vec![0.0_f32; count * 3];
    for i in 0..count {
        let in_pix = glam::Vec3::new(rgb_in[i * 3], rgb_in[i * 3 + 1], rgb_in[i * 3 + 2]);
        let out_pix = rraw_color::curves::apply_tone_curves_lut(in_pix, &lut);
        cpu_out[i * 3] = out_pix.x;
        cpu_out[i * 3 + 1] = out_pix.y;
        cpu_out[i * 3 + 2] = out_pix.z;
    }

    // Execute GPU with local_contrast_strength = 0.0 to isolate tone curves
    let params = Stage5Params {
        width,
        height,
        mipmap_level: 0,
        base_octaves: 0,
        local_contrast_strength: 0.0,
        tone_curves_lut: Some(lut),
        clut_3d: None,
        clut_3d_strength: 1.0,
        smart_lighting_shadow_lift: 0.0,
        smart_lighting_highlight_compression: 0.0,
    };

    let gpu_res = pipeline.execute(&ctx, &rgb_in, &params);
    assert!(gpu_res.is_ok());
    let gpu_out = match gpu_res {
        Ok(res) => res,
        Err(_) => return,
    };

    assert_eq!(gpu_out.len(), count * 3);

    // Verify GPU and CPU match within float tolerance (< 1e-4 RMSE)
    let mut sse = 0.0_f64;
    for i in 0..(count * 3) {
        let diff = (gpu_out[i] - cpu_out[i]) as f64;
        sse += diff * diff;
    }
    let rmse = (sse / (count * 3) as f64).sqrt();
    assert!(
        rmse < 1e-4,
        "GPU vs CPU tone curve RMSE must be < 1e-4 (measured: {rmse:e})"
    );
}

#[test]
fn test_stage5_gpu_3d_clut_matches_cpu_reference() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping GPU test (no adapter available): {e}");
            return Ok(());
        }
    };

    let pipeline_res = Stage5Pipeline::new(&ctx);
    assert!(pipeline_res.is_ok());
    let pipeline = match pipeline_res {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };

    let width = 32_u32;
    let height = 32_u32;
    let count = (width * height) as usize;
    let mut rgb_in = vec![0.0_f32; count * 3];

    for i in 0..count {
        let u = (i % width as usize) as f32 / (width as f32 - 1.0);
        let v = (i / width as usize) as f32 / (height as f32 - 1.0);
        rgb_in[i * 3] = u;
        rgb_in[i * 3 + 1] = v;
        rgb_in[i * 3 + 2] = (1.0 - u * 0.5 - v * 0.5).clamp(0.0, 1.0);
    }

    // Generate a 17x17x17 Classic Chrome 3D CLUT
    let clut = rraw_color::ColorLut3d::generate_film_profile(
        rraw_color::FilmSimulationProfile::ClassicChrome,
        17,
    )?;

    // Compute CPU reference using trilinear interpolation
    let mut cpu_out = vec![0.0_f32; count * 3];
    for i in 0..count {
        let in_pix = glam::Vec3::new(rgb_in[i * 3], rgb_in[i * 3 + 1], rgb_in[i * 3 + 2]);
        let out_pix = clut.sample_trilinear(in_pix);
        cpu_out[i * 3] = out_pix.x;
        cpu_out[i * 3 + 1] = out_pix.y;
        cpu_out[i * 3 + 2] = out_pix.z;
    }

    // Execute GPU with local_contrast_strength = 0.0 to isolate 3D CLUT
    let params = Stage5Params::new(width, height, 0, 0, 0.0).with_clut_3d(clut.clone(), 1.0);

    let gpu_res = pipeline.execute(&ctx, &rgb_in, &params);
    assert!(gpu_res.is_ok());
    let gpu_out = match gpu_res {
        Ok(res) => res,
        Err(_) => return Ok(()),
    };

    assert_eq!(gpu_out.len(), count * 3);

    // Verify GPU and CPU match within float tolerance (< 2e-3 RMSE due to half-float precision)
    let mut sse = 0.0_f64;
    for i in 0..(count * 3) {
        let diff = (gpu_out[i] - cpu_out[i]) as f64;
        sse += diff * diff;
    }
    let rmse = (sse / (count * 3) as f64).sqrt();
    assert!(
        rmse < 2e-3,
        "GPU vs CPU 3D CLUT RMSE must be < 2e-3 (measured: {rmse:e})"
    );

    // Test with strength = 0.5 (blended 50%)
    let params_half = Stage5Params::new(width, height, 0, 0, 0.0).with_clut_3d(clut, 0.5);
    let gpu_half = pipeline.execute(&ctx, &rgb_in, &params_half)?;
    for i in 0..(count * 3) {
        let expected = rgb_in[i] * 0.5 + cpu_out[i] * 0.5;
        let diff = (gpu_half[i] - expected).abs();
        assert!(
            diff < 3e-3,
            "Index {i}: diff {diff} exceeds tolerance at 50% blend"
        );
    }

    Ok(())
}

#[test]
fn test_stage5_gpu_smart_lighting_scale_invariance_and_chromaticity(
) -> Result<(), rraw_gpu::GpuError> {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let pipeline = match Stage5Pipeline::new(&ctx) {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };

    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    let mut rgb_in = vec![0.0_f32; count * 3];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 3) as usize;
            // Ramp with distinct non-neutral R, G, B channels
            let t = (x as f32 / width as f32) * 0.9 + 0.05;
            rgb_in[idx] = t * 1.2;
            rgb_in[idx + 1] = t * 0.9;
            rgb_in[idx + 2] = t * 0.6;
        }
    }

    // Test scale-invariance L=0 vs L=2 with smart lighting active
    let params_l0 = Stage5Params::new(width, height, 0, 6, 0.2).with_smart_lighting(0.5, 0.4);
    let params_l2 = Stage5Params::new(width, height, 2, 6, 0.2).with_smart_lighting(0.5, 0.4);

    let out_l0 = pipeline.execute(&ctx, &rgb_in, &params_l0)?;
    let out_l2 = pipeline.execute(&ctx, &rgb_in, &params_l2)?;

    // Chromaticity check on out_l0: R/G and B/G ratios preserved within 2e-3
    for i in 0..count {
        let r_in = rgb_in[i * 3];
        let g_in = rgb_in[i * 3 + 1];
        let b_in = rgb_in[i * 3 + 2];
        let r_out = out_l0[i * 3];
        let g_out = out_l0[i * 3 + 1];
        let b_out = out_l0[i * 3 + 2];

        if g_in > 1e-4 && g_out > 1e-4 {
            let in_rg = r_in / g_in;
            let out_rg = r_out / g_out;
            assert!(
                (in_rg - out_rg).abs() < 2e-3,
                "Chromaticity R/G shift at pixel {i}"
            );

            let in_bg = b_in / g_in;
            let out_bg = b_out / g_out;
            assert!(
                (in_bg - out_bg).abs() < 2e-3,
                "Chromaticity B/G shift at pixel {i}"
            );
        }
    }

    // Scale-invariance RMSE check
    let mut sse = 0.0_f64;
    for i in 0..(count * 3) {
        let diff = (out_l0[i] - out_l2[i]) as f64;
        sse += diff * diff;
    }
    let rmse = (sse / (count * 3) as f64).sqrt();
    assert!(
        rmse < 0.015,
        "Scale-invariance RMSE must be < 1.5% (measured: {rmse})"
    );

    Ok(())
}
