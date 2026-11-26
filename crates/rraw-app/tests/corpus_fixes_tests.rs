#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

use rraw_app::app::RrawApp;
use rraw_app::preview::pipeline::clip_saturated_sensor_highlights;
use rraw_app::preview::LoadedPhotoView;
use rraw_app::state::DarkroomGradingState;
use rraw_core::SmartLightingMode;

#[test]
fn test_open_empty_directory_clears_active_photo() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = RrawApp::new_headless();
    app.session.loaded_photo = Some(LoadedPhotoView {
        path: PathBuf::from("/tmp/previous_photo.dng"),
        camera_make: "Test".to_owned(),
        camera_model: "Cam".to_owned(),
        lens_model: String::new(),
        dimensions: (100, 100),
        iso: 100,
        shutter_str: String::new(),
        aperture_str: String::new(),
        focal_length_str: String::new(),
        date_time: String::new(),
        exposure_bias: String::new(),
        rating: 0,
        exif_tags: Vec::new(),
    });

    let empty_dir =
        std::env::temp_dir().join(format!("rraw_empty_dir_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&empty_dir);
    fs::create_dir_all(&empty_dir)?;

    app.open_directory(&empty_dir);

    let _ = fs::remove_dir_all(&empty_dir);
    assert!(app.library.directory_entries.is_empty());
    assert!(
        app.session.loaded_photo.is_none(),
        "opening an empty directory must clear previously displayed photo"
    );
    assert!(app.session.base_preview.is_none());
    assert!(app.session.preview_texture.is_none());
    Ok(())
}

#[test]
fn test_subdirectories_sorted_case_insensitively() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("rraw_subdir_sort_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("Nikon"))?;
    fs::create_dir_all(root.join("Minolta"))?;
    fs::create_dir_all(root.join("moto g8plus"))?;

    let mut app = RrawApp::new_headless();
    app.open_directory(&root);
    let names: Vec<String> = app
        .library
        .cached_subdirectories
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    let _ = fs::remove_dir_all(&root);
    assert_eq!(names, vec!["Minolta", "moto g8plus", "Nikon"]);
    Ok(())
}

#[test]
fn test_clip_saturated_sensor_highlights_neutralizes_magenta_and_preserves_midtones_and_hdr() {
    let wb = [2.2_f32, 1.0_f32, 1.8_f32];

    // Clipped white sensor pixel (1.0, 1.0, 1.0) must become neutral white after WB
    let clipped = clip_saturated_sensor_highlights([1.0, 1.0, 1.0], wb);
    let wb_r = clipped[0] * wb[0];
    let wb_g = clipped[1] * wb[1];
    let wb_b = clipped[2] * wb[2];
    assert!((wb_r - 1.0).abs() < 1e-5);
    assert!((wb_g - 1.0).abs() < 1e-5);
    assert!((wb_b - 1.0).abs() < 1e-5);

    // Sensor clipped above nominal white level (e.g. Nikon Coolpix P7100: 4095/3800 = 1.0776)
    let p7100_wb = [1.088_f32, 1.0_f32, 3.397_f32];
    let p7100_clipped = clip_saturated_sensor_highlights([1.0776, 1.0776, 1.0776], p7100_wb);
    assert!((p7100_clipped[0] * p7100_wb[0] - 1.0).abs() < 1e-4);
    assert!((p7100_clipped[2] * p7100_wb[2] - 1.0).abs() < 1e-4);

    // Midtone pixel untouched
    let mid = [0.3_f32, 0.4_f32, 0.2_f32];
    assert_eq!(clip_saturated_sensor_highlights(mid, wb), mid);

    // Unclipped HDR float pixel (> 1.30) untouched
    let hdr = [2.5_f32, 2.0_f32, 1.8_f32];
    assert_eq!(clip_saturated_sensor_highlights(hdr, wb), hdr);
}

#[test]
fn test_default_smart_lighting_is_slight() {
    let grading = DarkroomGradingState::default();
    assert_eq!(grading.smart_lighting.mode, SmartLightingMode::Slight);
}

#[test]
fn test_base_preview_from_linear_rgb_and_export_fallback() {
    use rraw_app::preview::BaseRgbPreview;
    use rraw_app::state::ExportScaleMode;
    use rraw_core::SignedCfaBuffer;

    let samples = vec![[0.25_f32, 0.5_f32, 0.75_f32]; 16 * 8];
    let full = BaseRgbPreview::from_linear_rgb_with_color_and_target_width(
        &samples,
        16,
        8,
        [1.0, 1.0, 1.0],
        None,
        32,
    );
    assert_eq!(full.width, 16);
    assert_eq!(full.height, 8);
    assert_eq!(full.samples_rgb.len(), 128);
    assert_eq!(full.samples_rgb[0], [0.25, 0.5, 0.75]);

    let scaled = BaseRgbPreview::from_linear_rgb_with_color_and_target_width(
        &samples,
        16,
        8,
        [1.0, 1.0, 1.0],
        None,
        8,
    );
    assert_eq!(scaled.width, 8);
    assert_eq!(scaled.height, 4);
    assert_eq!(scaled.samples_rgb.len(), 32);

    // Export builder with empty CFA (Linear RGB or raster image) must preserve ref_preview
    let empty_cfa = SignedCfaBuffer::default();
    let exported = RrawApp::build_export_base_static(ExportScaleMode::Original, &empty_cfa, &full);
    assert_eq!(exported.width, 16);
    assert_eq!(exported.height, 8);
}
