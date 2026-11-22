#![forbid(unsafe_code)]

//! Draggable Before/After split comparison divider.

use egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, Ui, Vec2};

/// Paints the draggable Before/After divider and updates `split_position` while dragging.
pub fn render_split_divider(
    ui: &Ui,
    image_rect: Rect,
    split_position: &mut f32,
    label_before: &str,
    label_after: &str,
) {
    let x = egui::lerp(
        image_rect.min.x..=image_rect.max.x,
        split_position.clamp(0.0, 1.0),
    );
    let handle = Rect::from_center_size(
        Pos2::new(x, image_rect.center().y),
        Vec2::new(28.0, image_rect.height()),
    );
    let resp = ui.interact(
        handle,
        ui.auto_id_with("split_divider"),
        egui::Sense::drag(),
    );
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
        *split_position = crate::view_tools::split_from_pointer(image_rect, p.x).clamp(0.0, 1.0);
    }

    let x = egui::lerp(
        image_rect.min.x..=image_rect.max.x,
        split_position.clamp(0.0, 1.0),
    );
    let painter: Painter = ui.painter().with_clip_rect(image_rect);
    let line_col = Color32::from_white_alpha(220);
    painter.line_segment(
        [
            Pos2::new(x, image_rect.min.y),
            Pos2::new(x, image_rect.max.y),
        ],
        Stroke::new(1.5, line_col),
    );

    let circle_center = Pos2::new(x, image_rect.center().y);
    painter.circle_filled(circle_center, 10.0, Color32::from_black_alpha(200));
    painter.circle_stroke(circle_center, 10.0, Stroke::new(1.5, line_col));
    painter.text(
        circle_center,
        egui::Align2::CENTER_CENTER,
        "⬌",
        FontId::proportional(11.0),
        Color32::WHITE,
    );

    let label_font = FontId::proportional(12.0);
    let label_y = image_rect.min.y + 14.0;
    if x - image_rect.min.x > 50.0 {
        let galley =
            painter.layout_no_wrap(label_before.to_owned(), label_font.clone(), Color32::WHITE);
        let rect = egui::Align2::RIGHT_CENTER
            .anchor_size(Pos2::new(x - 12.0, label_y), galley.size())
            .expand2(Vec2::new(6.0, 3.0));
        painter.rect_filled(rect, 4.0, Color32::from_black_alpha(170));
        painter.galley(rect.min + Vec2::new(6.0, 3.0), galley, Color32::WHITE);
    }
    if image_rect.max.x - x > 50.0 {
        let galley = painter.layout_no_wrap(label_after.to_owned(), label_font, Color32::WHITE);
        let rect = egui::Align2::LEFT_CENTER
            .anchor_size(Pos2::new(x + 12.0, label_y), galley.size())
            .expand2(Vec2::new(6.0, 3.0));
        painter.rect_filled(rect, 4.0, Color32::from_black_alpha(170));
        painter.galley(rect.min + Vec2::new(6.0, 3.0), galley, Color32::WHITE);
    }
}
