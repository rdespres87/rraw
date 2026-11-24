#![forbid(unsafe_code)]

//! Thumbnail aspect-ratio calculation and texture painting for filmstrip cards.

use egui::Color32;

/// Computes the letterboxed or pillarboxed rectangle fitting `tex_size` inside `thumb_rect`.
#[must_use]
pub fn compute_fitted_thumbnail_rect(thumb_rect: egui::Rect, tex_size: egui::Vec2) -> egui::Rect {
    let tex_aspect = tex_size.x / tex_size.y.max(1.0);
    let frame_aspect = thumb_rect.width() / thumb_rect.height().max(1.0);

    if tex_aspect > frame_aspect {
        let h = (thumb_rect.width() / tex_aspect).clamp(10.0, thumb_rect.height());
        let y = thumb_rect.center().y - h * 0.5;
        egui::Rect::from_min_size(
            egui::pos2(thumb_rect.left(), y),
            egui::vec2(thumb_rect.width(), h),
        )
    } else {
        let w = (thumb_rect.height() * tex_aspect).clamp(10.0, thumb_rect.width());
        let x = thumb_rect.center().x - w * 0.5;
        egui::Rect::from_min_size(
            egui::pos2(x, thumb_rect.top()),
            egui::vec2(w, thumb_rect.height()),
        )
    }
}

/// Paints either the decoded thumbnail texture or a loading placeholder inside `thumb_rect`.
///
/// Returns `true` when the thumbnail is missing from cache and should be requested asynchronously.
pub fn paint_thumbnail_frame(
    painter: &egui::Painter,
    thumb_rect: egui::Rect,
    texture: Option<&egui::TextureHandle>,
    ext: &str,
) -> bool {
    painter.rect_filled(thumb_rect, 2.0, Color32::from_rgb(14, 16, 20));

    if let Some(tex) = texture {
        let fitted_rect = compute_fitted_thumbnail_rect(thumb_rect, tex.size_vec2());
        painter.image(
            tex.id(),
            fitted_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.rect_stroke(
            thumb_rect,
            2.0,
            egui::Stroke::new(1.0, Color32::from_rgb(45, 50, 60)),
            egui::StrokeKind::Outside,
        );
        false
    } else {
        painter.rect_stroke(
            thumb_rect,
            2.0,
            egui::Stroke::new(1.0, Color32::from_rgb(35, 38, 45)),
            egui::StrokeKind::Outside,
        );
        painter.text(
            thumb_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("📷 [{ext}]"),
            egui::FontId::monospace(10.0),
            Color32::from_rgb(110, 120, 135),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::compute_fitted_thumbnail_rect;

    #[test]
    fn landscape_thumbnail_letterboxes_vertically() {
        let frame = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(110.0, 62.0));
        let fitted = compute_fitted_thumbnail_rect(frame, egui::vec2(300.0, 100.0));
        assert!((fitted.width() - 110.0).abs() < 1e-3);
        assert!(fitted.height() < 62.0);
        assert!((fitted.center().y - frame.center().y).abs() < 1e-3);
    }

    #[test]
    fn portrait_thumbnail_pillarboxes_horizontally() {
        let frame = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(110.0, 62.0));
        let fitted = compute_fitted_thumbnail_rect(frame, egui::vec2(100.0, 200.0));
        assert!((fitted.height() - 62.0).abs() < 1e-3);
        assert!((fitted.width() - 31.0).abs() < 1e-3);
        assert!((fitted.center().x - frame.center().x).abs() < 1e-3);
    }
}
