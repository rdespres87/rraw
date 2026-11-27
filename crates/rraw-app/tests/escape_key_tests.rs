#![forbid(unsafe_code)]

use rraw_app::app::RrawApp;
use rraw_app::state::{CropDragHandle, ParallelToolPhase};
use std::path::PathBuf;

#[test]
fn test_escape_dismisses_shortcuts_dialog_first() {
    let mut app = RrawApp::new_headless();
    app.export.show_shortcuts_dialog = true;
    app.export.show_export_dialog = true;
    app.viewport.crop.is_editing = true;
    app.library
        .selected_photos
        .insert(PathBuf::from("/test/photo.dng"));

    // Press Escape -> Should dismiss shortcuts dialog first
    let dismissed = app.handle_escape_key();
    assert!(dismissed);
    assert!(!app.export.show_shortcuts_dialog);
    // Other states must remain untouched
    assert!(app.export.show_export_dialog);
    assert!(app.viewport.crop.is_editing);
    assert_eq!(app.library.selected_photos.len(), 1);

    // Press Escape again -> Should dismiss export dialog
    let dismissed2 = app.handle_escape_key();
    assert!(dismissed2);
    assert!(!app.export.show_export_dialog);
    assert!(app.viewport.crop.is_editing);
    assert_eq!(app.library.selected_photos.len(), 1);

    // Press Escape again -> Should cancel crop editing
    let dismissed3 = app.handle_escape_key();
    assert!(dismissed3);
    assert!(!app.viewport.crop.is_editing);
    assert_eq!(app.viewport.crop.drag_handle, CropDragHandle::None);
    assert_eq!(app.library.selected_photos.len(), 1);

    // Press Escape again -> Should clear selected photos
    let dismissed4 = app.handle_escape_key();
    assert!(dismissed4);
    assert!(app.library.selected_photos.is_empty());

    // Press Escape again -> Nothing to dismiss, returns false
    let dismissed5 = app.handle_escape_key();
    assert!(!dismissed5);
}

#[test]
fn test_escape_cancels_perspective_and_brush_tools() {
    let mut app = RrawApp::new_headless();
    app.viewport.geometry_tools.parallel_phase = ParallelToolPhase::DrawingLine1;
    app.library
        .selected_photos
        .insert(PathBuf::from("/test/photo.dng"));

    let dismissed = app.handle_escape_key();
    assert!(dismissed);
    assert_eq!(
        app.viewport.geometry_tools.parallel_phase,
        ParallelToolPhase::Inactive
    );
    assert_eq!(app.library.selected_photos.len(), 1);

    app.session.grading.masks.active_brush_mask_idx = Some(0);
    let dismissed_brush = app.handle_escape_key();
    assert!(dismissed_brush);
    assert!(app.session.grading.masks.active_brush_mask_idx.is_none());
    assert_eq!(app.library.selected_photos.len(), 1);

    let dismissed_sel = app.handle_escape_key();
    assert!(dismissed_sel);
    assert!(app.library.selected_photos.is_empty());
}

#[test]
fn test_render_shortcuts_dialog_dismisses_on_escape() {
    let mut app = RrawApp::new_headless();
    app.export.show_shortcuts_dialog = true;

    let ctx = egui::Context::default();

    // 1. First frame without Escape key -> dialog stays open
    let mut out1 = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.render_shortcuts_dialog(ui.ctx());
    });
    out1.textures_delta.clear();
    assert!(app.export.show_shortcuts_dialog);

    // 2. Second frame with Escape key press event -> dialog closes
    let mut raw_input = egui::RawInput::default();
    raw_input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });

    let mut out2 = ctx.run_ui(raw_input, |ui| {
        app.render_shortcuts_dialog(ui.ctx());
    });
    out2.textures_delta.clear();
    assert!(!app.export.show_shortcuts_dialog);
}

#[test]
fn test_render_export_dialog_dismisses_on_escape() {
    let mut app = RrawApp::new_headless();
    app.export.show_export_dialog = true;

    let ctx = egui::Context::default();

    // 1. First frame without Escape key -> dialog stays open
    let mut out1 = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.render_export_dialog(ui.ctx());
    });
    out1.textures_delta.clear();
    assert!(app.export.show_export_dialog);

    // 2. Second frame with Escape key press event -> dialog closes
    let mut raw_input = egui::RawInput::default();
    raw_input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });

    let mut out2 = ctx.run_ui(raw_input, |ui| {
        app.render_export_dialog(ui.ctx());
    });
    out2.textures_delta.clear();
    assert!(!app.export.show_export_dialog);
}
