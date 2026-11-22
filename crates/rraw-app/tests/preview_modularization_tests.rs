#![forbid(unsafe_code)]

//! TDD and regression test suite for Task 15.2 Step 3: Monolithic Preview Pipeline Decomposition.
//!
//! Validates:
//! 1. Preview submodules (`types`, `orientation`, `masks`, `decoder`, `pipeline`).
//! 2. Contract behavior and state mutation with zero panic/unwrap.
//! 3. Clean isolation and modularity of color pipeline, decoding, and AI mask inference.

use std::path::PathBuf;

use rraw_app::preview::{
    compute_cam_to_srgb_matrix, eval_all_mask_adjustments, infer_preview_semantic_masks,
    is_raw_extension, oriented_dimensions, BaseRgbPreview, LoadedPhotoView,
};
use rraw_core::SignedCfaBuffer;

#[test]
fn test_orientation_helpers() {
    assert_eq!(oriented_dimensions(1920, 1080, 1), (1920, 1080));
    assert_eq!(oriented_dimensions(1920, 1080, 6), (1080, 1920));
    assert_eq!(oriented_dimensions(1920, 1080, 8), (1080, 1920));

    assert!(is_raw_extension("dng"));
    assert!(is_raw_extension("ARW"));
    assert!(is_raw_extension("cr3"));
    assert!(!is_raw_extension("jpg"));
    assert!(!is_raw_extension("png"));
}

#[test]
fn test_loaded_photo_view_export_json() {
    let view = LoadedPhotoView {
        path: PathBuf::from("/test/photo.dng"),
        camera_make: "Sony".to_string(),
        camera_model: "ILCE-7RM4".to_string(),
        lens_model: "FE 24-70mm F2.8 GM".to_string(),
        dimensions: (9504, 6336),
        iso: 100,
        shutter_str: "1/250 s".to_string(),
        aperture_str: "f/4.0".to_string(),
        focal_length_str: "50 mm".to_string(),
        date_time: "2026:11:22 10:00:00".to_string(),
        exposure_bias: "0 EV".to_string(),
        rating: 5,
        exif_tags: vec![("Software".to_string(), "Rraw 0.1".to_string())],
    };

    let json = view.export_exif_json();
    assert!(json.contains("ILCE-7RM4"));
    assert!(json.contains("Sony"));
    assert!(json.contains("9504x6336"));
}

#[test]
fn test_cam_to_srgb_matrix() {
    let wb = [2.0, 1.0, 1.5];
    let cm = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let mat = compute_cam_to_srgb_matrix(Some(&cm), &wb);
    let row0_sum = mat[0] + mat[1] + mat[2];
    assert!((row0_sum - 1.0).abs() < 1e-3);
}

#[test]
fn test_base_rgb_preview_from_cfa() {
    let cfa = SignedCfaBuffer::default();
    let base = BaseRgbPreview::from_cfa(&cfa);
    assert_eq!(base.width, 1);
    assert_eq!(base.height, 1);
    assert_eq!(base.samples_rgb.len(), 1);
}

#[test]
fn test_masks_eval_adjustments_neutral() {
    let cfa = SignedCfaBuffer::default();
    let base = BaseRgbPreview::from_cfa(&cfa);
    let (ev, contrast, sat, temp) =
        eval_all_mask_adjustments(&base.semantic_masks, &[], &[], 0.5, 0.5);
    assert_eq!(ev, 0.0);
    assert_eq!(contrast, 0.0);
    assert_eq!(sat, 0.0);
    assert_eq!(temp, 0.0);

    let samples = [[0.5, 0.5, 0.5]];
    let wb = [1.0, 1.0, 1.0];
    let mat = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let masks = infer_preview_semantic_masks(&samples, 1, 1, wb, mat);
    assert_eq!(masks.width, 1);
    assert_eq!(masks.height, 1);
}
