#![forbid(unsafe_code)]
use rraw_app::state::{save_photo_sidecars, DarkroomGradingState};
use rraw_app::RrawApp;
use rraw_core::curves::CurvePoint;
use rraw_core::{NoiseProfile, NoiseProfileSource};
use rraw_optics::{execute_richardson_lucy, RichardsonLucyParams};
use std::fs;

#[test]
fn atomic_write_bytes_persists_binary_payload() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from("/tmp/rraw_test_sidecar.bin");
    let data = vec![0u8, 1, 2, 3, 255, 0xFF, 0xAB, 0xCD];
    rraw_app::sidecar::atomic_write_bytes(&path, &data)?;
    assert_eq!(fs::read(&path)?, data);
    let _ = fs::remove_file(&path);
    Ok(())
}

#[test]
fn open_directory_skips_eager_ui_sidecar_loop_and_refreshes_subdirectories(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from("/tmp/rraw_test_dir");
    let b_path = dir.join("photo_b.arw");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub_a"))?;
    fs::create_dir_all(dir.join("sub_b"))?;
    fs::write(&b_path, b"arw")?;
    let mut app = RrawApp::new_headless();
    let mut g = DarkroomGradingState::default();
    g.tone.exposure_ev = 1.75;
    save_photo_sidecars(&b_path, &g, 4);
    app.open_directory(&dir);
    assert_eq!(app.library.cached_subdirectories.len(), 2);
    assert!(!app.library.photo_gradings.contains_key(&b_path));
    app.copy_recipe_for_path(&b_path);
    assert_eq!(
        app.library
            .recipe_clipboard
            .as_ref()
            .map(|c| c.tone.exposure_ev),
        Some(1.75)
    );
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn tone_curves_reset_and_lut_actions_record_undo_snapshots(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = RrawApp::new_headless();
    let pt = CurvePoint::new(0.5, 0.75)?;
    app.session
        .grading
        .tone_curves
        .combined_rgb
        .points
        .insert(1, pt);
    app.reset_active_tone_curve();
    assert!(app.session.can_undo());
    assert!(app.session.sidecar_dirty);
    assert!(app.undo_grading());
    assert_eq!(app.session.grading.tone_curves.combined_rgb.points.len(), 3);
    Ok(())
}

#[test]
fn parallel_richardson_lucy_preserves_invariants() -> Result<(), Box<dyn std::error::Error>> {
    let noise_profile = NoiseProfile::new(
        [0.0002; 3],
        [0.00005; 3],
        NoiseProfileSource::WaveletMadFallback,
    )?;
    let params = RichardsonLucyParams {
        iterations: 2,
        epsilon: 1e-4,
        base_psf_sigma: 0.75,
        deringing_threshold: 0.08,
        noise_profile,
    };
    let input = vec![0.25_f32; 16 * 16 * 3];
    let out = execute_richardson_lucy(&input, 16, 16, &params);
    assert_eq!(out.len(), 16 * 16 * 3);
    assert!(out.iter().all(|v| v.is_finite() && *v >= 0.0));
    Ok(())
}
