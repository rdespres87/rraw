#![forbid(unsafe_code)]

//! Exposure and basic tone grading inspector section.

use egui::Ui;
use rraw_core::{SmartLightingMode, SmartLightingParams};

use super::SectionOutput;
use crate::grading::ToneGradingParams;
use crate::i18n::{Key, Language};

/// Renders the Exposure & Tone darkroom section.
pub fn render(
    ui: &mut Ui,
    tone: &mut ToneGradingParams,
    smart_lighting: &mut SmartLightingParams,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    ui.label(t(Key::InspectorParamExposure));
    let r1 = ui.add(egui::Slider::new(&mut tone.exposure_ev, -5.0..=5.0).show_value(true));
    if r1.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamContrast));
    let r2 = ui.add(egui::Slider::new(&mut tone.contrast, -100.0..=100.0).show_value(true));
    if r2.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamHighlights));
    let r3 = ui.add(egui::Slider::new(&mut tone.highlights, -100.0..=100.0).show_value(true));
    if r3.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamShadows));
    let r4 = ui.add(egui::Slider::new(&mut tone.shadows, -100.0..=100.0).show_value(true));
    if r4.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamBlacks));
    let r5 = ui.add(egui::Slider::new(&mut tone.blacks, -100.0..=100.0).show_value(true));
    if r5.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t(Key::InspectorSmartLightingTitle)).strong())
            .on_hover_text(t(Key::InspectorSmartLightingTooltip));
    });

    let cur_mode = smart_lighting.mode;
    let mut new_mode = cur_mode;
    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(
                cur_mode == SmartLightingMode::Off,
                t(Key::InspectorSmartLightingModeOff),
            )
            .clicked()
        {
            new_mode = SmartLightingMode::Off;
        }
        if ui
            .selectable_label(
                cur_mode == SmartLightingMode::Slight,
                t(Key::InspectorSmartLightingModeSlight),
            )
            .clicked()
        {
            new_mode = SmartLightingMode::Slight;
        }
        if ui
            .selectable_label(
                cur_mode == SmartLightingMode::Medium,
                t(Key::InspectorSmartLightingModeMedium),
            )
            .clicked()
        {
            new_mode = SmartLightingMode::Medium;
        }
        if ui
            .selectable_label(
                cur_mode == SmartLightingMode::Strong,
                t(Key::InspectorSmartLightingModeStrong),
            )
            .clicked()
        {
            new_mode = SmartLightingMode::Strong;
        }
        if ui
            .selectable_label(
                cur_mode == SmartLightingMode::Custom,
                t(Key::InspectorSmartLightingModeCustom),
            )
            .clicked()
        {
            new_mode = SmartLightingMode::Custom;
        }
    });

    if new_mode != cur_mode {
        smart_lighting.mode = new_mode;
        if smart_lighting.intensity <= 1.0 {
            smart_lighting.intensity = 100.0;
        }
        if new_mode == SmartLightingMode::Custom
            && smart_lighting.custom_shadow_lift == 0.0
            && smart_lighting.custom_highlight_compression == 0.0
        {
            smart_lighting.custom_shadow_lift = 0.50;
            smart_lighting.custom_highlight_compression = 0.30;
        }
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    if smart_lighting.is_active() {
        if smart_lighting.intensity <= 1.0 {
            smart_lighting.intensity = 100.0;
        }
        ui.add_space(2.0);
        ui.label(t(Key::InspectorSmartLightingIntensity));
        let r_int = ui.add(
            egui::Slider::new(&mut smart_lighting.intensity, 0.0..=100.0)
                .suffix(" %")
                .show_value(true),
        );
        if r_int.changed() {
            out.mark_custom_preset = true;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        if smart_lighting.mode == SmartLightingMode::Custom {
            ui.label(t(Key::InspectorSmartLightingShadowLift));
            let mut shadow_pct = smart_lighting.custom_shadow_lift * 100.0;
            let r_sh = ui.add(
                egui::Slider::new(&mut shadow_pct, 0.0..=100.0)
                    .suffix(" %")
                    .show_value(true),
            );
            if r_sh.changed() {
                smart_lighting.custom_shadow_lift = shadow_pct / 100.0;
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }

            ui.label(t(Key::InspectorSmartLightingHighlightComp));
            let mut high_pct = smart_lighting.custom_highlight_compression * 100.0;
            let r_hi = ui.add(
                egui::Slider::new(&mut high_pct, 0.0..=100.0)
                    .suffix(" %")
                    .show_value(true),
            );
            if r_hi.changed() {
                smart_lighting.custom_highlight_compression = high_pct / 100.0;
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        }
    }

    ui.add_space(4.0);

    if ui.button(t(Key::InspectorResetTone)).clicked() {
        tone.exposure_ev = 0.0;
        tone.contrast = 0.0;
        tone.highlights = 0.0;
        tone.shadows = 0.0;
        tone.blacks = 0.0;
        *smart_lighting = SmartLightingParams::default();
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
