#![forbid(unsafe_code)]

//! Straighten and rotation inspector section.

use egui::{Pos2, Ui};

use super::SectionOutput;
use crate::grading::GeometryGradingParams;
use crate::i18n::{Key, Language};

/// Renders the Straighten darkroom section.
pub fn render(
    ui: &mut Ui,
    geometry: &mut GeometryGradingParams,
    horizon_tool_active: &mut bool,
    horizon_start_point: &mut Option<Pos2>,
    status_message: &mut String,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    ui.label(t(Key::InspectorParamStraightenAngle));
    let r_rot = ui.add(
        egui::Slider::new(&mut geometry.rotation_degrees, -45.0..=45.0)
            .step_by(0.1)
            .suffix("°")
            .show_value(true),
    );
    if r_rot.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.horizontal(|ui| {
        let btn_w = ((ui.available_width() - 8.0) / 3.0).floor().max(40.0);
        if ui
            .add_sized(
                [btn_w, 22.0],
                egui::Button::new(t(Key::InspectorResetAngle)).truncate(),
            )
            .on_hover_text(t(Key::InspectorResetAngle))
            .clicked()
        {
            geometry.rotation_degrees = 0.0;
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
        if ui
            .add_sized(
                [btn_w, 22.0],
                egui::Button::new(t(Key::InspectorRotate90Left)).truncate(),
            )
            .on_hover_text(t(Key::InspectorRotate90Left))
            .clicked()
        {
            geometry.rotation_degrees = (geometry.rotation_degrees - 90.0).clamp(-45.0, 45.0);
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
        if ui
            .add_sized(
                [btn_w, 22.0],
                egui::Button::new(t(Key::InspectorRotate90Right)).truncate(),
            )
            .on_hover_text(t(Key::InspectorRotate90Right))
            .clicked()
        {
            geometry.rotation_degrees = (geometry.rotation_degrees + 90.0).clamp(-45.0, 45.0);
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
    });

    let horizon_btn_text = if *horizon_tool_active {
        t(Key::StraightenHorizonActive)
    } else {
        t(Key::StraightenHorizonDraw)
    };
    let horizon_btn = ui
        .add_sized(
            [ui.available_width(), 24.0],
            egui::Button::new(horizon_btn_text)
                .selected(*horizon_tool_active)
                .truncate(),
        )
        .on_hover_text(horizon_btn_text);
    if horizon_btn.clicked() {
        *horizon_tool_active = !*horizon_tool_active;
        *horizon_start_point = None;
        if *horizon_tool_active {
            *status_message = t(Key::StraightenHorizonStatus).to_string();
        }
    }

    ui.horizontal(|ui| {
        let c_fx = ui.checkbox(&mut geometry.flip_horizontal, t(Key::InspectorParamFlipH));
        let c_fy = ui.checkbox(&mut geometry.flip_vertical, t(Key::InspectorParamFlipV));
        if c_fx.changed() || c_fy.changed() {
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
    });

    out
}
