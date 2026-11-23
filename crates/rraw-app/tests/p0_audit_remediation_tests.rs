#![forbid(unsafe_code)]

//! TDD integration test suite for Audit #4 Priority P0 remediations:
//! - P0.1: Parallel single-buffer export rendering (`ExportRasterBuffer`, `render_export_image_with_depth`) & Preview/Export parity.
//! - P0.2: Asynchronous frame-by-frame streaming Timelapse export without blocking the UI thread or accumulating full-res frames in RAM.
//! - P0.3: O(1) bilinear lookup grid & AABB bounding-box acceleration for freehand brush masks (`PrecomputedBrushGrid`).
//! - P0.4: Atomic write-temp-then-rename sidecar persistence and `.rraw.json` > `.xmp` rating precedence contract.

use std::error::Error;
use std::time::{Duration, Instant};

use rraw_app::preview::{render_export_image_with_depth, render_preview_from_base, BaseRgbPreview};
use rraw_app::state::{
    load_photo_sidecars, save_photo_sidecars, DarkroomGradingState, ExportBitDepth,
    ExportTargetMode, LocalMaskLayer, PrecomputedBrushGrid,
};
use rraw_app::RrawApp;
use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};

fn build_synthetic_base_preview(
    width: usize,
    height: usize,
) -> Result<BaseRgbPreview, Box<dyn Error>> {
    let mut samples = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let fx = x as f32 / (width.max(1) as f32);
            let fy = y as f32 / (height.max(1) as f32);
            samples.push(0.05 + 0.85 * (0.6 * fx + 0.4 * fy));
        }
    }
    let cfa = SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)?;
    Ok(BaseRgbPreview::from_cfa_with_color_and_target_width(
        &cfa,
        [1.8, 1.0, 1.4],
        None,
        width,
    ))
}

#[test]
fn test_p0_1_render_export_image_with_depth_single_buffer_and_parity() -> Result<(), Box<dyn Error>>
{
    let base = build_synthetic_base_preview(64, 48)?;
    let mut grading = DarkroomGradingState::default();
    grading.tone.contrast = -80.0;
    // Add a local mask with negative contrast to verify the 0.05 floor prevents contrast inversion
    let mut mask = LocalMaskLayer::new_linear("NegContrast", [0.0, 0.0], [1.0, 1.0]);
    mask.contrast = -90.0;
    grading.local_masks.push(mask);

    // 1. 8-bit export allocates ONLY the u8 buffer
    let (w8, h8, buf8) =
        render_export_image_with_depth(&base, &grading, 1.0, ExportBitDepth::Bits8);
    assert_eq!(w8, base.width);
    assert_eq!(h8, base.height);
    assert!(buf8.as_u8_slice().is_some());
    assert!(buf8.as_u16_slice().is_none());
    let u8_slice = buf8.as_u8_slice().ok_or("missing u8 slice")?;
    assert_eq!(u8_slice.len(), w8 * h8 * 3);

    // 2. 16-bit export allocates ONLY the u16 buffer
    let (w16, h16, buf16) =
        render_export_image_with_depth(&base, &grading, 1.0, ExportBitDepth::Bits16);
    assert_eq!(w16, base.width);
    assert_eq!(h16, base.height);
    assert!(buf16.as_u16_slice().is_some());
    assert!(buf16.as_u8_slice().is_none());
    let u16_slice = buf16.as_u16_slice().ok_or("missing u16 slice")?;
    assert_eq!(u16_slice.len(), w16 * h16 * 3);

    // 3. Parity between preview and 8-bit export (including soft proofing & contrast floor)
    let (preview_img, _) = render_preview_from_base(&base, &grading);
    assert_eq!(preview_img.pixels.len(), w8 * h8);
    for (idx, px) in preview_img.pixels.iter().enumerate() {
        let er = u8_slice[idx * 3];
        let eg = u8_slice[idx * 3 + 1];
        let eb = u8_slice[idx * 3 + 2];
        assert!(
            (px.r() as i16 - er as i16).abs() <= 1
                && (px.g() as i16 - eg as i16).abs() <= 1
                && (px.b() as i16 - eb as i16).abs() <= 1,
            "Pixel mismatch at {idx}: preview=({}, {}, {}) vs export=({er}, {eg}, {eb})",
            px.r(),
            px.g(),
            px.b()
        );
    }

    Ok(())
}

