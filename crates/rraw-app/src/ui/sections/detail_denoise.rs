#![forbid(unsafe_code)]

//! Detail, denoising and capture sharpening inspector section.

use egui::Ui;

use super::SectionOutput;
use crate::grading::DenoiseSharpenParams;
use crate::i18n::{Key, Language};

/// Renders the Detail & Denoise darkroom section.
pub fn render(
    ui: &mut Ui,
    denoise_sharpen: &mut DenoiseSharpenParams,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    // Hardware ANE Neural Denoising Section
    let caps = rraw_coreml::CoreMlHardwareCapabilities::detect();
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(26, 32, 40))
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let r_ane = ui
                    .checkbox(
                        &mut denoise_sharpen.ane_neural_denoise,
                        egui::RichText::new(t(Key::DetailAneDenoise)).strong(),
                    )
                    .on_hover_text(t(Key::DetailAneDenoiseTooltip));
                if r_ane.changed() {
                    out.mark_custom_preset = true;
                    out.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if caps.has_ane {
                        ui.label(
                            egui::RichText::new(t(Key::DetailAneBadgeActive))
                                .small()
                                .color(egui::Color32::from_rgb(90, 220, 140)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(t(Key::DetailAneBadgeFallback))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                });
            });

            if denoise_sharpen.ane_neural_denoise {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label(t(Key::DetailAneStrength));
                    let r_strength = ui.add(
                        egui::Slider::new(&mut denoise_sharpen.ane_denoise_strength, 0.0..=100.0)
                            .show_value(true)
                            .suffix("%"),
                    );
                    if r_strength.changed() {
                        out.mark_custom_preset = true;
                        out.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                });
                ui.label(
                    egui::RichText::new(t(Key::DetailAneStatusTile))
                        .small()
                        .color(egui::Color32::from_rgb(130, 180, 240)),
                );
            }
        });

    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t(Key::InspectorParamChromaDenoise)).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if (denoise_sharpen.color_denoise - 25.0).abs() > 0.1
                && ui
                    .small_button(t(Key::DetailAutoDenoise))
                    .on_hover_text(t(Key::DetailAutoDenoiseTooltip))
                    .clicked()
            {
                denoise_sharpen.color_denoise = 25.0;
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        });
    });

    let r_cd = ui
        .add(
            egui::Slider::new(&mut denoise_sharpen.color_denoise, 0.0..=100.0)
                .show_value(true)
                .suffix("%"),
        )
        .on_hover_text(t(Key::DetailChromaTooltip));
    if r_cd.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.add_space(4.0);
    ui.label(egui::RichText::new(t(Key::InspectorParamLumaDenoise)).strong());
    let r_ld = ui
        .add(
            egui::Slider::new(&mut denoise_sharpen.luma_denoise, 0.0..=100.0)
                .show_value(true)
                .suffix("%"),
        )
        .on_hover_text(t(Key::DetailLumaTooltip));
    if r_ld.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.add_space(4.0);
    ui.label(t(Key::InspectorParamCaptureSharpening));
    let r_s = ui.add(
        egui::Slider::new(&mut denoise_sharpen.capture_sharpening_amount, 0.0..=2.0)
            .show_value(true),
    );
    if r_s.changed() {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.add_space(4.0);
    if ui.button(t(Key::InspectorResetDetailDenoise)).clicked() {
        denoise_sharpen.color_denoise = 25.0;
        denoise_sharpen.luma_denoise = 0.0;
        denoise_sharpen.capture_sharpening_amount = 0.5;
        denoise_sharpen.ane_neural_denoise = false;
        denoise_sharpen.ane_denoise_strength = 50.0;
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
