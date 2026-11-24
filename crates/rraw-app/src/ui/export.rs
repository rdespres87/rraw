#![forbid(unsafe_code)]

//! Image export dialog, batch processor, and RAW Timelapse & Temporal Deflicker sequence exporter.

use std::path::PathBuf;

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::state::*;

/// Configuration parameters for single image encoding and disk serialization.
#[derive(Debug, Clone, Copy)]
pub struct ExportWriteConfig {
    pub format: ExportFormatChoice,
    pub jpeg_quality: u8,
    pub bit_depth: ExportBitDepth,
    pub scale_mode: ExportScaleMode,
    pub hdr_gain_map: bool,
}

impl RrawApp {
    pub fn render_timelapse_controls_ui(&mut self, ui: &mut egui::Ui, id_salt_prefix: &str) {
        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let tl_paths = self.timelapse_candidate_paths();
        let frame_count = tl_paths.len();
        let dur_s = self.export.timelapse_fps.duration_seconds(frame_count);

        let seq_text = crate::i18n::tc(lang, "Export.TimelapseSequence")
            .replace("{count}", &frame_count.to_string());
        let dur_text = crate::i18n::tc(lang, "Export.TimelapseDuration")
            .replace("{dur}", &format!("{dur_s:.2}"))
            .replace("{fps}", &self.export.timelapse_fps.fps().to_string());
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(seq_text)
                    .strong()
                    .color(Color32::from_rgb(120, 220, 255)),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(dur_text)
                    .strong()
                    .color(Color32::from_rgb(130, 235, 165)),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(t(Key::TimelapseFps));
            egui::ComboBox::from_id_salt(format!("{id_salt_prefix}_timelapse_fps_combo"))
                .selected_text(self.export.timelapse_fps.label())
                .show_ui(ui, |ui| {
                    for fps in rraw_export::TimelapseFrameRate::ALL {
                        ui.selectable_value(&mut self.export.timelapse_fps, fps, fps.label());
                    }
                });
        });

        ui.separator();
        ui.checkbox(
            &mut self.export.timelapse_deflicker.enabled,
            t(Key::TimelapseDeflickerTitle),
        )
        .on_hover_text(t(Key::TimelapseDeflickerTooltip));

        if self.export.timelapse_deflicker.enabled {
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseWindowSize));
                let mut rad = self.export.timelapse_deflicker.window_radius as u32;
                if ui
                    .add(
                        egui::Slider::new(&mut rad, 1..=15)
                            .suffix(crate::i18n::tc(lang, "Export.TimelapseFramesSuffix")),
                    )
                    .changed()
                {
                    self.export.timelapse_deflicker.window_radius = rad as usize;
                }
            });
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseIntensity));
                let mut pct = (self.export.timelapse_deflicker.strength * 100.0).round();
                if ui
                    .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%"))
                    .changed()
                {
                    self.export.timelapse_deflicker.strength = (pct / 100.0).clamp(0.0, 1.0);
                }
            });
        }

        ui.separator();
        ui.checkbox(
            &mut self.export.timelapse_keyframe_ramping,
            t(Key::TimelapseHolyGrailTitle),
        )
        .on_hover_text(t(Key::TimelapseHolyGrailTooltip));

        if self.export.timelapse_keyframe_ramping && frame_count >= 2 {
            let first_path = tl_paths[0].clone();
            let last_path = tl_paths[frame_count - 1].clone();

            if let Some(loaded) = &self.session.loaded_photo {
                self.library
                    .photo_gradings
                    .insert(loaded.path.clone(), self.session.grading.clone());
            }
            if !self.library.photo_gradings.contains_key(&first_path) {
                let g = load_photo_sidecars(&first_path).0.unwrap_or_default();
                self.library.photo_gradings.insert(first_path.clone(), g);
            }
            if !self.library.photo_gradings.contains_key(&last_path) {
                let g = load_photo_sidecars(&last_path).0.unwrap_or_default();
                self.library.photo_gradings.insert(last_path.clone(), g);
            }

            let first_name = first_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Image 1")
                .to_string();
            let last_name = last_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Image N")
                .to_string();

            let mut open_preview_target: Option<PathBuf> = None;

            ui.add_space(4.0);
            // Keyframe 1 (Start)
            let mut g_first = self
                .library
                .photo_gradings
                .get(&first_path)
                .cloned()
                .unwrap_or_default();
            let mut first_changed = false;
            let mut first_commit_disk = false;
            ui.horizontal(|ui| {
                let start_title = format!("{} (#1 : {first_name})", t(Key::TimelapseStartLabel));
                ui.label(
                    egui::RichText::new(start_title)
                        .small()
                        .strong()
                        .color(Color32::from_rgb(255, 215, 110)),
                );
                if ui
                    .small_button(t(Key::TimelapseViewButton))
                    .on_hover_text(t(Key::TimelapseViewStartTooltip))
                    .clicked()
                {
                    open_preview_target = Some(first_path.clone());
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t(Key::TimelapseExpShort)).small());
                let r_exp = ui.add(
                    egui::Slider::new(&mut g_first.tone.exposure_ev, -4.0..=4.0)
                        .step_by(0.05)
                        .suffix(" EV"),
                );
                if r_exp.changed() {
                    first_changed = true;
                }
                if r_exp.drag_stopped() || r_exp.lost_focus() {
                    first_commit_disk = true;
                }
                ui.label(egui::RichText::new(t(Key::TimelapseTempShort)).small());
                let r_tmp = ui.add(
                    egui::Slider::new(&mut g_first.color.kelvin, 2000.0..=12000.0)
                        .step_by(50.0)
                        .suffix(" K"),
                );
                if r_tmp.changed() {
                    first_changed = true;
                }
                if r_tmp.drag_stopped() || r_tmp.lost_focus() {
                    first_commit_disk = true;
                }
            });
            if first_changed {
                self.library
                    .photo_gradings
                    .insert(first_path.clone(), g_first.clone());
                if self
                    .session
                    .loaded_photo
                    .as_ref()
                    .is_some_and(|p| p.path == first_path)
                {
                    self.session.grading = g_first.clone();
                    self.session.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            if first_commit_disk {
                let r = self
                    .library
                    .photo_ratings
                    .get(&first_path)
                    .copied()
                    .unwrap_or(0);
                save_photo_sidecars(&first_path, &g_first, r);
            }

            ui.add_space(2.0);
            // Keyframe N (End)
            let mut g_last = self
                .library
                .photo_gradings
                .get(&last_path)
                .cloned()
                .unwrap_or_default();
            let mut last_changed = false;
            let mut last_commit_disk = false;
            ui.horizontal(|ui| {
                let end_title = format!(
                    "{} (#{frame_count} : {last_name})",
                    t(Key::TimelapseEndLabel)
                );
                ui.label(
                    egui::RichText::new(end_title)
                        .small()
                        .strong()
                        .color(Color32::from_rgb(155, 195, 255)),
                );
                if ui
                    .small_button(t(Key::TimelapseViewButton))
                    .on_hover_text(t(Key::TimelapseViewEndTooltip))
                    .clicked()
                {
                    open_preview_target = Some(last_path.clone());
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t(Key::TimelapseExpShort)).small());
                let r_exp = ui.add(
                    egui::Slider::new(&mut g_last.tone.exposure_ev, -4.0..=4.0)
                        .step_by(0.05)
                        .suffix(" EV"),
                );
                if r_exp.changed() {
                    last_changed = true;
                }
                if r_exp.drag_stopped() || r_exp.lost_focus() {
                    last_commit_disk = true;
                }
                ui.label(egui::RichText::new(t(Key::TimelapseTempShort)).small());
                let r_tmp = ui.add(
                    egui::Slider::new(&mut g_last.color.kelvin, 2000.0..=12000.0)
                        .step_by(50.0)
                        .suffix(" K"),
                );
                if r_tmp.changed() {
                    last_changed = true;
                }
                if r_tmp.drag_stopped() || r_tmp.lost_focus() {
                    last_commit_disk = true;
                }
            });
            if last_changed {
                self.library
                    .photo_gradings
                    .insert(last_path.clone(), g_last.clone());
                if self
                    .session
                    .loaded_photo
                    .as_ref()
                    .is_some_and(|p| p.path == last_path)
                {
                    self.session.grading = g_last.clone();
                    self.session.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            if last_commit_disk {
                let r = self
                    .library
                    .photo_ratings
                    .get(&last_path)
                    .copied()
                    .unwrap_or(0);
                save_photo_sidecars(&last_path, &g_last, r);
            }

            if let Some(target) = open_preview_target {
                self.open_raw_file(&target);
            }
        }
    }

    /// Renders the export configuration dialog window.
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.export.show_export_dialog {
            return;
        }

        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let mut is_open = self.export.show_export_dialog;
        let mut trigger_export = false;
        let mut close_dialog = false;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close_dialog = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.egui_wants_keyboard_input() {
            trigger_export = true;
        }

        egui::Window::new(t(Key::ExportWindowTitle))
            .id(egui::Id::NULL.with("export_timelapse_modal_window"))
            .open(&mut is_open)
            .movable(true)
            .collapsible(true)
            .resizable(false)
            .default_width(470.0)
            .default_pos(egui::pos2(780.0, 52.0))
            .show(ctx, |ui| {
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
                            &mut self.export.export_target_mode,
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
                            &mut self.export.export_target_mode,
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
                            &mut self.export.export_target_mode,
                            ExportTargetMode::TimelapseSequence,
                            t(Key::ExportTargetTimelapse),
                        )
                        .clicked()
                    {
                        self.export
                            .set_target_mode(ExportTargetMode::TimelapseSequence);
                        if self.export.export_subfolder == "Export" {
                            self.export.export_subfolder = "Timelapse".to_string();
                        }
                    }
                });

                match self.export.export_target_mode {
                    ExportTargetMode::CurrentPhoto => {
                        if let Some(photo) = &self.session.loaded_photo {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(t(Key::ExportActivePhoto)).strong());
                                if let Some(name) = photo.path.file_name().and_then(|n| n.to_str())
                                {
                                    ui.label(
                                        egui::RichText::new(name)
                                            .color(Color32::from_rgb(100, 180, 255)),
                                    );
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

                ui.separator();
                ui.heading(t(Key::ExportFormatHeading));
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Jpeg,
                        "JPEG",
                    );
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Png,
                        "PNG",
                    );
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Tiff,
                        "TIFF",
                    );
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Avif,
                        "AVIF",
                    );
                });

                ui.add_space(4.0);
                match self.export.export_format {
                    ExportFormatChoice::Jpeg => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportJpegQuality));
                            ui.add(
                                egui::Slider::new(&mut self.export.export_jpeg_quality, 1..=100)
                                    .suffix("%"),
                            );
                        });
                        let quality_desc = if self.export.export_jpeg_quality >= 92 {
                            t(Key::ExportJpegQualityMax)
                        } else if self.export.export_jpeg_quality >= 80 {
                            t(Key::ExportJpegQualityBalanced)
                        } else {
                            t(Key::ExportJpegQualityLow)
                        };
                        ui.label(
                            egui::RichText::new(quality_desc)
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    ExportFormatChoice::Avif => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportJpegQuality));
                            ui.add(
                                egui::Slider::new(&mut self.export.export_jpeg_quality, 1..=100)
                                    .suffix("%"),
                            );
                        });
                        let quality_desc = if self.export.export_jpeg_quality >= 92 {
                            t(Key::ExportJpegQualityMax)
                        } else if self.export.export_jpeg_quality >= 80 {
                            t(Key::ExportJpegQualityBalanced)
                        } else {
                            t(Key::ExportJpegQualityLow)
                        };
                        ui.label(
                            egui::RichText::new(quality_desc)
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    ExportFormatChoice::Png => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportBitDepth));
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits8,
                                t(Key::ExportBitDepth8Standard),
                            );
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits16,
                                t(Key::ExportBitDepth16Lossless),
                            );
                        });
                    }
                    ExportFormatChoice::Tiff => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportBitDepth));
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits8,
                                t(Key::ExportBitDepth8),
                            );
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits16,
                                t(Key::ExportBitDepth16Studio),
                            );
                        });
                    }
                }

                ui.add_space(4.0);
                ui.checkbox(
                    &mut self.export.export_hdr_gain_map,
                    crate::i18n::export_hdr_gain_map_checkbox_label(lang),
                )
                .on_hover_text(crate::i18n::export_hdr_gain_map_checkbox_tooltip(lang));

                ui.separator();
                ui.heading(t(Key::ExportResizeHeading));
                egui::ComboBox::from_id_salt("export_scale_combo")
                    .selected_text(self.export.export_scale_mode.label_lang(lang))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::Original,
                            ExportScaleMode::Original.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::Percent75,
                            ExportScaleMode::Percent75.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::Percent50,
                            ExportScaleMode::Percent50.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::FitLongEdge2048,
                            ExportScaleMode::FitLongEdge2048.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::FitLongEdge3840,
                            ExportScaleMode::FitLongEdge3840.label_lang(lang),
                        );
                    });

                ui.separator();
                ui.heading(t(Key::ExportDestinationHeading));
                ui.horizontal(|ui| {
                    let dest_label = if let Some(dir) = &self.export.export_custom_dir {
                        dir.to_string_lossy().to_string()
                    } else if let Some(photo) = &self.session.loaded_photo {
                        photo
                            .path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| t(Key::ExportSourceFolder).to_string())
                    } else {
                        t(Key::ExportSourceFolder).to_string()
                    };
                    ui.label(egui::RichText::new(dest_label).small().monospace());
                    if ui.button(t(Key::SidebarBrowseButton)).clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.export.export_custom_dir = Some(folder);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut self.export.export_use_subfolder,
                        t(Key::ExportUseSubfolder),
                    );
                    if self.export.export_use_subfolder {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export.export_subfolder)
                                .desired_width(120.0),
                        );
                    }
                });

                if self.export.export_target_mode == ExportTargetMode::TimelapseSequence {
                    ui.horizontal(|ui| {
                        ui.label(t(Key::ExportSequencePrefix));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export.timelapse_sequence_prefix)
                                .desired_width(120.0),
                        );
                        let ext = self.export.export_format.extension();
                        let sample = rraw_export::format_sequence_filename(
                            &self.export.timelapse_sequence_prefix,
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
                            egui::TextEdit::singleline(&mut self.export.export_suffix)
                                .desired_width(120.0),
                        );
                    });
                }

                if self.export.is_exporting {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new(&self.export.export_progress_label).strong());
                    });
                    ui.add(egui::ProgressBar::new(self.export.export_progress).animate(true));
                }

                if let Some((msg, success)) = &self.export.export_status {
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
                            !self.export.is_exporting,
                            egui::Button::new(t(Key::ExportCancel)),
                        )
                        .clicked()
                    {
                        close_dialog = true;
                    }
                    let export_btn_text = match self.export.export_target_mode {
                        ExportTargetMode::CurrentPhoto => t(Key::ExportBtnSingle),
                        ExportTargetMode::BatchSelection => t(Key::ExportBtnBatch),
                        ExportTargetMode::TimelapseSequence => t(Key::ExportBtnTimelapse),
                    };
                    if ui
                        .add_enabled(
                            !self.export.is_exporting,
                            egui::Button::new(egui::RichText::new(export_btn_text).strong())
                                .fill(Color32::from_rgb(30, 110, 210)),
                        )
                        .clicked()
                    {
                        trigger_export = true;
                    }
                });
            });

        if close_dialog {
            is_open = false;
        }
        self.export.set_dialog_open(
            crate::controllers::export::ActiveModalDialog::Export,
            is_open,
        );

        if trigger_export {
            self.export.is_exporting = true;
            self.export.export_progress = 0.05;
            self.export.export_progress_label =
                crate::i18n::tc(lang, "Export.PreparingStatus").to_string();
            self.export.export_status = None;
            // Close the export modal immediately while progress is tracked in the status bar
            self.export
                .close_dialog(crate::controllers::export::ActiveModalDialog::Export);

            if let Err(err) = self.spawn_background_export() {
                self.export.is_exporting = false;
                self.export.export_status =
                    Some((format!("{}{err}", t(Key::ExportFailedPrefix)), false));
                self.status_message = format!("{}{err}", t(Key::ExportFailedPrefix));
            }
        }
    }
}
