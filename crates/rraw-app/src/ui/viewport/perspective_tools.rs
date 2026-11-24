#![forbid(unsafe_code)]

//! Interactive perspective leveling and parallel lines tools.

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};

use super::ViewportToolContext;
use crate::grading::GeometryGradingParams;
use crate::i18n::{Key, Language};
use crate::state::ParallelToolPhase;
use crate::ui::sections::geometry::ParallelLinesToolState;

/// Output result from perspective tool interactions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PerspectiveToolOutput {
    pub mark_custom_preset: bool,
    pub needs_preview_update: bool,
    pub save_sidecar: bool,
}

fn draw_parallel_tool_banner(
    painter: &egui::Painter,
    available_rect: Rect,
    phase: ParallelToolPhase,
    cur_lang: Language,
) {
    let instruction = match phase {
        ParallelToolPhase::DrawingLine1 => Key::GeometryParallelStatus1.translate(cur_lang),
        ParallelToolPhase::WaitingLine2 | ParallelToolPhase::DrawingLine2 => {
            Key::GeometryParallelStatus2.translate(cur_lang)
        }
        ParallelToolPhase::Inactive => "",
    };
    painter.rect_filled(
        Rect::from_min_size(
            available_rect.left_top() + Vec2::new(16.0, 16.0),
            Vec2::new(480.0, 32.0),
        ),
        6.0,
        Color32::from_rgba_unmultiplied(16, 20, 28, 230),
    );
    painter.text(
        available_rect.left_top() + Vec2::new(28.0, 32.0),
        egui::Align2::LEFT_CENTER,
        format!("📐 {instruction}"),
        egui::FontId::proportional(14.0),
        Color32::from_rgb(0, 240, 255),
    );
}

