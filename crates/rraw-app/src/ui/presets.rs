#![forbid(unsafe_code)]

//! Color grading presets selection and quick controls.

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::state::*;

impl RrawApp {
    /// Renders the presets dropdown list and quick controls in the left panel.
    pub fn render_presets_section(&mut self, ui: &mut egui::Ui) {
        let cur_lang = self.lang();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(self.t(Key::PresetsStyleLabel))
                        .strong()
                        .size(12.0),
                );
                let combo_w = (ui.available_width() - 32.0).max(80.0);
                egui::ComboBox::from_id_salt("left_panel_preset_combo")
                    .selected_text(self.session.current_preset.name_lang(cur_lang))
                    .width(combo_w)
                    .show_ui(ui, |ui| {
                        for preset in GradingPreset::ALL {
                            if preset == GradingPreset::Custom {
                                continue;
                            }
                            let is_sel = self.session.current_preset == preset;
                            if ui
                                .selectable_label(is_sel, preset.name_lang(cur_lang))
                                .clicked()
                            {
                                self.session.current_preset = preset;
                                preset.apply(&mut self.session.grading);
                                self.session.needs_preview_update = true;
                                self.save_current_photo_sidecar();
                                ui.ctx().request_repaint();
                            }
                        }
                    });

                if ui
                    .small_button("↺")
                    .on_hover_text(self.t(Key::PresetsResetTooltip))
                    .clicked()
                {
                    GradingPreset::AsShot.apply(&mut self.session.grading);
                    self.session.current_preset = GradingPreset::AsShot;
                    self.session.needs_preview_update = true;
                    self.save_current_photo_sidecar();
                    ui.ctx().request_repaint();
                }
            });

            if self.session.current_preset != GradingPreset::AsShot
                && self.session.current_preset != GradingPreset::Custom
            {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(self.session.current_preset.description_lang(cur_lang))
                        .small()
                        .italics()
                        .color(Color32::from_rgb(180, 190, 205)),
                );
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(self.t(Key::PresetsLutLabel))
                        .strong()
                        .size(12.0),
                );
                if ui
                    .button(self.t(Key::PresetsImportLut))
                    .on_hover_text(self.t(Key::PresetsImportLutTooltip))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Cube 3D LUT (*.cube)", &["cube"])
                        .pick_file()
                    {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(lut) = rraw_color::ColorLut3d::from_cube_str(&content) {
                                let name = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("Custom LUT")
                                    .to_string();
                                self.session.grading.custom_clut = Some(CustomClutState {
                                    name,
                                    path: Some(path),
                                    lut: std::sync::Arc::new(lut),
                                    strength: 1.0,
                                });
                                self.session.current_preset = GradingPreset::Custom;
                                self.session.needs_preview_update = true;
                                self.save_current_photo_sidecar();
                                ui.ctx().request_repaint();
                            }
                        }
                    }
                }
            });

            let mut remove_lut = false;
            let remove_lut_tooltip = self.t(Key::PresetsRemoveLutTooltip);
            let lut_intensity_label = self.t(Key::PresetsLutIntensity);
            if let Some(clut) = &mut self.session.grading.custom_clut {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("🎞 {} ({}³)", clut.name, clut.lut.size()))
                            .small()
                            .strong()
                            .color(Color32::from_rgb(100, 200, 255)),
                    );
                    if ui
                        .small_button("✖")
                        .on_hover_text(remove_lut_tooltip)
                        .clicked()
                    {
                        remove_lut = true;
                    }
                });

                if !remove_lut {
                    let mut pct = clut.strength * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut pct, 0.0..=100.0).text(lut_intensity_label))
                        .changed()
                    {
                        clut.strength = pct / 100.0;
                        self.session.needs_preview_update = true;
                        self.save_current_photo_sidecar();
                        ui.ctx().request_repaint();
                    }
                }
            }

            if remove_lut {
                self.session.grading.custom_clut = None;
                self.session.needs_preview_update = true;
                self.save_current_photo_sidecar();
                ui.ctx().request_repaint();
            }
        });
    }

    /// Renders the presets selection tab (delegates to dropdown section).
    pub fn render_presets_tab(&mut self, ui: &mut egui::Ui) {
        self.render_presets_section(ui);
    }
}
