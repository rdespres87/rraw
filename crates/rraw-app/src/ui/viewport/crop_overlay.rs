#![forbid(unsafe_code)]

//! Interactive crop box overlay, rule-of-thirds grid and handle interaction.

use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

use super::ViewportToolContext;
use crate::grading::GeometryGradingParams;
use crate::i18n::Key;
use crate::state::CropDragHandle;

/// Output result from crop overlay interactions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CropOverlayOutput {
    pub needs_preview_update: bool,
    pub save_sidecar: bool,
}

/// Interactive state for the crop overlay and handle manipulation.
pub struct CropToolState<'a> {
    pub geometry: &'a mut GeometryGradingParams,
    pub is_crop_editing: &'a mut bool,
    pub crop_drag_handle: &'a mut CropDragHandle,
    pub crop_show_grid: bool,
}

/// Hit-tests `pos` against the crop rectangle corners, edges, and interior within `margin` pixels.
#[must_use]
pub fn hit_test_crop_handle(pos: Pos2, crop_screen_rect: Rect, margin: f32) -> CropDragHandle {
    let near_left = (pos.x - crop_screen_rect.min.x).abs() < margin;
    let near_right = (pos.x - crop_screen_rect.max.x).abs() < margin;
    let near_top = (pos.y - crop_screen_rect.min.y).abs() < margin;
    let near_bottom = (pos.y - crop_screen_rect.max.y).abs() < margin;
    let in_x_range =
        pos.x >= crop_screen_rect.min.x - margin && pos.x <= crop_screen_rect.max.x + margin;
    let in_y_range =
        pos.y >= crop_screen_rect.min.y - margin && pos.y <= crop_screen_rect.max.y + margin;

    if near_top && near_left {
        CropDragHandle::TopLeft
    } else if near_top && near_right {
        CropDragHandle::TopRight
    } else if near_bottom && near_left {
        CropDragHandle::BottomLeft
    } else if near_bottom && near_right {
        CropDragHandle::BottomRight
    } else if near_top && in_x_range {
        CropDragHandle::Top
    } else if near_bottom && in_x_range {
        CropDragHandle::Bottom
    } else if near_left && in_y_range {
        CropDragHandle::Left
    } else if near_right && in_y_range {
        CropDragHandle::Right
    } else if crop_screen_rect.contains(pos) {
        CropDragHandle::Center
    } else {
        CropDragHandle::None
    }
}

fn draw_crop_dimming_and_grid(
    painter: &egui::Painter,
    preview_rect: Rect,
    crop_screen_rect: Rect,
    show_grid: bool,
) {
    let mask_color = Color32::from_rgba_unmultiplied(0, 0, 0, 170);
    let mask_rects = [
        Rect::from_min_max(
            preview_rect.min,
            egui::pos2(preview_rect.max.x, crop_screen_rect.min.y),
        ),
        Rect::from_min_max(
            egui::pos2(preview_rect.min.x, crop_screen_rect.max.y),
            preview_rect.max,
        ),
        Rect::from_min_max(
            egui::pos2(preview_rect.min.x, crop_screen_rect.min.y),
            egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
        ),
        Rect::from_min_max(
            egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
            egui::pos2(preview_rect.max.x, crop_screen_rect.max.y),
        ),
    ];
    for rect in mask_rects {
        painter.rect_filled(rect, 0.0, mask_color);
    }

    painter.rect_stroke(
        crop_screen_rect,
        0.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Outside,
    );

    if show_grid {
        let cw = crop_screen_rect.width();
        let ch = crop_screen_rect.height();
        let grid_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 60));

        for frac in [1.0 / 3.0, 2.0 / 3.0] {
            let x = crop_screen_rect.min.x + cw * frac;
            painter.line_segment(
                [
                    egui::pos2(x, crop_screen_rect.min.y),
                    egui::pos2(x, crop_screen_rect.max.y),
                ],
                grid_stroke,
            );
            let y = crop_screen_rect.min.y + ch * frac;
            painter.line_segment(
                [
                    egui::pos2(crop_screen_rect.min.x, y),
                    egui::pos2(crop_screen_rect.max.x, y),
                ],
                grid_stroke,
            );
        }
    }
}

