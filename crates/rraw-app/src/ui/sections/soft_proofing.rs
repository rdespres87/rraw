#![forbid(unsafe_code)]

//! Soft proofing and print simulation inspector section.

use egui::{Color32, Ui};
use rraw_color::{SoftProofConfig, SoftProofProfile, SoftProofRenderingIntent};

use super::SectionOutput;
use crate::i18n::{Key, Language};

/// Renders the Soft Proofing darkroom section.
pub fn render(
    ui: &mut Ui,
    soft_proof: &mut SoftProofConfig,
    out_of_gamut_count: u32,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let mut sp_changed = false;
    let t = |key: Key| key.translate(cur_lang);

    if ui
        .checkbox(
            &mut soft_proof.enabled,
            egui::RichText::new(t(Key::InspectorEnableSoftProof)).strong(),
        )
        .changed()
    {
        sp_changed = true;
    }

    ui.add_space(2.0);
    ui.label(t(Key::InspectorParamSoftProofProfile));
    let combo_w = (ui.available_width() - 8.0).max(100.0);
    egui::ComboBox::from_id_salt("soft_proof_profile_combo")
        .selected_text(crate::i18n::soft_proof_profile_label_lang(
            soft_proof.profile,
            cur_lang,
        ))
        .width(combo_w)
        .show_ui(ui, |ui| {
            for prof in SoftProofProfile::ALL {
                if ui
                    .selectable_value(
                        &mut soft_proof.profile,
                        prof,
                        crate::i18n::soft_proof_profile_label_lang(prof, cur_lang),
                    )
                    .changed()
                {
                    if prof.is_print_medium() {
                        soft_proof.simulate_paper_and_ink = true;
                    }
                    sp_changed = true;
                }
            }
        });

    ui.add_space(2.0);
    ui.label(t(Key::InspectorParamSoftProofIntent));
    let intent_w = (ui.available_width() - 8.0).max(100.0);
    egui::ComboBox::from_id_salt("soft_proof_intent_combo")
        .selected_text(crate::i18n::soft_proof_intent_label_lang(
            soft_proof.intent,
            cur_lang,
        ))
        .width(intent_w)
        .show_ui(ui, |ui| {
            for intent in SoftProofRenderingIntent::ALL {
                if ui
                    .selectable_value(
                        &mut soft_proof.intent,
                        intent,
                        crate::i18n::soft_proof_intent_label_lang(intent, cur_lang),
                    )
                    .changed()
                {
                    sp_changed = true;
                }
            }
        });

    ui.add_space(2.0);
    if ui
        .checkbox(
            &mut soft_proof.simulate_paper_and_ink,
            t(Key::InspectorParamSoftProofPaper),
        )
        .on_hover_text(t(Key::TopBarPaperInkTooltip))
        .changed()
    {
        sp_changed = true;
    }

    let alert_label = if soft_proof.show_gamut_warning {
        egui::RichText::new(t(Key::InspectorGamutAlertStrong))
            .color(Color32::from_rgb(255, 110, 230))
    } else {
        egui::RichText::new(t(Key::InspectorGamutAlert))
    };
    if ui
        .checkbox(&mut soft_proof.show_gamut_warning, alert_label)
        .on_hover_text(t(Key::InspectorGamutAlertTooltip))
        .changed()
    {
        sp_changed = true;
    }

    let short_prof = crate::i18n::soft_proof_profile_short_label_lang(soft_proof.profile, cur_lang);
    let status_text = if out_of_gamut_count > 0 {
        let msg = crate::i18n::tc(cur_lang, "SoftProof.OutOfGamutPixels")
            .replace("{count}", &out_of_gamut_count.to_string())
            .replace("{profile}", short_prof);
        egui::RichText::new(msg)
            .small()
            .color(Color32::from_rgb(255, 130, 220))
    } else {
        let msg =
            crate::i18n::tc(cur_lang, "SoftProof.InGamutStatus").replace("{profile}", short_prof);
        egui::RichText::new(msg)
            .small()
            .color(Color32::from_rgb(110, 220, 145))
    };
    ui.label(status_text);

    if (soft_proof.enabled || soft_proof.show_gamut_warning || soft_proof.simulate_paper_and_ink)
        && ui.button(t(Key::InspectorResetSoftProof)).clicked()
    {
        *soft_proof = SoftProofConfig::default();
        sp_changed = true;
    }

    if sp_changed {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
