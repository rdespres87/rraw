#![forbid(unsafe_code)]

//! Interactive multi-channel PCHIP parametric tone curve widget.

use egui::{Color32, Pos2, Stroke, Vec2};
use rraw_color::curves::PchipSpline;
use rraw_core::curves::{ChannelCurve, CurvePoint};

use crate::app::RrawApp;
use crate::state::CurveChannel;

impl RrawApp {
    /// Returns a mutable reference to the currently active channel curve.
    fn active_curve_mut(&mut self) -> &mut ChannelCurve {
        match self.active_curve_channel {
            CurveChannel::CombinedRgb => &mut self.grading.tone_curves.combined_rgb,
            CurveChannel::Red => &mut self.grading.tone_curves.red,
            CurveChannel::Green => &mut self.grading.tone_curves.green,
            CurveChannel::Blue => &mut self.grading.tone_curves.blue,
            CurveChannel::Luma => &mut self.grading.tone_curves.luma,
        }
    }

    /// Returns a shared reference to the currently active channel curve.
    fn active_curve(&self) -> &ChannelCurve {
        match self.active_curve_channel {
            CurveChannel::CombinedRgb => &self.grading.tone_curves.combined_rgb,
            CurveChannel::Red => &self.grading.tone_curves.red,
            CurveChannel::Green => &self.grading.tone_curves.green,
            CurveChannel::Blue => &self.grading.tone_curves.blue,
            CurveChannel::Luma => &self.grading.tone_curves.luma,
        }
    }

    /// Color representing the active curve in canvas rendering.
    fn active_curve_color(&self) -> Color32 {
        match self.active_curve_channel {
            CurveChannel::CombinedRgb => Color32::from_rgb(240, 240, 245),
            CurveChannel::Red => Color32::from_rgb(255, 90, 90),
            CurveChannel::Green => Color32::from_rgb(90, 230, 110),
            CurveChannel::Blue => Color32::from_rgb(90, 160, 255),
            CurveChannel::Luma => Color32::from_rgb(255, 215, 80),
        }
    }

