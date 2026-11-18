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
    };

    let params_l2 = Stage5Params {
        width,
        height,
        mipmap_level: 2,
        base_octaves: 6,
        local_contrast_strength: 0.2,
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
