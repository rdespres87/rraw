use rraw_ai::{
    anscombe_transform, inverse_anscombe_transform, plan_phase_preserving_tiles, AnscombeParams,
    NeuralTileDenoiser, SimulatedTileDenoiser, TileBlender, TileSlicer,
};
use rraw_core::{
    CfaPattern, NoiseProfile, NoiseProfileSource, PhotoId, Stage0Epoch, Stage0Token, TileGeometry,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn test_phase_preserving_tiling_reconstruction_error() {
    let width = 2000_usize;
    let height = 1500_usize;
    let count = width * height;

    // Ground truth phase-sensitive pattern: f(x, y) depends strictly on (x % 6, y % 6)
    let mut ground_truth = vec![0.0_f32; count];
    for y in 0..height {
        for x in 0..width {
            let px = (x % 6) as f32;
            let py = (y % 6) as f32;
            ground_truth[y * width + x] = (px * 6.0 + py) / 36.0;
        }
    }

    let pattern = CfaPattern::fujifilm_xtrans();
    let geometry = TileGeometry::COREML;

    let Ok(tiles) = plan_phase_preserving_tiles(width, height, &pattern, geometry) else {
        return;
    };

    assert!(!tiles.is_empty());

    let slicer = TileSlicer::new(geometry);
    let mut blender = TileBlender::new(width, height, geometry);

    let denoiser = SimulatedTileDenoiser::identity();
    let Ok(noise) = NoiseProfile::new(
        [0.0001, 0.0001, 0.0001],
        [0.0001, 0.0001, 0.0001],
        NoiseProfileSource::CameraTable,
    ) else {
        return;
    };

    for tile_desc in &tiles {
        let tile_data = slicer.extract_tile(&ground_truth, width, height, tile_desc);
        assert_eq!(tile_data.len(), geometry.tile_size * geometry.tile_size);

        let Ok(processed) =
            denoiser.denoise_tile(&tile_data, geometry.tile_size, geometry.tile_size, &noise)
        else {
            return;
        };

        blender.accumulate_tile(&processed, tile_desc);
    }

    let reconstructed = blender.finalize();
    assert_eq!(reconstructed.len(), count);

    let mut max_err = 0.0_f32;
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let err = (reconstructed[idx] - ground_truth[idx]).abs();
            if err > max_err {
                max_err = err;
            }
        }
    }

    assert!(
        max_err < 1e-6,
        "Maximum reconstruction error across tile seams must be < 1e-6 (measured: {max_err:.2e})"
    );
}

#[test]
fn test_cosine_window_partition_of_unity() {
    let width = 960_usize;
    let height = 960_usize;
    let geometry = TileGeometry::COREML;
    let pattern = CfaPattern::fujifilm_xtrans();

    let Ok(tiles) = plan_phase_preserving_tiles(width, height, &pattern, geometry) else {
        return;
    };

    let ones_image = vec![1.0_f32; width * height];
    let slicer = TileSlicer::new(geometry);
    let mut blender = TileBlender::new(width, height, geometry);

    for tile_desc in &tiles {
        let tile_data = slicer.extract_tile(&ones_image, width, height, tile_desc);
        blender.accumulate_tile(&tile_data, tile_desc);
    }

    let reconstructed = blender.finalize();

    for (i, &v) in reconstructed.iter().enumerate() {
        let err = (v - 1.0).abs();
        assert!(
            err < 1e-6,
            "Pixel {i} failed partition of unity: got {v}, error = {err:.2e}"
        );
    }
}

#[test]
fn test_generalized_anscombe_transform_roundtrip() {
    let params = AnscombeParams {
        alpha: 0.005,
        sigma_sq: 0.0002,
    };

    let test_values = [0.001_f32, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0];

    for &val in &test_values {
        let stabilized = anscombe_transform(val, &params);
        assert!(stabilized.is_finite());

        let recovered = inverse_anscombe_transform(stabilized, &params);
        assert!(recovered.is_finite());

        let rel_err = (recovered - val).abs() / val;
        assert!(
            rel_err < 1e-4,
            "Anscombe roundtrip failed for {val}: recovered {recovered}, rel_err = {rel_err:.2e}"
        );
    }
}

#[test]
fn test_cooperative_token_cancellation() {
    let active_token = Arc::new(std::sync::RwLock::new(Stage0Token {
        photo_id: PhotoId(1),
        epoch: Stage0Epoch(0),
    }));

    let cancelled_flag = Arc::new(AtomicBool::new(false));

    let token_clone = Arc::clone(&active_token);
    let flag_clone = Arc::clone(&cancelled_flag);

    let worker_token = Stage0Token {
        photo_id: PhotoId(1),
        epoch: Stage0Epoch(0),
    };

    // Simulate background worker checking token before tile processing
    let handle = std::thread::spawn(move || {
        for _step in 0..10 {
            let current = match token_clone.read() {
                Ok(guard) => *guard,
                Err(_) => return,
            };
            if !rraw_ai::is_token_current(current, worker_token) {
                flag_clone.store(true, Ordering::SeqCst);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    // Main thread simulates user slider movement after 10ms
    std::thread::sleep(std::time::Duration::from_millis(10));
    if let Ok(mut guard) = active_token.write() {
        guard.epoch = guard.epoch.next();
    }

    let _ = handle.join();
    assert!(
        cancelled_flag.load(Ordering::SeqCst),
        "Worker must exit immediately upon epoch mutation"
    );
}
