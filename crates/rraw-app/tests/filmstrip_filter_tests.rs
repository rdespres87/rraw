#![forbid(unsafe_code)]

use rraw_app::app::RrawApp;
use rraw_app::state::RatingFilter;
use std::path::{Path, PathBuf};

#[test]
fn test_matches_active_filter_raw_and_raster() {
    let mut app = RrawApp::new_headless();

    // With raw_only_filter = true, only genuine RAW formats match
    app.raw_only_filter = true;
    assert!(app.matches_active_filter(Path::new("photo.DNG")));
    assert!(app.matches_active_filter(Path::new("photo.dng")));
    assert!(app.matches_active_filter(Path::new("photo.arw")));
    assert!(app.matches_active_filter(Path::new("photo.nef")));
    assert!(app.matches_active_filter(Path::new("photo.cr3")));

    // Raster files must not match
    assert!(!app.matches_active_filter(Path::new("screenshot.png")));
    assert!(!app.matches_active_filter(Path::new("Capture d'écran.png")));
    assert!(!app.matches_active_filter(Path::new("export.jpg")));
    assert!(!app.matches_active_filter(Path::new("image.jpeg")));
    assert!(!app.matches_active_filter(Path::new("rendered.tiff")));

    // When raw_only_filter = false, both match
    app.raw_only_filter = false;
    assert!(app.matches_active_filter(Path::new("photo.DNG")));
    assert!(app.matches_active_filter(Path::new("screenshot.png")));
    assert!(app.matches_active_filter(Path::new("export.jpg")));
}

#[test]
fn test_first_filtered_entry_skips_non_raw() {
    let mut app = RrawApp::new_headless();

    app.directory_entries = vec![
        PathBuf::from("Capture d'écran 21.11.2026.png"),
        PathBuf::from("IMGP3879_export.jpg"),
        PathBuf::from("IMGP3879.DNG"),
        PathBuf::from("IMGP3898.DNG"),
    ];

    // Under raw_only_filter, the first entry should be the first RAW (skipping PNG and JPG)
    app.raw_only_filter = true;
    let first = app.first_filtered_entry();
    assert_eq!(first.as_deref(), Some(Path::new("IMGP3879.DNG")));

    // Without raw_only_filter, the first entry should be the first alphabetical file
    app.raw_only_filter = false;
    let first_all = app.first_filtered_entry();
    assert_eq!(
        first_all.as_deref(),
        Some(Path::new("Capture d'écran 21.11.2026.png"))
    );
}

#[test]
fn test_matches_active_filter_with_rating() {
    let mut app = RrawApp::new_headless();

    let dng_path = PathBuf::from("IMGP3879.DNG");
    let png_path = PathBuf::from("screenshot.png");

    app.photo_ratings.insert(dng_path.clone(), 3);
    app.photo_ratings.insert(png_path.clone(), 4);

    // Rating filter MinStars(3)
    app.rating_filter = RatingFilter::MinStars(3);
    app.raw_only_filter = true;

    // DNG has 3 stars and is RAW -> matches
    assert!(app.matches_active_filter(&dng_path));
    // PNG has 4 stars (>= 3) but is NOT RAW -> does NOT match under raw_only_filter
    assert!(!app.matches_active_filter(&png_path));

    // When raw_only_filter is disabled, PNG matches
    app.raw_only_filter = false;
    assert!(app.matches_active_filter(&png_path));

    // When rating filter is MinStars(5), neither matches
    app.rating_filter = RatingFilter::MinStars(5);
    assert!(!app.matches_active_filter(&dng_path));
    assert!(!app.matches_active_filter(&png_path));
}

#[test]
fn test_filter_switch_fallback_when_no_raw_match() {
    let mut app = RrawApp::new_headless();

    app.directory_entries = vec![
        PathBuf::from("screenshot1.png"),
        PathBuf::from("screenshot2.png"),
        PathBuf::from("photo.jpg"),
    ];

    app.raw_only_filter = true;
    assert_eq!(app.first_filtered_entry(), None);
}
