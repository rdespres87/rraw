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
    pub is_manual_override: bool,
    pub is_dng_opcode: bool,
}

#[derive(Clone, Default)]
struct LensSearchCache {
    query: String,
    results: Vec<String>,
}

fn cached_lens_search(ui: &Ui, query: &str) -> Vec<String> {
    if query.is_empty() {
        return Vec::new();
    }
    let id = ui.scope_id().with("rraw_optics_lens_search_cache");
    ui.ctx().data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<LensSearchCache>(id);
        if cache.query != query {
            cache.query = query.to_owned();
            cache.results = rraw_optics::LensfunDatabase::global().search_lenses(query, 6);
        }
        cache.results.clone()
    })
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

    // Segmented mode selector: [ Profile | Manual ]
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

    match optics.optical_tab {
        OpticalCorrectionTab::Profile => {
            render_profile_tab(ui, optics, lens_info, &mut out, &t);
        }
        OpticalCorrectionTab::Manual => {
            render_manual_tab(ui, optics, &mut out, &t);
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

fn render_profile_tab(
    ui: &mut Ui,
    optics: &mut OpticsGradingParams,
    lens_info: Option<DetectedLensInfo>,
    out: &mut SectionOutput,
    t: &dyn Fn(Key) -> &'static str,
) {
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

    if !optics.lens_profile_auto {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(t(Key::InspectorLensProfileDisabledNotice))
                .size(11.0)
                .color(Color32::from_rgb(170, 175, 185)),
        );
        return;
    }

    render_lens_badge(ui, optics, lens_info, out, t);
    ui.add_space(2.0);
    render_lens_search(ui, optics, out, t);
    ui.add_space(2.0);

    ui.label(t(Key::InspectorLensProfileDistortion));
    if ui
        .add(
            egui::Slider::new(&mut optics.lens_distortion_strength, 0.0..=200.0)
                .show_value(true)
                .suffix(" %"),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorLensProfileVignette));
    if ui
        .add(
            egui::Slider::new(&mut optics.lens_vignette_strength, 0.0..=200.0)
                .show_value(true)
                .suffix(" %"),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorLensProfileCa));
    if ui
        .add(
            egui::Slider::new(&mut optics.lens_ca_strength, 0.0..=200.0)
                .show_value(true)
                .suffix(" %"),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }
}

