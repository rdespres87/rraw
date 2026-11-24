#![forbid(unsafe_code)]

//! Right-hand darkroom development inspector panel and collapsible sections.

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::state::*;
use crate::ui::sections;

impl RrawApp {
    /// Returns true if a darkroom section contains non-default adjustment values.
    #[must_use]
    pub fn is_section_active(&self, section: DarkroomSection) -> bool {
        match section {
            DarkroomSection::ExposureTone => {
                self.session.grading.tone.exposure_ev != 0.0
                    || self.session.grading.tone.contrast != 0.0
                    || self.session.grading.tone.highlights != 0.0
                    || self.session.grading.tone.shadows != 0.0
                    || self.session.grading.tone.blacks != 0.0
                    || self.session.grading.tone.saturation != 0.0
                    || self.session.grading.tone.vibrance != 0.0
                    || self.session.grading.smart_lighting.mode
                        != rraw_core::SmartLightingMode::Slight
                    || (self.session.grading.smart_lighting.intensity - 1.0).abs() > 0.01
                    || self.session.grading.smart_lighting.custom_shadow_lift != 0.0
                    || self
                        .session
                        .grading
                        .smart_lighting
                        .custom_highlight_compression
                        != 0.0
            }
            DarkroomSection::ToneCurves => !self.session.grading.tone_curves.is_identity(),
            DarkroomSection::WhiteBalanceColor => {
                (self.session.grading.color.kelvin - 5500.0).abs() > 1.0
                    || self.session.grading.color.tint != 0.0
                    || self.session.grading.custom_clut.is_some()
                    || self.session.grading.color.is_monochrome
            }
            DarkroomSection::SoftProofing => self.session.grading.soft_proof.enabled,
            DarkroomSection::DetailDenoise => {
                (self.session.grading.denoise_sharpen.wavelet_denoise_sigma - 1.0).abs() > 0.01
                    || (self.session.grading.denoise_sharpen.color_denoise - 25.0).abs() > 0.01
                    || self.session.grading.denoise_sharpen.luma_denoise > 0.0
                    || (self
                        .session
                        .grading
                        .denoise_sharpen
                        .capture_sharpening_amount
                        - 0.5)
                        .abs()
                        > 0.01
            }
            DarkroomSection::Crop => self.session.grading.geometry.crop_enabled,
            DarkroomSection::Straighten => {
                self.session.grading.geometry.rotation_degrees != 0.0
                    || self.session.grading.geometry.flip_horizontal
                    || self.session.grading.geometry.flip_vertical
            }
            DarkroomSection::Geometry => {
                self.session.grading.geometry.keystone_vertical != 0.0
                    || self.session.grading.geometry.keystone_horizontal != 0.0
            }
            DarkroomSection::OpticalCorrections => {
                self.session.grading.optics.optical_tab
                    != crate::state::OpticalCorrectionTab::Profile
                    || !self.session.grading.optics.lens_profile_auto
                    || (self.session.grading.optics.lens_distortion_strength - 100.0).abs() > 0.01
                    || (self.session.grading.optics.lens_vignette_strength - 100.0).abs() > 0.01
                    || (self.session.grading.optics.lens_ca_strength - 100.0).abs() > 0.01
                    || self.session.grading.optics.distortion_amount != 0.0
                    || self.session.grading.optics.defringe_purple_amount != 0.0
                    || self.session.grading.optics.defringe_green_amount != 0.0
                    || (self.session.grading.optics.defringe_threshold - 0.08).abs() > 0.005
                    || self.session.grading.optics.defringe_preview_mask
                    || (self.session.grading.optics.vignette_amount - 50.0).abs() > 0.01
            }
            DarkroomSection::Effects => self.session.grading.effects.is_active(),
            DarkroomSection::AiSemanticMasks => !self.session.grading.masks.is_empty(),
            DarkroomSection::Timelapse => false,
            DarkroomSection::LegacyOther => false,
        }
    }

