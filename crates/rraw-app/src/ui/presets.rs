#![forbid(unsafe_code)]

//! Color grading presets selection and quick controls.

use egui::Color32;

use crate::app::RrawApp;
use crate::state::*;

impl RrawApp {
    /// Renders the presets dropdown list and quick controls in the left panel.
    pub fn render_presets_section(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Style :").strong().size(12.0));
                egui::ComboBox::from_id_salt("left_panel_preset_combo")
                    .selected_text(self.current_preset.name())
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for preset in GradingPreset::ALL {
                            if preset == GradingPreset::Custom {
                                continue;
                            }
                            let is_sel = self.current_preset == preset;
                            if ui.selectable_label(is_sel, preset.name()).clicked() {
                                self.current_preset = preset;
                                preset.apply(&mut self.grading);
                                self.needs_preview_update = true;
                                self.save_current_photo_sidecar();
                                ui.ctx().request_repaint();
                            }
                        }
                    });

                if ui
                    .small_button("↺ As Shot")
                    .on_hover_text("Réinitialiser aux réglages boîtier d'origine")
                    .clicked()
                {
                    GradingPreset::AsShot.apply(&mut self.grading);
                    self.current_preset = GradingPreset::AsShot;
                    self.needs_preview_update = true;
                    self.save_current_photo_sidecar();
                    ui.ctx().request_repaint();
                }
            });

            if self.current_preset != GradingPreset::AsShot
                && self.current_preset != GradingPreset::Custom
            {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(self.current_preset.description())
                        .small()
                        .italics()
                        .color(Color32::from_rgb(180, 190, 205)),
                );
            }
        });
    }

    /// Renders the presets selection tab (delegates to dropdown section).
    pub fn render_presets_tab(&mut self, ui: &mut egui::Ui) {
        self.render_presets_section(ui);
    }
}