fn render_lens_badge(
    ui: &mut Ui,
    optics: &mut OpticsGradingParams,
    lens_info: Option<DetectedLensInfo>,
    out: &mut SectionOutput,
    t: &dyn Fn(Key) -> &'static str,
) {
    let (lens_name, extra_meta, badge_icon, badge_text, frame_color) = if let Some(info) = lens_info
    {
        if info.is_dng_opcode {
            (
                info.name,
                info.extra_metadata,
                "🏷️",
                t(Key::InspectorLensProfileDngBadge),
                Color32::from_rgb(20, 36, 44),
            )
        } else if info.is_manual_override {
            (
                info.name,
                info.extra_metadata,
                "🔍",
                t(Key::InspectorLensProfileManualBadge),
                Color32::from_rgb(36, 32, 48),
            )
        } else if info.is_exact {
            (
                info.name,
                info.extra_metadata,
                "🎯",
                t(Key::InspectorLensProfileCalibratedBadge),
                Color32::from_rgb(28, 36, 48),
            )
        } else {
            (
                info.name,
                info.extra_metadata,
                "📐",
                t(Key::InspectorLensProfileGeneric),
                Color32::from_rgb(32, 34, 38),
            )
        }
    } else {
        (
            t(Key::InspectorLensProfileGeneric).to_string(),
            String::new(),
            "📐",
            t(Key::InspectorLensProfileGeneric),
            Color32::from_rgb(32, 34, 38),
        )
    };

    egui::Frame::group(ui.style())
        .fill(frame_color)
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{badge_icon} {badge_text} :"));
                    ui.label(
                        egui::RichText::new(&lens_name)
                            .strong()
                            .color(Color32::from_rgb(140, 200, 255)),
                    );
                    if optics.lens_profile_override.is_some()
                        && ui
                            .small_button("✖")
                            .on_hover_text(t(Key::InspectorLensProfileReset))
                            .clicked()
                    {
                        optics.lens_profile_override = None;
                        out.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
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
}

fn render_lens_search(
    ui: &mut Ui,
    optics: &mut OpticsGradingParams,
    out: &mut SectionOutput,
    t: &dyn Fn(Key) -> &'static str,
) {
    ui.collapsing(t(Key::InspectorLensProfileSearch), |ui| {
        ui.horizontal(|ui| {
            let search_res = ui.add(
                egui::TextEdit::singleline(&mut optics.lens_search_query)
                    .hint_text(t(Key::InspectorLensProfileSearchHint))
                    .desired_width((ui.available_width() - 28.0).max(60.0)),
            );
            if search_res.changed() {
                ui.ctx().request_repaint();
            }
            if !optics.lens_search_query.is_empty() && ui.small_button("✖").clicked() {
                optics.lens_search_query.clear();
                ui.ctx().request_repaint();
            }
        });

        let query = optics.lens_search_query.trim();
        if !query.is_empty() {
            let matches = cached_lens_search(ui, query);
            if matches.is_empty() {
                ui.label(
                    egui::RichText::new(t(Key::InspectorLensProfileNoMatch))
                        .weak()
                        .size(11.0),
                );
            } else {
                for candidate in matches {
                    let is_current = optics.lens_profile_override.as_deref() == Some(&candidate);
                    let text = egui::RichText::new(&candidate).size(11.0);
                    let btn = if is_current {
                        egui::Button::new(text).fill(Color32::from_rgb(40, 70, 100))
                    } else {
                        egui::Button::new(text)
                    };
                    if ui.add(btn).clicked() {
                        optics.lens_profile_override = Some(candidate);
                        out.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                }
            }
        }

        if optics.lens_profile_override.is_some() {
            ui.add_space(2.0);
            if ui.button(t(Key::InspectorLensProfileReset)).clicked() {
                optics.lens_profile_override = None;
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        }
    });
}

fn render_manual_tab(
    ui: &mut Ui,
    optics: &mut OpticsGradingParams,
    out: &mut SectionOutput,
    t: &dyn Fn(Key) -> &'static str,
) {
    ui.label(egui::RichText::new(t(Key::InspectorLensManualTitle)).strong());

    if ui
        .checkbox(
            &mut optics.distortion_correction,
            t(Key::InspectorLensDistortion),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamDistortion));
    if ui
        .add(egui::Slider::new(&mut optics.distortion_amount, -50.0..=50.0).show_value(true))
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.separator();

    if ui
        .checkbox(
            &mut optics.chromatic_aberration_correction,
            t(Key::InspectorLensCa),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamDefringePurple));
    if ui
        .add(
            egui::Slider::new(&mut optics.defringe_purple_amount, 0.0..=100.0)
                .show_value(true)
                .suffix(" %"),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamDefringeGreen));
    if ui
        .add(
            egui::Slider::new(&mut optics.defringe_green_amount, 0.0..=100.0)
                .show_value(true)
                .suffix(" %"),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorParamDefringeThreshold));
    let mut thresh_pct = optics.defringe_threshold * 100.0;
    if ui
        .add(
            egui::Slider::new(&mut thresh_pct, 1.0..=30.0)
                .show_value(true)
                .suffix(" %"),
        )
        .changed()
    {
        optics.defringe_threshold = (thresh_pct / 100.0).clamp(0.01, 0.30);
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    if ui
        .checkbox(
            &mut optics.defringe_preview_mask,
            t(Key::InspectorDefringePreviewMask),
        )
        .on_hover_text(t(Key::InspectorDefringePreviewMaskTooltip))
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.separator();

    ui.label(egui::RichText::new(t(Key::InspectorLensVignetteTitle)).strong());
    if ui
        .checkbox(
            &mut optics.vignette_correction,
            t(Key::InspectorLensVignette),
        )
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    ui.label(t(Key::InspectorCorrectionStrength));
    if ui
        .add(egui::Slider::new(&mut optics.vignette_amount, 0.0..=100.0).show_value(true))
        .changed()
    {
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_lens_search_memoizes_queries_and_handles_empty() {
        let ctx = egui::Context::default();
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(cached_lens_search(ui, "").is_empty());
            let first = cached_lens_search(ui, "Canon");
            let second = cached_lens_search(ui, "Canon");
            assert_eq!(first, second);
        });
        full.textures_delta.clear();
    }

    #[test]
    fn render_supports_profile_and_manual_tabs() {
        let ctx = egui::Context::default();
        let mut optics = OpticsGradingParams::default();
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            let out_prof = render(ui, &mut optics, None, Language::En);
            assert!(!out_prof.needs_preview_update);

            optics.optical_tab = OpticalCorrectionTab::Manual;
            let out_man = render(ui, &mut optics, None, Language::Fr);
            assert!(!out_man.needs_preview_update);
        });
        full.textures_delta.clear();
    }
}
