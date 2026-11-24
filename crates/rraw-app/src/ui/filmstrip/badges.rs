#![forbid(unsafe_code)]

//! Overlay badges, selection checkboxes, filenames, and star ratings for filmstrip cards.

use egui::Color32;

/// Paints the top-left RAW/format extension pill inside `thumb_rect`.
pub fn paint_format_badge(painter: &egui::Painter, thumb_rect: egui::Rect, ext: &str) {
    let badge_rect = egui::Rect::from_min_size(
        thumb_rect.min + egui::vec2(3.0, 3.0),
        egui::vec2(30.0, 13.0),
    );
    painter.rect_filled(
        badge_rect,
        2.0,
        Color32::from_rgba_premultiplied(0, 0, 0, 180),
    );
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        ext,
        egui::FontId::monospace(9.0),
        Color32::from_rgb(200, 210, 230),
    );
}

/// Paints the green dot indicating the photo is warm in the decoded cache.
pub fn paint_cache_indicator(painter: &egui::Painter, thumb_rect: egui::Rect) {
    painter.text(
        egui::pos2(thumb_rect.max.x - 26.0, thumb_rect.min.y + 3.0),
        egui::Align2::RIGHT_TOP,
        "●",
        egui::FontId::proportional(11.0),
        Color32::from_rgb(80, 220, 120),
    );
}

/// Paints the top-right batch selection checkbox and returns `true` if clicked this frame.
pub fn paint_selection_checkbox(
    ui: &egui::Ui,
    painter: &egui::Painter,
    response: &egui::Response,
    thumb_rect: egui::Rect,
    is_multi_selected: bool,
) -> bool {
    let chk_rect = egui::Rect::from_min_size(
        egui::pos2(thumb_rect.max.x - 23.0, thumb_rect.min.y + 2.0),
        egui::vec2(20.0, 20.0),
    );
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let hovered_chk = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| chk_rect.expand(2.0).contains(p));
    if hovered_chk {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let clicked_chk =
        response.clicked() && pointer_pos.is_some_and(|p| chk_rect.expand(3.0).contains(p));

    if is_multi_selected {
        painter.circle_filled(chk_rect.center(), 9.0, Color32::from_rgb(30, 130, 240));
        painter.text(
            chk_rect.center(),
            egui::Align2::CENTER_CENTER,
            "✓",
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
    } else {
        let circle_col = if hovered_chk {
            Color32::from_rgba_premultiplied(50, 130, 230, 220)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 140)
        };
        painter.circle_filled(chk_rect.center(), 8.0, circle_col);
        painter.circle_stroke(
            chk_rect.center(),
            8.0,
            egui::Stroke::new(1.2, Color32::from_rgb(160, 175, 200)),
        );
    }

    clicked_chk
}

/// Safely truncates a filename to at most 14 Unicode scalar values for card display.
#[must_use]
pub fn truncate_card_filename(file_name: &str) -> String {
    if file_name.chars().count() > 14 {
        let prefix: String = file_name.chars().take(13).collect();
        format!("{prefix}…")
    } else {
        file_name.to_owned()
    }
}

/// Paints the truncated filename label centered below the thumbnail.
pub fn paint_file_name(
    painter: &egui::Painter,
    card_rect: egui::Rect,
    file_name: &str,
    is_selected: bool,
) {
    let name_color = if is_selected {
        Color32::WHITE
    } else {
        Color32::from_rgb(200, 205, 215)
    };
    painter.text(
        egui::pos2(card_rect.center().x, card_rect.min.y + 70.0),
        egui::Align2::CENTER_TOP,
        truncate_card_filename(file_name),
        egui::FontId::proportional(10.5),
        name_color,
    );
}

/// Paints the 5 interactive star rating glyphs and returns the new rating if a star was clicked.
pub fn paint_star_rating_row(
    ui: &egui::Ui,
    painter: &egui::Painter,
    response: &egui::Response,
    card_rect: egui::Rect,
    current_rating: u8,
) -> Option<u8> {
    let star_y = card_rect.min.y + 90.0;
    let star_spacing = 15.0;
    let star_start_x = card_rect.center().x - (2.0 * star_spacing);

    let hover_pos = ui.input(|i| i.pointer.hover_pos());
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let mut hovered_star = None;
    let mut new_rating = None;

    for s in 1..=5_u8 {
        let star_pos = egui::pos2(star_start_x + (f32::from(s) - 1.0) * star_spacing, star_y);
        let star_rect = egui::Rect::from_center_size(star_pos, egui::vec2(14.0, 14.0));
        if hover_pos.is_some_and(|p| star_rect.contains(p)) {
            hovered_star = Some(s);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() && pointer_pos.is_some_and(|p| star_rect.contains(p)) {
            new_rating = Some(if current_rating == s { 0 } else { s });
        }
    }

    for s in 1..=5_u8 {
        let star_pos = egui::pos2(star_start_x + (f32::from(s) - 1.0) * star_spacing, star_y);
        let is_lit = hovered_star.map_or(s <= current_rating, |h| s <= h);
        let (star_char, star_col) = if is_lit {
            ("★", Color32::from_rgb(255, 205, 50))
        } else {
            ("☆", Color32::from_rgb(75, 80, 95))
        };
        painter.text(
            star_pos,
            egui::Align2::CENTER_CENTER,
            star_char,
            egui::FontId::proportional(13.0),
            star_col,
        );
    }

    new_rating
}

#[cfg(test)]
mod tests {
    use super::truncate_card_filename;

    #[test]
    fn truncates_multibyte_utf8_filename_safely() {
        let short = truncate_card_filename("IMG_001.CR3");
        assert_eq!(short, "IMG_001.CR3");

        let utf8_long = truncate_card_filename("été_pyrénées_2026_raw.dng");
        assert_eq!(utf8_long.chars().count(), 14);
        assert!(utf8_long.ends_with('…'));
    }
}
