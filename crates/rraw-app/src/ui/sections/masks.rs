#![forbid(unsafe_code)]

//! AI semantic masks and local geometric/brush masks inspector section.

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

/// Renders the AI Semantic & Local geometric/brush masks darkroom section.
#[allow(clippy::too_many_lines)]
pub fn render(ui: &mut Ui, ctx: &mut MaskUiContext<'_>) -> SectionOutput {
    let local_masks = &mut *ctx.local_masks;
    let show_local_mask_overlay = &mut *ctx.show_local_mask_overlay;
    let active_brush_mask_idx = &mut *ctx.active_brush_mask_idx;
    let semantic_masks = &mut *ctx.semantic_masks;
    let show_semantic_mask_overlay = &mut *ctx.show_semantic_mask_overlay;
    let cur_lang = ctx.language;
    let mut out = SectionOutput::nothing();
    let mut changed = false;
    let t = |key: Key| key.translate(cur_lang);

    ui.label(egui::RichText::new(t(Key::InspectorGeometricMasksTitle)).strong());
    ui.vertical(|ui| {
        if ui
            .button(t(Key::InspectorParamAddLinear))
            .on_hover_text(t(Key::InspectorAddLinearTooltip))
            .clicked()
        {
            let count = local_masks.len() + 1;
            local_masks.push(LocalMaskLayer::new_linear(
                format!("{} {count}", t(Key::MaskLinearPrefix)),
                [0.5, 0.0],
                [0.5, 0.45],
            ));
            *show_local_mask_overlay = None;
            changed = true;
        }
        if ui
            .button(t(Key::InspectorParamAddRadial))
            .on_hover_text(t(Key::InspectorAddRadialTooltip))
            .clicked()
        {
            let count = local_masks.len() + 1;
            local_masks.push(LocalMaskLayer::new_radial(
                format!("{} {count}", t(Key::MaskRadialPrefix)),
                [0.5, 0.5],
                [0.3, 0.3],
            ));
            *show_local_mask_overlay = None;
            *active_brush_mask_idx = None;
            changed = true;
        }
        if ui
            .button(t(Key::InspectorParamAddBrush))
            .on_hover_text(t(Key::InspectorAddBrushTooltip))
            .clicked()
        {
            let count = local_masks.len() + 1;
            local_masks.push(LocalMaskLayer::new_brush(
                format!("{} {count}", t(Key::MaskBrushPrefix)),
                0.08,
                0.65,
            ));
            let new_idx = local_masks.len() - 1;
            *show_local_mask_overlay = None;
            *active_brush_mask_idx = Some(new_idx);
            changed = true;
        }
    });

    let mut local_mask_to_remove: Option<usize> = None;
    let mut active_local_overlay = *show_local_mask_overlay;
    let mut active_brush_idx = *active_brush_mask_idx;

    for (idx, mask) in local_masks.iter_mut().enumerate() {
        ui.add_space(3.0);
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(30, 34, 40))
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
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
                        if !mask.enabled && active_brush_idx == Some(idx) {
                            active_brush_idx = None;
                        }
                        changed = true;
                    }

                    if mask.enabled && matches!(mask.kind, LocalMaskKind::FreehandBrush { .. }) {
                        let is_painting = active_brush_idx == Some(idx);
                        if ui
                            .selectable_label(is_painting, t(Key::InspectorPaintButton))
                            .on_hover_text(t(Key::InspectorPaintTooltip))
                            .clicked()
                        {
                            active_brush_idx = if is_painting { None } else { Some(idx) };
                        }
                    }

                    let is_overlay = active_local_overlay == Some(idx);
                    if ui
                        .selectable_label(is_overlay, "👁")
                        .on_hover_text(t(Key::InspectorOverlayTooltip))
                        .clicked()
                    {
                        active_local_overlay = if is_overlay { None } else { Some(idx) };
                        changed = true;
                    }

                    if ui
                        .checkbox(&mut mask.invert, t(Key::InspectorParamInvertMask))
                        .on_hover_text(t(Key::InspectorInvertTooltip))
                        .changed()
                    {
                        changed = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("✖")
                            .on_hover_text(t(Key::InspectorDeleteMaskTooltip))
                            .clicked()
                        {
                            local_mask_to_remove = Some(idx);
                        }
                    });
                });

                if mask.enabled {
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamExposure));
                        if ui
                            .add(
                                egui::Slider::new(&mut mask.exposure_ev, -3.0..=3.0)
                                    .step_by(0.05)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamContrast));
                        if ui
                            .add(
                                egui::Slider::new(&mut mask.contrast, -100.0..=100.0)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamSaturation));
                        if ui
                            .add(
                                egui::Slider::new(&mut mask.saturation, -100.0..=100.0)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamKelvin));
                        if ui
                            .add(
                                egui::Slider::new(&mut mask.temperature_shift, -100.0..=100.0)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorSmoothness));
                        if ui
                            .add(egui::Slider::new(&mut mask.feather, 0.05..=1.0).show_value(true))
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    // Coordinates controls
                    match &mut mask.kind {
                        LocalMaskKind::LinearGradient { start, end } => {
                            ui.horizontal(|ui| {
                                ui.label(t(Key::InspectorStartY));
                                if ui
                                    .add(
                                        egui::Slider::new(&mut start[1], 0.0..=1.0)
                                            .show_value(true),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(t(Key::InspectorEndY));
                                if ui
                                    .add(egui::Slider::new(&mut end[1], 0.0..=1.0).show_value(true))
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                        }
                        LocalMaskKind::RadialGradient {
                            center,
                            radius,
                            rotation_deg: _,
                        } => {
                            ui.horizontal(|ui| {
                                ui.label(t(Key::InspectorCenterX));
                                if ui
                                    .add(
                                        egui::Slider::new(&mut center[0], 0.0..=1.0)
                                            .show_value(true),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(t(Key::InspectorCenterY));
                                if ui
                                    .add(
                                        egui::Slider::new(&mut center[1], 0.0..=1.0)
                                            .show_value(true),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(t(Key::InspectorRadiusX));
                                if ui
                                    .add(
                                        egui::Slider::new(&mut radius[0], 0.05..=1.0)
                                            .show_value(true),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(t(Key::InspectorRadiusY));
                                if ui
                                    .add(
                                        egui::Slider::new(&mut radius[1], 0.05..=1.0)
                                            .show_value(true),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });
                        }
                        LocalMaskKind::FreehandBrush {
                            dabs,
                            brush_radius,
                            brush_flow,
                            erase_mode,
                        } => {
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
                        }
                    }
                }
            });
    }

    if let Some(remove_idx) = local_mask_to_remove {
        if remove_idx < local_masks.len() {
            local_masks.remove(remove_idx);
            if active_local_overlay == Some(remove_idx) {
                active_local_overlay = None;
            }
            if active_brush_idx == Some(remove_idx) {
                active_brush_idx = None;
            } else if let Some(b_idx) = active_brush_idx {
                if b_idx > remove_idx {
                    active_brush_idx = Some(b_idx - 1);
                }
            }
            changed = true;
        }
    }
    *show_local_mask_overlay = active_local_overlay;
    *active_brush_mask_idx = active_brush_idx;

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t(Key::InspectorAiMasksTitle)).strong());
        let caps = rraw_coreml::CoreMlHardwareCapabilities::detect();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if caps.has_ane {
                ui.label(
                    egui::RichText::new(t(Key::MaskAiHardwareAne))
                        .small()
                        .color(Color32::from_rgb(90, 220, 140)),
                );
            }
        });
    });
    ui.label(
        egui::RichText::new(t(Key::InspectorAiMasksSubtitle))
            .small()
            .color(Color32::GRAY),
    );
    ui.horizontal_wrapped(|ui| {
        for cat in SemanticMaskCategory::ALL {
            let exists = semantic_masks.iter().any(|m| m.category == cat);
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
                    semantic_masks.retain(|m| m.category != cat);
                    if *show_semantic_mask_overlay == Some(cat) {
                        *show_semantic_mask_overlay = None;
                    }
                } else {
                    semantic_masks.push(SemanticMaskLayer::new(cat));
                }
                out.mark_custom_preset = true;
                out.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        }
    });

    let mut category_to_remove: Option<SemanticMaskCategory> = None;
    let mut active_overlay = *show_semantic_mask_overlay;

    for layer in semantic_masks.iter_mut() {
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
                                cur_lang,
                            ))
                            .strong(),
                        )
                        .changed()
                    {
                        changed = true;
                    }

                    let is_overlay = active_overlay == Some(layer.category);
                    if ui
                        .selectable_label(is_overlay, "👁 Masque")
                        .on_hover_text(t(Key::InspectorAiOverlayTooltip))
                        .clicked()
                    {
                        active_overlay = if is_overlay {
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

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("✖")
                            .on_hover_text(t(Key::InspectorAiDeleteTooltip))
                            .clicked()
                        {
                            category_to_remove = Some(layer.category);
                        }
                    });
                });

                if layer.enabled {
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamExposure));
                        if ui
                            .add(
                                egui::Slider::new(&mut layer.exposure_ev, -3.0..=3.0)
                                    .step_by(0.05)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamContrast));
                        if ui
                            .add(
                                egui::Slider::new(&mut layer.contrast, -100.0..=100.0)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamSaturation));
                        if ui
                            .add(
                                egui::Slider::new(&mut layer.saturation, -100.0..=100.0)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamKelvin));
                        if ui
                            .add(
                                egui::Slider::new(&mut layer.temperature_shift, -100.0..=100.0)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorFeather));
                        if ui
                            .add(
                                egui::Slider::new(&mut layer.feather, 0.02..=0.50).show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamMaskOpacity));
                        if ui
                            .add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0).show_value(true))
                            .changed()
                        {
                            changed = true;
                        }
                    });
                }
            });
    }

    *show_semantic_mask_overlay = active_overlay;
    if let Some(cat) = category_to_remove {
        semantic_masks.retain(|m| m.category != cat);
        if *show_semantic_mask_overlay == Some(cat) {
            *show_semantic_mask_overlay = None;
        }
        changed = true;
    }

    if !semantic_masks.is_empty() && ui.button(t(Key::InspectorResetAiMasks)).clicked() {
        semantic_masks.clear();
        *show_semantic_mask_overlay = None;
        changed = true;
    }

    if changed {
        out.mark_custom_preset = true;
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    out
}
