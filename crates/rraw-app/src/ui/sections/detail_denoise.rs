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

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t(Key::InspectorParamChromaDenoise)).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if (denoise_sharpen.color_denoise - 25.0).abs() > 0.1
                && ui
                    .small_button("Auto (25)")
                    .on_hover_text("Rétablir la valeur automatique recommandée")
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
        .on_hover_text(
            "Supprime les marbrures et tavelures colorées (magenta/vertes) sans flouter les détails nets",
        );
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
        .on_hover_text(
            "Lisse le grain noir et blanc dans les aplats tout en préservant le micro-contraste des contours",
        );
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
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
