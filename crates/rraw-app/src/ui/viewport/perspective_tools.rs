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

/// Computes `(keystone_vertical, rotation_degrees)` from two drawn perspective lines.
#[must_use]
fn compute_parallel_lines_correction(line1: (Pos2, Pos2), line2: (Pos2, Pos2)) -> (f32, f32) {
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

    let keystone = (-convergence * 65.0).clamp(-50.0, 50.0);
    let rotation = (-avg_tilt * 57.29578).clamp(-45.0, 45.0);
    (keystone, rotation)
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
                let (keystone, rotation) = compute_parallel_lines_correction(l1, (p1, p2));
                geometry.keystone_vertical = keystone;
                geometry.rotation_degrees = rotation;
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                out.save_sidecar = true;
                *tool_state.status_message = format!(
                    "{} : Keystone V {:.1}, Rotation {:.1}°",
                    Key::GeometryAdjustedStatus.translate(cur_lang),
                    keystone,
                    rotation
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
        let (keystone, rotation) = compute_parallel_lines_correction(l1, l2);
        assert!(keystone.abs() < 1e-4);
        assert!(rotation.abs() < 1e-4);
    }

    #[test]
    fn test_parallel_lines_converging_upward_yields_positive_keystone() {
        let l1 = (Pos2::new(100.0, 300.0), Pos2::new(140.0, 100.0));
        let l2 = (Pos2::new(300.0, 300.0), Pos2::new(260.0, 100.0));
        let (keystone, rotation) = compute_parallel_lines_correction(l1, l2);
        assert!(keystone > 0.0 && keystone <= 50.0);
        assert!(rotation.abs() < 1e-4);
    }

    #[test]
    fn test_parallel_lines_tilted_yields_clamped_rotation() {
        let l1 = (Pos2::new(100.0, 300.0), Pos2::new(140.0, 100.0));
        let l2 = (Pos2::new(300.0, 300.0), Pos2::new(340.0, 100.0));
        let (keystone, rotation) = compute_parallel_lines_correction(l1, l2);
        assert!(keystone.abs() < 1e-4);
        assert!((-45.0..0.0).contains(&rotation));
    }
}
