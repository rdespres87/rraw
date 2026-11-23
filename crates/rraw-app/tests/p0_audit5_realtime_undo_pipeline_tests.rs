#![forbid(unsafe_code)]

//! TDD Integration Tests for Deep Maturity Audit #5 — Chantier P0:
//! - P0.1: Eliminate synchronous UI-thread `fsync` spam during continuous slider drags & coalesce sidecar writes.
//! - P0.2: Cache and alphabetically sort explorer subdirectories in `LibraryController`.
//! - P0.3: Bounded Undo / Redo history stack (`PhotoSession::MAX_UNDO_HISTORY = 64`, `undo_grading`, `redo_grading`).
//! - P0.4: Unified per-pixel grading kernel (`PreparedGradingFrame`) guaranteeing preview and export parity.

use std::fs;

use rraw_app::controllers::session::PhotoSession;
use rraw_app::{
    load_photo_sidecars, render_export_image_with_depth, render_preview_from_base_profiled,
    BaseRgbPreview, DarkroomGradingState, ExportBitDepth, GradingPreset, LoadedPhotoView,
    PreparedGradingFrame, RrawApp,
};
use rraw_color::{SoftProofProfile, SoftProofRenderingIntent};
use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};

fn make_dummy_loaded_photo(path: std::path::PathBuf) -> LoadedPhotoView {
    LoadedPhotoView {
        path,
        camera_make: "TestMake".to_string(),
        camera_model: "TestCam".to_string(),
        dimensions: (8, 8),
        shutter_str: "1/250 s".to_string(),
        aperture_str: "f/2.8".to_string(),
        iso: 100,
        focal_length_str: "50 mm".to_string(),
        lens_model: "TestLens 50mm".to_string(),
        date_time: "2026-11-23 12:00:00".to_string(),
        exposure_bias: "0 EV".to_string(),
        rating: 4,
        exif_tags: Vec::new(),
    }
}