    /// Resets all parameters of a specific darkroom section back to standard defaults.
    pub fn reset_section(&mut self, section: DarkroomSection) {
        if section != DarkroomSection::AiSemanticMasks {
            self.session.grading.masks.active_brush_mask_idx = None;
        }
        self.record_undo_snapshot();
        match section {
            DarkroomSection::ExposureTone => {
                self.session.grading.tone.exposure_ev = 0.0;
                self.session.grading.tone.contrast = 0.0;
                self.session.grading.tone.highlights = 0.0;
                self.session.grading.tone.shadows = 0.0;
                self.session.grading.tone.blacks = 0.0;
                self.session.grading.tone.saturation = 0.0;
                self.session.grading.tone.vibrance = 0.0;
                self.session.grading.smart_lighting =
                    DarkroomGradingState::default().smart_lighting;
            }
            DarkroomSection::ToneCurves => {
                self.session.grading.tone_curves = rraw_core::curves::ToneCurvesRecipe::default();
            }
            DarkroomSection::WhiteBalanceColor => {
                self.session.grading.color.kelvin = 5500.0;
                self.session.grading.color.tint = 0.0;
                self.session.grading.custom_clut = None;
                self.session.grading.color.is_monochrome = false;
                self.session.grading.color.display_transform = rraw_color::DisplayTransform::AgX;
            }
            DarkroomSection::SoftProofing => {
                self.session.grading.soft_proof.enabled = false;
                self.session.grading.soft_proof.show_gamut_warning = false;
            }
            DarkroomSection::DetailDenoise => {
                self.session.grading.denoise_sharpen.wavelet_denoise_sigma = 1.0;
                self.session.grading.denoise_sharpen.color_denoise = 25.0;
                self.session.grading.denoise_sharpen.luma_denoise = 0.0;
                self.session
                    .grading
                    .denoise_sharpen
                    .capture_sharpening_amount = 0.5;
            }
            DarkroomSection::Crop => {
                self.session.grading.geometry.crop_enabled = false;
                self.session.grading.geometry.crop_left = 0.0;
                self.session.grading.geometry.crop_top = 0.0;
                self.session.grading.geometry.crop_right = 1.0;
                self.session.grading.geometry.crop_bottom = 1.0;
            }
            DarkroomSection::Straighten => {
                self.session.grading.geometry.rotation_degrees = 0.0;
                self.session.grading.geometry.flip_horizontal = false;
                self.session.grading.geometry.flip_vertical = false;
            }
            DarkroomSection::Geometry => {
                self.session.grading.geometry.geometry_enabled = true;
                self.session.grading.geometry.keystone_vertical = 0.0;
                self.session.grading.geometry.keystone_horizontal = 0.0;
                self.session.grading.geometry.auto_crop_geometry = true;
            }
            DarkroomSection::OpticalCorrections => {
                self.session.grading.optics.optical_tab =
                    crate::state::OpticalCorrectionTab::Profile;
                self.session.grading.optics.lens_profile_auto = true;
                self.session.grading.optics.lens_distortion_strength = 100.0;
                self.session.grading.optics.lens_vignette_strength = 100.0;
                self.session.grading.optics.lens_ca_strength = 100.0;
                self.session.grading.optics.distortion_correction = true;
                self.session.grading.optics.distortion_amount = 0.0;
                self.session.grading.optics.chromatic_aberration_correction = true;
                self.session.grading.optics.defringe_purple_amount = 0.0;
                self.session.grading.optics.defringe_green_amount = 0.0;
                self.session.grading.optics.defringe_threshold = 0.08;
                self.session.grading.optics.defringe_preview_mask = false;
                self.session.grading.optics.vignette_correction = true;
                self.session.grading.optics.vignette_amount = 50.0;
            }
            DarkroomSection::Effects => {
                self.session.grading.effects = EffectsGradingParams::default();
            }
            DarkroomSection::AiSemanticMasks => {
                self.session.grading.masks.semantic_masks.clear();
                self.session.grading.masks.local_masks.clear();
                self.session.grading.masks.active_brush_mask_idx = None;
            }
            DarkroomSection::Timelapse | DarkroomSection::LegacyOther => {}
        }

        self.session.grading.preset = GradingPreset::Custom;
        self.session.current_preset = GradingPreset::Custom;
        self.session.needs_preview_update = true;
        self.save_current_photo_sidecar();
    }