fn draw_crop_handles(painter: &egui::Painter, crop_screen_rect: Rect) {
    let bracket_len = 16.0_f32
        .min(crop_screen_rect.width() * 0.25)
        .min(crop_screen_rect.height() * 0.25);
    let bracket_stroke = Stroke::new(3.0, Color32::WHITE);

    let corners = [
        (crop_screen_rect.left_top(), 1.0_f32, 1.0_f32),
        (crop_screen_rect.right_top(), -1.0, 1.0),
        (crop_screen_rect.left_bottom(), 1.0, -1.0),
        (crop_screen_rect.right_bottom(), -1.0, -1.0),
    ];
    for (origin, sx, sy) in corners {
        painter.line_segment(
            [origin, origin + Vec2::new(sx * bracket_len, 0.0)],
            bracket_stroke,
        );
        painter.line_segment(
            [origin, origin + Vec2::new(0.0, sy * bracket_len)],
            bracket_stroke,
        );
    }

    let pill_stroke = Stroke::new(2.5, Color32::WHITE);
    let edge_pill_len = 14.0_f32
        .min(crop_screen_rect.width() * 0.2)
        .min(crop_screen_rect.height() * 0.2);

    let cx = crop_screen_rect.center().x;
    let cy = crop_screen_rect.center().y;
    for y in [crop_screen_rect.min.y, crop_screen_rect.max.y] {
        let mid = egui::pos2(cx, y);
        painter.line_segment(
            [
                mid - Vec2::new(edge_pill_len, 0.0),
                mid + Vec2::new(edge_pill_len, 0.0),
            ],
            pill_stroke,
        );
    }
    for x in [crop_screen_rect.min.x, crop_screen_rect.max.x] {
        let mid = egui::pos2(x, cy);
        painter.line_segment(
            [
                mid - Vec2::new(0.0, edge_pill_len),
                mid + Vec2::new(0.0, edge_pill_len),
            ],
            pill_stroke,
        );
    }
}

fn cursor_for_crop_handle(hovered: CropDragHandle, active: CropDragHandle) -> egui::CursorIcon {
    match hovered {
        CropDragHandle::TopLeft | CropDragHandle::BottomRight => egui::CursorIcon::ResizeNwSe,
        CropDragHandle::TopRight | CropDragHandle::BottomLeft => egui::CursorIcon::ResizeNeSw,
        CropDragHandle::Top | CropDragHandle::Bottom => egui::CursorIcon::ResizeVertical,
        CropDragHandle::Left | CropDragHandle::Right => egui::CursorIcon::ResizeHorizontal,
        CropDragHandle::Center => {
            if active == CropDragHandle::Center {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            }
        }
        CropDragHandle::None => egui::CursorIcon::Default,
    }
}

fn apply_corner_resize(
    geometry: &mut GeometryGradingParams,
    handle: CropDragHandle,
    dx: f32,
    dy: f32,
    aspect_scale: f32,
    target_aspect: Option<f32>,
) {
    match handle {
        CropDragHandle::BottomRight => {
            let new_right = (geometry.crop_right + dx).clamp(geometry.crop_left + 0.05, 1.0);
            if let Some(r) = target_aspect {
                let h_norm = (new_right - geometry.crop_left) * aspect_scale / r;
                if geometry.crop_top + h_norm <= 1.0 {
                    geometry.crop_bottom = geometry.crop_top + h_norm;
                    geometry.crop_right = new_right;
                }
            } else {
                geometry.crop_right = new_right;
                geometry.crop_bottom =
                    (geometry.crop_bottom + dy).clamp(geometry.crop_top + 0.05, 1.0);
            }
        }
        CropDragHandle::BottomLeft => {
            let new_left = (geometry.crop_left + dx).clamp(0.0, geometry.crop_right - 0.05);
            if let Some(r) = target_aspect {
                let h_norm = (geometry.crop_right - new_left) * aspect_scale / r;
                if geometry.crop_top + h_norm <= 1.0 {
                    geometry.crop_bottom = geometry.crop_top + h_norm;
                    geometry.crop_left = new_left;
                }
            } else {
                geometry.crop_left = new_left;
                geometry.crop_bottom =
                    (geometry.crop_bottom + dy).clamp(geometry.crop_top + 0.05, 1.0);
            }
        }
        CropDragHandle::TopRight => {
            let new_right = (geometry.crop_right + dx).clamp(geometry.crop_left + 0.05, 1.0);
            if let Some(r) = target_aspect {
                let h_norm = (new_right - geometry.crop_left) * aspect_scale / r;
                if geometry.crop_bottom >= h_norm {
                    geometry.crop_top = geometry.crop_bottom - h_norm;
                    geometry.crop_right = new_right;
                }
            } else {
                geometry.crop_right = new_right;
                geometry.crop_top =
                    (geometry.crop_top + dy).clamp(0.0, geometry.crop_bottom - 0.05);
            }
        }
        CropDragHandle::TopLeft => {
            let new_left = (geometry.crop_left + dx).clamp(0.0, geometry.crop_right - 0.05);
            if let Some(r) = target_aspect {
                let h_norm = (geometry.crop_right - new_left) * aspect_scale / r;
                if geometry.crop_bottom >= h_norm {
                    geometry.crop_top = geometry.crop_bottom - h_norm;
                    geometry.crop_left = new_left;
                }
            } else {
                geometry.crop_left = new_left;
                geometry.crop_top =
                    (geometry.crop_top + dy).clamp(0.0, geometry.crop_bottom - 0.05);
            }
        }
        _ => {}
    }
}