fn draw_parallel_line_overlay(
    painter: &egui::Painter,
    p1: Pos2,
    p2: Pos2,
    color: Color32,
    label: Option<&str>,
) {
    painter.line_segment([p1, p2], Stroke::new(2.5, color));
    painter.circle_filled(p1, 5.0, color);
    painter.circle_filled(p2, 5.0, color);
    if let Some(text) = label {
        painter.text(
            (p1 + p2.to_vec2()) * 0.5 + Vec2::new(10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
}

/// Dominant orientation detected from two user-drawn perspective lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelCorrectionAxis {
    Vertical,
    Horizontal,
}

/// Perspective correction computed from two drawn reference lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParallelLinesCorrection {
    pub axis: ParallelCorrectionAxis,
    pub keystone: f32,
    pub rotation_degrees: f32,
}

fn compute_vertical_correction(
    line1: (Pos2, Pos2),
    line2: (Pos2, Pos2),
) -> ParallelLinesCorrection {
    let (l1_a, l1_b) = line1;
    let (l2_a, l2_b) = line2;
    let (a1, b1) = if l1_b.y < l1_a.y {
        (l1_a, l1_b)
    } else {
        (l1_b, l1_a)
    };
    let (a2, b2) = if l2_b.y < l2_a.y {
        (l2_a, l2_b)
    } else {
        (l2_b, l2_a)
    };

    let slope1 = (b1.x - a1.x) / (a1.y - b1.y).max(1.0);
    let slope2 = (b2.x - a2.x) / (a2.y - b2.y).max(1.0);
    let (left_slope, right_slope) = if a1.x < a2.x {
        (slope1, slope2)
    } else {
        (slope2, slope1)
    };

    let convergence = right_slope - left_slope;
    let avg_tilt = (slope1 + slope2) * 0.5;
    ParallelLinesCorrection {
        axis: ParallelCorrectionAxis::Vertical,
        keystone: (-convergence * 65.0).clamp(-50.0, 50.0),
        rotation_degrees: (-avg_tilt * 57.29578).clamp(-45.0, 45.0),
    }
}

fn compute_horizontal_correction(
    line1: (Pos2, Pos2),
    line2: (Pos2, Pos2),
) -> ParallelLinesCorrection {
    let (l1_a, l1_b) = line1;
    let (l2_a, l2_b) = line2;
    let (a1, b1) = if l1_a.x <= l1_b.x {
        (l1_a, l1_b)
    } else {
        (l1_b, l1_a)
    };
    let (a2, b2) = if l2_a.x <= l2_b.x {
        (l2_a, l2_b)
    } else {
        (l2_b, l2_a)
    };

    let slope1 = (b1.y - a1.y) / (b1.x - a1.x).max(1.0);
    let slope2 = (b2.y - a2.y) / (b2.x - a2.x).max(1.0);
    let (top_slope, bottom_slope) = if a1.y < a2.y {
        (slope1, slope2)
    } else {
        (slope2, slope1)
    };

    let convergence = bottom_slope - top_slope;
    let avg_tilt = (slope1 + slope2) * 0.5;
    ParallelLinesCorrection {
        axis: ParallelCorrectionAxis::Horizontal,
        keystone: (convergence * 65.0).clamp(-50.0, 50.0),
        rotation_degrees: (-avg_tilt.atan().to_degrees()).clamp(-45.0, 45.0),
    }
}

/// Computes perspective keystone (auto-detected vertical or horizontal) and rotation
/// from two user-drawn reference lines.
#[must_use]
pub fn compute_parallel_lines_correction(
    line1: (Pos2, Pos2),
    line2: (Pos2, Pos2),
) -> ParallelLinesCorrection {
    let total_dx = (line1.1.x - line1.0.x).abs() + (line2.1.x - line2.0.x).abs();
    let total_dy = (line1.1.y - line1.0.y).abs() + (line2.1.y - line2.0.y).abs();
    if total_dx > total_dy {
        compute_horizontal_correction(line1, line2)
    } else {
        compute_vertical_correction(line1, line2)
    }
}

fn apply_parallel_drag_release(
    p1: Pos2,
    p2: Pos2,
    tool_state: &mut ParallelLinesToolState<'_>,
    geometry: &mut GeometryGradingParams,
    cur_lang: Language,
) -> PerspectiveToolOutput {
    let mut out = PerspectiveToolOutput::default();
    let dist_sq = (p2.x - p1.x).powi(2) + (p2.y - p1.y).powi(2);
    if dist_sq <= 64.0 {
        return out;
    }

    match *tool_state.phase {
        ParallelToolPhase::DrawingLine1 => {
            *tool_state.line1 = Some((p1, p2));
            *tool_state.phase = ParallelToolPhase::WaitingLine2;
        }
        ParallelToolPhase::WaitingLine2 | ParallelToolPhase::DrawingLine2 => {
            *tool_state.line2 = Some((p1, p2));
            if let Some(l1) = *tool_state.line1 {
                let correction = compute_parallel_lines_correction(l1, (p1, p2));
                let axis_label = match correction.axis {
                    ParallelCorrectionAxis::Vertical => {
                        geometry.keystone_vertical = correction.keystone;
                        "Keystone V"
                    }
                    ParallelCorrectionAxis::Horizontal => {
                        geometry.keystone_horizontal = correction.keystone;
                        "Keystone H"
                    }
                };
                geometry.rotation_degrees = correction.rotation_degrees;
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                out.save_sidecar = true;
                *tool_state.status_message = format!(
                    "{} : {axis_label} {:.1}, Rotation {:.1}°",
                    Key::GeometryAdjustedStatus.translate(cur_lang),
                    correction.keystone,
                    correction.rotation_degrees
                );
            }
            *tool_state.phase = ParallelToolPhase::Inactive;
            *tool_state.line1 = None;
            *tool_state.line2 = None;
        }
        ParallelToolPhase::Inactive => {}
    }

    out
}

/// Handles the 2-parallel-lines perspective correction tool.
pub fn handle_parallel_lines_tool(
    ui: &mut Ui,
    ctx: &ViewportToolContext<'_>,
    tool_state: &mut ParallelLinesToolState<'_>,
    geometry: &mut GeometryGradingParams,
) -> PerspectiveToolOutput {
    let ViewportToolContext {
        resp,
        painter,
        available_rect,
        language: cur_lang,
        ..
    } = *ctx;
    let mut out = PerspectiveToolOutput::default();
    if *tool_state.phase == ParallelToolPhase::Inactive {
        return out;
    }

    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    draw_parallel_tool_banner(painter, available_rect, *tool_state.phase, cur_lang);

    if let Some((p1, p2)) = *tool_state.line1 {
        draw_parallel_line_overlay(
            painter,
            p1,
            p2,
            Color32::from_rgb(50, 230, 100),
            Some(Key::GeometryLine1Label.translate(cur_lang)),
        );
    }

    if resp.drag_started() {
        *tool_state.drag_start = resp.interact_pointer_pos();
    }

    if resp.dragged() {
        if let (Some(p1), Some(p2)) = (*tool_state.drag_start, resp.interact_pointer_pos()) {
            let stroke_col = match *tool_state.phase {
                ParallelToolPhase::DrawingLine1 => Color32::from_rgb(0, 240, 255),
                _ => Color32::from_rgb(255, 220, 50),
            };
            draw_parallel_line_overlay(painter, p1, p2, stroke_col, None);
        }
    }

    if resp.drag_stopped() {
        if let (Some(p1), Some(p2)) = (*tool_state.drag_start, resp.interact_pointer_pos()) {
            out = apply_parallel_drag_release(p1, p2, tool_state, geometry, cur_lang);
        }
        *tool_state.drag_start = None;
    }

    out
}

/// Handles the interactive horizon leveling tool.
pub fn handle_horizon_tool(
    ui: &mut Ui,
    ctx: &ViewportToolContext<'_>,
    horizon_tool_active: &mut bool,
    horizon_start_point: &mut Option<Pos2>,
    geometry: &mut GeometryGradingParams,
) -> PerspectiveToolOutput {
    let ViewportToolContext { resp, painter, .. } = *ctx;
    let mut out = PerspectiveToolOutput::default();
    if !*horizon_tool_active {
        return out;
    }

    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    if resp.drag_started() {
        *horizon_start_point = resp.interact_pointer_pos();
    }
    if resp.dragged() {
        if let (Some(p1), Some(p2)) = (*horizon_start_point, resp.interact_pointer_pos()) {
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            let angle = -dy.atan2(dx).to_degrees();
            painter.line_segment([p1, p2], Stroke::new(2.5, Color32::from_rgb(0, 240, 255)));
            painter.circle_filled(p1, 4.0, Color32::from_rgb(0, 240, 255));
            painter.circle_filled(p2, 4.0, Color32::from_rgb(0, 240, 255));
            painter.text(
                (p1 + p2.to_vec2()) * 0.5 + Vec2::new(0.0, -18.0),
                egui::Align2::CENTER_CENTER,
                format!("{:.1}°", angle),
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );
        }
    }
    if resp.drag_stopped() {
        if let (Some(p1), Some(p2)) = (*horizon_start_point, resp.interact_pointer_pos()) {
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            if (dx * dx + dy * dy) > 36.0 {
                let angle = -dy.atan2(dx).to_degrees();
                geometry.rotation_degrees = angle.clamp(-45.0, 45.0);
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                out.save_sidecar = true;
            }
        }
        *horizon_start_point = None;
        *horizon_tool_active = false;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_lines_vertical_yields_zero_correction() {
        let l1 = (Pos2::new(100.0, 300.0), Pos2::new(100.0, 100.0));
        let l2 = (Pos2::new(300.0, 300.0), Pos2::new(300.0, 100.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Vertical);
        assert!(c.keystone.abs() < 1e-4);
        assert!(c.rotation_degrees.abs() < 1e-4);
    }

    #[test]
    fn test_parallel_lines_converging_upward_yields_positive_keystone() {
        let l1 = (Pos2::new(100.0, 300.0), Pos2::new(140.0, 100.0));
        let l2 = (Pos2::new(300.0, 300.0), Pos2::new(260.0, 100.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Vertical);
        assert!(c.keystone > 0.0 && c.keystone <= 50.0);
        assert!(c.rotation_degrees.abs() < 1e-4);
    }

    #[test]
    fn test_parallel_lines_tilted_yields_clamped_rotation() {
        let l1 = (Pos2::new(100.0, 300.0), Pos2::new(140.0, 100.0));
        let l2 = (Pos2::new(300.0, 300.0), Pos2::new(340.0, 100.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Vertical);
        assert!(c.keystone.abs() < 1e-4);
        assert!((-45.0..0.0).contains(&c.rotation_degrees));
    }

    #[test]
    fn test_parallel_lines_horizontal_yields_zero_correction() {
        let l1 = (Pos2::new(100.0, 100.0), Pos2::new(400.0, 100.0));
        let l2 = (Pos2::new(100.0, 300.0), Pos2::new(400.0, 300.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Horizontal);
        assert!(c.keystone.abs() < 1e-4);
        assert!(c.rotation_degrees.abs() < 1e-4);
    }

    #[test]
    fn test_parallel_lines_horizontal_converging_right_yields_negative_keystone() {
        let l1 = (Pos2::new(100.0, 100.0), Pos2::new(400.0, 145.0));
        let l2 = (Pos2::new(100.0, 300.0), Pos2::new(400.0, 255.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Horizontal);
        assert!((-50.0..0.0).contains(&c.keystone));
        assert!(c.rotation_degrees.abs() < 1e-4);
    }

    #[test]
    fn test_parallel_lines_horizontal_converging_left_yields_positive_keystone() {
        let l1 = (Pos2::new(400.0, 100.0), Pos2::new(100.0, 145.0));
        let l2 = (Pos2::new(400.0, 300.0), Pos2::new(100.0, 255.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Horizontal);
        assert!((0.0..=50.0).contains(&c.keystone));
        assert!(c.rotation_degrees.abs() < 1e-4);
    }

    #[test]
    fn test_parallel_lines_horizontal_tilted_yields_rotation_only() {
        let l1 = (Pos2::new(100.0, 100.0), Pos2::new(400.0, 160.0));
        let l2 = (Pos2::new(100.0, 300.0), Pos2::new(400.0, 360.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Horizontal);
        assert!(c.keystone.abs() < 1e-4);
        assert!((-45.0..0.0).contains(&c.rotation_degrees));
    }

    #[test]
    fn test_parallel_lines_horizontal_extreme_convergence_clamps_to_bounds() {
        let l1 = (Pos2::new(100.0, 100.0), Pos2::new(200.0, 190.0));
        let l2 = (Pos2::new(100.0, 300.0), Pos2::new(200.0, 210.0));
        let c = compute_parallel_lines_correction(l1, l2);
        assert_eq!(c.axis, ParallelCorrectionAxis::Horizontal);
        assert_eq!(c.keystone, -50.0);
    }

    #[test]
    fn test_apply_parallel_drag_release_horizontal_preserves_vertical_keystone() {
        let mut phase = ParallelToolPhase::WaitingLine2;
        let mut line1 = Some((Pos2::new(100.0, 100.0), Pos2::new(400.0, 145.0)));
        let mut line2 = None;
        let mut drag_start = None;
        let mut status_message = String::new();
        let mut tool_state = ParallelLinesToolState {
            phase: &mut phase,
            line1: &mut line1,
            line2: &mut line2,
            drag_start: &mut drag_start,
            status_message: &mut status_message,
        };
        let mut geometry = GeometryGradingParams {
            keystone_vertical: 12.5,
            ..GeometryGradingParams::default()
        };

        let out = apply_parallel_drag_release(
            Pos2::new(100.0, 300.0),
            Pos2::new(400.0, 255.0),
            &mut tool_state,
            &mut geometry,
            Language::En,
        );

        assert!(out.mark_custom_preset && out.needs_preview_update && out.save_sidecar);
        assert_eq!(geometry.keystone_vertical, 12.5);
        assert!(geometry.keystone_horizontal < 0.0);
        assert!(status_message.contains("Keystone H"));
        assert_eq!(phase, ParallelToolPhase::Inactive);
    }
}
