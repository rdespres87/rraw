#![forbid(unsafe_code)]

//! Mouse pan, zoom, brush-radius keyboard shortcuts, and before/after hold interaction for the viewport.

use crate::controllers::viewport::ViewportController;
use crate::state::{CropDragHandle, DarkroomGradingState, LocalMaskKind};

use super::canvas::ViewportLayout;

/// Clamps `pan_offset` so the zoomed image never drifts outside `available_rect`.
#[must_use]
pub fn clamp_pan_offset(
    pan_offset: egui::Vec2,
    display_size: egui::Vec2,
    available_rect: egui::Rect,
    margin: f32,
    zoom_level: f32,
) -> egui::Vec2 {
    if zoom_level <= 0.0 {
        return egui::Vec2::ZERO;
    }
    let max_pan_x = ((display_size.x - available_rect.width() + margin * 2.0) * 0.5).max(0.0);
    let max_pan_y = ((display_size.y - available_rect.height() + margin * 2.0) * 0.5).max(0.0);
    egui::vec2(
        pan_offset.x.clamp(-max_pan_x, max_pan_x),
        pan_offset.y.clamp(-max_pan_y, max_pan_y),
    )
}

/// Computes the next `(zoom_level, pan_offset)` after a smooth vertical scroll wheel event.
#[must_use]
pub fn compute_scroll_zoom(
    current_scale: f32,
    fit_scale: f32,
    scroll_y: f32,
    hover_pos: Option<egui::Pos2>,
    center: egui::Pos2,
    pan_offset: egui::Vec2,
) -> (f32, egui::Vec2) {
    let zoom_factor = (scroll_y * 0.003).exp();
    let min_scale = fit_scale;
    let max_scale = (fit_scale * 20.0).max(10.0);
    let new_scale = (current_scale * zoom_factor).clamp(min_scale, max_scale);

    if (new_scale - fit_scale).abs() / fit_scale < 0.015 || new_scale <= fit_scale {
        (0.0, egui::Vec2::ZERO)
    } else {
        let mut next_pan = pan_offset;
        if let Some(mouse_pos) = hover_pos {
            let old_offset = mouse_pos - center;
            let scale_change = new_scale / current_scale;
            next_pan -= old_offset * (scale_change - 1.0);
        }
        (new_scale, next_pan)
    }
}

/// Handles `[` and `]` keyboard shortcuts to resize the active freehand brush mask radius.
pub fn handle_brush_radius_shortcuts(ui: &egui::Ui, grading: &mut DarkroomGradingState) {
    let Some(mask_idx) = grading.masks.active_brush_mask_idx else {
        return;
    };
    if ui.input(|i| i.key_pressed(egui::Key::OpenBracket)) {
        if let Some(mask) = grading.masks.local_masks.get_mut(mask_idx) {
            if let LocalMaskKind::FreehandBrush { brush_radius, .. } = &mut mask.kind {
                let step = if *brush_radius > 0.05 {
                    0.01
                } else if *brush_radius > 0.01 {
                    0.002
                } else {
                    0.001
                };
                *brush_radius = (*brush_radius - step).max(0.001);
                ui.ctx().request_repaint();
            }
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::CloseBracket)) {
        if let Some(mask) = grading.masks.local_masks.get_mut(mask_idx) {
            if let LocalMaskKind::FreehandBrush { brush_radius, .. } = &mut mask.kind {
                let step = if *brush_radius >= 0.05 {
                    0.01
                } else if *brush_radius >= 0.01 {
                    0.002
                } else {
                    0.001
                };
                *brush_radius = (*brush_radius + step).min(0.5);
                ui.ctx().request_repaint();
            }
        }
    }
}

/// Read-only layout and tool context for viewport pan/zoom interaction.
pub struct PanZoomContext<'a> {
    pub available_rect: egui::Rect,
    pub layout: &'a ViewportLayout,
    pub is_tool_active: bool,
    pub is_brush_active: bool,
}

