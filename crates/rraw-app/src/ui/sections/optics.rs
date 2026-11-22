#![forbid(unsafe_code)]

//! Lens optical corrections, defringing and vignetting inspector section.

use egui::{Color32, Ui};

use super::SectionOutput;
use crate::grading::OpticsGradingParams;
use crate::i18n::{Key, Language};
use crate::state::OpticalCorrectionTab;

/// Detected lens metadata summary passed to the optical corrections view.
#[derive(Debug, Clone, Default)]
pub struct DetectedLensInfo {
    pub name: String,
    pub is_exact: bool,
    pub extra_metadata: String,
}

/// Renders the Optical Corrections darkroom section.
pub fn render(
    ui: &mut Ui,
    optics: &mut OpticsGradingParams,
    lens_info: Option<DetectedLensInfo>,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    // Segmented mode selector: [ 🎯 Profil d'objectif | 🛠️ Manuel ]
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
        let tab_w = ((ui.available_width() - 4.0) / 2.0).max(60.0);
        let is_prof = optics.optical_tab == OpticalCorrectionTab::Profile;
        let is_man = optics.optical_tab == OpticalCorrectionTab::Manual;

        if ui
            .add_sized(
                [tab_w, 22.0],
                egui::Button::new(egui::RichText::new(t(Key::InspectorLensTabProfile)).size(11.0))
                    .selected(is_prof),
            )
            .clicked()
        {
            optics.optical_tab = OpticalCorrectionTab::Profile;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
        if ui
            .add_sized(
                [tab_w, 22.0],
                egui::Button::new(egui::RichText::new(t(Key::InspectorLensTabManual)).size(11.0))
                    .selected(is_man),
            )
            .clicked()
        {
            optics.optical_tab = OpticalCorrectionTab::Manual;
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }
    });

    ui.add_space(4.0);

    if optics.optical_tab == OpticalCorrectionTab::Profile {
        ui.label(egui::RichText::new(t(Key::InspectorLensProfileTitle)).strong());

        let c_auto = ui
            .checkbox(
                &mut optics.lens_profile_auto,
                t(Key::InspectorLensProfileAuto),
            )
            .on_hover_text(t(Key::InspectorLensProfileAutoTooltip));
        if c_auto.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        if optics.lens_profile_auto {
            let (lens_name, is_exact, extra_meta) = if let Some(info) = lens_info {
                (info.name, info.is_exact, info.extra_metadata)
            } else {
                (
                    t(Key::InspectorLensProfileGeneric).to_string(),
                    false,
                    String::new(),
                )
            };

            let badge_icon = if is_exact { "🎯" } else { "📐" };
            let frame_color = if is_exact {
                Color32::from_rgb(28, 36, 48)
            } else {
                Color32::from_rgb(32, 34, 38)
            };

            egui::Frame::group(ui.style())
                .fill(frame_color)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "{badge_icon} {}",
                                t(Key::InspectorLensProfileDetected)
                            ));
                            ui.label(
                                egui::RichText::new(lens_name)
                                    .strong()
                                    .color(Color32::from_rgb(140, 200, 255)),
                            );
                        });
                        if !extra_meta.is_empty() {
                            ui.label(
                                egui::RichText::new(extra_meta)
                                    .size(10.5)
                                    .color(Color32::from_rgb(170, 180, 195)),
                            );
                        }
                    });
                });

            ui.add_space(2.0);
            ui.label(t(Key::InspectorLensProfileDistortion));
            let s_ld = ui.add(
                egui::Slider::new(&mut optics.lens_distortion_strength, 0.0..=200.0)
                    .show_value(true)
                    .suffix(" %"),
            );
            if s_ld.changed() {
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }

            ui.label(t(Key::InspectorLensProfileVignette));
            let s_lv = ui.add(
                egui::Slider::new(&mut optics.lens_vignette_strength, 0.0..=200.0)
                    .show_value(true)
                    .suffix(" %"),
            );
            if s_lv.changed() {
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }

            ui.label(t(Key::InspectorLensProfileCa));
            let s_lca = ui.add(
                egui::Slider::new(&mut optics.lens_ca_strength, 0.0..=200.0)
                    .show_value(true)
                    .suffix(" %"),
            );
            if s_lca.changed() {
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        } else {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(t(Key::InspectorLensProfileDisabledNotice))
                    .size(11.0)
                    .color(Color32::from_rgb(170, 175, 185)),
            );
        }
    }

    if optics.optical_tab == OpticalCorrectionTab::Manual {
        ui.label(egui::RichText::new(t(Key::InspectorLensManualTitle)).strong());

        let c_dist = ui.checkbox(
            &mut optics.distortion_correction,
            t(Key::InspectorLensDistortion),
        );
        if c_dist.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorParamDistortion));
        let r_da =
            ui.add(egui::Slider::new(&mut optics.distortion_amount, -50.0..=50.0).show_value(true));
        if r_da.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.separator();

        let c_ca = ui.checkbox(
            &mut optics.chromatic_aberration_correction,
            t(Key::InspectorLensCa),
        );
        if c_ca.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorParamDefringePurple));
        let r_dp = ui.add(
            egui::Slider::new(&mut optics.defringe_purple_amount, 0.0..=100.0)
                .show_value(true)
                .suffix(" %"),
        );
        if r_dp.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorParamDefringeGreen));
        let r_dg = ui.add(
            egui::Slider::new(&mut optics.defringe_green_amount, 0.0..=100.0)
                .show_value(true)
                .suffix(" %"),
        );
        if r_dg.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorParamDefringeThreshold));
        let mut thresh_pct = optics.defringe_threshold * 100.0;
        let r_dt = ui.add(
            egui::Slider::new(&mut thresh_pct, 1.0..=30.0)
                .show_value(true)
                .suffix(" %"),
        );
        if r_dt.changed() {
            optics.defringe_threshold = (thresh_pct / 100.0).clamp(0.01, 0.30);
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        let c_dpm = ui
            .checkbox(
                &mut optics.defringe_preview_mask,
                t(Key::InspectorDefringePreviewMask),
            )
            .on_hover_text(t(Key::InspectorDefringePreviewMaskTooltip));
        if c_dpm.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.separator();

        ui.label(egui::RichText::new(t(Key::InspectorLensVignetteTitle)).strong());
        let c_vig = ui.checkbox(
            &mut optics.vignette_correction,
            t(Key::InspectorLensVignette),
        );
        if c_vig.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.label(t(Key::InspectorCorrectionStrength));
        let r_va =
            ui.add(egui::Slider::new(&mut optics.vignette_amount, 0.0..=100.0).show_value(true));
        if r_va.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        ui.separator();

        ui.label(egui::RichText::new(t(Key::InspectorCreativeVignetteTitle)).strong());
        ui.label(t(Key::InspectorParamVignetteAmount));
        let r_cva = ui.add(
            egui::Slider::new(&mut optics.creative_vignette_amount, -100.0..=100.0)
                .show_value(true),
        );
        if r_cva.changed() {
            out.needs_preview_update = true;
            ui.ctx().request_repaint();
        }

        if optics.creative_vignette_amount.abs() > 0.1 {
            ui.label(t(Key::InspectorParamVignetteMidpoint));
            let r_cvm = ui.add(
                egui::Slider::new(&mut optics.creative_vignette_midpoint, 10.0..=90.0)
                    .show_value(true),
            );
            if r_cvm.changed() {
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }

            ui.label(t(Key::InspectorParamVignetteFeather));
            let r_cvf = ui.add(
                egui::Slider::new(&mut optics.creative_vignette_feather, 10.0..=90.0)
                    .show_value(true),
            );
            if r_cvf.changed() {
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }

            ui.label(t(Key::InspectorParamVignetteRoundness));
            let r_cvr = ui.add(
                egui::Slider::new(&mut optics.creative_vignette_roundness, -100.0..=100.0)
                    .show_value(true),
            );
            if r_cvr.changed() {
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        }
    }

    ui.separator();

    if ui.button(t(Key::InspectorResetOptical)).clicked() {
        *optics = OpticsGradingParams::default();
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
