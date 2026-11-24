#![forbid(unsafe_code)]

//! AI semantic masks and local geometric/brush masks inspector section.

use std::ops::RangeInclusive;

use egui::{Color32, Ui};
use rraw_core::SemanticMaskCategory;

use super::SectionOutput;
use crate::i18n::{Key, Language};
use crate::state::{LocalMaskKind, LocalMaskLayer, SemanticMaskLayer};

/// Mutable mask state and language context for the AI Semantic & Local Masks inspector section.
pub struct MaskUiContext<'a> {
    pub local_masks: &'a mut Vec<LocalMaskLayer>,
    pub show_local_mask_overlay: &'a mut Option<usize>,
    pub active_brush_mask_idx: &'a mut Option<usize>,
    pub semantic_masks: &'a mut Vec<SemanticMaskLayer>,
    pub show_semantic_mask_overlay: &'a mut Option<SemanticMaskCategory>,
    pub language: Language,
}

fn labeled_slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    step: Option<f64>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut slider = egui::Slider::new(value, range).show_value(true);
        if let Some(s) = step {
            slider = slider.step_by(s);
        }
        if ui.add(slider).changed() {
            changed = true;
        }
    });
    changed
}

fn render_local_mask_creation_buttons(ui: &mut Ui, ctx: &mut MaskUiContext<'_>) -> bool {
    let cur_lang = ctx.language;
    let t = |key: Key| key.translate(cur_lang);
    let mut changed = false;

    ui.vertical(|ui| {
        if ui
            .button(t(Key::InspectorParamAddLinear))
            .on_hover_text(t(Key::InspectorAddLinearTooltip))
            .clicked()
        {
            let count = ctx.local_masks.len() + 1;
            ctx.local_masks.push(LocalMaskLayer::new_linear(
                format!("{} {count}", t(Key::MaskLinearPrefix)),
                [0.5, 0.0],
                [0.5, 0.45],
            ));
            *ctx.show_local_mask_overlay = None;
            changed = true;
        }
        if ui
            .button(t(Key::InspectorParamAddRadial))
            .on_hover_text(t(Key::InspectorAddRadialTooltip))
            .clicked()
        {
            let count = ctx.local_masks.len() + 1;
            ctx.local_masks.push(LocalMaskLayer::new_radial(
                format!("{} {count}", t(Key::MaskRadialPrefix)),
                [0.5, 0.5],
                [0.3, 0.3],
            ));
            *ctx.show_local_mask_overlay = None;
            *ctx.active_brush_mask_idx = None;
            changed = true;
        }
        if ui
            .button(t(Key::InspectorParamAddBrush))
            .on_hover_text(t(Key::InspectorAddBrushTooltip))
            .clicked()
        {
            let count = ctx.local_masks.len() + 1;
            ctx.local_masks.push(LocalMaskLayer::new_brush(
                format!("{} {count}", t(Key::MaskBrushPrefix)),
                0.08,
                0.65,
            ));
            let new_idx = ctx.local_masks.len() - 1;
            *ctx.show_local_mask_overlay = None;
            *ctx.active_brush_mask_idx = Some(new_idx);
            changed = true;
        }
    });

    changed
}

fn render_local_mask_sliders(ui: &mut Ui, mask: &mut LocalMaskLayer, lang: Language) -> bool {
    let t = |key: Key| key.translate(lang);
    let mut changed = false;
    changed |= labeled_slider(
        ui,
        t(Key::InspectorParamExposure),
        &mut mask.exposure_ev,
        -3.0..=3.0,
        Some(0.05),
    );
    changed |= labeled_slider(
        ui,
        t(Key::InspectorParamContrast),
        &mut mask.contrast,
        -100.0..=100.0,
        None,
    );
    changed |= labeled_slider(
        ui,
        t(Key::InspectorParamSaturation),
        &mut mask.saturation,
        -100.0..=100.0,
        None,
    );
    changed |= labeled_slider(
        ui,
        t(Key::InspectorParamKelvin),
        &mut mask.temperature_shift,
        -100.0..=100.0,
        None,
    );
    changed |= labeled_slider(
        ui,
        t(Key::InspectorSmoothness),
        &mut mask.feather,
        0.05..=1.0,
        None,
    );
    changed
}

