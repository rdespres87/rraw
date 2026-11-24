#![forbid(unsafe_code)]

//! Export target mode, scaling, destination folder, filename template, and footer controls.

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::state::{ExportScaleMode, ExportTargetMode};

/// Actions emitted by the export dialog footer buttons.
#[derive(Default)]
pub(super) struct ExportFooterAction {
    pub close_dialog: bool,
    pub trigger_export: bool,
}

impl RrawApp {
    /// Renders the export target selector (single photo, batch selection, or timelapse sequence).
    pub(super) fn render_export_target_section(&mut self, ui: &mut egui::Ui, lang: Language) {
        let t = |key: Key| key.translate(lang);

        ui.heading(t(Key::ExportTargetHeading));
        let sel_count = self
            .library
            .selected_photos
            .iter()
            .filter(|p| self.matches_active_filter(p))
            .count();

        ui.horizontal_wrapped(|ui| {
            if ui
                .radio_value(
                    &mut self.export.config.target_mode,
                    ExportTargetMode::CurrentPhoto,
                    t(Key::ExportTargetCurrent),
                )
                .clicked()
            {
                self.export.set_target_mode(ExportTargetMode::CurrentPhoto);
            }
            let batch_word = t(Key::ExportTargetBatch);
            let s_suffix = if sel_count > 1 { "s" } else { "" };
            let batch_label = format!("{batch_word} ({sel_count} photo{s_suffix})");
            if ui
                .radio_value(
                    &mut self.export.config.target_mode,
                    ExportTargetMode::BatchSelection,
                    batch_label,
                )
                .clicked()
            {
                self.export
                    .set_target_mode(ExportTargetMode::BatchSelection);
            }
            if ui
                .radio_value(
                    &mut self.export.config.target_mode,
                    ExportTargetMode::TimelapseSequence,
                    t(Key::ExportTargetTimelapse),
                )
                .clicked()
            {
                self.export
                    .set_target_mode(ExportTargetMode::TimelapseSequence);
                if self.export.config.subfolder == "Export" {
                    self.export.config.subfolder = "Timelapse".to_owned();
                }
            }
        });

        match self.export.config.target_mode {
            ExportTargetMode::CurrentPhoto => self.render_active_photo_summary(ui, lang),
            ExportTargetMode::BatchSelection => {
                if sel_count == 0 {
                    ui.label(
                        egui::RichText::new(t(Key::ExportNoBatchWarning))
                            .small()
                            .color(Color32::from_rgb(255, 180, 50)),
                    );
                }
            }
            ExportTargetMode::TimelapseSequence => {
                ui.add_space(4.0);
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(24, 28, 36))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        self.render_timelapse_controls_ui(ui, "dialog");
                    });
            }
        }
    }

    fn render_active_photo_summary(&self, ui: &mut egui::Ui, lang: Language) {
        let t = |key: Key| key.translate(lang);
        if let Some(photo) = &self.session.loaded_photo {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t(Key::ExportActivePhoto)).strong());
                if let Some(name) = photo.path.file_name().and_then(|n| n.to_str()) {
                    ui.label(egui::RichText::new(name).color(Color32::from_rgb(100, 180, 255)));
                }
            });
            let crop_text = if self.session.grading.geometry.crop_enabled {
                t(Key::ExportCropApplied)
            } else {
                ""
            };
            let sensor_title = t(Key::ExportSensorResolution);
            ui.label(
                egui::RichText::new(format!(
                    "{sensor_title} {} × {} px{crop_text}",
                    photo.dimensions.0, photo.dimensions.1
                ))
                .small()
                .color(Color32::LIGHT_GRAY),
            );
        }
    }

    /// Renders output resizing, destination directory picker, subfolder, and filename template controls.
    pub(super) fn render_export_destination_section(&mut self, ui: &mut egui::Ui, lang: Language) {
        let t = |key: Key| key.translate(lang);

        ui.separator();
        ui.heading(t(Key::ExportResizeHeading));
        egui::ComboBox::from_id_salt("export_scale_combo")
            .selected_text(self.export.config.scale_mode.label_lang(lang))
            .show_ui(ui, |ui| {
                for mode in [
                    ExportScaleMode::Original,
                    ExportScaleMode::Percent75,
                    ExportScaleMode::Percent50,
                    ExportScaleMode::FitLongEdge2048,
                    ExportScaleMode::FitLongEdge3840,
                ] {
                    ui.selectable_value(
                        &mut self.export.config.scale_mode,
                        mode,
                        mode.label_lang(lang),
                    );
                }
            });

        ui.separator();
        ui.heading(t(Key::ExportDestinationHeading));
        ui.horizontal(|ui| {
            let dest_label = if let Some(dir) = &self.export.config.custom_dir {
                dir.to_string_lossy().into_owned()
            } else if let Some(photo) = &self.session.loaded_photo {
                photo
                    .path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| t(Key::ExportSourceFolder).to_owned())
            } else {
                t(Key::ExportSourceFolder).to_owned()
            };
            ui.label(egui::RichText::new(dest_label).small().monospace());
            if ui.button(t(Key::SidebarBrowseButton)).clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.export.config.custom_dir = Some(folder);
                }
            }
        });

        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.export.config.use_subfolder,
                t(Key::ExportUseSubfolder),
            );
            if self.export.config.use_subfolder {
                ui.add(
                    egui::TextEdit::singleline(&mut self.export.config.subfolder)
                        .desired_width(120.0),
                );
            }
        });

        if self.export.config.target_mode == ExportTargetMode::TimelapseSequence {
            ui.horizontal(|ui| {
                ui.label(t(Key::ExportSequencePrefix));
                ui.add(
                    egui::TextEdit::singleline(&mut self.export.config.timelapse.sequence_prefix)
                        .desired_width(120.0),
                );
                let ext = self.export.config.format.extension();
                let sample = rraw_export::format_sequence_filename(
                    &self.export.config.timelapse.sequence_prefix,
                    1,
                    ext,
                );
                ui.label(
                    egui::RichText::new(format!("→ {sample}"))
                        .small()
                        .monospace(),
                );
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(t(Key::ExportFileSuffix));
                ui.add(
                    egui::TextEdit::singleline(&mut self.export.config.suffix).desired_width(120.0),
                );
            });
        }
    }

    /// Renders export progress, status feedback, and Cancel/Export action buttons.
    pub(super) fn render_export_footer(
        &self,
        ui: &mut egui::Ui,
        lang: Language,
    ) -> ExportFooterAction {
        let t = |key: Key| key.translate(lang);
        let mut action = ExportFooterAction::default();

        if self.export.stats.is_exporting {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(&self.export.stats.progress_label).strong());
            });
            ui.add(egui::ProgressBar::new(self.export.stats.progress).animate(true));
        }

        if let Some((msg, success)) = &self.export.stats.status {
            ui.add_space(6.0);
            let col = if *success {
                Color32::from_rgb(100, 220, 120)
            } else {
                Color32::from_rgb(255, 100, 100)
            };
            ui.label(egui::RichText::new(msg).color(col));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.export.stats.is_exporting,
                    egui::Button::new(t(Key::ExportCancel)),
                )
                .clicked()
            {
                action.close_dialog = true;
            }
            let export_btn_text = match self.export.config.target_mode {
                ExportTargetMode::CurrentPhoto => t(Key::ExportBtnSingle),
                ExportTargetMode::BatchSelection => t(Key::ExportBtnBatch),
                ExportTargetMode::TimelapseSequence => t(Key::ExportBtnTimelapse),
            };
            if ui
                .add_enabled(
                    !self.export.stats.is_exporting,
                    egui::Button::new(egui::RichText::new(export_btn_text).strong())
                        .fill(Color32::from_rgb(30, 110, 210)),
                )
                .clicked()
            {
                action.trigger_export = true;
            }
        });

        action
    }
}
