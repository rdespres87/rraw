#![forbid(unsafe_code)]

//! White balance and color transform inspector section.

use egui::Ui;
use rraw_color::DisplayTransform;

use super::SectionOutput;
use crate::grading::{ColorGradingParams, ToneGradingParams};
use crate::i18n::{Key, Language};

/// Renders the White Balance & Color darkroom section.
pub fn render(
    ui: &mut Ui,
    color: &mut ColorGradingParams,
    tone: &mut ToneGradingParams,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    ui.label(t(Key::InspectorParamKelvin));
    let r_k = ui.add(egui::Slider::new(&mut color.kelvin, 2000.0..=12000.0).show_value(true));
    if r_k.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamTint));
    let r_t = ui.add(egui::Slider::new(&mut color.tint, -50.0..=50.0).show_value(true));
    if r_t.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamVibrance));
    let r_vib = ui.add(egui::Slider::new(&mut tone.vibrance, -100.0..=100.0).show_value(true));
    if r_vib.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamSaturation));
    let r_sat = ui.add(egui::Slider::new(&mut tone.saturation, -100.0..=100.0).show_value(true));
    if r_sat.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamDrt));
    let drt_w = (ui.available_width() - 8.0).max(100.0);
    egui::ComboBox::from_id_salt("drt_select")
        .selected_text(format!("{:?}", color.display_transform))
        .width(drt_w)
        .show_ui(ui, |ui| {
            let c1 = ui.selectable_value(
                &mut color.display_transform,
                DisplayTransform::AgX,
                t(Key::InspectorDrtAgx),
            );
            let c2 = ui.selectable_value(
                &mut color.display_transform,
                DisplayTransform::OpenDrt,
                t(Key::InspectorDrtOpenDrt),
            );
            if c1.changed() || c2.changed() {
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        });

    let c_mono = ui.checkbox(&mut color.is_monochrome, t(Key::InspectorParamMonochrome));
    if c_mono.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    if ui.button(t(Key::InspectorResetColor)).clicked() {
        color.kelvin = 5500.0;
        color.tint = 0.0;
        tone.vibrance = 0.0;
        tone.saturation = 0.0;
        color.is_monochrome = false;
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