fn render_brush_mask_controls<T>(
    ui: &mut Ui,
    dabs: &mut Vec<T>,
    brush_radius: &mut f32,
    brush_flow: &mut f32,
    erase_mode: &mut bool,
    lang: Language,
) -> bool {
    let t = |key: Key| key.translate(lang);
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(t(Key::InspectorParamBrushRadius));
        if ui
            .add(
                egui::Slider::new(brush_radius, 0.001..=0.35)
                    .step_by(0.001)
                    .show_value(true)
                    .custom_formatter(|n, _| {
                        if n < 0.01 {
                            format!("{:.2}%", n * 100.0)
                        } else {
                            format!("{:.1}%", n * 100.0)
                        }
                    }),
            )
            .on_hover_text(t(Key::InspectorBrushRadiusTooltip))
            .changed()
        {
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(t(Key::InspectorParamBrushFlow));
        if ui
            .add(
                egui::Slider::new(brush_flow, 0.05..=1.0)
                    .step_by(0.05)
                    .show_value(true)
                    .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
            )
            .on_hover_text(t(Key::MaskBrushFlowTooltip))
            .changed()
        {
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let is_erase = *erase_mode;
        if ui
            .selectable_label(!is_erase, "🖌️")
            .on_hover_text(t(Key::MaskBrushPaintTooltip))
            .clicked()
        {
            *erase_mode = false;
        }
        if ui
            .selectable_label(is_erase, "🧹")
            .on_hover_text(t(Key::MaskBrushEraseTooltip))
            .clicked()
        {
            *erase_mode = true;
        }
        ui.add_space(8.0);
        let dab_count = dabs.len();
        ui.label(format!("({dab_count})"));
        if dab_count > 0
            && ui
                .small_button("🗑️")
                .on_hover_text(t(Key::MaskBrushClearTooltip))
                .clicked()
        {
            dabs.clear();
            changed = true;
        }
    });

    changed
}

fn render_local_mask_kind_controls(ui: &mut Ui, kind: &mut LocalMaskKind, lang: Language) -> bool {
    let t = |key: Key| key.translate(lang);
    let mut changed = false;

    match kind {
        LocalMaskKind::LinearGradient { start, end } => {
            changed |= labeled_slider(ui, t(Key::InspectorStartY), &mut start[1], 0.0..=1.0, None);
            changed |= labeled_slider(ui, t(Key::InspectorEndY), &mut end[1], 0.0..=1.0, None);
        }
        LocalMaskKind::RadialGradient {
            center,
            radius,
            rotation_deg: _,
        } => {
            changed |= labeled_slider(
                ui,
                t(Key::InspectorCenterX),
                &mut center[0],
                0.0..=1.0,
                None,
            );
            changed |= labeled_slider(
                ui,
                t(Key::InspectorCenterY),
                &mut center[1],
                0.0..=1.0,
                None,
            );
            changed |= labeled_slider(
                ui,
                t(Key::InspectorRadiusX),
                &mut radius[0],
                0.05..=1.0,
                None,
            );
            changed |= labeled_slider(
                ui,
                t(Key::InspectorRadiusY),
                &mut radius[1],
                0.05..=1.0,
                None,
            );
        }
        LocalMaskKind::FreehandBrush {
            dabs,
            brush_radius,
            brush_flow,
            erase_mode,
        } => {
            changed |=
                render_brush_mask_controls(ui, dabs, brush_radius, brush_flow, erase_mode, lang);
        }
    }

    changed
}

fn render_local_mask_card(
    ui: &mut Ui,
    idx: usize,
    mask: &mut LocalMaskLayer,
    active_local_overlay: &mut Option<usize>,
    active_brush_idx: &mut Option<usize>,
    lang: Language,
) -> (bool, bool) {
    let t = |key: Key| key.translate(lang);
    let mut changed = false;
    let mut remove_requested = false;

    ui.add_space(3.0);
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(30, 34, 40))
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let icon = match &mask.kind {
                    LocalMaskKind::LinearGradient { .. } => "📏",
                    LocalMaskKind::RadialGradient { .. } => "⭕",
                    LocalMaskKind::FreehandBrush { .. } => "🖌️",
                };
                if ui
                    .checkbox(
                        &mut mask.enabled,
                        egui::RichText::new(format!("{icon} {}", mask.name)).strong(),
                    )
                    .changed()
                {
                    if !mask.enabled && *active_brush_idx == Some(idx) {
                        *active_brush_idx = None;
                    }
                    changed = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("✖")
                        .on_hover_text(t(Key::InspectorDeleteMaskTooltip))
                        .clicked()
                    {
                        remove_requested = true;
                    }
                });
            });

            ui.horizontal_wrapped(|ui| {
                if mask.enabled && matches!(mask.kind, LocalMaskKind::FreehandBrush { .. }) {
                    let is_painting = *active_brush_idx == Some(idx);
                    if ui
                        .selectable_label(is_painting, t(Key::InspectorPaintButton))
                        .on_hover_text(t(Key::InspectorPaintTooltip))
                        .clicked()
                    {
                        *active_brush_idx = if is_painting { None } else { Some(idx) };
                    }
                }

                let is_overlay = *active_local_overlay == Some(idx);
                if ui
                    .selectable_label(is_overlay, "👁")
                    .on_hover_text(t(Key::InspectorOverlayTooltip))
                    .clicked()
                {
                    *active_local_overlay = if is_overlay { None } else { Some(idx) };
                    changed = true;
                }

                if ui
                    .checkbox(&mut mask.invert, t(Key::InspectorParamInvertMask))
                    .on_hover_text(t(Key::InspectorInvertTooltip))
                    .changed()
                {
                    changed = true;
                }
            });

            if mask.enabled {
                changed |= render_local_mask_sliders(ui, mask, lang);
                changed |= render_local_mask_kind_controls(ui, &mut mask.kind, lang);
            }
        });

    (changed, remove_requested)
}

