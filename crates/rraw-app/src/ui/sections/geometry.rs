#![forbid(unsafe_code)]

//! Perspective, keystone and parallel lines geometry inspector section.

use egui::{Pos2, Ui};

use super::SectionOutput;
use crate::grading::GeometryGradingParams;
use crate::i18n::{Key, Language};
use crate::state::ParallelToolPhase;

/// Interactive state for the 2-parallel-lines perspective correction tool.
pub struct ParallelLinesToolState<'a> {
    pub phase: &'a mut ParallelToolPhase,
    pub line1: &'a mut Option<(Pos2, Pos2)>,
    pub line2: &'a mut Option<(Pos2, Pos2)>,
    pub drag_start: &'a mut Option<Pos2>,
    pub status_message: &'a mut String,
}

/// Renders the Geometry & Keystone darkroom section.
pub fn render(
    ui: &mut Ui,
    geometry: &mut GeometryGradingParams,
    tool_state: &mut ParallelLinesToolState<'_>,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    let c_geom = ui.checkbox(
        &mut geometry.geometry_enabled,
        t(Key::InspectorEnableGeometry),
    );
    if c_geom.changed() {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    if geometry.geometry_enabled {
        let is_parallel_active = *tool_state.phase != ParallelToolPhase::Inactive;
        let parallel_btn_label = if is_parallel_active {
            t(Key::GeometryParallelBtnActive)
        } else {
            t(Key::GeometryParallelBtn)
        };
        let btn = ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::Button::new(parallel_btn_label)
                    .selected(is_parallel_active)
                    .truncate(),
            )
            .on_hover_text(parallel_btn_label);
        if btn.clicked() {
            if is_parallel_active {
                *tool_state.phase = ParallelToolPhase::Inactive;
                *tool_state.line1 = None;
                *tool_state.line2 = None;
                *tool_state.drag_start = None;
            } else {
                *tool_state.phase = ParallelToolPhase::DrawingLine1;
                *tool_state.line1 = None;
                *tool_state.line2 = None;
                *tool_state.drag_start = None;
                *tool_state.status_message = t(Key::GeometryParallelStatus1).to_string();
            }
        }

        ui.label(t(Key::InspectorParamKeystoneV));
        let r_kv = ui.add(
            egui::Slider::new(&mut geometry.keystone_vertical, -50.0..=50.0)
                .step_by(0.5)
                .show_value(true),
        );
        if r_kv.changed() {
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorParamKeystoneH));
        let r_kh = ui.add(
            egui::Slider::new(&mut geometry.keystone_horizontal, -50.0..=50.0)
                .step_by(0.5)
                .show_value(true),
        );
        if r_kh.changed() {
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        let c_ac = ui.checkbox(&mut geometry.auto_crop_geometry, t(Key::InspectorAutoCrop));
        if c_ac.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        if ui
            .add_sized(
                [ui.available_width(), 22.0],
                egui::Button::new(t(Key::InspectorResetGeometry)).truncate(),
            )
            .clicked()
        {
            geometry.keystone_vertical = 0.0;
            geometry.keystone_horizontal = 0.0;
            geometry.rotation_degrees = 0.0;
            geometry.flip_horizontal = false;
            geometry.flip_vertical = false;
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
    }

    out
}
