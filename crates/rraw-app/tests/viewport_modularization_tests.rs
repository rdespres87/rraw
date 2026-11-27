#![forbid(unsafe_code)]

//! TDD and regression test suite for modular viewport decomposition.
//!
//! Validates:
//! 1. Viewport submodules (`hud`, `split_compare`, `crop_overlay`, `perspective_tools`, `brush_overlay`).
//! 2. Contract behavior and state mutation with zero panic/unwrap.
//! 3. Clean isolation and modularity of vector tools and HUD overlays.

use egui::{Context, Pos2, Rect, Vec2};
use rraw_app::grading::GeometryGradingParams;
use rraw_app::i18n::Language;
use rraw_app::state::{CropDragHandle, ParallelToolPhase};
use rraw_app::ui::sections::geometry::ParallelLinesToolState;
use rraw_app::ui::viewport::{brush_overlay, crop_overlay, hud, perspective_tools, split_compare};
use rraw_color::SoftProofConfig;

fn with_test_ui<F: FnMut(&mut egui::Ui)>(mut f: F) {
    let ctx = Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_min_size(Vec2::new(800.0, 600.0));
        f(ui);
    });
    out.textures_delta.clear();
}

#[test]
fn test_hud_original_badge() {
    with_test_ui(|ui| {
        let available_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        hud::render_original_badge(ui.painter(), available_rect, "ORIGINAL (RÉFÉRENCE)");
    });
}

#[test]
fn test_hud_soft_proofing() {
    with_test_ui(|ui| {
        let available_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let config = SoftProofConfig {
            enabled: true,
            simulate_paper_and_ink: true,
            show_gamut_warning: true,
            ..Default::default()
        };
        hud::render_soft_proofing_hud(ui.painter(), available_rect, &config, 42, Language::Fr);
        hud::render_soft_proofing_hud(ui.painter(), available_rect, &config, 42, Language::En);
    });
}

#[test]
fn test_split_compare_divider() {
    with_test_ui(|ui| {
        let image_rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let mut split = 0.5f32;
        split_compare::render_split_divider(ui, image_rect, &mut split, "Avant", "Après");
        assert!((split - 0.5).abs() < f32::EPSILON);
    });
}

#[test]
fn test_crop_overlay_handles_inactive() {
    with_test_ui(|ui| {
        let available_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let preview_rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(600.0, 400.0));
        let resp = ui.interact(
            available_rect,
            ui.auto_id_with("viewport_canvas"),
            egui::Sense::hover(),
        );
        let painter = ui.painter().clone();

        let mut geom = GeometryGradingParams::default();
        let mut is_crop_editing = true;
        let mut crop_drag_handle = CropDragHandle::None;
        let mut tool_state = crop_overlay::CropToolState {
            geometry: &mut geom,
            is_crop_editing: &mut is_crop_editing,
            crop_drag_handle: &mut crop_drag_handle,
            crop_show_grid: true,
        };

        let tool_ctx = rraw_app::ui::viewport::ViewportToolContext {
            resp: &resp,
            painter: &painter,
            available_rect,
            preview_rect,
            language: Language::Fr,
        };
        let out = crop_overlay::handle_crop_overlay(ui, &tool_ctx, &mut tool_state);
        assert!(!out.needs_preview_update);
        assert!(!out.save_sidecar);
    });
}

#[test]
fn test_perspective_tools_inactive() {
    with_test_ui(|ui| {
        let available_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let resp = ui.interact(
            available_rect,
            ui.auto_id_with("viewport_canvas"),
            egui::Sense::hover(),
        );
        let painter = ui.painter().clone();

        let mut phase = ParallelToolPhase::Inactive;
        let mut line1 = None;
        let mut line2 = None;
        let mut drag_start = None;
        let mut status_message = String::new();
        let mut tool_state = ParallelLinesToolState {
            phase: &mut phase,
            line1: &mut line1,
            line2: &mut line2,
            drag_start: &mut drag_start,
            status_message: &mut status_message,
        };
        let mut geom = GeometryGradingParams::default();
        let tool_ctx = rraw_app::ui::viewport::ViewportToolContext {
            resp: &resp,
            painter: &painter,
            available_rect,
            preview_rect: available_rect,
            language: Language::Fr,
        };

        let out = perspective_tools::handle_parallel_lines_tool(
            ui,
            &tool_ctx,
            &mut tool_state,
            &mut geom,
        );
        assert!(!out.mark_custom_preset);
        assert!(!out.needs_preview_update);
    });
}

#[test]
fn test_brush_painting_no_pointer() {
    with_test_ui(|ui| {
        let available_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let preview_rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(600.0, 400.0));
        let resp = ui.interact(
            available_rect,
            ui.auto_id_with("viewport_canvas"),
            egui::Sense::hover(),
        );
        let geom = GeometryGradingParams::default();
        let mut masks = vec![];
        let ctx = brush_overlay::BrushViewportContext {
            available_rect,
            preview_rect,
            is_panning_override: false,
            is_cropped_view: false,
        };

        let (needs_update, save_sidecar) =
            brush_overlay::handle_brush_painting(ui, &resp, &ctx, &geom, &mut masks, None);
        assert!(!needs_update);
        assert!(!save_sidecar);
    });
}
