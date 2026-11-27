#![forbid(unsafe_code)]

//! Application chrome UI components: top control bar, bottom status bar, and global keyboard shortcuts.

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::{I18nEngine, Key};
use crate::state::*;

impl RrawApp {
    /// Handles global keyboard shortcuts (navigation, undo/redo, recipe clipboard, modal dialogs, ratings).
    pub fn handle_global_keyboard_shortcuts(&mut self, ui: &mut egui::Ui) {
        let wants_keyboard = ui.ctx().egui_wants_keyboard_input();

        if !wants_keyboard {
            self.handle_navigation_shortcuts(ui);
        }

        self.handle_clipboard_and_undo_shortcuts(ui, wants_keyboard);

        // Global Escape key handling:
        // - If a modal/dialog is open (shortcuts guide, export window), Escape always dismisses it, even if a child widget has focus.
        // - If no dialog is open and a text field has focus, Escape is left for egui to unfocus the active input.
        // - If no text field is focused, Escape hierarchically cancels the active tool or batch selection.
        if ui.input(|i| i.key_pressed(egui::Key::Escape))
            && (self.export.show_shortcuts_dialog
                || self.export.show_export_dialog
                || self.export.show_about_dialog
                || !wants_keyboard)
        {
            self.handle_escape_key();
        }

        // Classical keyboard selection shortcuts: Cmd/Ctrl+A, Cmd/Ctrl+D, Space (when no text field focused)
        if !wants_keyboard {
            self.handle_file_and_tab_shortcuts(ui);
            self.handle_selection_and_compare_shortcuts(ui);
            self.handle_viewport_and_rating_shortcuts(ui);
        }
    }

