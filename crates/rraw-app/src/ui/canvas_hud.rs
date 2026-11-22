#![forbid(unsafe_code)]

//! Top-bar display toggles and the translucent shooting-parameter HUD painted over the canvas.

use egui::{Color32, FontId, Rect, Sense};

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::view_tools::shooting_summary;

const HUD_FILL: Color32 = Color32::from_black_alpha(170);
const HUD_MARGIN: f32 = 12.0;
const HUD_PADDING: egui::Vec2 = egui::vec2(10.0, 6.0);
const STAR_LIT: Color32 = Color32::from_rgb(255, 200, 50);
const STAR_DIM: Color32 = Color32::from_rgb(120, 125, 135);
const STAR_SPACING: f32 = 18.0;

impl RrawApp {
    /// Compact viewport display toggles rendered in the top bar.
    pub fn render_view_toggles(&mut self, ui: &mut egui::Ui) {
        if ui
            .selectable_label(self.viewport.split_compare, self.t(Key::ToggleBeforeAfter))
            .on_hover_text(self.t(Key::ToggleBeforeAfterTooltip))
            .clicked()
        {
            self.viewport.split_compare = !self.viewport.split_compare;
        }

        if ui
            .selectable_label(
                self.viewport.clipping_overlay.is_active(),
                self.t(Key::ToggleClipping),
            )
            .on_hover_text(self.t(Key::ToggleClippingTooltip))
            .clicked()
        {
            self.toggle_clipping_overlay();
        }

        if ui
            .selectable_label(
                self.layout_config.show_canvas_hud,
                self.t(Key::ToggleInfoHud),
            )
            .on_hover_text(self.t(Key::ToggleInfoHudTooltip))
            .clicked()
        {
            self.layout_config.show_canvas_hud = !self.layout_config.show_canvas_hud;
            self.layout_config.save();
        }

        if ui
            .selectable_label(
                self.layout_config.filmstrip_visible,
                self.t(Key::ToggleFilmstrip),
            )
            .on_hover_text(self.t(Key::ToggleFilmstripTooltip))
            .clicked()
        {
            self.layout_config.filmstrip_visible = !self.layout_config.filmstrip_visible;
            self.layout_config.save();
        }

        if ui
            .selectable_label(
                self.export.show_shortcuts_dialog,
                self.t(Key::ToggleShortcuts),
            )
            .on_hover_text(self.t(Key::ToggleShortcutsTooltip))
            .clicked()
        {
            self.export.show_shortcuts_dialog = !self.export.show_shortcuts_dialog;
        }
    }

    /// Paints the translucent HUD (file name, shooting parameters, rating) inside `canvas`.
    /// Returns the rating clicked by the user, if any, so the caller can persist it.
    pub fn render_canvas_hud(&self, ui: &egui::Ui, canvas: Rect) -> Option<u8> {
        if !self.layout_config.show_canvas_hud {
            return None;
        }
        let photo = self.session.loaded_photo.as_ref()?;
        let painter = ui.painter().with_clip_rect(canvas);

        // Bottom-left information card
        let file_name = photo
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let summary = shooting_summary(
            &photo.focal_length_str,
            &photo.aperture_str,
            &photo.shutter_str,
            photo.iso,
        );
        let gear = [photo.camera_model.trim(), photo.lens_model.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");

        let lines = [
            (file_name, FontId::proportional(13.0), Color32::WHITE),
            (
                summary.as_str(),
                FontId::monospace(12.0),
                Color32::from_rgb(220, 225, 235),
            ),
            (
                gear.as_str(),
                FontId::proportional(11.0),
                Color32::from_rgb(150, 155, 165),
            ),
        ];
        let galleys: Vec<_> = lines
            .into_iter()
            .filter(|(text, ..)| !text.is_empty())
            .map(|(text, font, col)| painter.layout_no_wrap(text.to_owned(), font, col))
            .collect();

        if !galleys.is_empty() {
            let width = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max);
            let height: f32 = galleys.iter().map(|g| g.size().y).sum::<f32>()
                + 2.0 * (galleys.len().saturating_sub(1)) as f32;
            let card = Rect::from_min_size(
                egui::pos2(
                    canvas.min.x + HUD_MARGIN,
                    canvas.max.y - HUD_MARGIN - height - HUD_PADDING.y * 2.0,
                ),
                egui::vec2(width, height) + HUD_PADDING * 2.0,
            );
            painter.rect_filled(card, 6.0, HUD_FILL);
            let mut y = card.min.y + HUD_PADDING.y;
            for g in galleys {
                let h = g.size().y;
                painter.galley(egui::pos2(card.min.x + HUD_PADDING.x, y), g, Color32::WHITE);
                y += h + 2.0;
            }
        }

        // Bottom-right interactive rating pill
        let pill = Rect::from_min_size(
            egui::pos2(
                canvas.max.x - HUD_MARGIN - STAR_SPACING * 5.0 - HUD_PADDING.x * 2.0,
                canvas.max.y - HUD_MARGIN - 28.0,
            ),
            egui::vec2(STAR_SPACING * 5.0 + HUD_PADDING.x * 2.0, 28.0),
        );
        painter.rect_filled(pill, 14.0, HUD_FILL);

        let current = photo.rating;
        let star_rects: Vec<(u8, Rect)> = (1..=5_u8)
            .map(|s| {
                let cx = pill.min.x + HUD_PADDING.x + STAR_SPACING * (f32::from(s) - 0.5);
                (
                    s,
                    Rect::from_center_size(
                        egui::pos2(cx, pill.center().y),
                        egui::vec2(STAR_SPACING, 24.0),
                    ),
                )
            })
            .collect();

        let responses: Vec<_> = star_rects
            .iter()
            .map(|&(s, r)| {
                (
                    s,
                    r,
                    ui.interact(r, ui.auto_id_with(("canvas_hud_star", s)), Sense::click()),
                )
            })
            .collect();
        let hovered = responses
            .iter()
            .find(|(_, _, resp)| resp.hovered())
            .map(|(s, ..)| *s);

        let mut clicked = None;
        for (s, r, resp) in responses {
            let lit = hovered.map_or(s <= current, |h| s <= h);
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                if lit { "★" } else { "☆" },
                FontId::proportional(16.0),
                if lit { STAR_LIT } else { STAR_DIM },
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                clicked = Some(if current == s { 0 } else { s });
            }
        }
        clicked
    }
}
