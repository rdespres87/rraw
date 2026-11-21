use rraw_app::state::AppLayoutConfig;
use std::path::PathBuf;

#[test]
fn test_workflow_session_persistence_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AppLayoutConfig::default();
    assert_eq!(config.last_open_directory, None);
    assert_eq!(config.last_open_photo, None);

    let test_dir = PathBuf::from("/tmp/rraw_test_workflow_dir");
    let test_photo = PathBuf::from("/tmp/rraw_test_workflow_dir/sample_photo.dng");

    config.last_open_directory = Some(test_dir.clone());
    config.last_open_photo = Some(test_photo.clone());

    let json = serde_json::to_string(&config)?;
    let restored: AppLayoutConfig = serde_json::from_str(&json)?;

    assert_eq!(restored.last_open_directory, Some(test_dir));
    assert_eq!(restored.last_open_photo, Some(test_photo));
    Ok(())
}

#[test]
fn test_workflow_session_legacy_json_backward_compatibility(
) -> Result<(), Box<dyn std::error::Error>> {
    let legacy_json = r#"{
        "section_order": [],
        "active_tool_family": "All",
        "active_tools_only": false,
        "filmstrip_visible": true,
        "show_canvas_hud": true,
        "language": "fr"
    }"#;

    let restored: AppLayoutConfig = serde_json::from_str(legacy_json)?;
    assert_eq!(restored.last_open_directory, None);
    assert_eq!(restored.last_open_photo, None);
    Ok(())
}

#[test]
fn test_workflow_session_headless_state_tracking() {
    let mut app = rraw_app::RrawApp::new_headless();
    assert_eq!(app.layout_config.last_open_directory, None);
    assert_eq!(app.layout_config.last_open_photo, None);

    let temp_dir = std::env::temp_dir().join("rraw_test_dir_tracking");
    let _ = std::fs::create_dir_all(&temp_dir);

    app.open_directory(&temp_dir);
    assert_eq!(
        app.layout_config.last_open_directory,
        Some(temp_dir.clone())
    );

    // Clean up temp directory
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_workflow_session_open_photo_updates_directory_and_photo() {
    let mut app = rraw_app::RrawApp::new_headless();

    let temp_dir = std::env::temp_dir().join("rraw_test_photo_session");
    let _ = std::fs::create_dir_all(&temp_dir);
    let sample_file = temp_dir.join("test_frame.dng");
    let _ = std::fs::write(&sample_file, b"DNG_TEST_FIXTURE_DATA");

    app.open_raw_file(&sample_file);
    assert_eq!(app.layout_config.last_open_photo, Some(sample_file.clone()));
    assert_eq!(
        app.layout_config.last_open_directory,
        Some(temp_dir.clone())
    );

    // Clean up
    let _ = std::fs::remove_file(&sample_file);
    let _ = std::fs::remove_dir_all(&temp_dir);
}