    /// Renders the tone curves inspector section with interactive spline canvas.
    pub fn render_tone_curves_section(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // 1. Channel selector tabs
            ui.horizontal(|ui| {
                for ch in CurveChannel::ALL {
                    let is_active = self.active_curve_channel == ch;
                    if ui.selectable_label(is_active, ch.label()).clicked() {
                        self.active_curve_channel = ch;
                        self.dragging_curve_point = None;
                    }
                }
            });

            ui.add_space(4.0);

            // 2. Interactive Canvas
            let canvas_dim = (ui.available_width() - 12.0).clamp(180.0, 280.0);
            let canvas_size = Vec2::splat(canvas_dim);
            let (response, painter) =
                ui.allocate_painter(canvas_size, egui::Sense::click_and_drag());
            let rect = response.rect;

            // Background
            painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 30));
            painter.rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(50, 55, 65)),
                egui::StrokeKind::Inside,
            );

            // Grid lines (25%, 50%, 75%)
            let grid_color = Color32::from_rgb(38, 42, 50);
            for frac in [0.25, 0.50, 0.75] {
                let gx = rect.left() + rect.width() * frac;
                let gy = rect.top() + rect.height() * frac;
                painter.line_segment(
                    [Pos2::new(gx, rect.top()), Pos2::new(gx, rect.bottom())],
                    Stroke::new(1.0, grid_color),
                );
                painter.line_segment(
                    [Pos2::new(rect.left(), gy), Pos2::new(rect.right(), gy)],
                    Stroke::new(1.0, grid_color),
                );
            }

            // Diagonal identity guide
            painter.line_segment(
                [
                    Pos2::new(rect.left(), rect.bottom()),
                    Pos2::new(rect.right(), rect.top()),
                ],
                Stroke::new(1.0, Color32::from_rgb(45, 50, 60)),
            );

            // Live histogram underlay in background
            let hist_bins = match self.active_curve_channel {
                CurveChannel::Red => &self.histogram.r[..],
                CurveChannel::Green => &self.histogram.g[..],
                CurveChannel::Blue => &self.histogram.b[..],
                _ => &self.histogram.lum[..],
            };
            let max_bin = self.histogram.max_count.max(1) as f32;
            let hist_color = Color32::from_rgba_unmultiplied(100, 110, 130, 45);

            for (i, &count) in hist_bins.iter().enumerate() {
                let bin_frac = i as f32 / 255.0;
                let bx = rect.left() + bin_frac * rect.width();
                let bh = (count as f32 / max_bin).clamp(0.0, 1.0) * rect.height() * 0.75;
                let by = rect.bottom() - bh;
                painter.line_segment(
                    [Pos2::new(bx, rect.bottom()), Pos2::new(bx, by)],
                    Stroke::new(1.0, hist_color),
                );
            }

            // 3. Draw the spline curve
            let spline = PchipSpline::new(self.active_curve());
            let curve_color = self.active_curve_color();
            let steps = 100;
            let mut prev_pos: Option<Pos2> = None;

            for s in 0..=steps {
                let x_norm = s as f32 / steps as f32;
                let y_norm = spline.evaluate(x_norm);

                let canvas_x = rect.left() + x_norm * rect.width();
                let canvas_y = rect.bottom() - y_norm * rect.height();
                let cur_pos = Pos2::new(canvas_x, canvas_y);

                if let Some(prev) = prev_pos {
                    painter.line_segment([prev, cur_pos], Stroke::new(2.0, curve_color));
                }
                prev_pos = Some(cur_pos);
            }

            // 4. Draw control points and handle mouse drag / click interactions
            let pointer_pos = response
                .interact_pointer_pos()
                .or_else(|| ui.input(|i| i.pointer.interact_pos()));
            let mut point_to_remove: Option<usize> = None;
            let mut modified = false;

            let pts = self.active_curve().points.clone();
            for (idx, pt) in pts.iter().enumerate() {
                let px = rect.left() + pt.x * rect.width();
                let py = rect.bottom() - pt.y * rect.height();
                let pt_canvas = Pos2::new(px, py);

                let is_dragging = self.dragging_curve_point == Some(idx);
                let is_hovered = pointer_pos.is_some_and(|pos| pos.distance(pt_canvas) < 8.0);

                let fill = if is_dragging {
                    Color32::WHITE
                } else if is_hovered {
                    curve_color
                } else {
                    Color32::from_rgb(20, 22, 26)
                };
                let radius = if is_dragging || is_hovered { 6.0 } else { 4.5 };

                painter.circle_filled(pt_canvas, radius, fill);
                painter.circle_stroke(pt_canvas, radius, Stroke::new(1.5, curve_color));

                // Right-click or double-click to remove interior point
                if is_hovered
                    && (response.secondary_clicked() || response.double_clicked())
                    && idx > 0
                    && idx < pts.len() - 1
                {
                    point_to_remove = Some(idx);
                }
            }

            // Handle removing point
            if let Some(idx) = point_to_remove {
                self.active_curve_mut().remove_point(idx);
                self.dragging_curve_point = None;
                modified = true;
            }

            // Handle pointer press / drag start on canvas
            let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
            let primary_down = ui.input(|i| i.pointer.primary_down());

            if (response.drag_started() || (response.hovered() && primary_pressed))
                && self.dragging_curve_point.is_none()
            {
                if let Some(pos) = pointer_pos {
                    if rect.contains(pos) {
                        let mut closest_idx = None;
                        let mut closest_dist = 14.0_f32;

                        for (idx, pt) in self.active_curve().points.iter().enumerate() {
                            let px = rect.left() + pt.x * rect.width();
                            let py = rect.bottom() - pt.y * rect.height();
                            let dist = pos.distance(Pos2::new(px, py));
                            if dist < closest_dist {
                                closest_dist = dist;
                                closest_idx = Some(idx);
                            }
                        }

                        if let Some(idx) = closest_idx {
                            self.dragging_curve_point = Some(idx);
                        } else {
                            // Clicked/pressed on empty canvas: add new control point and immediately grab it
                            let new_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                            let new_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
                            if let Ok(new_pt) = CurvePoint::new(new_x, new_y) {
                                let inserted_idx = self.active_curve_mut().add_point(new_pt);
                                self.dragging_curve_point = Some(inserted_idx);
                                modified = true;
                            }
                        }
                    }
                }
            }

            // Handle ongoing dragging
            if (response.dragged() || primary_down) && self.dragging_curve_point.is_some() {
                if let (Some(drag_idx), Some(pos)) = (self.dragging_curve_point, pointer_pos) {
                    let len = self.active_curve().points.len();
                    if drag_idx < len {
                        let is_first = drag_idx == 0;
                        let is_last = drag_idx == len - 1;

                        let new_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
                        let new_x = if is_first {
                            0.0
                        } else if is_last {
                            1.0
                        } else {
                            let prev_x = self.active_curve().points[drag_idx - 1].x;
                            let next_x = self.active_curve().points[drag_idx + 1].x;
                            let min_x = (prev_x + 0.002).min(1.0);
                            let max_x = (next_x - 0.002).max(min_x);
                            let raw_x = (pos.x - rect.left()) / rect.width();
                            if min_x < max_x {
                                raw_x.clamp(min_x, max_x)
                            } else {
                                self.active_curve().points[drag_idx].x
                            }
                        };

                        if let Ok(pt) = CurvePoint::new(new_x, new_y) {
                            self.active_curve_mut().points[drag_idx] = pt;
                            modified = true;
                        }
                    }
                }
            }

            // Stop dragging on release
            if response.drag_stopped() || ui.input(|i| i.pointer.primary_released()) {
                self.dragging_curve_point = None;
            }

            if modified {
                self.needs_preview_update = true;
            }

            ui.add_space(6.0);

            // 5. Action controls (Reset buttons)
            ui.horizontal(|ui| {
                if ui
                    .button("↺ Réinitialiser canal")
                    .on_hover_text("Réinitialise la courbe du canal actif en ligne droite")
                    .clicked()
                {
                    *self.active_curve_mut() = ChannelCurve::new_linear();
                    self.dragging_curve_point = None;
                    self.needs_preview_update = true;
                }

                if ui
                    .button("Tout réinitialiser")
                    .on_hover_text("Réinitialise toutes les courbes RVB et Luma")
                    .clicked()
                {
                    self.grading.tone_curves = rraw_core::curves::ToneCurvesRecipe::default();
                    self.dragging_curve_point = None;
                    self.needs_preview_update = true;
                }
            });
        });
    }
}