fn render_geometric_masks_section(ui: &mut Ui, ctx: &mut MaskUiContext<'_>) -> bool {
    let cur_lang = ctx.language;
    ui.label(egui::RichText::new(Key::InspectorGeometricMasksTitle.translate(cur_lang)).strong());
    let mut changed = render_local_mask_creation_buttons(ui, ctx);

    let mut local_mask_to_remove: Option<usize> = None;
    let mut active_local_overlay = *ctx.show_local_mask_overlay;
    let mut active_brush_idx = *ctx.active_brush_mask_idx;

    for (idx, mask) in ctx.local_masks.iter_mut().enumerate() {
        let (card_changed, remove_req) = render_local_mask_card(
            ui,
            idx,
            mask,
            &mut active_local_overlay,
            &mut active_brush_idx,
            cur_lang,
        );
        changed |= card_changed;
        if remove_req {
            local_mask_to_remove = Some(idx);
        }
    }

    if let Some(remove_idx) = local_mask_to_remove {
        if remove_idx < ctx.local_masks.len() {
            ctx.local_masks.remove(remove_idx);
            adjust_index_after_removal(&mut active_local_overlay, remove_idx);
            adjust_index_after_removal(&mut active_brush_idx, remove_idx);
            changed = true;
        }
    }
    *ctx.show_local_mask_overlay = active_local_overlay;
    *ctx.active_brush_mask_idx = active_brush_idx;

    changed
}

fn adjust_index_after_removal(slot: &mut Option<usize>, remove_idx: usize) {
    if let Some(current) = *slot {
        if current == remove_idx {
            *slot = None;
        } else if current > remove_idx {
            *slot = Some(current - 1);
        }
    }
}

fn render_semantic_mask_card(
    ui: &mut Ui,
    layer: &mut SemanticMaskLayer,
    active_overlay: &mut Option<SemanticMaskCategory>,
    lang: Language,
) -> (bool, bool) {
    let t = |key: Key| key.translate(lang);
    let mut changed = false;
    let mut remove_requested = false;

    ui.add_space(4.0);
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(33, 36, 42))
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .checkbox(
                        &mut layer.enabled,
                        egui::RichText::new(SemanticMaskLayer::category_label_lang(
                            layer.category,
                            lang,
                        ))
                        .strong(),
                    )
                    .changed()
                {
                    changed = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("✖")
                        .on_hover_text(t(Key::InspectorAiDeleteTooltip))
                        .clicked()
                    {
                        remove_requested = true;
                    }
                });
            });

            ui.horizontal_wrapped(|ui| {
                let is_overlay = *active_overlay == Some(layer.category);
                if ui
                    .selectable_label(is_overlay, "👁")
                    .on_hover_text(t(Key::InspectorAiOverlayTooltip))
                    .clicked()
                {
                    *active_overlay = if is_overlay {
                        None
                    } else {
                        Some(layer.category)
                    };
                    changed = true;
                }

                if ui
                    .checkbox(&mut layer.invert, t(Key::InspectorParamInvertMask))
                    .on_hover_text(t(Key::InspectorAiInvertTooltip))
                    .changed()
                {
                    changed = true;
                }
            });

            if layer.enabled {
                changed |= labeled_slider(
                    ui,
                    t(Key::InspectorParamExposure),
                    &mut layer.exposure_ev,
                    -3.0..=3.0,
                    Some(0.05),
                );
                changed |= labeled_slider(
                    ui,
                    t(Key::InspectorParamContrast),
                    &mut layer.contrast,
                    -100.0..=100.0,
                    None,
                );
                changed |= labeled_slider(
                    ui,
                    t(Key::InspectorParamSaturation),
                    &mut layer.saturation,
                    -100.0..=100.0,
                    None,
                );
                changed |= labeled_slider(
                    ui,
                    t(Key::InspectorParamKelvin),
                    &mut layer.temperature_shift,
                    -100.0..=100.0,
                    None,
                );
                changed |= labeled_slider(
                    ui,
                    t(Key::InspectorFeather),
                    &mut layer.feather,
                    0.02..=0.50,
                    None,
                );
                changed |= labeled_slider(
                    ui,
                    t(Key::InspectorParamMaskOpacity),
                    &mut layer.opacity,
                    0.0..=1.0,
                    None,
                );
            }
        });

    (changed, remove_requested)
}

