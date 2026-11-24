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

/// Renders the crop box overlay, handles mouse dragging, resizing and floating validation pill.
#[allow(clippy::too_many_lines)]
pub fn handle_crop_overlay(
    ui: &mut Ui,
    ctx: &ViewportToolContext<'_>,
    tool_state: &mut CropToolState<'_>,
) -> CropOverlayOutput {
    let ViewportToolContext {
        resp,
        painter,
        available_rect,
        preview_rect,
        language: cur_lang,
    } = *ctx;
    let geometry = &mut *tool_state.geometry;
    let is_crop_editing = &mut *tool_state.is_crop_editing;
    let crop_drag_handle = &mut *tool_state.crop_drag_handle;
    let mut out = CropOverlayOutput::default();
    let pw = preview_rect.width();
    let ph = preview_rect.height();

    let crop_screen_min = egui::pos2(
        preview_rect.min.x + geometry.crop_left * pw,
        preview_rect.min.y + geometry.crop_top * ph,
    );
    let crop_screen_max = egui::pos2(
        preview_rect.min.x + geometry.crop_right * pw,
        preview_rect.min.y + geometry.crop_bottom * ph,
    );
    let crop_screen_rect = Rect::from_min_max(crop_screen_min, crop_screen_max);

    // 1. Dimmed outer mask outside crop box
    let mask_color = Color32::from_rgba_unmultiplied(0, 0, 0, 170);
    painter.rect_filled(
        Rect::from_min_max(
            preview_rect.min,
            egui::pos2(preview_rect.max.x, crop_screen_rect.min.y),
        ),
        0.0,
        mask_color,
    );
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(preview_rect.min.x, crop_screen_rect.max.y),
            preview_rect.max,
        ),
        0.0,
        mask_color,
    );
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(preview_rect.min.x, crop_screen_rect.min.y),
            egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
        ),
        0.0,
        mask_color,
    );
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
            egui::pos2(preview_rect.max.x, crop_screen_rect.max.y),
        ),
        0.0,
        mask_color,
    );

    // 2. Bright border around crop box
    painter.rect_stroke(
        crop_screen_rect,
        0.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Outside,
    );

    // 3. Rule-of-thirds composition grid
    if geometry.crop_show_grid {
        let cw = crop_screen_rect.width();
        let ch = crop_screen_rect.height();
        let grid_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 60));

        let x1 = crop_screen_rect.min.x + cw / 3.0;
        let x2 = crop_screen_rect.min.x + cw * 2.0 / 3.0;
        painter.line_segment(
            [
                egui::pos2(x1, crop_screen_rect.min.y),
                egui::pos2(x1, crop_screen_rect.max.y),
            ],
            grid_stroke,
        );
        painter.line_segment(
            [
                egui::pos2(x2, crop_screen_rect.min.y),
                egui::pos2(x2, crop_screen_rect.max.y),
            ],
            grid_stroke,
        );

        let y1 = crop_screen_rect.min.y + ch / 3.0;
        let y2 = crop_screen_rect.min.y + ch * 2.0 / 3.0;
        painter.line_segment(
            [
                egui::pos2(crop_screen_rect.min.x, y1),
                egui::pos2(crop_screen_rect.max.x, y1),
            ],
            grid_stroke,
        );
        painter.line_segment(
            [
                egui::pos2(crop_screen_rect.min.x, y2),
                egui::pos2(crop_screen_rect.max.x, y2),
            ],
            grid_stroke,
        );
    }

    // 4. Corner L-brackets & Edge handles
    let bracket_len = 16.0_f32
        .min(crop_screen_rect.width() * 0.25)
        .min(crop_screen_rect.height() * 0.25);
    let bracket_stroke = Stroke::new(3.0, Color32::WHITE);

    // Top-left
    painter.line_segment(
        [
            crop_screen_rect.left_top(),
            crop_screen_rect.left_top() + Vec2::new(bracket_len, 0.0),
        ],
        bracket_stroke,
    );
    painter.line_segment(
        [
            crop_screen_rect.left_top(),
            crop_screen_rect.left_top() + Vec2::new(0.0, bracket_len),
        ],
        bracket_stroke,
    );
    // Top-right
    painter.line_segment(
        [
            crop_screen_rect.right_top(),
            crop_screen_rect.right_top() - Vec2::new(bracket_len, 0.0),
        ],
        bracket_stroke,
    );
    painter.line_segment(
        [
            crop_screen_rect.right_top(),
            crop_screen_rect.right_top() + Vec2::new(0.0, bracket_len),
        ],
        bracket_stroke,
    );
    // Bottom-left
    painter.line_segment(
        [
            crop_screen_rect.left_bottom(),
            crop_screen_rect.left_bottom() + Vec2::new(bracket_len, 0.0),
        ],
        bracket_stroke,
    );
    painter.line_segment(
        [
            crop_screen_rect.left_bottom(),
            crop_screen_rect.left_bottom() - Vec2::new(0.0, bracket_len),
        ],
        bracket_stroke,
    );
    // Bottom-right
    painter.line_segment(
        [
            crop_screen_rect.right_bottom(),
            crop_screen_rect.right_bottom() - Vec2::new(bracket_len, 0.0),
        ],
        bracket_stroke,
    );
    painter.line_segment(
        [
            crop_screen_rect.right_bottom(),
            crop_screen_rect.right_bottom() - Vec2::new(0.0, bracket_len),
        ],
        bracket_stroke,
    );

    // Mid-edge pills
    let pill_stroke = Stroke::new(2.5, Color32::WHITE);
    let edge_pill_len = 14.0_f32
        .min(crop_screen_rect.width() * 0.2)
        .min(crop_screen_rect.height() * 0.2);
    // Top edge
    let top_mid = egui::pos2(crop_screen_rect.center().x, crop_screen_rect.min.y);
    painter.line_segment(
        [
            top_mid - Vec2::new(edge_pill_len, 0.0),
            top_mid + Vec2::new(edge_pill_len, 0.0),
        ],
        pill_stroke,
    );
    // Bottom edge
    let bot_mid = egui::pos2(crop_screen_rect.center().x, crop_screen_rect.max.y);
    painter.line_segment(
        [
            bot_mid - Vec2::new(edge_pill_len, 0.0),
            bot_mid + Vec2::new(edge_pill_len, 0.0),
        ],
        pill_stroke,
    );
    // Left edge
    let left_mid = egui::pos2(crop_screen_rect.min.x, crop_screen_rect.center().y);
    painter.line_segment(
        [
            left_mid - Vec2::new(0.0, edge_pill_len),
            left_mid + Vec2::new(0.0, edge_pill_len),
        ],
        pill_stroke,
    );
    // Right edge
    let right_mid = egui::pos2(crop_screen_rect.max.x, crop_screen_rect.center().y);
    painter.line_segment(
        [
            right_mid - Vec2::new(0.0, edge_pill_len),
            right_mid + Vec2::new(0.0, edge_pill_len),
        ],
        pill_stroke,
    );

    // 5. Mouse Hit-Testing & Interactive Resizing
    let margin = 14.0;
    let (pointer_pos, press_origin) = ui.input(|i| {
        (
            i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()),
            i.pointer.press_origin(),
        )
    });

    let active_or_hovered = if *crop_drag_handle != CropDragHandle::None {
        *crop_drag_handle
    } else if let Some(pos) = pointer_pos {
        hit_test_crop_handle(pos, crop_screen_rect, margin)
    } else {
        CropDragHandle::None
    };

    let cursor = match active_or_hovered {
        CropDragHandle::TopLeft | CropDragHandle::BottomRight => egui::CursorIcon::ResizeNwSe,
        CropDragHandle::TopRight | CropDragHandle::BottomLeft => egui::CursorIcon::ResizeNeSw,
        CropDragHandle::Top | CropDragHandle::Bottom => egui::CursorIcon::ResizeVertical,
        CropDragHandle::Left | CropDragHandle::Right => egui::CursorIcon::ResizeHorizontal,
        CropDragHandle::Center => {
            if *crop_drag_handle == CropDragHandle::Center {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            }
        }
        CropDragHandle::None => egui::CursorIcon::Default,
    };
    if active_or_hovered != CropDragHandle::None {
        ui.ctx().set_cursor_icon(cursor);
    }

    if resp.drag_started() {
        if let Some(origin) = press_origin.or(pointer_pos) {
            *crop_drag_handle = hit_test_crop_handle(origin, crop_screen_rect, margin);
        }
    }

    if resp.dragged() && *crop_drag_handle != CropDragHandle::None {
        let delta = resp.drag_delta();
        let dx = delta.x / pw;
        let dy = delta.y / ph;
        let target_aspect = geometry.crop_aspect.ratio(pw, ph);

        match *crop_drag_handle {
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
            CropDragHandle::BottomRight => {
                let new_right = (geometry.crop_right + dx).clamp(geometry.crop_left + 0.05, 1.0);
                if let Some(r) = target_aspect {
                    let w_norm = new_right - geometry.crop_left;
                    let h_norm = w_norm * (pw / ph) / r;
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
                    let w_norm = geometry.crop_right - new_left;
                    let h_norm = w_norm * (pw / ph) / r;
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
                    let w_norm = new_right - geometry.crop_left;
                    let h_norm = w_norm * (pw / ph) / r;
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
                    let w_norm = geometry.crop_right - new_left;
                    let h_norm = w_norm * (pw / ph) / r;
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
            CropDragHandle::Right => {
                let new_right = (geometry.crop_right + dx).clamp(geometry.crop_left + 0.05, 1.0);
                geometry.crop_right = new_right;
                if let Some(r) = target_aspect {
                    let w_norm = new_right - geometry.crop_left;
                    let h_norm = w_norm * (pw / ph) / r;
                    let center_y = (geometry.crop_top + geometry.crop_bottom) * 0.5;
                    geometry.crop_top = (center_y - h_norm * 0.5).clamp(0.0, 1.0 - h_norm);
                    geometry.crop_bottom = geometry.crop_top + h_norm;
                }
            }
            CropDragHandle::Left => {
                let new_left = (geometry.crop_left + dx).clamp(0.0, geometry.crop_right - 0.05);
                geometry.crop_left = new_left;
                if let Some(r) = target_aspect {
                    let w_norm = geometry.crop_right - new_left;
                    let h_norm = w_norm * (pw / ph) / r;
                    let center_y = (geometry.crop_top + geometry.crop_bottom) * 0.5;
                    geometry.crop_top = (center_y - h_norm * 0.5).clamp(0.0, 1.0 - h_norm);
                    geometry.crop_bottom = geometry.crop_top + h_norm;
                }
            }
            CropDragHandle::Bottom => {
                let new_bot = (geometry.crop_bottom + dy).clamp(geometry.crop_top + 0.05, 1.0);
                geometry.crop_bottom = new_bot;
                if let Some(r) = target_aspect {
                    let h_norm = new_bot - geometry.crop_top;
                    let w_norm = h_norm / (pw / ph) * r;
                    let center_x = (geometry.crop_left + geometry.crop_right) * 0.5;
                    geometry.crop_left = (center_x - w_norm * 0.5).clamp(0.0, 1.0 - w_norm);
                    geometry.crop_right = geometry.crop_left + w_norm;
                }
            }
            CropDragHandle::Top => {
                let new_top = (geometry.crop_top + dy).clamp(0.0, geometry.crop_bottom - 0.05);
                geometry.crop_top = new_top;
                if let Some(r) = target_aspect {
                    let h_norm = geometry.crop_bottom - new_top;
                    let w_norm = h_norm / (pw / ph) * r;
                    let center_x = (geometry.crop_left + geometry.crop_right) * 0.5;
                    geometry.crop_left = (center_x - w_norm * 0.5).clamp(0.0, 1.0 - w_norm);
                    geometry.crop_right = geometry.crop_left + w_norm;
                }
            }
            CropDragHandle::None => {}
        }
        ui.ctx().request_repaint();
    }

    if resp.drag_stopped() {
        *crop_drag_handle = CropDragHandle::None;
        out.needs_preview_update = true;
        out.save_sidecar = true;
        ui.ctx().request_repaint();
    }

    // Validate crop on Enter key or double-click inside crop box
    let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let dbl_click_crop = resp.double_clicked()
        && resp
            .interact_pointer_pos()
            .is_some_and(|p| crop_screen_rect.contains(p));

    if enter_pressed || dbl_click_crop {
        *is_crop_editing = false;
        out.needs_preview_update = true;
        out.save_sidecar = true;
        ui.ctx().request_repaint();
    }

    // Floating validation pill at top of viewport
    let banner_rect = Rect::from_min_size(
        available_rect.left_top() + Vec2::new(16.0, 16.0),
        Vec2::new(380.0, 32.0),
    );
    painter.rect_filled(
        banner_rect,
        6.0,
        Color32::from_rgba_unmultiplied(16, 20, 28, 230),
    );
    painter.rect_stroke(
        banner_rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(60, 120, 200)),
        StrokeKind::Outside,
    );
    painter.text(
        banner_rect.center() - Vec2::new(35.0, 0.0),
        egui::Align2::CENTER_CENTER,
        Key::CropOverlayBanner.translate(cur_lang),
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
    painter.rect_filled(
        val_btn_rect,
        4.0,
        if val_resp.hovered() {
            Color32::from_rgb(45, 150, 80)
        } else {
            Color32::from_rgb(35, 125, 65)
        },
    );
    painter.text(
        val_btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        Key::CropOverlayValidate.translate(cur_lang),
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );
    if val_resp.clicked() {
        *is_crop_editing = false;
        out.needs_preview_update = true;
        out.save_sidecar = true;
        ui.ctx().request_repaint();
    }

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
}