/// Handles viewport pan clamping, drag panning, double-click 100%/fit toggle, and wheel zoom.
///
/// Returns `true` when Space or Middle-mouse panning override is active.
pub fn handle_pan_and_zoom(
    ui: &egui::Ui,
    resp: &egui::Response,
    ctx: &PanZoomContext<'_>,
    viewport: &mut ViewportController,
) -> bool {
    let is_holding_space = ui.input(|i| i.key_down(egui::Key::Space));
    let is_middle_drag = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
    let is_panning_override =
        (is_holding_space || is_middle_drag) && viewport.camera.zoom_level > 0.0;

    viewport.camera.pan_offset = clamp_pan_offset(
        viewport.camera.pan_offset,
        ctx.layout.display_size,
        ctx.available_rect,
        ctx.layout.margin,
        viewport.camera.zoom_level,
    );

    if resp.dragged()
        && (!ctx.is_tool_active || is_panning_override)
        && viewport.crop.drag_handle == CropDragHandle::None
        && viewport.camera.zoom_level > 0.0
    {
        viewport.camera.pan_offset = clamp_pan_offset(
            viewport.camera.pan_offset + resp.drag_delta(),
            ctx.layout.display_size,
            ctx.available_rect,
            ctx.layout.margin,
            viewport.camera.zoom_level,
        );
    }

    if resp.double_clicked() && !viewport.crop.is_editing && !ctx.is_tool_active {
        viewport.camera.zoom_level = if viewport.camera.zoom_level <= 0.0 {
            1.0
        } else {
            0.0
        };
        viewport.camera.pan_offset = egui::Vec2::ZERO;
    }

    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll.abs() > 0.0
        && resp.hovered()
        && (!ctx.is_tool_active || ctx.is_brush_active)
        && !viewport.crop.is_editing
    {
        let (next_zoom, next_pan) = compute_scroll_zoom(
            ctx.layout.current_scale,
            ctx.layout.fit_scale,
            scroll,
            resp.hover_pos(),
            ctx.layout.center,
            viewport.camera.pan_offset,
        );
        viewport.camera.zoom_level = next_zoom;
        viewport.camera.pan_offset = next_pan;
    }

    is_panning_override
}

/// Context flags used to resolve whether the unprocessed original preview should be shown.
#[derive(Debug, Clone, Copy)]
pub struct OriginalPreviewContext {
    pub available_rect: egui::Rect,
    pub preview_rect: egui::Rect,
    pub is_comparing_original: bool,
    pub is_any_dialog_open: bool,
    pub is_tool_active: bool,
}

/// Evaluates whether the user is currently requesting the unprocessed original preview.
pub fn resolve_show_original(
    ui: &egui::Ui,
    viewport: &ViewportController,
    ctx: &OriginalPreviewContext,
) -> bool {
    let is_holding_right_click = ui.input(|i| {
        i.pointer.button_down(egui::PointerButton::Secondary)
            && i.pointer
                .interact_pos()
                .is_some_and(|p| ctx.available_rect.contains(p))
    });

    let is_holding_shortcut_key = !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| {
            i.key_down(egui::Key::P)
                || i.key_down(egui::Key::Backslash)
                || (i.key_down(egui::Key::D) && !i.modifiers.command && !i.modifiers.ctrl)
        });

    let is_holding_left_fit = !viewport.split.active
        && !ctx.is_any_dialog_open
        && !ui.ctx().egui_wants_pointer_input()
        && ui.input(|i| {
            i.pointer.button_down(egui::PointerButton::Primary)
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| ctx.preview_rect.contains(p))
        })
        && !ctx.is_tool_active
        && viewport.crop.drag_handle == CropDragHandle::None
        && viewport.camera.zoom_level <= 0.0;

    ctx.is_comparing_original
        || is_holding_left_fit
        || is_holding_right_click
        || is_holding_shortcut_key
}

#[cfg(test)]
mod tests {
    use super::{clamp_pan_offset, compute_scroll_zoom};

    #[test]
    fn clamp_pan_offset_resets_at_fit_zoom() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let clamped = clamp_pan_offset(
            egui::vec2(120.0, -80.0),
            egui::vec2(800.0, 600.0),
            rect,
            16.0,
            0.0,
        );
        assert_eq!(clamped, egui::Vec2::ZERO);
    }

    #[test]
    fn scroll_zoom_snaps_back_to_fit_when_zooming_out() {
        let (zoom, pan) = compute_scroll_zoom(
            0.5,
            0.5,
            -100.0,
            Some(egui::pos2(400.0, 300.0)),
            egui::pos2(400.0, 300.0),
            egui::vec2(20.0, 10.0),
        );
        assert_eq!(zoom, 0.0);
        assert_eq!(pan, egui::Vec2::ZERO);
    }
}