fn render_ai_semantic_masks_section(
    ui: &mut Ui,
    ctx: &mut MaskUiContext<'_>,
    out: &mut SectionOutput,
) -> bool {
    let cur_lang = ctx.language;
    let t = |key: Key| key.translate(cur_lang);
    let mut changed = false;

    ui.separator();
    ui.label(egui::RichText::new(t(Key::InspectorAiMasksTitle)).strong());
    let caps = rraw_coreml::CoreMlHardwareCapabilities::detect();
    if caps.has_ane {
        ui.label(
            egui::RichText::new(t(Key::MaskAiHardwareAne))
                .small()
                .color(Color32::from_rgb(90, 220, 140)),
        );
    }
    ui.label(
        egui::RichText::new(t(Key::InspectorAiMasksSubtitle))
            .small()
            .color(Color32::GRAY),
    );
    ui.horizontal_wrapped(|ui| {
        for cat in SemanticMaskCategory::ALL {
            let exists = ctx.semantic_masks.iter().any(|m| m.category == cat);
            let label = SemanticMaskLayer::category_label_lang(cat, cur_lang);
            let btn_text = if exists {
                egui::RichText::new(format!("✓ {label}"))
                    .strong()
                    .color(Color32::from_rgb(110, 225, 145))
            } else {
                egui::RichText::new(label.to_string())
            };
            let tooltip = if exists {
                t(Key::MaskAiDeselectTooltip)
            } else {
                t(Key::MaskAiSelectTooltip)
            };
            if ui
                .selectable_label(exists, btn_text)
                .on_hover_text(tooltip)
                .clicked()
            {
                if exists {
                    ctx.semantic_masks.retain(|m| m.category != cat);
                    if *ctx.show_semantic_mask_overlay == Some(cat) {
                        *ctx.show_semantic_mask_overlay = None;
                    }
                } else {
                    ctx.semantic_masks.push(SemanticMaskLayer::new(cat));
                }
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        }
    });

    let mut category_to_remove: Option<SemanticMaskCategory> = None;
    let mut active_overlay = *ctx.show_semantic_mask_overlay;

    for layer in ctx.semantic_masks.iter_mut() {
        let (card_changed, remove_req) =
            render_semantic_mask_card(ui, layer, &mut active_overlay, cur_lang);
        changed |= card_changed;
        if remove_req {
            category_to_remove = Some(layer.category);
        }
    }

    *ctx.show_semantic_mask_overlay = active_overlay;
    if let Some(cat) = category_to_remove {
        ctx.semantic_masks.retain(|m| m.category != cat);
        if *ctx.show_semantic_mask_overlay == Some(cat) {
            *ctx.show_semantic_mask_overlay = None;
        }
        changed = true;
    }

    if !ctx.semantic_masks.is_empty() && ui.button(t(Key::InspectorResetAiMasks)).clicked() {
        ctx.semantic_masks.clear();
        *ctx.show_semantic_mask_overlay = None;
        changed = true;
    }

    changed
}

/// Renders the AI Semantic & Local geometric/brush masks darkroom section.
pub fn render(ui: &mut Ui, ctx: &mut MaskUiContext<'_>) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let mut changed = render_geometric_masks_section(ui, ctx);
    changed |= render_ai_semantic_masks_section(ui, ctx, &mut out);

    if changed {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::adjust_index_after_removal;

    #[test]
    fn test_adjust_index_after_removal_clears_decrements_or_preserves() {
        let mut slot = Some(1);
        adjust_index_after_removal(&mut slot, 1);
        assert_eq!(slot, None);

        let mut slot = Some(2);
        adjust_index_after_removal(&mut slot, 0);
        assert_eq!(slot, Some(1));

        let mut slot = Some(0);
        adjust_index_after_removal(&mut slot, 2);
        assert_eq!(slot, Some(0));
    }
}
