#![forbid(unsafe_code)]

//! Live RGB and luminance histogram widget.

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::Key;

impl RrawApp {
    /// Renders an embellished live RGB / Luminance histogram widget inside `ui`.
    pub fn render_histogram_widget(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(self.t(Key::SidebarHistogramTitle))
                        .strong()
                        .size(11.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.histogram.out_of_gamut_count > 0
                        && (self.grading.soft_proof.enabled
                            || self.grading.soft_proof.show_gamut_warning)
                    {
                        ui.label(
                            egui::RichText::new(format!(
                                "⚠️ Gamut ({})",
                                self.histogram.out_of_gamut_count
                            ))
                            .small()
                            .color(Color32::from_rgb(255, 110, 230)),
                        );
                    }
                    // Clickable clipping toggles: paint blown highlights / crushed shadows on canvas
                    let hl_on = self.clipping_overlay.highlights;
                    let hl_text =
                        egui::RichText::new(format!("HL {}", self.histogram.highlight_clipped))
                            .small()
                            .color(Color32::from_rgb(255, 90, 90));
                    if ui
                        .selectable_label(hl_on, hl_text)
                        .on_hover_text(self.t(Key::HistogramHighlightClippingTooltip))
                        .clicked()
                    {
                        self.clipping_overlay.highlights = !hl_on;
                        self.needs_preview_update = true;
                    }
                    let sh_on = self.clipping_overlay.shadows;
                    let sh_text =
                        egui::RichText::new(format!("Noirs {}", self.histogram.shadow_clipped))
                            .small()
                            .color(Color32::from_rgb(80, 160, 255));
                    if ui
                        .selectable_label(sh_on, sh_text)
                        .on_hover_text(self.t(Key::HistogramShadowClippingTooltip))
                        .clicked()
                    {
                        self.clipping_overlay.shadows = !sh_on;
                        self.needs_preview_update = true;
                    }
                });
            });

            let hist_height = 84.0;
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), hist_height),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(rect);

            // Deep OLED rounded background card with subtle border
            painter.rect_filled(rect, 4.0, Color32::from_rgb(12, 13, 16));
            painter.rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0, Color32::from_rgb(38, 42, 50)),
                egui::StrokeKind::Outside,
            );

            // 4 Tonal zones: Noirs (0-25%), Ombres (25-50%), Exposition (50-75%), Hautes Lum. (75-100%)
            for quartile in [0.25_f32, 0.50, 0.75] {
                let x = rect.left() + quartile * rect.width();
                painter.line_segment(
                    [
                        egui::pos2(x, rect.top() + 2.0),
                        egui::pos2(x, rect.bottom() - 14.0),
                    ],
                    egui::Stroke::new(1.0, Color32::from_rgb(28, 30, 36)),
                );
            }

            // Sqrt-scaled height for beautiful smooth curves
            let max_c = (self.histogram.max_count as f32).sqrt().max(1.0);

            let channel_points = |bins: &[u32; 256]| -> Vec<egui::Pos2> {
                bins.iter()
                    .enumerate()
                    .map(|(i, &count)| {
                        let x = rect.left() + (i as f32 / 255.0) * rect.width();
                        let height_ratio = ((count as f32).sqrt() / max_c).clamp(0.0, 1.0);
                        let y = rect.bottom() - 14.0 - height_ratio * (rect.height() - 18.0);
                        egui::pos2(x, y)
                    })
                    .collect()
            };

            // Luminance backdrop filled shape
            let mut lum_polygon = vec![egui::pos2(rect.left(), rect.bottom() - 14.0)];
            lum_polygon.extend(channel_points(&self.histogram.lum));
            lum_polygon.push(egui::pos2(rect.right(), rect.bottom() - 14.0));
            painter.add(egui::Shape::convex_polygon(
                lum_polygon,
                Color32::from_rgba_unmultiplied(210, 220, 235, 38),
                egui::Stroke::NONE,
            ));

            // Translucent glowing RGB curves
            painter.add(egui::Shape::line(
                channel_points(&self.histogram.r),
                egui::Stroke::new(1.3, Color32::from_rgba_unmultiplied(255, 75, 75, 200)),
            ));
            painter.add(egui::Shape::line(
                channel_points(&self.histogram.g),
                egui::Stroke::new(1.3, Color32::from_rgba_unmultiplied(75, 230, 85, 200)),
            ));
            painter.add(egui::Shape::line(
                channel_points(&self.histogram.b),
                egui::Stroke::new(1.3, Color32::from_rgba_unmultiplied(75, 160, 255, 200)),
            ));

            // Zone labels at the bottom of the card
            let label_col = Color32::from_rgb(85, 90, 105);
            let label_font = egui::FontId::proportional(8.5);
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.125, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Noirs",
                label_font.clone(),
                label_col,
            );
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.375, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Ombres",
                label_font.clone(),
                label_col,
            );
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.625, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Exposition",
                label_font.clone(),
                label_col,
            );
            painter.text(
                egui::pos2(rect.left() + rect.width() * 0.875, rect.bottom() - 7.0),
                egui::Align2::CENTER_CENTER,
                "Hautes Lum.",
                label_font,
                label_col,
            );

            if response.hovered() {
                if let Some(pos) = response.hover_pos() {
                    let pct = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let bin_idx = (pct * 255.0).round() as usize;
                    let lum_v = self.histogram.lum[bin_idx.min(255)];
                    painter.line_segment(
                        [
                            egui::pos2(pos.x, rect.top()),
                            egui::pos2(pos.x, rect.bottom() - 14.0),
                        ],
                        egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120)),
                    );
                    let tip = format!("Niveau: {} | Pixels: {}", bin_idx, lum_v);
                    painter.text(
                        egui::pos2(
                            pos.x.clamp(rect.left() + 45.0, rect.right() - 45.0),
                            rect.top() + 6.0,
                        ),
                        egui::Align2::CENTER_TOP,
                        tip,
                        egui::FontId::monospace(9.5),
                        Color32::WHITE,
                    );
                }
            }
        });
    }
}
