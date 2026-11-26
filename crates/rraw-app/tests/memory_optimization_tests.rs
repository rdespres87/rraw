use rraw_app::preview::{BaseRgbPreview, CachedPhotoEntry, LoadedPhotoView};
use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};
use std::sync::Arc;

#[test]
fn test_base_rgb_preview_zero_copy_clone() {
    let samples: Vec<[f32; 3]> = (0..1024)
        .map(|i| [i as f32, (i + 1) as f32, (i + 2) as f32])
        .collect();
    let samples_arc: Arc<[[f32; 3]]> = Arc::from(samples);

    let base = BaseRgbPreview {
        width: 32,
        height: 32,
        samples_rgb: Arc::clone(&samples_arc),
        as_shot_wb: [1.0, 1.0, 1.0],
        cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
            width: 32,
            height: 32,
            subject: vec![0.0; 1024],
            sky: vec![0.0; 1024],
            background: vec![1.0; 1024],
            portrait: vec![0.0; 1024],
        }),
        exif_orientation: 1,
        matched_lens: None,
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(1.8),
    };

    let cloned = base.clone();
    assert!(Arc::ptr_eq(&base.samples_rgb, &cloned.samples_rgb));
    assert!(Arc::ptr_eq(&base.semantic_masks, &cloned.semantic_masks));
    assert_eq!(base.samples_rgb.len(), 1024);
    assert_eq!(cloned.samples_rgb.len(), 1024);
}

#[test]
fn test_cached_photo_entry_memory_footprint() -> Result<(), Box<dyn std::error::Error>> {
    let raw_samples = vec![0.5_f32; 4096];
    let cfa = SignedCfaBuffer::new(64, 64, CfaPattern::Bayer(BayerOrigin::Rggb), raw_samples)?;

    let samples_rgb = Arc::new([[0.5_f32, 0.5, 0.5]; 1024]);
    let base = BaseRgbPreview {
        width: 32,
        height: 32,
        samples_rgb,
        as_shot_wb: [1.0, 1.0, 1.0],
        cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
            width: 32,
            height: 32,
            subject: vec![0.0; 1024],
            sky: vec![0.0; 1024],
            background: vec![1.0; 1024],
            portrait: vec![0.0; 1024],
        }),
        exif_orientation: 1,
        matched_lens: None,
        focal_length_mm: None,
        aperture_f_number: None,
    };

    let entry = CachedPhotoEntry {
        path: std::path::PathBuf::from("/mock/test.raw"),
        photo_view: LoadedPhotoView {
            path: std::path::PathBuf::from("/mock/test.raw"),
            camera_make: "Mock".to_string(),
            camera_model: "Mock Z9".to_string(),
            lens_model: "Mock 50mm".to_string(),
            dimensions: (8256, 5504),
            iso: 100,
            shutter_str: "1/200 s".to_string(),
            aperture_str: "f/2.8".to_string(),
            focal_length_str: "50 mm".to_string(),
            date_time: "2026-11-23".to_string(),
            exposure_bias: "0.0 EV".to_string(),
            rating: 5,
            exif_tags: vec![],
        },
        base_preview: base,
        cfa,
        linear_rgb: None,
    };

    let footprint = entry.memory_footprint_bytes();
    // CFA: 4096 * 4 = 16384 bytes
    // RGB samples: 1024 * 12 = 12288 bytes
    // Semantic masks: 4 * 1024 * 4 = 16384 bytes
    // Total approx >= 40000 bytes
    assert!(footprint >= 40_000);
    Ok(())
}

#[test]
fn test_photo_cache_budget_eviction() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = rraw_app::RrawApp::new_headless();

    // Insert 25 mock photos (each exceeding default capacity or testing budget eviction)
    for i in 0..25 {
        let path = std::path::PathBuf::from(format!("/mock/photo_{i}.raw"));
        let raw_samples = vec![0.5_f32; 1024];
        let cfa = SignedCfaBuffer::new(32, 32, CfaPattern::Bayer(BayerOrigin::Rggb), raw_samples)?;
        let samples_rgb = Arc::new([[0.5_f32, 0.5, 0.5]; 256]);
        let base = BaseRgbPreview {
            width: 16,
            height: 16,
            samples_rgb,
            as_shot_wb: [1.0, 1.0, 1.0],
            cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
                width: 16,
                height: 16,
                subject: vec![0.0; 256],
                sky: vec![0.0; 256],
                background: vec![1.0; 256],
                portrait: vec![0.0; 256],
            }),
            exif_orientation: 1,
            matched_lens: None,
            focal_length_mm: None,
            aperture_f_number: None,
        };

        let entry = CachedPhotoEntry {
            path: path.clone(),
            photo_view: LoadedPhotoView {
                path,
                camera_make: "Mock".to_string(),
                camera_model: "Mock Camera".to_string(),
                lens_model: "50mm".to_string(),
                dimensions: (16, 16),
                iso: 100,
                shutter_str: "1/100".to_string(),
                aperture_str: "f/2.0".to_string(),
                focal_length_str: "50mm".to_string(),
                date_time: "2026-11-23".to_string(),
                exposure_bias: "0.0".to_string(),
                rating: 0,
                exif_tags: vec![],
            },
            base_preview: base,
            cfa,
            linear_rgb: None,
        };

        app.insert_cached_photo(entry);
    }

    // Must be capped at MAX_PHOTO_CACHE_ENTRIES (10)
    assert!(app.photo_cache.len() <= 10);
    // Most recent photo should be at the front
    assert_eq!(
        app.photo_cache[0].path,
        std::path::PathBuf::from("/mock/photo_24.raw")
    );
    Ok(())
}