fn apply_edge_resize(
    geometry: &mut GeometryGradingParams,
    handle: CropDragHandle,
    dx: f32,
    dy: f32,
    aspect_scale: f32,
    target_aspect: Option<f32>,
) {
    match handle {
        CropDragHandle::Right | CropDragHandle::Left => {
            if handle == CropDragHandle::Right {
                geometry.crop_right =
                    (geometry.crop_right + dx).clamp(geometry.crop_left + 0.05, 1.0);
            } else {
                geometry.crop_left =
                    (geometry.crop_left + dx).clamp(0.0, geometry.crop_right - 0.05);
            }
            if let Some(r) = target_aspect {
                let h_norm = (geometry.crop_right - geometry.crop_left) * aspect_scale / r;
                let center_y = (geometry.crop_top + geometry.crop_bottom) * 0.5;
                geometry.crop_top = (center_y - h_norm * 0.5).clamp(0.0, 1.0 - h_norm);
                geometry.crop_bottom = geometry.crop_top + h_norm;
            }
        }
        CropDragHandle::Bottom | CropDragHandle::Top => {
            if handle == CropDragHandle::Bottom {
                geometry.crop_bottom =
                    (geometry.crop_bottom + dy).clamp(geometry.crop_top + 0.05, 1.0);
            } else {
                geometry.crop_top =
                    (geometry.crop_top + dy).clamp(0.0, geometry.crop_bottom - 0.05);
            }
            if let Some(r) = target_aspect {
                let w_norm = (geometry.crop_bottom - geometry.crop_top) / aspect_scale * r;
                let center_x = (geometry.crop_left + geometry.crop_right) * 0.5;
                geometry.crop_left = (center_x - w_norm * 0.5).clamp(0.0, 1.0 - w_norm);
                geometry.crop_right = geometry.crop_left + w_norm;
            }
        }
        _ => {}
    }
}

fn apply_crop_drag_delta(
    geometry: &mut GeometryGradingParams,
    handle: CropDragHandle,
    dx: f32,
    dy: f32,
    pw: f32,
    ph: f32,
) {
    if pw <= f32::EPSILON || ph <= f32::EPSILON {
        return;
    }
    let target_aspect = geometry.crop_aspect.ratio(pw, ph);
    let aspect_scale = pw / ph;

    match handle {
        CropDragHandle::Center => {
            let w = geometry.crop_right - geometry.crop_left;
            let h = geometry.crop_bottom - geometry.crop_top;
            let new_left = (geometry.crop_left + dx).clamp(0.0, 1.0 - w);
            let new_top = (geometry.crop_top + dy).clamp(0.0, 1.0 - h);
            geometry.crop_left = new_left;
            geometry.crop_right = new_left + w;
            geometry.crop_top = new_top;
            geometry.crop_bottom = new_top + h;
        }
        CropDragHandle::TopLeft
        | CropDragHandle::TopRight
        | CropDragHandle::BottomLeft
        | CropDragHandle::BottomRight => {
            apply_corner_resize(geometry, handle, dx, dy, aspect_scale, target_aspect);
        }
        CropDragHandle::Top
        | CropDragHandle::Bottom
        | CropDragHandle::Left
        | CropDragHandle::Right => {
            apply_edge_resize(geometry, handle, dx, dy, aspect_scale, target_aspect);
        }
        CropDragHandle::None => {}
    }
}

