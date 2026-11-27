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
        match self.viewport.curve_editor.active_channel {
            CurveChannel::CombinedRgb => &mut self.session.grading.tone_curves.combined_rgb,
            CurveChannel::Red => &mut self.session.grading.tone_curves.red,
            CurveChannel::Green => &mut self.session.grading.tone_curves.green,
            CurveChannel::Blue => &mut self.session.grading.tone_curves.blue,
            CurveChannel::Luma => &mut self.session.grading.tone_curves.luma,
        }
    }

    /// Returns a shared reference to the currently active channel curve.
    fn active_curve(&self) -> &ChannelCurve {
        match self.viewport.curve_editor.active_channel {
            CurveChannel::CombinedRgb => &self.session.grading.tone_curves.combined_rgb,
            CurveChannel::Red => &self.session.grading.tone_curves.red,
            CurveChannel::Green => &self.session.grading.tone_curves.green,
            CurveChannel::Blue => &self.session.grading.tone_curves.blue,
            CurveChannel::Luma => &self.session.grading.tone_curves.luma,
        }
    }

    /// Color representing the active curve in canvas rendering.
    fn active_curve_color(&self) -> Color32 {
        match self.viewport.curve_editor.active_channel {
            CurveChannel::CombinedRgb => Color32::from_rgb(240, 240, 245),
            CurveChannel::Red => Color32::from_rgb(255, 90, 90),
            CurveChannel::Green => Color32::from_rgb(90, 230, 110),
            CurveChannel::Blue => Color32::from_rgb(90, 160, 255),
            CurveChannel::Luma => Color32::from_rgb(255, 215, 80),
        }
    }

    /// Checks whether a specific channel's curve is modified from identity.
    fn is_channel_modified(&self, ch: CurveChannel) -> bool {
        match ch {
            CurveChannel::CombinedRgb => {
                !self.session.grading.tone_curves.combined_rgb.is_identity()
            }
            CurveChannel::Red => !self.session.grading.tone_curves.red.is_identity(),
            CurveChannel::Green => !self.session.grading.tone_curves.green.is_identity(),
            CurveChannel::Blue => !self.session.grading.tone_curves.blue.is_identity(),
            CurveChannel::Luma => !self.session.grading.tone_curves.luma.is_identity(),
        }
    }

    /// Resets the currently active tone curve channel to linear identity, recording an Undo snapshot and marking sidecars dirty.
    pub fn reset_active_tone_curve(&mut self) {
        if self.active_curve().is_identity() {
            return;
        }
        self.record_undo_snapshot();
        *self.active_curve_mut() = ChannelCurve::new_linear();
        self.viewport.curve_editor.dragging_point = None;
        self.session.needs_preview_update = true;
        self.session.sidecar_dirty = true;
    }

    /// Resets all tone curve channels to linear identity, recording an Undo snapshot and marking sidecars dirty.
    pub fn reset_all_tone_curves(&mut self) {
        if self.session.grading.tone_curves.is_identity() {
            return;
        }
        self.record_undo_snapshot();
        self.session.grading.tone_curves = rraw_core::curves::ToneCurvesRecipe::default();
        self.viewport.curve_editor.dragging_point = None;
        self.session.needs_preview_update = true;
        self.session.sidecar_dirty = true;
    }

    /// Renders the tone curves inspector section with interactive spline canvas.
    pub fn render_tone_curves_section(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            self.render_channel_tabs(ui);
            ui.add_space(4.0);

            let canvas_dim = (ui.available_width() - 12.0).clamp(180.0, 280.0);
            let (response, painter) =
                ui.allocate_painter(Vec2::splat(canvas_dim), egui::Sense::click_and_drag());
            let rect = response.rect;

            self.paint_curve_canvas_background(&painter, rect);

            let spline = PchipSpline::new(self.active_curve());
            let curve_color = self.active_curve_color();
            self.paint_curve_splines(&painter, rect, &spline, curve_color);

            let pointer_pos = response
                .interact_pointer_pos()
                .or_else(|| ui.input(|i| i.pointer.interact_pos()));

            let (readout_coords, is_point_readout, point_to_remove) = self
                .render_control_points_and_hover(
                    rect,
                    &painter,
                    &response,
                    pointer_pos,
                    &spline,
                    curve_color,
                );

            self.handle_curve_pointer_interaction(
                ui,
                &response,
                rect,
                pointer_pos,
                point_to_remove,
            );

            ui.add_space(3.0);
            self.render_curve_readout_and_actions(ui, readout_coords, is_point_readout);
        });
    }

    /// Draws the channel selector tabs (RGB, R, G, B, Luma) with modification indicators.
    fn render_channel_tabs(&mut self, ui: &mut egui::Ui) {
        let cur_lang = self.lang();
        ui.horizontal(|ui| {
            for ch in CurveChannel::ALL {
                let is_active = self.viewport.curve_editor.active_channel == ch;
                let is_mod = self.is_channel_modified(ch);
                let label = if is_mod {
                    format!("{} •", ch.label_lang(cur_lang))
                } else {
                    ch.label_lang(cur_lang).to_string()
                };
                if ui.selectable_label(is_active, label).clicked() {
                    self.viewport.curve_editor.active_channel = ch;
                    self.viewport.curve_editor.dragging_point = None;
                }
            }
        });
    }

    /// Paints the curve canvas background: dark fill, border, grid lines, diagonal, and histogram underlay.
    fn paint_curve_canvas_background(&self, painter: &egui::Painter, rect: egui::Rect) {
        painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 30));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, Color32::from_rgb(50, 55, 65)),
            egui::StrokeKind::Inside,
        );

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

        painter.line_segment(
            [
                Pos2::new(rect.left(), rect.bottom()),
                Pos2::new(rect.right(), rect.top()),
            ],
            Stroke::new(1.0, Color32::from_rgb(45, 50, 60)),
        );

        let (hist_bins, hist_color) = match self.viewport.curve_editor.active_channel {
            CurveChannel::Red => (
                &self.viewport.histogram.r[..],
                Color32::from_rgba_unmultiplied(225, 80, 80, 48),
            ),
            CurveChannel::Green => (
                &self.viewport.histogram.g[..],
                Color32::from_rgba_unmultiplied(75, 205, 95, 48),
            ),
            CurveChannel::Blue => (
                &self.viewport.histogram.b[..],
                Color32::from_rgba_unmultiplied(75, 150, 235, 48),
            ),
            CurveChannel::CombinedRgb => (
                &self.viewport.histogram.lum[..],
                Color32::from_rgba_unmultiplied(160, 165, 185, 45),
            ),
            CurveChannel::Luma => (
                &self.viewport.histogram.lum[..],
                Color32::from_rgba_unmultiplied(235, 200, 70, 48),
            ),
        };

        let max_c = (self.viewport.histogram.max_count as f32).sqrt().max(1.0);
        let bin_denom = hist_bins.len().saturating_sub(1).max(1) as f32;
        for (i, &count) in hist_bins.iter().enumerate() {
            let bin_frac = i as f32 / bin_denom;
            let bx = rect.left() + bin_frac * rect.width();
            let bh = ((count as f32).sqrt() / max_c).clamp(0.0, 1.0) * rect.height() * 0.82;
            let by = rect.bottom() - bh;
            painter.line_segment(
                [Pos2::new(bx, rect.bottom()), Pos2::new(bx, by)],
                Stroke::new(1.0, hist_color),
            );
        }
    }

    /// Paints the ghost master curve (when editing a non-RGB channel) and the active curve spline.
    fn paint_curve_splines(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        active_spline: &PchipSpline,
        curve_color: Color32,
    ) {
        if self.viewport.curve_editor.active_channel != CurveChannel::CombinedRgb {
            let master_spline = PchipSpline::new(&self.session.grading.tone_curves.combined_rgb);
            let ghost_color = Color32::from_rgba_unmultiplied(200, 205, 220, 50);
            Self::paint_spline_polyline(painter, rect, &master_spline, ghost_color, 1.2);
        }

        Self::paint_spline_polyline(painter, rect, active_spline, curve_color, 2.0);
    }

    /// Samples a `PchipSpline` at 101 points across `[0, 1]` and draws it into `rect`.
    fn paint_spline_polyline(
        painter: &egui::Painter,
        rect: egui::Rect,
        spline: &PchipSpline,
        color: Color32,
        stroke_width: f32,
    ) {
        let steps = 100;
        let stroke = Stroke::new(stroke_width, color);
        let mut prev_pos: Option<Pos2> = None;
        for s in 0..=steps {
            let x_norm = s as f32 / steps as f32;
            let y_norm = spline.evaluate(x_norm).clamp(0.0, 1.0);
            let pos = Pos2::new(
                rect.left() + x_norm * rect.width(),
                rect.bottom() - y_norm * rect.height(),
            );
            if let Some(prev) = prev_pos {
                painter.line_segment([prev, pos], stroke);
            }
            prev_pos = Some(pos);
        }
    }

    /// Renders control-point handles and the hover evaluation guide on the curve canvas.
    fn render_control_points_and_hover(
        &self,
        rect: egui::Rect,
        painter: &egui::Painter,
        response: &egui::Response,
        pointer_pos: Option<Pos2>,
        spline: &PchipSpline,
        curve_color: Color32,
    ) -> (Option<(f32, f32)>, bool, Option<usize>) {
        let mut point_to_remove: Option<usize> = None;
        let mut readout_coords: Option<(f32, f32)> = None;
        let mut is_point_readout = false;

        let points = &self.active_curve().points;
        for (idx, pt) in points.iter().enumerate() {
            let px = rect.left() + pt.x * rect.width();
            let py = rect.bottom() - pt.y * rect.height();
            let pt_canvas = Pos2::new(px, py);

            let is_dragging = self.viewport.curve_editor.dragging_point == Some(idx);
            let is_hovered = pointer_pos.is_some_and(|pos| pos.distance(pt_canvas) < 8.0);

            if is_dragging || is_hovered {
                readout_coords = Some((pt.x, pt.y));
                is_point_readout = true;
            }

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

            if is_hovered
                && (response.secondary_clicked() || response.double_clicked())
                && idx > 0
                && idx < points.len() - 1
            {
                point_to_remove = Some(idx);
            }
        }

        if readout_coords.is_none() {
            if let Some(pos) = pointer_pos {
                if rect.contains(pos) {
                    let cursor_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let eval_y = spline.evaluate(cursor_x).clamp(0.0, 1.0);
                    readout_coords = Some((cursor_x, eval_y));

                    let marker_pos = Pos2::new(
                        rect.left() + cursor_x * rect.width(),
                        rect.bottom() - eval_y * rect.height(),
                    );
                    painter.line_segment(
                        [
                            Pos2::new(marker_pos.x, rect.top()),
                            Pos2::new(marker_pos.x, rect.bottom()),
                        ],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 130, 150, 40)),
                    );
                    painter.circle_filled(
                        marker_pos,
                        3.0,
                        Color32::from_rgba_unmultiplied(220, 225, 235, 160),
                    );
                }
            }
        }

        (readout_coords, is_point_readout, point_to_remove)
    }

    /// Handles pointer-driven mutation of the active curve (removal, insertion, dragging, and release).
    fn handle_curve_pointer_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        pointer_pos: Option<Pos2>,
        point_to_remove: Option<usize>,
    ) {
        let mut modified = false;

        if let Some(idx) = point_to_remove {
            self.active_curve_mut().remove_point(idx);
            self.viewport.curve_editor.dragging_point = None;
            modified = true;
        }

        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let primary_down = ui.input(|i| i.pointer.primary_down());

        if (response.drag_started() || (response.hovered() && primary_pressed))
            && self.viewport.curve_editor.dragging_point.is_none()
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
                        self.viewport.curve_editor.dragging_point = Some(idx);
                    } else {
                        let new_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                        let new_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
                        if let Ok(new_pt) = CurvePoint::new(new_x, new_y) {
                            let inserted_idx = self.active_curve_mut().add_point(new_pt);
                            self.viewport.curve_editor.dragging_point = Some(inserted_idx);
                            modified = true;
                        }
                    }
                }
            }
        }

        if (response.dragged() || primary_down)
            && self.viewport.curve_editor.dragging_point.is_some()
        {
            if let (Some(drag_idx), Some(pos)) =
                (self.viewport.curve_editor.dragging_point, pointer_pos)
            {
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

        if response.drag_stopped() || ui.input(|i| i.pointer.primary_released()) {
            self.viewport.curve_editor.dragging_point = None;
        }

        if modified {
            self.session.needs_preview_update = true;
        }
    }

    /// Renders the numerical In/Out readout bar and the Reset / Reset All action buttons.
    fn render_curve_readout_and_actions(
        &mut self,
        ui: &mut egui::Ui,
        readout_coords: Option<(f32, f32)>,
        is_point_readout: bool,
    ) {
        ui.horizontal(|ui| {
            if let Some((in_val, out_val)) = readout_coords {
                let prefix = if is_point_readout { "Point" } else { "Courbe" };
                ui.label(
                    egui::RichText::new(format!(
                        "{} • Entrée: {:>4.1}%  Sortie: {:>4.1}%",
                        prefix,
                        in_val * 100.0,
                        out_val * 100.0
                    ))
                    .monospace()
                    .size(11.0)
                    .color(Color32::from_rgb(205, 215, 230)),
                );
            } else {
                ui.label(
                    egui::RichText::new(self.t(crate::i18n::Key::ToneCurvesHoverPrompt))
                        .small()
                        .color(Color32::from_rgb(120, 128, 142)),
                );
            }
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui
                .button(self.t(crate::i18n::Key::InspectorResetTone))
                .on_hover_text(self.t(crate::i18n::Key::InspectorResetTone))
                .clicked()
            {
                self.reset_active_tone_curve();
            }

            if ui
                .button(self.t(crate::i18n::Key::ToneCurvesResetAll))
                .on_hover_text(self.t(crate::i18n::Key::ToneCurvesResetAllTooltip))
                .clicked()
            {
                self.reset_all_tone_curves();
            }
        });
    }
}
