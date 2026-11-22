#![forbid(unsafe_code)]

//! Interactive brush painting dabs and cursor ring indicator.

use egui::{Color32, Painter, Rect, Response, Stroke, Ui};

use crate::grading::GeometryGradingParams;
use crate::state::{LocalMaskKind, LocalMaskLayer};

/// Viewport and interaction context for brush painting and cursor overlay.
#[derive(Debug, Clone, Copy)]
pub struct BrushViewportContext {
    pub available_rect: Rect,
    pub preview_rect: Rect,
    pub is_panning_override: bool,
    pub is_cropped_view: bool,
}

/// Handles mouse painting/erasing interaction on active freehand brush mask.
pub fn handle_brush_painting(
    ui: &Ui,
    resp: &Response,
    ctx: &BrushViewportContext,
    geometry: &GeometryGradingParams,
    local_masks: &mut [LocalMaskLayer],
    active_brush_mask_idx: Option<usize>,
) -> (bool, bool) {
    let available_rect = ctx.available_rect;
    let preview_rect = ctx.preview_rect;
    let is_panning_override = ctx.is_panning_override;
    let is_cropped_view = ctx.is_cropped_view;
    let mut needs_preview_update = false;
    let mut save_sidecar = false;

    let is_primary_down = ui.input(|i| i.pointer.primary_down());
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());

    if is_primary_down && !is_panning_override {
        if let Some(p) = pointer_pos {
            if available_rect.contains(p) && preview_rect.contains(p) {
                let pw = preview_rect.width();
                let ph = preview_rect.height();
                if pw > 1.0 && ph > 1.0 {
                    let frac_x = (p.x - preview_rect.min.x) / pw;
                    let frac_y = (p.y - preview_rect.min.y) / ph;
                    let (u, v) = if is_cropped_view {
                        (
                            geometry.crop_left
                                + frac_x * (geometry.crop_right - geometry.crop_left),
                            geometry.crop_top + frac_y * (geometry.crop_bottom - geometry.crop_top),
                        )
                    } else {
                        (frac_x, frac_y)
                    };
                    if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                        if let Some(mask_idx) = active_brush_mask_idx {
                            if let Some(mask) = local_masks.get_mut(mask_idx) {
                                if mask.add_brush_dab(u, v) {
                                    needs_preview_update = true;
                                    ui.ctx().request_repaint();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if resp.drag_stopped() {
        save_sidecar = true;
    }

    (needs_preview_update, save_sidecar)
}

/// Renders the brush cursor ring preview showing radius, feathering and erase mode color.
pub fn render_brush_cursor_ring(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    ctx: &BrushViewportContext,
    local_masks: &[LocalMaskLayer],
    active_brush_mask_idx: Option<usize>,
) {
    let available_rect = ctx.available_rect;
    let preview_rect = ctx.preview_rect;
    let is_panning_override = ctx.is_panning_override;
    if is_panning_override {
        ui.ctx().set_cursor_icon(if resp.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    } else if let Some(hover_p) = ui.input(|i| i.pointer.hover_pos()) {
        if available_rect.contains(hover_p) && preview_rect.contains(hover_p) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if let Some(mask_idx) = active_brush_mask_idx {
                if let Some(mask) = local_masks.get(mask_idx) {
                    if let LocalMaskKind::FreehandBrush {
                        brush_radius,
                        erase_mode,
                        ..
                    } = mask.kind
                    {
                        let r_screen = brush_radius * preview_rect.width();
                        let stroke_col = if erase_mode {
                            Color32::from_rgb(255, 80, 80)
                        } else {
                            Color32::from_rgb(80, 200, 255)
                        };
                        painter.circle_stroke(hover_p, r_screen, Stroke::new(1.5, stroke_col));
                        let f = mask.feather.clamp(0.05, 1.0);
                        let r_inner = r_screen * (1.0 - f).max(0.0);
                        if r_inner > 2.0 {
                            painter.circle_stroke(
                                hover_p,
                                r_inner,
                                Stroke::new(1.0, stroke_col.gamma_multiply(0.4)),
                            );
                        }
                        painter.circle_filled(hover_p, 1.5, stroke_col);
                    }
                }
            }
        }
    }
}
