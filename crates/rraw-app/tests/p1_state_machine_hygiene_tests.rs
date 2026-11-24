#![forbid(unsafe_code)]

use rraw_app::app::RrawApp;
use rraw_app::controllers::export::ActiveModalDialog;
use rraw_app::controllers::viewport::ActiveCanvasTool;
use rraw_app::state::{
    DarkroomGradingState, ExportTargetMode, LeftPanelTab, OpticalCorrectionTab, ParallelToolPhase,
    SidecarRecipe, ToolFamily,
};

#[test]
fn test_p1_1_ephemeral_optics_ui_fields_excluded_from_sidecar_json(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut grading = DarkroomGradingState::default();
    grading.optics.lens_search_query = "Sigma 35mm Art".to_string();
    grading.optics.defringe_preview_mask = true;
    grading.optics.defringe_threshold = 0.14;
    grading.optics.optical_tab = OpticalCorrectionTab::Manual;

    let recipe = SidecarRecipe::from_state(&grading, 4);
    let serialized = serde_json::to_string_pretty(&recipe)?;

    assert!(
        !serialized.contains("lens_search_query"),
        "lens_search_query is an ephemeral UI field and must be excluded from sidecar JSON"
    );
    assert!(
        !serialized.contains("defringe_preview_mask"),
        "defringe_preview_mask is an ephemeral UI field and must be excluded from sidecar JSON"
    );
    assert!(
        serialized.contains("defringe_threshold"),
        "defringe_threshold is a persistent grading parameter and must be serialized"
    );

    let restored_recipe: SidecarRecipe = serde_json::from_str(&serialized)?;
    let mut restored = DarkroomGradingState::default();
    restored_recipe.apply_to(&mut restored);

    assert!(
        restored.optics.lens_search_query.is_empty(),
        "restored lens_search_query must be empty"
    );
    assert!(
        !restored.optics.defringe_preview_mask,
        "restored defringe_preview_mask must be false"
    );
    assert!(
        (restored.optics.defringe_threshold - 0.14).abs() < 1e-4,
        "restored defringe_threshold must round-trip to ~0.14, got {}",
        restored.optics.defringe_threshold
    );

    Ok(())
}

#[test]
fn test_p1_2_active_modal_dialog_exclusivity_and_target_mode_sync() {
    use rraw_app::controllers::ExportBatchController;

    let mut exp = ExportBatchController::default();

    exp.open_dialog(ActiveModalDialog::Export);
    assert_eq!(exp.active_modal_dialog(), Some(ActiveModalDialog::Export));
    assert!(exp.show_export_dialog);
    assert!(!exp.show_shortcuts_dialog);
    assert!(!exp.show_about_dialog);

    exp.open_dialog(ActiveModalDialog::Shortcuts);
    assert_eq!(
        exp.active_modal_dialog(),
        Some(ActiveModalDialog::Shortcuts)
    );
    assert!(!exp.show_export_dialog);
    assert!(exp.show_shortcuts_dialog);

    exp.toggle_dialog(ActiveModalDialog::Shortcuts);
    assert_eq!(exp.active_modal_dialog(), None);
    assert!(!exp.show_shortcuts_dialog);

    exp.set_target_mode(ExportTargetMode::BatchSelection);
    assert_eq!(exp.config.target_mode, ExportTargetMode::BatchSelection);
    assert!(exp.config.batch_selection);

    exp.set_target_mode(ExportTargetMode::CurrentPhoto);
    assert_eq!(exp.config.target_mode, ExportTargetMode::CurrentPhoto);
    assert!(!exp.config.batch_selection);
}

#[test]
fn test_p1_3_interactive_canvas_tools_mutual_exclusion() {
    let mut app = RrawApp::new_headless();

    app.activate_canvas_tool(ActiveCanvasTool::Crop);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::Crop);
    assert!(app.viewport.is_crop_editing);

    app.activate_canvas_tool(ActiveCanvasTool::Horizon);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::Horizon);
    assert!(!app.viewport.is_crop_editing);
    assert!(app.viewport.horizon_tool_active);

    app.activate_canvas_tool(ActiveCanvasTool::ParallelLines(
        ParallelToolPhase::DrawingLine1,
    ));
    assert!(!app.viewport.horizon_tool_active);
    assert_eq!(
        app.viewport.parallel_tool_phase,
        ParallelToolPhase::DrawingLine1
    );

    app.activate_canvas_tool(ActiveCanvasTool::FreehandBrush(2));
    assert_eq!(
        app.viewport.parallel_tool_phase,
        ParallelToolPhase::Inactive
    );
    assert_eq!(app.session.grading.masks.active_brush_mask_idx, Some(2));
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::FreehandBrush(2));

    app.activate_canvas_tool(ActiveCanvasTool::None);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::None);
    assert!(app.session.grading.masks.active_brush_mask_idx.is_none());
}

#[test]
fn test_p1_4_layout_config_strict_synchronization_with_app_state() {
    let mut app = RrawApp::new_headless();

    app.set_left_panel_tab(LeftPanelTab::Exif);
    assert_eq!(app.left_panel_tab, LeftPanelTab::Exif);
    assert_eq!(app.layout_config.left_panel_tab, LeftPanelTab::Exif);

    app.set_active_tool_family(ToolFamily::Detail);
    assert_eq!(app.active_tool_family, ToolFamily::Detail);
    assert_eq!(app.layout_config.active_tool_family, ToolFamily::Detail);

    app.set_active_tools_only(true);
    assert!(app.active_tools_only);
    assert!(app.layout_config.active_tools_only);
}