fn handle_crop_drag_interaction(
    ui: &mut Ui,
    ctx: &ViewportToolContext<'_>,
    crop_screen_rect: Rect,
    tool_state: &mut CropToolState<'_>,
    out: &mut CropOverlayOutput,
) {
    let margin = 14.0;
    let pw = ctx.preview_rect.width();
    let ph = ctx.preview_rect.height();
    let (pointer_pos, press_origin) = ui.input(|i| {
        (
            i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()),
            i.pointer.press_origin(),
        )
    });

    let active_or_hovered = if *tool_state.crop_drag_handle != CropDragHandle::None {
        *tool_state.crop_drag_handle
    } else if let Some(pos) = pointer_pos {
        hit_test_crop_handle(pos, crop_screen_rect, margin)
    } else {
        CropDragHandle::None
    };

    if active_or_hovered != CropDragHandle::None {
        ui.ctx().set_cursor_icon(cursor_for_crop_handle(
            active_or_hovered,
            *tool_state.crop_drag_handle,
        ));
    }

    if ctx.resp.drag_started() {
        if let Some(origin) = press_origin.or(pointer_pos) {
            *tool_state.crop_drag_handle = hit_test_crop_handle(origin, crop_screen_rect, margin);
        }
    }

    if ctx.resp.dragged()
        && *tool_state.crop_drag_handle != CropDragHandle::None
        && pw > f32::EPSILON
        && ph > f32::EPSILON
    {
        let delta = ctx.resp.drag_delta();
        apply_crop_drag_delta(
            tool_state.geometry,
            *tool_state.crop_drag_handle,
            delta.x / pw,
            delta.y / ph,
            pw,
            ph,
        );
        ui.ctx().request_repaint();
    }

    if ctx.resp.drag_stopped() {
        *tool_state.crop_drag_handle = CropDragHandle::None;
        out.needs_preview_update = true;
        out.save_sidecar = true;
        ui.ctx().request_repaint();
    }

    let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let dbl_click_crop = ctx.resp.double_clicked()
        && ctx
            .resp
            .interact_pointer_pos()
            .is_some_and(|p| crop_screen_rect.contains(p));

    if enter_pressed || dbl_click_crop {
        *tool_state.is_crop_editing = false;
        out.needs_preview_update = true;
        out.save_sidecar = true;
        ui.ctx().request_repaint();
    }
}

fn draw_crop_validation_pill(
    ui: &mut Ui,
    ctx: &ViewportToolContext<'_>,
    is_crop_editing: &mut bool,
    out: &mut CropOverlayOutput,
) {
    let banner_rect = Rect::from_min_size(
        ctx.available_rect.left_top() + Vec2::new(16.0, 16.0),
        Vec2::new(380.0, 32.0),
    );
    ctx.painter.rect_filled(
        banner_rect,
        6.0,
        Color32::from_rgba_unmultiplied(16, 20, 28, 230),
    );
    ctx.painter.rect_stroke(
        banner_rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(60, 120, 200)),
        StrokeKind::Outside,
    );
    ctx.painter.text(
        banner_rect.center() - Vec2::new(35.0, 0.0),
        egui::Align2::CENTER_CENTER,
        Key::CropOverlayBanner.translate(ctx.language),
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );

    let val_btn_rect = Rect::from_min_size(
        Pos2::new(banner_rect.max.x - 74.0, banner_rect.min.y + 4.0),
        Vec2::new(68.0, 24.0),
    );
    let val_btn_id = ui.auto_id_with("crop_validate_btn");
    let val_resp = ui.interact(val_btn_rect, val_btn_id, egui::Sense::click());
    if val_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ctx.painter.rect_filled(
        val_btn_rect,
        4.0,
        if val_resp.hovered() {
            Color32::from_rgb(45, 150, 80)
        } else {
            Color32::from_rgb(35, 125, 65)
        },
    );
    ctx.painter.text(
        val_btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        Key::CropOverlayValidate.translate(ctx.language),
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );
    if val_resp.clicked() {
        *is_crop_editing = false;
        out.needs_preview_update = true;
        out.save_sidecar = true;
        ui.ctx().request_repaint();
    }
}

