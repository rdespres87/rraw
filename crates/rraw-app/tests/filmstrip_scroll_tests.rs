#![forbid(unsafe_code)]

use rraw_app::app::RrawApp;
use std::path::PathBuf;

#[test]
fn test_scroll_filmstrip_to_active_initially_false() {
    let app = RrawApp::new_headless();
    assert!(
        !app.library.scroll_filmstrip_to_active,
        "scroll_filmstrip_to_active flag must be false initially"
    );
}

#[test]
fn test_scroll_filmstrip_to_active_after_navigate_relative() {
    let mut app = RrawApp::new_headless();
    let base_dir = PathBuf::from("/virtual/photos");
    app.library.current_directory = Some(base_dir.clone());
    app.library.directory_entries = vec![
        base_dir.join("photo0.dng"),
        base_dir.join("photo1.dng"),
        base_dir.join("photo2.dng"),
    ];

    app.navigate_relative(1);
    assert!(
        app.library.scroll_filmstrip_to_active,
        "navigate_relative(1) must trigger scroll_filmstrip_to_active"
    );
    assert_eq!(
        app.layout_config.last_open_photo.as_deref(),
        Some(base_dir.join("photo1.dng").as_path())
    );

    app.library.scroll_filmstrip_to_active = false;
    app.navigate_relative(-1);
    assert!(
        app.library.scroll_filmstrip_to_active,
        "navigate_relative(-1) must trigger scroll_filmstrip_to_active"
    );
    assert_eq!(
        app.layout_config.last_open_photo.as_deref(),
        Some(base_dir.join("photo0.dng").as_path())
    );
}

#[test]
fn test_scroll_filmstrip_to_active_after_navigate_to_filtered_index() {
    let mut app = RrawApp::new_headless();
    let base_dir = PathBuf::from("/virtual/photos");
    app.library.current_directory = Some(base_dir.clone());
    app.library.directory_entries = vec![
        base_dir.join("photo0.dng"),
        base_dir.join("photo1.dng"),
        base_dir.join("photo2.dng"),
    ];

    app.navigate_to_filtered_index(2, false);
    assert!(
        app.library.scroll_filmstrip_to_active,
        "navigate_to_filtered_index must trigger scroll_filmstrip_to_active"
    );
    assert_eq!(
        app.layout_config.last_open_photo.as_deref(),
        Some(base_dir.join("photo2.dng").as_path())
    );
}

#[test]
fn test_consecutive_navigate_relative_while_decoding_preserves_position() {
    let mut app = RrawApp::new_headless();
    let base_dir = PathBuf::from("/virtual/photos");
    app.library.current_directory = Some(base_dir.clone());
    app.library.directory_entries = vec![
        base_dir.join("p0.dng"),
        base_dir.join("p1.dng"),
        base_dir.join("p2.dng"),
        base_dir.join("p3.dng"),
        base_dir.join("p4.dng"),
    ];

    // Navigate to p1
    app.navigate_relative(1);
    assert_eq!(
        app.layout_config.last_open_photo.as_deref(),
        Some(base_dir.join("p1.dng").as_path())
    );

    // Simulate background decoding in progress: loaded_photo is None
    app.session.loaded_photo = None;

    // Navigate again immediately (user tapping Right arrow quickly)
    app.navigate_relative(1);
    assert_eq!(
        app.layout_config.last_open_photo.as_deref(),
        Some(base_dir.join("p2.dng").as_path()),
        "Second navigation step must advance to p2, not fall back to p0+1=p1"
    );

    // Navigate backward immediately while still decoding
    app.session.loaded_photo = None;
    app.navigate_relative(-1);
    assert_eq!(
        app.layout_config.last_open_photo.as_deref(),
        Some(base_dir.join("p1.dng").as_path()),
        "Backward navigation while decoding must retreat to p1"
    );
}
