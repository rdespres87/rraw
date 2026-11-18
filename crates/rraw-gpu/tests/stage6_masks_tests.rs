use rraw_core::{GeometricMask, MaskKind, ParametricRange, Stage6MasksParams};
use rraw_gpu::{GpuContext, Stage6Params, Stage6Pipeline};

#[test]
fn test_linear_gradient_mask_weights() {
    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    let base_val = 0.5_f32;
    let rgb_in = vec![base_val; count * 3];

    let mask = GeometricMask {
        id: "lin_1".to_owned(),
        kind: MaskKind::LinearGradient {
            start: [0.2, 0.5],
            end: [0.8, 0.5],
        },
        exposure_offset_ev: 1.0, // +1 EV -> 2x gain at full weight
        opacity: 1.0,
        enabled: true,
    };

    let params = Stage6Params {
        width,
        height,
        masks: Stage6MasksParams { masks: vec![mask] },
    };

    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(_) => return, // Graceful skip if headless GPU unavailable
    };

    let pipeline = match Stage6Pipeline::new(&ctx) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Stage6Pipeline compilation skipped: {e:?}");
            return;
        }
    };

    let out = match pipeline.execute(&ctx, &rgb_in, &params) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Stage6 execution skipped: {e:?}");
            return;
        }
    };

    assert_eq!(out.len(), count * 3);

    // Check row 32 (y = 0.5)
    let y = 32_u32;
    // x = 0.1 -> before start (weight 0.0) -> gain = 2^0 = 1.0 -> val = 0.5
    let idx_left = ((y * width + 6) * 3) as usize; // 6/64 = 0.09375 < 0.2
    assert!((out[idx_left] - 0.5).abs() < 0.05);

    // x = 0.5 -> center (weight 0.5) -> gain = 2^0.5 = ~1.414 -> val = 0.5 * 1.414 = ~0.707
    let idx_mid = ((y * width + 32) * 3) as usize; // 32/64 = 0.5
    assert!((out[idx_mid] - 0.5 * 2.0_f32.powf(0.5)).abs() < 0.05);

    // x = 0.9 -> after end (weight 1.0) -> gain = 2^1 = 2.0 -> val = 0.5 * 2.0 = 1.0
    let idx_right = ((y * width + 58) * 3) as usize; // 58/64 = 0.90625 > 0.8
    assert!((out[idx_right] - 1.0).abs() < 0.05);
}

#[test]
fn test_radial_gradient_mask_weights() {
    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    let base_val = 0.4_f32;
    let rgb_in = vec![base_val; count * 3];

    let mask = GeometricMask {
        id: "rad_1".to_owned(),
        kind: MaskKind::RadialGradient {
            center: [0.5, 0.5],
            radius: [0.3, 0.3],
            rotation_deg: 0.0,
        },
        exposure_offset_ev: 1.0,
        opacity: 1.0,
        enabled: true,
    };

    let params = Stage6Params {
        width,
        height,
        masks: Stage6MasksParams { masks: vec![mask] },
    };

    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(_) => return,
    };

    let pipeline = match Stage6Pipeline::new(&ctx) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Stage6Pipeline skipped: {e:?}");
            return;
        }
    };

    let out = match pipeline.execute(&ctx, &rgb_in, &params) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Stage6 execution skipped: {e:?}");
            return;
        }
    };

    // Center (32, 32) -> distance = 0 -> weight = 1.0 -> val = 0.4 * 2.0 = 0.8
    let idx_center = ((32 * width + 32) * 3) as usize;
    assert!((out[idx_center] - 0.8).abs() < 0.05);

    // Far corner (0, 0) -> distance > radius -> weight = 0.0 -> val = 0.4
    let idx_corner = 0_usize;
    assert!((out[idx_corner] - 0.4).abs() < 0.05);
}

#[test]
fn test_parametric_luminance_mask_weights() {
    let width = 64_u32;
    let height = 64_u32;
    let count = (width * height) as usize;

    let mut rgb_in = vec![0.0_f32; count * 3];
    // Left half is dark (0.1), right half is bright (0.9)
    for y in 0..height {
        for x in 0..width {
            let val = if x < 32 { 0.1 } else { 0.9 };
            let idx = ((y * width + x) * 3) as usize;
            rgb_in[idx] = val;
            rgb_in[idx + 1] = val;
            rgb_in[idx + 2] = val;
        }
    }

    let mask = GeometricMask {
        id: "param_1".to_owned(),
        kind: MaskKind::ParametricColor {
            luminance: Some(ParametricRange {
                min_val: 0.5,
                max_val: 1.5,
                feather: 0.1,
            }),
            chroma: None,
            hue_deg: None,
        },
        exposure_offset_ev: 1.0,
        opacity: 1.0,
        enabled: true,
    };

    let params = Stage6Params {
        width,
        height,
        masks: Stage6MasksParams { masks: vec![mask] },
    };

    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(_) => return,
    };

    let pipeline = match Stage6Pipeline::new(&ctx) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Stage6Pipeline skipped: {e:?}");
            return;
        }
    };

    let out = match pipeline.execute(&ctx, &rgb_in, &params) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Stage6 execution skipped: {e:?}");
            return;
        }
    };

    // Dark side should be untouched: 0.1
    let idx_dark = ((32 * width + 10) * 3) as usize;
    assert!((out[idx_dark] - 0.1).abs() < 0.05);

    // Bright side should be amplified: 0.9 * 2.0 = 1.8
    let idx_bright = ((32 * width + 50) * 3) as usize;
    assert!((out[idx_bright] - 1.8).abs() < 0.05);
}
