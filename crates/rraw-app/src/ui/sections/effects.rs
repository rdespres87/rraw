#![forbid(unsafe_code)]

//! Creative visual effects and post-crop vignetting inspector section.

use egui::Ui;

use super::SectionOutput;
use crate::grading::EffectsGradingParams;
use crate::i18n::{Key, Language};

/// Renders the Effects darkroom section.
pub fn render(
    ui: &mut Ui,
    effects: &mut EffectsGradingParams,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    ui.label(egui::RichText::new(t(Key::InspectorCreativeVignetteTitle)).strong());
    ui.label(t(Key::InspectorParamVignetteAmount));
    let r_cva = ui.add(
        egui::Slider::new(&mut effects.creative_vignette_amount, -100.0..=100.0).show_value(true),
    );
    if r_cva.changed() {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    if effects.creative_vignette_amount.abs() > 0.1 {
        ui.label(t(Key::InspectorParamVignetteMidpoint));
        let r_cvm = ui.add(
            egui::Slider::new(&mut effects.creative_vignette_midpoint, 10.0..=90.0)
                .show_value(true),
        );
        if r_cvm.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorParamVignetteFeather));
        let r_cvf = ui.add(
            egui::Slider::new(&mut effects.creative_vignette_feather, 10.0..=90.0).show_value(true),
        );
        if r_cvf.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorParamVignetteRoundness));
        let r_cvr = ui.add(
            egui::Slider::new(&mut effects.creative_vignette_roundness, -100.0..=100.0)
                .show_value(true),
        );
        if r_cvr.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
    }

    ui.separator();

    if ui.button(t(Key::InspectorResetEffects)).clicked() {
        *effects = EffectsGradingParams::default();
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
