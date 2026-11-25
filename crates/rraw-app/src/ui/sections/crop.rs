#![forbid(unsafe_code)]

//! Crop tool inspector section.

use egui::{Color32, Ui};

use super::SectionOutput;
use crate::grading::GeometryGradingParams;
use crate::i18n::{Key, Language};
use crate::state::CropAspectRatio;

/// Ephemeral UI flags for the Crop section (not persisted in `.rraw.json` sidecars).
pub struct CropUiFlags<'a> {
    pub is_crop_editing: &'a mut bool,
    pub show_grid: &'a mut bool,
}

/// Renders the Crop darkroom section.
pub fn render(
    ui: &mut Ui,
    geometry: &mut GeometryGradingParams,
    flags: &mut CropUiFlags<'_>,
    photo_dimensions: Option<(u32, u32)>,
    cur_lang: Language,
) -> SectionOutput {
    let mut out = SectionOutput::nothing();
    let t = |key: Key| key.translate(cur_lang);

    let c_crop = ui.checkbox(&mut geometry.crop_enabled, t(Key::InspectorEnableCrop));
    if c_crop.changed() {
        if !geometry.crop_enabled {
            *flags.is_crop_editing = false;
        }
        out.needs_preview_update = true;
        ui.ctx().request_repaint();
    }

    if !geometry.crop_enabled {
        *flags.is_crop_editing = false;
    }

    if geometry.crop_enabled {
        ui.horizontal(|ui| {
            ui.label(t(Key::InspectorParamCropRatio));
            egui::ComboBox::from_id_salt("crop_ratio_select")
                .selected_text(geometry.crop_aspect.label_lang(cur_lang))
                .show_ui(ui, |ui| {
                    for ratio in CropAspectRatio::ALL {
                        let is_sel = geometry.crop_aspect == ratio;
                        if ui
                            .selectable_label(is_sel, ratio.label_lang(cur_lang))
                            .clicked()
                        {
                            geometry.crop_aspect = ratio;
                            let (orig_w, orig_h) = if let Some(dims) = photo_dimensions {
                                (dims.0 as f32, dims.1 as f32)
                            } else {
                                (3.0, 2.0)
                            };
                            if let Some(target_r) = ratio.ratio(orig_w, orig_h) {
                                let sensor_aspect = orig_w / orig_h.max(1.0);
                                let norm_aspect = target_r / sensor_aspect;
                                let (crop_w, crop_h) = if norm_aspect <= 1.0 {
                                    (norm_aspect, 1.0)
                                } else {
                                    (1.0, 1.0 / norm_aspect)
                                };
                                geometry.crop_left = ((1.0 - crop_w) * 0.5).clamp(0.0, 0.45);
                                geometry.crop_right =
                                    (geometry.crop_left + crop_w).clamp(0.55, 1.0);
                                geometry.crop_top = ((1.0 - crop_h) * 0.5).clamp(0.0, 0.45);
                                geometry.crop_bottom =
                                    (geometry.crop_top + crop_h).clamp(0.55, 1.0);
                            }
                            *flags.is_crop_editing = true;
                            out.needs_preview_update = true;
                            out.save_sidecar = true;
                            ui.ctx().request_repaint();
                        }
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if *flags.is_crop_editing {
                if ui
                    .button(
                        egui::RichText::new(t(Key::InspectorValidateCrop))
                            .strong()
                            .color(Color32::from_rgb(100, 220, 120)),
                    )
                    .clicked()
                {
                    *flags.is_crop_editing = false;
                    out.needs_preview_update = true;
                    out.save_sidecar = true;
                    ui.ctx().request_repaint();
                }
            } else if ui
                .button(egui::RichText::new(t(Key::InspectorAdjustCrop)).strong())
                .clicked()
            {
                *flags.is_crop_editing = true;
                ui.ctx().request_repaint();
            }

            if ui.button(t(Key::InspectorResetCrop)).clicked() {
                geometry.crop_left = 0.0;
                geometry.crop_top = 0.0;
                geometry.crop_right = 1.0;
                geometry.crop_bottom = 1.0;
                geometry.crop_aspect = CropAspectRatio::Free;
                *flags.is_crop_editing = false;
                out.needs_preview_update = true;
                out.save_sidecar = true;
                ui.ctx().request_repaint();
            }
        });

        let c_grid = ui.checkbox(flags.show_grid, t(Key::InspectorShowCropGrid));
        if c_grid.changed() {
            ui.ctx().request_repaint();
        }

        ui.label(
            egui::RichText::new(t(Key::CropHint))
                .small()
                .color(Color32::GRAY),
        );
    }

    out
}