/// Renders the crop box overlay, handles mouse dragging, resizing and floating validation pill.
pub fn handle_crop_overlay(
    ui: &mut Ui,
    ctx: &ViewportToolContext<'_>,
    tool_state: &mut CropToolState<'_>,
) -> CropOverlayOutput {
    let mut out = CropOverlayOutput::default();
    let pw = ctx.preview_rect.width();
    let ph = ctx.preview_rect.height();

    let crop_screen_min = egui::pos2(
        ctx.preview_rect.min.x + tool_state.geometry.crop_left * pw,
        ctx.preview_rect.min.y + tool_state.geometry.crop_top * ph,
    );
    let crop_screen_max = egui::pos2(
        ctx.preview_rect.min.x + tool_state.geometry.crop_right * pw,
        ctx.preview_rect.min.y + tool_state.geometry.crop_bottom * ph,
    );
    let crop_screen_rect = Rect::from_min_max(crop_screen_min, crop_screen_max);

    draw_crop_dimming_and_grid(
        ctx.painter,
        ctx.preview_rect,
        crop_screen_rect,
        tool_state.crop_show_grid,
    );
    draw_crop_handles(ctx.painter, crop_screen_rect);
    handle_crop_drag_interaction(ui, ctx, crop_screen_rect, tool_state, &mut out);
    draw_crop_validation_pill(ui, ctx, tool_state.is_crop_editing, &mut out);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_test_crop_handle_corners_edges_center_and_outside() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 250.0));
        let m = 14.0;

        assert_eq!(
            hit_test_crop_handle(Pos2::new(105.0, 105.0), rect, m),
            CropDragHandle::TopLeft
        );
        assert_eq!(
            hit_test_crop_handle(Pos2::new(296.0, 246.0), rect, m),
            CropDragHandle::BottomRight
        );
        assert_eq!(
            hit_test_crop_handle(Pos2::new(200.0, 102.0), rect, m),
            CropDragHandle::Top
        );
        assert_eq!(
            hit_test_crop_handle(Pos2::new(298.0, 175.0), rect, m),
            CropDragHandle::Right
        );
        assert_eq!(
            hit_test_crop_handle(Pos2::new(200.0, 175.0), rect, m),
            CropDragHandle::Center
        );
        assert_eq!(
            hit_test_crop_handle(Pos2::new(20.0, 20.0), rect, m),
            CropDragHandle::None
        );
    }

    #[test]
    fn test_apply_crop_drag_delta_center_pan_clamped() {
        let mut geom = GeometryGradingParams {
            crop_left: 0.2,
            crop_right: 0.7,
            crop_top: 0.2,
            crop_bottom: 0.6,
            ..GeometryGradingParams::default()
        };
        apply_crop_drag_delta(&mut geom, CropDragHandle::Center, 0.5, -0.5, 1000.0, 800.0);
        assert!((geom.crop_left - 0.5).abs() < 1e-5);
        assert!((geom.crop_right - 1.0).abs() < 1e-5);
        assert!(geom.crop_top.abs() < 1e-5);
        assert!((geom.crop_bottom - 0.4).abs() < 1e-5);
    }

    #[test]
    fn test_apply_crop_drag_delta_bottom_right_respects_min_size() {
        let mut geom = GeometryGradingParams {
            crop_left: 0.2,
            crop_right: 0.8,
            crop_top: 0.2,
            crop_bottom: 0.8,
            ..GeometryGradingParams::default()
        };
        apply_crop_drag_delta(
            &mut geom,
            CropDragHandle::BottomRight,
            -0.9,
            -0.9,
            1000.0,
            1000.0,
        );
        assert!((geom.crop_right - 0.25).abs() < 1e-5);
        assert!((geom.crop_bottom - 0.25).abs() < 1e-5);
    }

    #[test]
    fn test_apply_crop_drag_delta_ignores_zero_dimensions() {
        let mut geom = GeometryGradingParams {
            crop_left: 0.2,
            crop_right: 0.8,
            crop_top: 0.2,
            crop_bottom: 0.8,
            ..GeometryGradingParams::default()
        };
        apply_crop_drag_delta(&mut geom, CropDragHandle::BottomRight, 0.1, 0.1, 0.0, 100.0);
        assert!((geom.crop_right - 0.8).abs() < 1e-5);
        assert!(geom.crop_right.is_finite() && geom.crop_bottom.is_finite());
    }
}
