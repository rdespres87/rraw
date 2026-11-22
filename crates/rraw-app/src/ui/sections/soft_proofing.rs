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
        .selected_text(soft_proof.profile.label())
        .width(combo_w)
        .show_ui(ui, |ui| {
            for prof in SoftProofProfile::ALL {
                if ui
                    .selectable_value(&mut soft_proof.profile, prof, prof.label())
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
        .selected_text(soft_proof.intent.label())
        .width(intent_w)
        .show_ui(ui, |ui| {
            for intent in SoftProofRenderingIntent::ALL {
                if ui
                    .selectable_value(&mut soft_proof.intent, intent, intent.label())
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

    let status_text = if out_of_gamut_count > 0 {
        egui::RichText::new(format!(
            "⚠️ {out_of_gamut_count} pixels hors-gamut ({})",
            soft_proof.profile.short_label()
        ))
        .small()
        .color(Color32::from_rgb(255, 130, 220))
    } else {
        egui::RichText::new(format!(
            "✓ 100% dans le gamut ({})",
            soft_proof.profile.short_label()
        ))
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