    /// Renders the complete right-hand darkroom development panel with family filters, search, and active-only toggle.
    pub fn render_darkroom_panel(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let order = self.layout_config.section_order.clone();

        egui::Panel::right("darkroom_panel_fixed")
            .resizable(false)
            .exact_size(360.0)
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(24, 26, 31)),
            )
            .show(ui, |ui| {
                ui.add_space(4.0);

                // 1. Header with panel title and layout reset
                ui.horizontal(|ui| {
                    ui.heading(t(Key::DarkroomTitle));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button(t(Key::DarkroomResetLayout))
                            .on_hover_text(t(Key::DarkroomResetLayoutTooltip))
                            .clicked()
                        {
                            self.reset_layout();
                        }
                    });
                });

                ui.add_space(4.0);

                // 2. Family Category Filter Bar (2 balanced rows of 3 buttons with equal width)
                let btn_w = ((ui.available_width() - 8.0) / 3.0).floor();
                let row1 = [ToolFamily::All, ToolFamily::Light, ToolFamily::Color];
                let row2 = [
                    ToolFamily::Detail,
                    ToolFamily::Geometry,
                    ToolFamily::LocalMasks,
                ];

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                    for &family in &row1 {
                        let is_sel = self.active_tool_family == family;
                        let text = egui::RichText::new(family.label_lang(lang)).size(11.0);
                        if ui
                            .add_sized([btn_w, 22.0], egui::Button::new(text).selected(is_sel))
                            .clicked()
                        {
                            self.set_active_tool_family(family);
                            self.layout_config.save();
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                    for &family in &row2 {
                        let is_sel = self.active_tool_family == family;
                        let text = egui::RichText::new(family.label_lang(lang)).size(11.0);
                        if ui
                            .add_sized([btn_w, 22.0], egui::Button::new(text).selected(is_sel))
                            .clicked()
                        {
                            self.set_active_tool_family(family);
                            self.layout_config.save();
                        }
                    }
                });

                ui.add_space(3.0);

                // 3. Active-only toggle full width
                let active_label = if self.active_tools_only {
                    egui::RichText::new(t(Key::DarkroomActiveOnlyActive))
                        .color(egui::Color32::from_rgb(100, 190, 255))
                        .size(11.5)
                } else {
                    egui::RichText::new(t(Key::DarkroomActiveOnly)).size(11.5)
                };
                if ui
                    .add_sized(
                        [ui.available_width(), 22.0],
                        egui::Button::new(active_label).selected(self.active_tools_only),
                    )
                    .on_hover_text(t(Key::DarkroomActiveOnlyTooltip))
                    .clicked()
                {
                    self.set_active_tools_only(!self.active_tools_only);
                    self.layout_config.save();
                }

                ui.separator();

                // 4. Scrollable Sections List with Dynamic Filtering
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut rendered_count = 0;

                        for (idx, section) in order.iter().enumerate() {
                            // Filter by ToolFamily
                            if self.active_tool_family != ToolFamily::All
                                && section.family() != self.active_tool_family
                            {
                                continue;
                            }

                            // Filter by Active Only
                            if self.active_tools_only && !self.is_section_active(*section) {
                                continue;
                            }

                            self.render_darkroom_section(ui, *section, idx);
                            rendered_count += 1;
                        }

                        if rendered_count == 0 {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new(t(Key::InspectorNoMatchingTools))
                                        .color(egui::Color32::GRAY)
                                        .italics(),
                                );
                                if self.active_tools_only
                                    && ui.button(t(Key::InspectorShowAllTools)).clicked()
                                {
                                    self.set_active_tools_only(false);
                                    self.layout_config.save();
                                }
                            });
                        }
                    });
            });
    }

    /// Moves a darkroom section up or down in the persistent layout order.
    pub fn move_section(&mut self, idx: usize, up: bool) {
        if up && idx > 0 {
            self.layout_config.section_order.swap(idx, idx - 1);
            self.layout_config.save();
        } else if !up && idx + 1 < self.layout_config.section_order.len() {
            self.layout_config.section_order.swap(idx, idx + 1);
            self.layout_config.save();
        }
    }

    /// Renders a single collapsible section in the darkroom inspector.
    pub fn render_darkroom_section(
        &mut self,
        ui: &mut egui::Ui,
        section: DarkroomSection,
        idx: usize,
    ) {
        let is_first = idx == 0;
        let is_last = idx + 1 == self.layout_config.section_order.len();
        let is_active = self.is_section_active(section);

        let cur_lang = self.lang();
        let t = |key: Key| key.translate(cur_lang);
        let header_title = egui::RichText::new(section.title_lang(cur_lang)).strong();

        let is_specific_family = self.active_tool_family != ToolFamily::All;
        let default_open = is_specific_family
            || matches!(
                section,
                DarkroomSection::ExposureTone
                    | DarkroomSection::ToneCurves
                    | DarkroomSection::WhiteBalanceColor
            );

        egui::Frame::group(ui.style())
            .fill(egui::Color32::from_rgb(20, 22, 27))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                let id = ui.make_persistent_id(format!("sec_{:?}", section));
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    default_open,
                );
                if is_specific_family {
                    state.set_open(true);
                }
                let is_section_open = state.is_open();
                if section == DarkroomSection::AiSemanticMasks && !is_section_open {
                    self.session.grading.masks.active_brush_mask_idx = None;
                }

                state
                    .show_header(ui, |ui| {
                        if is_active {
                            ui.label(
                                egui::RichText::new("●")
                                    .color(egui::Color32::from_rgb(100, 190, 255))
                                    .size(9.0),
                            )
                            .on_hover_text(crate::i18n::tc(
                                cur_lang,
                                "Inspector.ActiveModuleTooltip",
                            ));
                        }
                        ui.label(header_title);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !is_last
                                && ui
                                    .small_button("▼")
                                    .on_hover_text(t(Key::InspectorMoveDown))
                                    .clicked()
                            {
                                self.move_section(idx, false);
                            }
                            if !is_first
                                && ui
                                    .small_button("▲")
                                    .on_hover_text(t(Key::InspectorMoveUp))
                                    .clicked()
                            {
                                self.move_section(idx, true);
                            }
                            if ui
                                .small_button("↺")
                                .on_hover_text(t(Key::ResetModuleTooltip))
                                .clicked()
                            {
                                self.reset_section(section);
                            }
                        });
                    })
                    .body_unindented(|ui| {
                        ui.add_space(2.0);
                        let pre_edit_grading = self.session.grading.clone();
                        let pointer_down = ui.ctx().input(|i| i.pointer.any_down());

                        let out = match section {
                            DarkroomSection::ExposureTone => sections::exposure_tone::render(
                                ui,
                                &mut self.session.grading.tone,
                                &mut self.session.grading.smart_lighting,
                                cur_lang,
                            ),
                            DarkroomSection::WhiteBalanceColor => sections::white_balance::render(
                                ui,
                                &mut self.session.grading.color,
                                &mut self.session.grading.tone,
                                cur_lang,
                            ),
                            DarkroomSection::AiSemanticMasks => {
                                let prev_crop = self.viewport.is_crop_editing;
                                let prev_horizon = self.viewport.horizon_tool_active;
                                let prev_phase = self.viewport.parallel_tool_phase;
                                let prev_brush = self.session.grading.masks.active_brush_mask_idx;
                                let mut mask_ctx = sections::masks::MaskUiContext {
                                    local_masks: &mut self.session.grading.masks.local_masks,
                                    show_local_mask_overlay: &mut self
                                        .session
                                        .grading
                                        .masks
                                        .show_local_mask_overlay,
                                    active_brush_mask_idx: &mut self
                                        .session
                                        .grading
                                        .masks
                                        .active_brush_mask_idx,
                                    semantic_masks: &mut self.session.grading.masks.semantic_masks,
                                    show_semantic_mask_overlay: &mut self
                                        .session
                                        .grading
                                        .masks
                                        .show_semantic_mask_overlay,
                                    language: cur_lang,
                                };
                                let out = sections::masks::render(ui, &mut mask_ctx);
                                self.reconcile_canvas_tool_transitions(
                                    prev_crop,
                                    prev_horizon,
                                    prev_phase,
                                    prev_brush,
                                );
                                out
                            }
                            DarkroomSection::SoftProofing => sections::soft_proofing::render(
                                ui,
                                &mut self.session.grading.soft_proof,
                                self.viewport.histogram.out_of_gamut_count,
                                cur_lang,
                            ),
                            DarkroomSection::ToneCurves => {
                                self.render_tone_curves_section(ui);
                                sections::SectionOutput::nothing()
                            }
                            DarkroomSection::Crop => {
                                let prev_crop = self.viewport.is_crop_editing;
                                let prev_horizon = self.viewport.horizon_tool_active;
                                let prev_phase = self.viewport.parallel_tool_phase;
                                let prev_brush = self.session.grading.masks.active_brush_mask_idx;
                                let dims = self.session.loaded_photo.as_ref().map(|p| p.dimensions);
                                let out = sections::crop::render(
                                    ui,
                                    &mut self.session.grading.geometry,
                                    &mut self.viewport.is_crop_editing,
                                    dims,
                                    cur_lang,
                                );
                                self.reconcile_canvas_tool_transitions(
                                    prev_crop,
                                    prev_horizon,
                                    prev_phase,
                                    prev_brush,
                                );
                                out
                            }
                            DarkroomSection::Straighten => {
                                let prev_crop = self.viewport.is_crop_editing;
                                let prev_horizon = self.viewport.horizon_tool_active;
                                let prev_phase = self.viewport.parallel_tool_phase;
                                let prev_brush = self.session.grading.masks.active_brush_mask_idx;
                                let out = sections::straighten::render(
                                    ui,
                                    &mut self.session.grading.geometry,
                                    &mut self.viewport.horizon_tool_active,
                                    &mut self.viewport.horizon_start_point,
                                    &mut self.status_message,
                                    cur_lang,
                                );
                                self.reconcile_canvas_tool_transitions(
                                    prev_crop,
                                    prev_horizon,
                                    prev_phase,
                                    prev_brush,
                                );
                                out
                            }
                            DarkroomSection::Geometry => {
                                let prev_crop = self.viewport.is_crop_editing;
                                let prev_horizon = self.viewport.horizon_tool_active;
                                let prev_phase = self.viewport.parallel_tool_phase;
                                let prev_brush = self.session.grading.masks.active_brush_mask_idx;
                                let mut tool_state = sections::geometry::ParallelLinesToolState {
                                    phase: &mut self.viewport.parallel_tool_phase,
                                    line1: &mut self.viewport.parallel_line1,
                                    line2: &mut self.viewport.parallel_line2,
                                    drag_start: &mut self.viewport.parallel_drag_start,
                                    status_message: &mut self.status_message,
                                };
                                let out = sections::geometry::render(
                                    ui,
                                    &mut self.session.grading.geometry,
                                    &mut tool_state,
                                    cur_lang,
                                );
                                self.reconcile_canvas_tool_transitions(
                                    prev_crop,
                                    prev_horizon,
                                    prev_phase,
                                    prev_brush,
                                );
                                out
                            }
                            DarkroomSection::DetailDenoise => sections::detail_denoise::render(
                                ui,
                                &mut self.session.grading.denoise_sharpen,
                                cur_lang,
                            ),
                            DarkroomSection::OpticalCorrections => {
                                let lens_info = if let Some(base) = &self.session.base_preview {
                                    let active_matched = if let Some(override_name) =
                                        &self.session.grading.optics.lens_profile_override
                                    {
                                        let focal = base.focal_length_mm.unwrap_or(50.0);
                                        let aperture = base.aperture_f_number;
                                        rraw_optics::LensfunDatabase::global()
                                            .find_lens(override_name)
                                            .map(|lens| rraw_optics::MatchedLens {
                                                profile_name: format!(
                                                    "{} {}",
                                                    lens.maker, lens.model
                                                ),
                                                maker: lens.maker.clone(),
                                                model: rraw_optics::LensfunDatabase::global()
                                                    .interpolate_model(lens, focal, aperture),
                                                is_exact_match: true,
                                            })
                                            .or_else(|| base.matched_lens.clone())
                                    } else {
                                        base.matched_lens.clone()
                                    };

                                    if let Some(matched) = active_matched {
                                        let meta_str =
                                            if let Some(photo) = &self.session.loaded_photo {
                                                if !photo.focal_length_str.is_empty()
                                                    && !photo.aperture_str.is_empty()
                                                {
                                                    format!(
                                                        "{} · {}",
                                                        photo.focal_length_str, photo.aperture_str
                                                    )
                                                } else {
                                                    photo.focal_length_str.clone()
                                                }
                                            } else {
                                                String::new()
                                            };
                                        let is_manual = self
                                            .session
                                            .grading
                                            .optics
                                            .lens_profile_override
                                            .is_some();
                                        let is_dng = matched.maker == "DNG Embedded";
                                        Some(sections::optics::DetectedLensInfo {
                                            name: matched.profile_name,
                                            is_exact: matched.is_exact_match,
                                            extra_metadata: meta_str,
                                            is_manual_override: is_manual,
                                            is_dng_opcode: is_dng,
                                        })
                                    } else {
                                        None
                                    }
                                } else if let Some(photo) = &self.session.loaded_photo {
                                    let meta_str = if !photo.focal_length_str.is_empty()
                                        && !photo.aperture_str.is_empty()
                                    {
                                        format!(
                                            "{} · {}",
                                            photo.focal_length_str, photo.aperture_str
                                        )
                                    } else {
                                        photo.focal_length_str.clone()
                                    };
                                    let active_name = self
                                        .session
                                        .grading
                                        .optics
                                        .lens_profile_override
                                        .as_ref()
                                        .unwrap_or(&photo.lens_model);
                                    let is_manual =
                                        self.session.grading.optics.lens_profile_override.is_some();
                                    if !active_name.is_empty() {
                                        Some(sections::optics::DetectedLensInfo {
                                            name: active_name.clone(),
                                            is_exact: true,
                                            extra_metadata: meta_str,
                                            is_manual_override: is_manual,
                                            is_dng_opcode: false,
                                        })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                sections::optics::render(
                                    ui,
                                    &mut self.session.grading.optics,
                                    lens_info,
                                    cur_lang,
                                )
                            }
                            DarkroomSection::Effects => sections::effects::render(
                                ui,
                                &mut self.session.grading.effects,
                                cur_lang,
                            ),
                            DarkroomSection::Timelapse => {
                                self.render_timelapse_controls_ui(ui, "inspector");
                                if sections::timelapse::render_export_button(ui, cur_lang) {
                                    self.export
                                        .set_target_mode(ExportTargetMode::TimelapseSequence);
                                    self.export.config.subfolder = "Timelapse".to_string();
                                    self.export.open_dialog(
                                        crate::controllers::export::ActiveModalDialog::Export,
                                    );
                                    self.export.stats.status = None;
                                }
                                sections::SectionOutput::nothing()
                            }
                            DarkroomSection::LegacyOther => sections::SectionOutput::nothing(),
                        };

                        if out.mark_custom_preset {
                            self.session.grading.preset = GradingPreset::Custom;
                            self.session.current_preset = GradingPreset::Custom;
                        }
                        if out.needs_preview_update {
                            self.session.needs_preview_update = true;
                            ui.ctx().request_repaint();
                        }
                        let grading_edited = self.session.grading != pre_edit_grading;
                        if grading_edited {
                            self.notify_grading_edited(pre_edit_grading, pointer_down);
                            ui.ctx().request_repaint();
                        } else if out.save_sidecar {
                            self.save_current_photo_sidecar();
                        }
                        if section != DarkroomSection::AiSemanticMasks
                            && (out.needs_preview_update || grading_edited)
                        {
                            self.session.grading.masks.active_brush_mask_idx = None;
                        }
                    });
            });

        if self.session.needs_preview_update {
            if let Some(photo) = &self.session.loaded_photo {
                self.library
                    .photo_gradings
                    .insert(photo.path.clone(), self.session.grading.clone());
            }
        }

        ui.add_space(4.0);
    }
}