#[test]
fn test_p0_1_continuous_slider_drag_coalesces_sidecar_writes_and_undo() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("rraw_p0_1_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let photo_path = dir.join("test_drag.dng");
    fs::write(&photo_path, b"dummy raw").map_err(|e| e.to_string())?;
    let json_sidecar = dir.join("test_drag.dng.rraw.json");

    let mut app = RrawApp::new_headless();
    app.session.loaded_photo = Some(make_dummy_loaded_photo(photo_path.clone()));

    // Simulate 25 continuous mouse-drag frames (pointer_down = true) scrubbing exposure from 0.0 to 2.5 EV
    for step in 1..=25 {
        let pre_edit = app.session.grading.clone();
        app.session.grading.tone.exposure_ev = (step as f32) * 0.1;
        app.notify_grading_edited(pre_edit, true);
    }

    // During active drag: sidecar is marked dirty in memory, NOT written to disk yet
    assert!(
        app.session.sidecar_dirty,
        "sidecar_dirty must be true while slider drag is active"
    );
    assert!(
        !app.flush_dirty_sidecar_if_idle(true),
        "flush_dirty_sidecar_if_idle(true) must defer disk I/O while pointer is down"
    );
    assert!(
        !json_sidecar.exists(),
        "No disk fsync/write should happen while pointer_down is true"
    );
    assert_eq!(
        app.session.undo_len(),
        1,
        "Continuous 25-frame slider drag must coalesce into exactly 1 undo snapshot"
    );

    // Release mouse button (pointer_down = false) -> flushes sidecars once
    assert!(
        app.flush_dirty_sidecar_if_idle(false),
        "Releasing pointer must flush dirty sidecars once"
    );
    assert!(
        !app.session.sidecar_dirty,
        "sidecar_dirty must be cleared after idle flush"
    );
    assert!(
        json_sidecar.exists(),
        "Sidecar file must exist on disk after pointer release"
    );

    let (disk_grading, disk_rating) = load_photo_sidecars(&photo_path);
    let loaded_g = disk_grading.ok_or_else(|| "Expected sidecar grading on disk".to_string())?;
    assert!(
        (loaded_g.tone.exposure_ev - 2.5).abs() < 1e-4,
        "Persisted exposure_ev must be 2.5, got {}",
        loaded_g.tone.exposure_ev
    );
    assert_eq!(disk_rating, Some(4));

    // A single Undo restores exposure_ev back to the pre-drag 0.0 value
    assert!(app.undo_grading(), "undo_grading must succeed");
    assert!(
        app.session.grading.tone.exposure_ev.abs() < 1e-5,
        "Single undo after 25-frame drag must restore pre-drag 0.0 EV, got {}",
        app.session.grading.tone.exposure_ev
    );

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_p0_2_explorer_subdirectories_cached_and_sorted() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("rraw_p0_2_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    fs::create_dir_all(dir.join("zebra")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("alpha")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("mango")).map_err(|e| e.to_string())?;
    fs::write(dir.join("note.txt"), b"not a directory").map_err(|e| e.to_string())?;

    let mut app = RrawApp::new_headless();
    app.open_directory(&dir);

    let names: Vec<String> = app
        .library
        .cached_subdirectories
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    assert_eq!(
        names,
        vec![
            "alpha".to_string(),
            "mango".to_string(),
            "zebra".to_string()
        ],
        "Cached subdirectories must exclude regular files and be sorted alphabetically"
    );

    // Add a new subdirectory and refresh cache
    fs::create_dir_all(dir.join("beta")).map_err(|e| e.to_string())?;
    app.library.refresh_subdirectories();

    let updated_names: Vec<String> = app
        .library
        .cached_subdirectories
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    assert_eq!(
        updated_names,
        vec![
            "alpha".to_string(),
            "beta".to_string(),
            "mango".to_string(),
            "zebra".to_string()
        ]
    );

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_p0_3_bounded_undo_redo_stack_and_roundtrip() {
    let mut app = RrawApp::new_headless();
    assert!(!app.session.can_undo());
    assert!(!app.session.can_redo());

    // Step 1: mutate exposure
    app.record_undo_snapshot();
    // Duplicate snapshot without changing state must be deduplicated
    app.record_undo_snapshot();
    assert_eq!(app.session.undo_len(), 1);
    app.session.grading.tone.exposure_ev = 1.25;

    // Step 2: mutate kelvin
    app.record_undo_snapshot();
    app.session.grading.color.kelvin = 6800.0;

    // Step 3: apply preset
    app.record_undo_snapshot();
    GradingPreset::CinematicAgX.apply(&mut app.session.grading);
    app.session.current_preset = GradingPreset::CinematicAgX;

    assert_eq!(app.session.undo_len(), 3);
    assert!(!app.session.can_redo());

    // Undo Step 3 -> back to kelvin = 6800.0, exposure = 1.25
    assert!(app.undo_grading());
    assert!((app.session.grading.color.kelvin - 6800.0).abs() < 1e-4);
    assert!((app.session.grading.tone.exposure_ev - 1.25).abs() < 1e-4);
    assert!(app.session.can_redo());

    // Undo Step 2 -> back to kelvin = 5500.0, exposure = 1.25
    assert!(app.undo_grading());
    assert!((app.session.grading.color.kelvin - 5500.0).abs() < 1e-4);
    assert!((app.session.grading.tone.exposure_ev - 1.25).abs() < 1e-4);

    // Undo Step 1 -> back to initial exposure = 0.0
    assert!(app.undo_grading());
    assert!(app.session.grading.tone.exposure_ev.abs() < 1e-5);
    assert!(!app.session.can_undo());
    assert_eq!(app.session.redo_len(), 3);

    // Redo all 3 steps forward
    assert!(app.redo_grading());
    assert!((app.session.grading.tone.exposure_ev - 1.25).abs() < 1e-4);
    assert!(app.redo_grading());
    assert!((app.session.grading.color.kelvin - 6800.0).abs() < 1e-4);
    assert!(app.redo_grading());
    assert_eq!(app.session.current_preset, GradingPreset::CinematicAgX);
    assert!(!app.session.can_redo());

    // Verify bounded capacity at PhotoSession::MAX_UNDO_HISTORY (64)
    for i in 0..80 {
        app.record_undo_snapshot();
        app.session.grading.tone.contrast = (i as f32) * 0.5 + 1.0;
    }
    assert_eq!(
        app.session.undo_len(),
        PhotoSession::MAX_UNDO_HISTORY,
        "Undo stack must be bounded by MAX_UNDO_HISTORY (64)"
    );
}

#[test]
fn test_p0_4_unified_grading_kernel_preview_and_export_parity() -> Result<(), String> {
    let width = 16;
    let height = 16;
    let mut samples = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let fx = x as f32 / (width.max(1) as f32);
            let fy = y as f32 / (height.max(1) as f32);
            samples.push(0.05 + 0.85 * (0.6 * fx + 0.4 * fy));
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)
        .map_err(|e| e.to_string())?;
    let base =
        BaseRgbPreview::from_cfa_with_color_and_target_width(&cfa, [1.12, 1.0, 1.18], None, width);

    let mut grading = DarkroomGradingState::default();
    grading.tone.exposure_ev = 0.45;
    grading.tone.contrast = 18.0;
    grading.tone.highlights = -20.0;
    grading.tone.shadows = 25.0;
    grading.tone.blacks = -8.0;
    grading.tone.vibrance = 15.0;
    grading.tone.saturation = 12.0;
    grading.color.kelvin = 6100.0;
    grading.color.tint = 4.0;
    grading.effects.creative_vignette_amount = -22.0;
    grading.soft_proof.enabled = true;
    grading.soft_proof.profile = SoftProofProfile::FineArtMattePaper;
    grading.soft_proof.intent = SoftProofRenderingIntent::Perceptual;
    grading.soft_proof.simulate_paper_and_ink = true;
    grading.soft_proof.show_gamut_warning = false;

    let kernel = PreparedGradingFrame::prepare(&base, &grading, false);
    assert_eq!(kernel.preview_w(), base.width);
    assert_eq!(kernel.preview_h(), base.height);

    let (preview_img, _hist, _timing) = render_preview_from_base_profiled(&base, &grading);
    let (exp_w, exp_h, exp_buf) =
        render_export_image_with_depth(&base, &grading, 1.0, ExportBitDepth::Bits8);
    assert_eq!((exp_w, exp_h), (base.width, base.height));

    let exp_u8 = exp_buf
        .as_u8_slice()
        .ok_or_else(|| "8-bit export must yield U8 slice".to_string())?;
    for i in 0..(base.width * base.height) {
        let p = preview_img.pixels[i];
        let er = exp_u8[i * 3];
        let eg = exp_u8[i * 3 + 1];
        let eb = exp_u8[i * 3 + 2];
        assert_eq!(
            [p.r(), p.g(), p.b()],
            [er, eg, eb],
            "Preview and 8-bit export pixel #{i} must be bit-identical under unified PreparedGradingFrame"
        );
    }
    Ok(())
}