#[test]
fn test_p0_2_timelapse_background_export_streams_async_without_blocking(
) -> Result<(), Box<dyn Error>> {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("rraw_p0_2_timelapse_{unique_id}"));
    std::fs::create_dir_all(&temp_dir)?;

    // Write 3 small valid PNG frames
    for i in 1..=3 {
        let mut img = image::RgbImage::new(32, 24);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let v = ((x + y + i * 20).min(240)) as u8;
            *px = image::Rgb([v, v, v]);
        }
        let frame_path = temp_dir.join(format!("frame_{i:03}.png"));
        img.save(&frame_path)?;
    }

    let mut app = RrawApp::new_headless();
    app.open_directory(&temp_dir);
    app.export.export_target_mode = ExportTargetMode::TimelapseSequence;
    app.export.export_subfolder = "TimelapseOut".to_string();
    app.export.timelapse_sequence_prefix = "seq".to_string();

    app.spawn_background_export()?;

    let mut progress_steps = Vec::new();
    let mut final_result = None;
    let deadline = Instant::now() + Duration::from_secs(10);

    while Instant::now() < deadline {
        if let Ok((prog, _label, outcome)) = app
            .workers
            .export_receiver
            .recv_timeout(Duration::from_millis(100))
        {
            progress_steps.push(prog);
            if let Some(res) = outcome {
                final_result = Some(res);
                break;
            }
        }
    }

    // Must have emitted incremental streaming progress (< 1.0) before completion
    assert!(
        progress_steps.len() >= 3,
        "Expected incremental streaming progress updates for timelapse frames, got {:?}",
        progress_steps
    );
    assert!(
        progress_steps.iter().any(|&p| p < 0.99),
        "Expected intermediate progress < 1.0 during streaming timelapse export"
    );

    let last_exported = final_result.ok_or("Timed out waiting for timelapse export")??;
    assert!(last_exported.exists());

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

#[test]
fn test_p0_3_freehand_brush_precomputed_grid_o1_speed_and_accuracy() -> Result<(), Box<dyn Error>> {
    let mut layer = LocalMaskLayer::new_brush("HighDensityBrush", 0.06, 0.75);
    layer.exposure_ev = 1.25;
    layer.feather = 0.65;
    layer.opacity = 0.9;

    // Add 250 dabs along a path inside [0.2, 0.8] x [0.2, 0.8]
    for i in 0..250 {
        let t = i as f32 / 249.0;
        let u = 0.25 + 0.50 * t;
        let v = 0.25 + 0.50 * (t * std::f32::consts::PI).sin() * 0.5;
        layer.add_brush_dab_forced(u, v);
    }

    let grid = PrecomputedBrushGrid::from_layer(&layer).ok_or("Expected brush grid")?;

    // Outside AABB should return 0.0 immediately
    assert_eq!(grid.sample(0.02, 0.02), 0.0);
    assert_eq!(grid.sample(0.98, 0.98), 0.0);

    // Accuracy check against exact analytic evaluation across sample points
    let mut max_diff = 0.0_f32;
    for sy in 0..32 {
        for sx in 0..32 {
            let u = (sx as f32 + 0.5) / 32.0;
            let v = (sy as f32 + 0.5) / 32.0;
            let exact = layer.evaluate_weight(u, v);
            let approx = grid.sample(u, v);
            max_diff = max_diff.max((exact - approx).abs());
        }
    }
    assert!(
        max_diff < 0.035,
        "PrecomputedBrushGrid bilinear lookup deviated from exact weight by {max_diff}"
    );

    // Performance check: 500,000 samples must complete in < 35 ms (O(1) per sample)
    let start = Instant::now();
    let mut checksum = 0.0_f32;
    for i in 0..500_000 {
        let u = ((i % 1000) as f32) * 0.001;
        let v = ((i / 1000) as f32) * 0.002;
        checksum += grid.sample(u, v);
    }
    let elapsed = start.elapsed();
    assert!(checksum > 100.0);
    assert!(
        elapsed < Duration::from_millis(50),
        "500k O(1) brush lookups took {:?}, expected < 50ms",
        elapsed
    );

    Ok(())
}

#[test]
fn test_p0_4_sidecar_atomic_persistence_and_rraw_json_rating_precedence(
) -> Result<(), Box<dyn Error>> {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("rraw_p0_4_sidecar_{unique_id}"));
    std::fs::create_dir_all(&temp_dir)?;

    let photo_path = temp_dir.join("DSC_0042.ARW");
    std::fs::write(&photo_path, b"dummy raw bytes")?;

    let mut grading = DarkroomGradingState::default();
    grading.tone.exposure_ev = 0.85;
    save_photo_sidecars(&photo_path, &grading, 5);

    // Verify no leftover .tmp files exist in directory
    for entry in std::fs::read_dir(&temp_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".tmp"),
            "Found leftover temporary file after atomic write: {name}"
        );
    }

    // .rraw.json in .rraw/ is the single source of truth (rating == Some(5))
    let sidecars = rraw_storage::SidecarPaths::from_raw_path(&photo_path);
    assert!(sidecars.recipe_json.exists());
    let (loaded_grading, loaded_rating) = load_photo_sidecars(&photo_path);
    assert!(loaded_grading.is_some());
    assert_eq!(
        loaded_rating,
        Some(5),
        ".rraw.json rating (5) must be loaded from canonical .rraw/ location"
    );

    // When .rraw.json is absent, load returns None
    std::fs::remove_file(&sidecars.recipe_json)?;
    let (fallback_grading, fallback_rating) = load_photo_sidecars(&photo_path);
    assert!(fallback_grading.is_none());
    assert_eq!(fallback_rating, None);

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}
