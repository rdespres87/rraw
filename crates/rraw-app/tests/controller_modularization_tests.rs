//! TDD integration tests for Task 16.2:
//! Architectural Decomposition of `RrawApp` into Specialized Domain Controllers (`rraw-app`).

use rraw_app::controllers::{
    BackgroundWorkers, ExportBatchController, LibraryController, PhotoSession, ViewportController,
};
use std::path::PathBuf;

#[test]
fn test_photo_session_independent_lifecycle() {
    let mut session = PhotoSession::default();
    assert!(!session.is_loaded());
    assert_eq!(session.active_path(), None);
    assert!(!session.needs_preview_update);
    assert!(!session.is_comparing_original);

    // Mark dirty triggers preview update flag
    session.mark_dirty();
    assert!(session.needs_preview_update);

    // Untouched raw grading reset
    session.grading.tone.exposure_ev = 2.5;
    session.reset_to_untouched();
    assert_eq!(session.grading.tone.exposure_ev, 0.0);
}

#[test]
fn test_viewport_controller_independent_operations() {
    let mut vp = ViewportController::default();
    assert!(vp.is_zoom_fit());
    assert_eq!(vp.zoom_level, 0.0);
    assert_eq!(vp.pan_offset, egui::Vec2::ZERO);
    assert!(!vp.split_compare);

    // Toggle split compare
    vp.toggle_split_compare();
    assert!(vp.split_compare);
    vp.toggle_split_compare();
    assert!(!vp.split_compare);

    // Zoom and pan manipulations
    vp.zoom_level = 1.0;
    vp.pan_offset = egui::Vec2::new(50.0, 100.0);
    assert!(!vp.is_zoom_fit());

    vp.reset_view();
    assert!(vp.is_zoom_fit());
    assert_eq!(vp.pan_offset, egui::Vec2::ZERO);
}

#[test]
fn test_library_controller_selection_and_counting() {
    let mut lib = LibraryController::default();
    assert!(lib.directory_entries.is_empty());
    assert!(lib.selected_photos.is_empty());

    let p1 = PathBuf::from("/photos/img1.CR2");
    let p2 = PathBuf::from("/photos/img2.CR2");
    let p3 = PathBuf::from("/photos/img3.JPG");

    lib.directory_entries = vec![p1.clone(), p2.clone(), p3.clone()];
    lib.photo_ratings.insert(p1.clone(), 5);
    lib.photo_ratings.insert(p2.clone(), 3);
    lib.photo_ratings.insert(p3.clone(), 0);

    assert_eq!(lib.count_photos_with_min_rating(4), 1);
    assert_eq!(lib.count_photos_with_min_rating(3), 2);
    assert_eq!(lib.count_photos_unrated(), 1);

    // Multi-selection operations
    lib.selected_photos.insert(p1.clone());
    lib.selected_photos.insert(p2.clone());
    assert!(lib.is_selected(&p1));
    assert!(lib.is_selected(&p2));
    assert!(!lib.is_selected(&p3));

    lib.clear_selection();
    assert!(lib.selected_photos.is_empty());
    assert!(!lib.is_selected(&p1));
}

#[test]
fn test_export_batch_controller_dialog_state() {
    let mut exp = ExportBatchController::default();
    assert!(!exp.is_any_dialog_open());

    exp.show_export_dialog = true;
    assert!(exp.is_any_dialog_open());

    exp.show_shortcuts_dialog = true;
    exp.show_about_dialog = true;
    assert!(exp.is_any_dialog_open());

    exp.close_all_dialogs();
    assert!(!exp.is_any_dialog_open());
    assert!(!exp.show_export_dialog);
    assert!(!exp.show_shortcuts_dialog);
    assert!(!exp.show_about_dialog);
}

#[test]
fn test_background_workers_headless_initialization() {
    let mut workers = BackgroundWorkers::new_headless();
    assert!(!workers.is_decoding);
    assert!(workers.thumbnail_pending.is_empty());
    assert_eq!(workers.thumbnail_textures.len(), 0);

    let p = PathBuf::from("/photos/sample.CR2");
    workers.request_thumbnail(p.clone());
    assert!(workers.thumbnail_pending.contains(&p));

    // Requesting same thumbnail again must be de-duplicated
    workers.request_thumbnail(p.clone());
    assert_eq!(workers.thumbnail_pending.len(), 1);
}