    fn handle_navigation_shortcuts(&mut self, ui: &mut egui::Ui) {
        let nav_prev =
            ui.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp));
        let nav_next = ui
            .input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageDown));
        let nav_home = ui.input(|i| i.key_pressed(egui::Key::Home));
        let nav_end = ui.input(|i| i.key_pressed(egui::Key::End));

        if nav_prev {
            self.navigate_relative(-1);
        } else if nav_next {
            self.navigate_relative(1);
        } else if nav_home {
            self.navigate_to_filtered_index(0, false);
        } else if nav_end {
            self.navigate_to_filtered_index(usize::MAX, false);
        }
    }

    fn handle_clipboard_and_undo_shortcuts(&mut self, ui: &mut egui::Ui, wants_keyboard: bool) {
        // Batch Recipe Clipboard & Undo/Redo shortcuts (when no text field has keyboard focus):
        // - Cmd/Ctrl + Z: Undo
        // - Cmd/Ctrl + Y (or Cmd/Ctrl + Shift + Z): Redo
        // - Cmd/Ctrl + C (or Cmd/Ctrl + Shift + C / egui::Event::Copy): Copy recipe
        // - Cmd/Ctrl + V (or Cmd/Ctrl + Shift + V / egui::Event::Paste): Paste recipe
        let (undo_cmd, redo_cmd, copy_recipe, paste_recipe) = ui.input(|i| {
            let cmd = i.modifiers.command || i.modifiers.ctrl;
            let has_copy_event = i.events.iter().any(|ev| matches!(ev, egui::Event::Copy));
            let has_paste_event = i
                .events
                .iter()
                .any(|ev| matches!(ev, egui::Event::Paste(_)));
            (
                !wants_keyboard && cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                !wants_keyboard
                    && cmd
                    && (i.key_pressed(egui::Key::Y)
                        || (i.modifiers.shift && i.key_pressed(egui::Key::Z))),
                !wants_keyboard && ((cmd && i.key_pressed(egui::Key::C)) || has_copy_event),
                !wants_keyboard && ((cmd && i.key_pressed(egui::Key::V)) || has_paste_event),
            )
        });
        if undo_cmd {
            if self.undo_grading() {
                ui.ctx().request_repaint();
            }
        } else if redo_cmd {
            if self.redo_grading() {
                ui.ctx().request_repaint();
            }
        } else if copy_recipe {
            self.copy_active_recipe();
        } else if paste_recipe {
            let ctx = ui.ctx().clone();
            self.paste_recipe_to_selection(&ctx);
        }
    }

    fn handle_file_and_tab_shortcuts(&mut self, ui: &mut egui::Ui) {
        // Top-level file & dialog shortcuts (Cmd/Ctrl + O / Shift+O / E / 1 / 2 / 3)
        let (cmd_open_file, cmd_open_folder, cmd_export, switch_tab_1, switch_tab_2, switch_tab_3) =
            ui.input(|i| {
                let cmd = i.modifiers.command || i.modifiers.ctrl;
                let mut tab1 = false;
                let mut tab2 = false;
                let mut tab3 = false;

                if cmd {
                    if i.key_pressed(egui::Key::Num1) {
                        tab1 = true;
                    }
                    if i.key_pressed(egui::Key::Num2) {
                        tab2 = true;
                    }
                    if i.key_pressed(egui::Key::Num3) {
                        tab3 = true;
                    }

                    // Inspect events to support both physical keys (AZERTY Mac) and logical keys (QWERTY)
                    for event in &i.events {
                        if let egui::Event::Key {
                            key,
                            physical_key,
                            pressed: true,
                            modifiers,
                            ..
                        } = event
                        {
                            if modifiers.command || modifiers.ctrl {
                                if *key == egui::Key::Num1 || *physical_key == Some(egui::Key::Num1)
                                {
                                    tab1 = true;
                                }
                                if *key == egui::Key::Num2 || *physical_key == Some(egui::Key::Num2)
                                {
                                    tab2 = true;
                                }
                                if *key == egui::Key::Num3 || *physical_key == Some(egui::Key::Num3)
                                {
                                    tab3 = true;
                                }
                            }
                        }
                    }
                }

                (
                    cmd && !i.modifiers.shift && i.key_pressed(egui::Key::O),
                    cmd && i.modifiers.shift && i.key_pressed(egui::Key::O),
                    cmd && i.key_pressed(egui::Key::E),
                    tab1,
                    tab2,
                    tab3,
                )
            });

        if cmd_open_file {
            self.open_image_dialog();
        } else if cmd_open_folder {
            self.open_folder_dialog();
        } else if cmd_export {
            self.export
                .open_dialog(crate::controllers::export::ActiveModalDialog::Export);
            self.export.stats.status = None;
        } else if switch_tab_1 {
            self.set_left_panel_tab(LeftPanelTab::Explorer);
            self.layout_config.save();
            ui.ctx().request_repaint();
        } else if switch_tab_2 {
            self.set_left_panel_tab(LeftPanelTab::Presets);
            self.layout_config.save();
            ui.ctx().request_repaint();
        } else if switch_tab_3 {
            self.set_left_panel_tab(LeftPanelTab::Exif);
            self.layout_config.save();
            ui.ctx().request_repaint();
        }
    }

    fn handle_selection_and_compare_shortcuts(&mut self, ui: &mut egui::Ui) {
        ui.input(|i| {
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::A) {
                for entry in &self.library.directory_entries {
                    if self.matches_active_filter(entry) {
                        self.library.selected_photos.insert(entry.clone());
                    }
                }
                let sel_count = self
                    .library
                    .selected_photos
                    .iter()
                    .filter(|p| self.matches_active_filter(p))
                    .count();
                self.status_message =
                    crate::i18n::batch_selection_count_message(self.lang(), sel_count);
            }
            if i.key_pressed(egui::Key::Backslash) {
                self.session.is_comparing_original = !self.session.is_comparing_original;
            }
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::D) {
                let prev = self.library.selected_photos.len();
                self.library.selected_photos.clear();
                self.session.grading.masks.active_brush_mask_idx = None;
                self.status_message = crate::i18n::batch_deselection_message(self.lang(), prev);
            }
            if i.key_pressed(egui::Key::Space) {
                // In Darkroom with brush active, Space is reserved for panning zoomed image
                if self.session.grading.masks.active_brush_mask_idx.is_none() {
                    if let Some(photo) = &self.session.loaded_photo {
                        if self.library.selected_photos.contains(&photo.path) {
                            self.library.selected_photos.remove(&photo.path);
                            self.status_message = crate::i18n::batch_toggle_photo_message(
                                self.lang(),
                                false,
                                self.library.selected_photos.len(),
                            );
                        } else {
                            self.library.selected_photos.insert(photo.path.clone());
                            self.status_message = crate::i18n::batch_toggle_photo_message(
                                self.lang(),
                                true,
                                self.library.selected_photos.len(),
                            );
                        }
                    }
                }
            }
        });
    }

    fn handle_viewport_and_rating_shortcuts(&mut self, ui: &mut egui::Ui) {
        // Viewport display shortcuts (ignored while a text field has keyboard focus)
        let (toggle_filmstrip, toggle_hud, toggle_split, toggle_clipping, toggle_shortcuts) = ui
            .input(|i| {
                (
                    i.key_pressed(egui::Key::F9),
                    i.key_pressed(egui::Key::I) && !i.modifiers.any(),
                    i.key_pressed(egui::Key::Y) && !i.modifiers.any(),
                    i.key_pressed(egui::Key::J) && !i.modifiers.any(),
                    i.key_pressed(egui::Key::F1),
                )
            });
        if toggle_filmstrip {
            self.layout_config.filmstrip_visible = !self.layout_config.filmstrip_visible;
            self.layout_config.save();
        }
        if toggle_hud {
            self.layout_config.show_canvas_hud = !self.layout_config.show_canvas_hud;
            self.layout_config.save();
        }
        if toggle_split {
            self.viewport.split_compare = !self.viewport.split_compare;
        }
        if toggle_clipping {
            self.toggle_clipping_overlay();
        }
        if toggle_shortcuts {
            self.export
                .toggle_dialog(crate::controllers::export::ActiveModalDialog::Shortcuts);
        }

        let has_cmd_ctrl_alt =
            ui.input(|i| i.modifiers.command || i.modifiers.ctrl || i.modifiers.alt);
        if !has_cmd_ctrl_alt {
            for digit in 0..=5 {
                let key = match digit {
                    0 => egui::Key::Num0,
                    1 => egui::Key::Num1,
                    2 => egui::Key::Num2,
                    3 => egui::Key::Num3,
                    4 => egui::Key::Num4,
                    5 => egui::Key::Num5,
                    _ => unreachable!(),
                };
                if ui.input(|i| i.key_pressed(key)) {
                    self.set_current_photo_rating(digit);
                }
            }
        }
    }

    fn open_image_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Supported Images",
                crate::preview::SUPPORTED_IMAGE_EXTENSIONS,
            )
            .pick_file()
        {
            self.open_raw_file(&path);
        }
    }

    fn open_folder_dialog(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.open_directory(&folder);
            if let Some(first_file) = self
                .first_filtered_entry()
                .or_else(|| self.library.directory_entries.first().cloned())
            {
                self.open_raw_file(&first_file);
            }
        }
    }

    /// Renders the top control and navigation bar.
    pub fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        let headroom = self.current_headroom();
        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(22, 24, 28)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.render_top_bar_file_and_history_controls(ui, headroom);
                    self.render_top_bar_zoom_controls(ui);
                    self.render_top_bar_soft_proof_controls(ui);
                });
            });
    }

    fn render_top_bar_file_and_history_controls(&mut self, ui: &mut egui::Ui, headroom: f32) {
        if ui.button(self.t(Key::TopBarOpenImage)).clicked() {
            self.open_image_dialog();
        }

        if ui.button(self.t(Key::TopBarOpenFolder)).clicked() {
            self.open_folder_dialog();
        }

        if ui.button(self.t(Key::TopBarExport)).clicked() {
            self.export
                .open_dialog(crate::controllers::export::ActiveModalDialog::Export);
            self.export.stats.status = None;
        }

        if ui
            .button(self.t(Key::TopBarTimelapse))
            .on_hover_text(self.t(Key::TopBarTimelapseTooltip))
            .clicked()
        {
            self.export
                .set_target_mode(ExportTargetMode::TimelapseSequence);
            self.export.config.subfolder = "Timelapse".to_string();
            self.export
                .open_dialog(crate::controllers::export::ActiveModalDialog::Export);
            self.export.stats.status = None;
        }

        ui.separator();

        let can_undo = self.session.can_undo();
        let can_redo = self.session.can_redo();
        let undo_tip = crate::i18n::tc(self.lang(), "TopBar.UndoTooltip");
        let redo_tip = crate::i18n::tc(self.lang(), "TopBar.RedoTooltip");
        if ui
            .add_enabled(can_undo, egui::Button::new("↶"))
            .on_hover_text(undo_tip)
            .clicked()
            && self.undo_grading()
        {
            ui.ctx().request_repaint();
        }
        if ui
            .add_enabled(can_redo, egui::Button::new("↷"))
            .on_hover_text(redo_tip)
            .clicked()
            && self.redo_grading()
        {
            ui.ctx().request_repaint();
        }

        ui.separator();

        // Live EDR Headroom Badge (compact; details on hover)
        let (headroom_text, headroom_col, headroom_tip) = if headroom > 1.05 {
            (
                format!("{} {headroom:.1}x", self.t(Key::TopBarEdrHeadroom)),
                egui::Color32::from_rgb(255, 200, 50),
                self.t(Key::TopBarEdrTooltip),
            )
        } else {
            (
                format!("{} {headroom:.1}x", self.t(Key::TopBarSdrHeadroom)),
                egui::Color32::LIGHT_GRAY,
                self.t(Key::TopBarSdrTooltip),
            )
        };
        ui.label(egui::RichText::new(headroom_text).color(headroom_col))
            .on_hover_text(headroom_tip);

        ui.separator();
        self.render_view_toggles(ui);

        ui.separator();
    }

    fn render_top_bar_zoom_controls(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(self.t(Key::TopBarZoom)).size(12.0));
        if ui
            .selectable_label(self.viewport.zoom_level <= 0.0, self.t(Key::TopBarZoomFit))
            .clicked()
        {
            self.viewport.zoom_level = 0.0;
            self.viewport.pan_offset = egui::Vec2::ZERO;
        }
        if ui
            .selectable_label(
                (self.viewport.zoom_level - 1.0).abs() < 0.01,
                self.t(Key::TopBarZoom11),
            )
            .clicked()
        {
            self.viewport.zoom_level = 1.0;
            self.viewport.pan_offset = egui::Vec2::ZERO;
        }
        let fit_s = self.viewport.last_fit_scale.max(0.001);
        if ui.small_button("➖").clicked() {
            let cur_s = if self.viewport.zoom_level <= 0.0 {
                fit_s
            } else {
                self.viewport.zoom_level
            };
            let new_s = (cur_s / 1.25).max(fit_s);
            if (new_s - fit_s).abs() / fit_s < 0.015 || new_s <= fit_s {
                self.viewport.zoom_level = 0.0;
                self.viewport.pan_offset = egui::Vec2::ZERO;
            } else {
                self.viewport.zoom_level = new_s;
            }
        }
        let zoom_pct_str = if self.viewport.zoom_level <= 0.0 {
            self.t(Key::TopBarZoomFitted).to_string()
        } else {
            format!(
                "{}%",
                ((self.viewport.zoom_level / fit_s) * 100.0).round() as u32
            )
        };
        ui.label(egui::RichText::new(zoom_pct_str).monospace().strong());
        if ui.small_button("➕").clicked() {
            let cur_s = if self.viewport.zoom_level <= 0.0 {
                fit_s
            } else {
                self.viewport.zoom_level
            };
            let new_s = (cur_s * 1.25).min(fit_s * 20.0);
            self.viewport.zoom_level = new_s;
        }
        if ui
            .button(self.t(Key::TopBarCenter))
            .on_hover_text(self.t(Key::TopBarCenterTooltip))
            .clicked()
        {
            self.viewport.pan_offset = egui::Vec2::ZERO;
        }

        ui.separator();
    }

    fn render_top_bar_soft_proof_controls(&mut self, ui: &mut egui::Ui) {
        let sp_active = self.session.grading.soft_proof.enabled;
        let sp_btn_label = if sp_active {
            egui::RichText::new(self.t(Key::TopBarSoftProofActive))
                .strong()
                .color(Color32::from_rgb(120, 230, 165))
        } else {
            egui::RichText::new(self.t(Key::TopBarSoftProofInactive))
        };
        if ui
            .selectable_label(sp_active, sp_btn_label)
            .on_hover_text(self.t(Key::TopBarSoftProofTooltip))
            .clicked()
        {
            self.record_undo_snapshot();
            self.session.grading.soft_proof.enabled = !self.session.grading.soft_proof.enabled;
            if !self.session.grading.soft_proof.enabled {
                self.session.grading.soft_proof.show_gamut_warning = false;
            }
            self.session.needs_preview_update = true;
            self.save_current_photo_sidecar();
            ui.ctx().request_repaint();
        }

        if sp_active {
            let cur_lang = self.lang();
            let prev_sp = self.session.grading.soft_proof;
            egui::ComboBox::from_id_salt("top_bar_soft_proof_profile")
                .selected_text(crate::i18n::soft_proof_profile_short_label_lang(
                    self.session.grading.soft_proof.profile,
                    cur_lang,
                ))
                .width(120.0)
                .show_ui(ui, |ui| {
                    for prof in rraw_color::SoftProofProfile::ALL {
                        if ui
                            .selectable_value(
                                &mut self.session.grading.soft_proof.profile,
                                prof,
                                crate::i18n::soft_proof_profile_label_lang(prof, cur_lang),
                            )
                            .changed()
                        {
                            self.session.grading.soft_proof.simulate_paper_and_ink =
                                prof.is_print_medium();
                        }
                    }
                });

            egui::ComboBox::from_id_salt("top_bar_soft_proof_intent")
                .selected_text(crate::i18n::soft_proof_intent_short_label_lang(
                    self.session.grading.soft_proof.intent,
                    cur_lang,
                ))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for intent in rraw_color::SoftProofRenderingIntent::ALL {
                        ui.selectable_value(
                            &mut self.session.grading.soft_proof.intent,
                            intent,
                            crate::i18n::soft_proof_intent_label_lang(intent, cur_lang),
                        );
                    }
                });

            let paper_ink_label = self.t(Key::TopBarPaperInk);
            let paper_ink_tip = self.t(Key::TopBarPaperInkTooltip);
            ui.checkbox(
                &mut self.session.grading.soft_proof.simulate_paper_and_ink,
                paper_ink_label,
            )
            .on_hover_text(paper_ink_tip);

            let oog_active = self.session.grading.soft_proof.show_gamut_warning;
            let oog_count = self.viewport.histogram.out_of_gamut_count;
            let oog_btn_label = if oog_active {
                if oog_count > 0 {
                    egui::RichText::new(crate::i18n::soft_proof_out_of_gamut_badge(
                        cur_lang, oog_count,
                    ))
                    .strong()
                    .color(Color32::from_rgb(255, 110, 230))
                } else {
                    egui::RichText::new(self.t(Key::TopBarGamutOk))
                        .strong()
                        .color(Color32::from_rgb(120, 230, 165))
                }
            } else {
                egui::RichText::new(self.t(Key::TopBarGamutAlert))
            };
            if ui
                .selectable_label(oog_active, oog_btn_label)
                .on_hover_text(self.t(Key::TopBarGamutTooltip))
                .clicked()
            {
                self.session.grading.soft_proof.show_gamut_warning =
                    !self.session.grading.soft_proof.show_gamut_warning;
            }

            if self.session.grading.soft_proof != prev_sp {
                let mut pre_edit = self.session.grading.clone();
                pre_edit.soft_proof = prev_sp;
                self.session.push_pre_edit_snapshot(pre_edit);
                self.session.needs_preview_update = true;
                self.save_current_photo_sidecar();
                ui.ctx().request_repaint();
            }
        }
    }

    /// Renders the bottom application status bar.
    pub fn render_status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(18, 19, 22)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.export.stats.is_exporting {
                        ui.spinner();
                        ui.label(egui::RichText::new(&self.export.stats.progress_label).strong());
                        ui.add(
                            egui::ProgressBar::new(self.export.stats.progress)
                                .desired_width(120.0)
                                .animate(true),
                        );
                        if ui
                            .small_button(crate::i18n::tc(self.lang(), "Export.CancelButton"))
                            .clicked()
                        {
                            self.cancel_active_export();
                        }
                        ui.label(egui::RichText::new("·").weak());
                    } else if self.workers.is_decoding {
                        ui.spinner();
                    }
                    ui.label(&self.status_message);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .link(egui::RichText::new(rraw_core::build_info::VERSION_TAG).weak())
                            .on_hover_text(rraw_core::build_info::BuildInfo::about_text())
                            .clicked()
                        {
                            self.export
                                .open_dialog(crate::controllers::export::ActiveModalDialog::About);
                        }
                        ui.label(egui::RichText::new("·").weak());
                        let (backend_label, backend_color) = match self.active_preview_backend {
                            crate::preview::PreviewBackendKind::GpuWgpu => {
                                ("⚡ GPU", Color32::from_rgb(120, 230, 165))
                            }
                            crate::preview::PreviewBackendKind::CpuRayon => {
                                ("⚙ CPU", Color32::from_rgb(165, 170, 180))
                            }
                        };
                        let adapter_hint = self
                            .gpu_engine
                            .adapter_name()
                            .unwrap_or("CPU Rayon fallback");
                        let backend_tooltip = if self.layout_config.gpu_preview_enabled {
                            if let Some(reason) = self.gpu_engine.last_fallback_reason() {
                                format!(
                                    "Preview backend: {backend_label} (fallback: {reason}) — click to force CPU"
                                )
                            } else {
                                format!(
                                    "Preview backend: {backend_label} ({adapter_hint}) — click to force CPU"
                                )
                            }
                        } else {
                            "Preview backend: CPU forced — click to enable WGPU acceleration"
                                .to_string()
                        };
                        if ui
                            .selectable_label(
                                self.layout_config.gpu_preview_enabled,
                                egui::RichText::new(backend_label)
                                    .small()
                                    .strong()
                                    .color(backend_color),
                            )
                            .on_hover_text(backend_tooltip)
                            .clicked()
                        {
                            self.layout_config.gpu_preview_enabled =
                                !self.layout_config.gpu_preview_enabled;
                            self.layout_config.save();
                            self.session.needs_preview_update = true;
                            ui.ctx().request_repaint();
                        }
                        ui.label(egui::RichText::new("·").weak());
                        ui.label(self.t(Key::StatusSurfaceInfo));
                        ui.label(egui::RichText::new("·").weak());

                        // Dynamic language selector from all registered catalogs
                        let cur_lang = self.layout_config.language;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                            let available = I18nEngine::global().available_languages();
                            for (i, &lang) in available.iter().enumerate() {
                                if i > 0 {
                                    ui.label(egui::RichText::new("|").weak());
                                }
                                if ui
                                    .selectable_label(cur_lang == lang, lang.code())
                                    .on_hover_text(lang.name())
                                    .clicked()
                                    && cur_lang != lang
                                {
                                    self.layout_config.language = lang;
                                    self.layout_config.save();
                                }
                            }
                        });
                    });
                });
            });
    }
}
