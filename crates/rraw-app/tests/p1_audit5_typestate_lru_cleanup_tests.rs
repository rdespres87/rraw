#![forbid(unsafe_code)]

use rraw_app::app::RrawApp;
use rraw_app::controllers::export::ActiveModalDialog;
use rraw_app::controllers::viewport::ActiveCanvasTool;
use rraw_app::i18n::Language;
use rraw_app::preview::LruThumbnailCache;
use rraw_app::state::ParallelToolPhase;
use rraw_app::ui::sections;
use std::path::{Path, PathBuf};

#[test]
fn test_p1_1_modal_dialog_close_and_escape_keep_active_dialog_synchronized() {
    let mut app = RrawApp::new_headless();

    // 1. Open About dialog via ExportBatchController::open_dialog
    app.export.open_dialog(ActiveModalDialog::About);
    assert_eq!(app.export.active_dialog, Some(ActiveModalDialog::About));
    assert!(app.export.show_about_dialog);

    // 2. Close About dialog via set_dialog_open(..., false)
    app.export.set_dialog_open(ActiveModalDialog::About, false);
    assert!(!app.export.show_about_dialog);
    assert_eq!(
        app.export.active_dialog, None,
        "closing About dialog must synchronize active_dialog to None"
    );

    // 3. Open Shortcuts dialog and dismiss via Escape key inside render_shortcuts_dialog
    app.export.open_dialog(ActiveModalDialog::Shortcuts);
    assert_eq!(app.export.active_dialog, Some(ActiveModalDialog::Shortcuts));

    let ctx = egui::Context::default();
    let mut raw_input = egui::RawInput::default();
    raw_input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let mut out = ctx.run_ui(raw_input, |ui| {
        app.render_shortcuts_dialog(ui.ctx());
    });
    out.textures_delta.clear();

    assert!(!app.export.show_shortcuts_dialog);
    assert_eq!(
        app.export.active_dialog, None,
        "dismissing Shortcuts window must synchronize active_dialog to None"
    );

    // 4. Open Export dialog and dismiss via RrawApp::handle_escape_key
    app.export.open_dialog(ActiveModalDialog::Export);
    assert_eq!(app.export.active_dialog, Some(ActiveModalDialog::Export));
    assert!(app.handle_escape_key());
    assert!(!app.export.show_export_dialog);
    assert_eq!(
        app.export.active_dialog, None,
        "handle_escape_key must synchronize active_dialog to None when dismissing Export dialog"
    );
}

#[test]
fn test_p1_2_inspector_canvas_tool_transitions_and_checkbox_deactivation() {
    let mut app = RrawApp::new_headless();

    // 1. Activate Horizon tool first
    app.activate_canvas_tool(ActiveCanvasTool::Horizon);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::Horizon);
    assert!(app.viewport.horizon_tool_active);

    // 2. Simulate Crop editing activated in inspector while Horizon was previously active
    app.viewport.is_crop_editing = true;
    app.reconcile_canvas_tool_transitions(false, true, ParallelToolPhase::Inactive, None);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::Crop);
    assert!(app.viewport.is_crop_editing);
    assert!(
        !app.viewport.horizon_tool_active,
        "activating Crop must automatically deactivate Horizon"
    );

    // 3. When geometry.crop_enabled is unchecked (false), rendering sections::crop::render
    // must automatically deactivate is_crop_editing
    app.session.grading.geometry.crop_enabled = false;
    app.viewport.is_crop_editing = true;
    let ctx = egui::Context::default();
    let mut out1 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let _ = sections::crop::render(
            ui,
            &mut app.session.grading.geometry,
            &mut app.viewport.is_crop_editing,
            Some((1600, 1200)),
            Language::En,
        );
    });
    out1.textures_delta.clear();
    assert!(
        !app.viewport.is_crop_editing,
        "disabling crop_enabled must clear is_crop_editing"
    );

    // 4. When geometry.geometry_enabled is unchecked (false), rendering sections::geometry::render
    // must automatically reset parallel_tool_phase to Inactive
    app.session.grading.geometry.geometry_enabled = false;
    app.viewport.parallel_tool_phase = ParallelToolPhase::DrawingLine1;
    let mut out2 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut tool_state = sections::geometry::ParallelLinesToolState {
            phase: &mut app.viewport.parallel_tool_phase,
            line1: &mut app.viewport.parallel_line1,
            line2: &mut app.viewport.parallel_line2,
            drag_start: &mut app.viewport.parallel_drag_start,
            status_message: &mut app.status_message,
        };
        let _ = sections::geometry::render(
            ui,
            &mut app.session.grading.geometry,
            &mut tool_state,
            Language::En,
        );
    });
    out2.textures_delta.clear();
    assert_eq!(
        app.viewport.parallel_tool_phase,
        ParallelToolPhase::Inactive,
        "disabling geometry_enabled must reset parallel_tool_phase to Inactive"
    );
}

#[test]
fn test_p1_3_lru_thumbnail_cache_get_and_touch_evicts_least_recently_used() {
    let mut cache = LruThumbnailCache::new(3);
    let p1 = PathBuf::from("a.dng");
    let p2 = PathBuf::from("b.dng");
    let p3 = PathBuf::from("c.dng");
    let p4 = PathBuf::from("d.dng");

    cache.insert_mock(p1.clone());
    cache.insert_mock(p2.clone());
    cache.insert_mock(p3.clone());
    assert_eq!(cache.len(), 3);

    // Accessing p1 via get_and_touch marks p1 as most recently used, making p2 the LRU victim
    let _ = cache.get_and_touch(Path::new("a.dng"));

    // Inserting p4 must evict p2 (least recently used), NOT p1
    cache.insert_mock(p4.clone());
    assert_eq!(cache.len(), 3);
    assert!(
        cache.contains_key(&p1),
        "p1 was touched via get_and_touch and must not be evicted"
    );
    assert!(
        !cache.contains_key(&p2),
        "p2 was the least recently used entry and must be evicted"
    );
    assert!(cache.contains_key(&p3));
    assert!(cache.contains_key(&p4));
}
