#![forbid(unsafe_code)]

//! Right-hand darkroom development inspector panel and collapsible sections.

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::state::*;
use crate::ui::sections;

impl RrawApp {
    /// Returns true if a darkroom section contains non-default adjustment values.
    #[must_use]
    pub fn is_section_active(&self, section: DarkroomSection) -> bool {
        match section {
            DarkroomSection::ExposureTone => {
                self.grading.tone.exposure_ev != 0.0
                    || self.grading.tone.contrast != 0.0
                    || self.grading.tone.highlights != 0.0
                    || self.grading.tone.shadows != 0.0
                    || self.grading.tone.blacks != 0.0
                    || self.grading.tone.saturation != 0.0
                    || self.grading.tone.vibrance != 0.0
                    || self.grading.smart_lighting.mode != rraw_core::SmartLightingMode::Slight
                    || (self.grading.smart_lighting.intensity - 1.0).abs() > 0.01
                    || self.grading.smart_lighting.custom_shadow_lift != 0.0
                    || self.grading.smart_lighting.custom_highlight_compression != 0.0
            }
            DarkroomSection::ToneCurves => !self.grading.tone_curves.is_identity(),
            DarkroomSection::WhiteBalanceColor => {
                (self.grading.color.kelvin - 5500.0).abs() > 1.0
                    || self.grading.color.tint != 0.0
                    || self.grading.custom_clut.is_some()
                    || self.grading.color.is_monochrome
            }
            DarkroomSection::SoftProofing => self.grading.soft_proof.enabled,
            DarkroomSection::DetailDenoise => {
                (self.grading.denoise_sharpen.wavelet_denoise_sigma - 1.0).abs() > 0.01
                    || (self.grading.denoise_sharpen.color_denoise - 25.0).abs() > 0.01
                    || self.grading.denoise_sharpen.luma_denoise > 0.0
                    || (self.grading.denoise_sharpen.capture_sharpening_amount - 0.5).abs() > 0.01
            }
            DarkroomSection::Crop => self.grading.geometry.crop_enabled,
            DarkroomSection::Straighten => {
                self.grading.geometry.rotation_degrees != 0.0
                    || self.grading.geometry.flip_horizontal
                    || self.grading.geometry.flip_vertical
            }
            DarkroomSection::Geometry => {
                self.grading.geometry.keystone_vertical != 0.0
                    || self.grading.geometry.keystone_horizontal != 0.0
            }
            DarkroomSection::OpticalCorrections => {
                self.grading.optics.optical_tab != crate::state::OpticalCorrectionTab::Profile
                    || !self.grading.optics.lens_profile_auto
                    || (self.grading.optics.lens_distortion_strength - 100.0).abs() > 0.01
                    || (self.grading.optics.lens_vignette_strength - 100.0).abs() > 0.01
                    || (self.grading.optics.lens_ca_strength - 100.0).abs() > 0.01
                    || self.grading.optics.distortion_amount != 0.0
                    || self.grading.optics.defringe_purple_amount != 0.0
                    || self.grading.optics.defringe_green_amount != 0.0
                    || (self.grading.optics.defringe_threshold - 0.08).abs() > 0.005
                    || self.grading.optics.defringe_preview_mask
                    || self.grading.optics.creative_vignette_amount != 0.0
                    || (self.grading.optics.vignette_amount - 50.0).abs() > 0.01
            }
            DarkroomSection::AiSemanticMasks => {
                !self.grading.semantic_masks.is_empty() || !self.grading.local_masks.is_empty()
            }
            DarkroomSection::Timelapse => false,
            DarkroomSection::LegacyOther => false,
        }
    }

    /// Resets all parameters of a specific darkroom section back to standard defaults.
    pub fn reset_section(&mut self, section: DarkroomSection) {
        match section {
            DarkroomSection::ExposureTone => {
                self.grading.tone.exposure_ev = 0.0;
                self.grading.tone.contrast = 0.0;
                self.grading.tone.highlights = 0.0;
                self.grading.tone.shadows = 0.0;
                self.grading.tone.blacks = 0.0;
                self.grading.tone.saturation = 0.0;
                self.grading.tone.vibrance = 0.0;
                self.grading.smart_lighting = DarkroomGradingState::default().smart_lighting;
            }
            DarkroomSection::ToneCurves => {
                self.grading.tone_curves = rraw_core::curves::ToneCurvesRecipe::default();
            }
            DarkroomSection::WhiteBalanceColor => {
                self.grading.color.kelvin = 5500.0;
                self.grading.color.tint = 0.0;
                self.grading.custom_clut = None;
                self.grading.color.is_monochrome = false;
                self.grading.color.display_transform = rraw_color::DisplayTransform::AgX;
            }
            DarkroomSection::SoftProofing => {
                self.grading.soft_proof.enabled = false;
                self.grading.soft_proof.show_gamut_warning = false;
            }
            DarkroomSection::DetailDenoise => {
                self.grading.denoise_sharpen.wavelet_denoise_sigma = 1.0;
                self.grading.denoise_sharpen.color_denoise = 25.0;
                self.grading.denoise_sharpen.luma_denoise = 0.0;
                self.grading.denoise_sharpen.capture_sharpening_amount = 0.5;
            }
            DarkroomSection::Crop => {
                self.grading.geometry.crop_enabled = false;
                self.grading.geometry.crop_left = 0.0;
                self.grading.geometry.crop_top = 0.0;
                self.grading.geometry.crop_right = 1.0;
                self.grading.geometry.crop_bottom = 1.0;
            }
            DarkroomSection::Straighten => {
                self.grading.geometry.rotation_degrees = 0.0;
                self.grading.geometry.flip_horizontal = false;
                self.grading.geometry.flip_vertical = false;
            }
            DarkroomSection::Geometry => {
                self.grading.geometry.geometry_enabled = true;
                self.grading.geometry.keystone_vertical = 0.0;
                self.grading.geometry.keystone_horizontal = 0.0;
                self.grading.geometry.auto_crop_geometry = true;
            }
            DarkroomSection::OpticalCorrections => {
                self.grading.optics.optical_tab = crate::state::OpticalCorrectionTab::Profile;
                self.grading.optics.lens_profile_auto = true;
                self.grading.optics.lens_distortion_strength = 100.0;
                self.grading.optics.lens_vignette_strength = 100.0;
                self.grading.optics.lens_ca_strength = 100.0;
                self.grading.optics.distortion_correction = true;
                self.grading.optics.distortion_amount = 0.0;
                self.grading.optics.chromatic_aberration_correction = true;
                self.grading.optics.defringe_purple_amount = 0.0;
                self.grading.optics.defringe_green_amount = 0.0;
                self.grading.optics.defringe_threshold = 0.08;
                self.grading.optics.defringe_preview_mask = false;
                self.grading.optics.vignette_correction = true;
                self.grading.optics.vignette_amount = 50.0;
                self.grading.optics.creative_vignette_amount = 0.0;
                self.grading.optics.creative_vignette_midpoint = 50.0;
                self.grading.optics.creative_vignette_feather = 50.0;
                self.grading.optics.creative_vignette_roundness = 0.0;
            }
            DarkroomSection::AiSemanticMasks => {
                self.grading.semantic_masks.clear();
                self.grading.local_masks.clear();
                self.grading.active_brush_mask_idx = None;
            }
            DarkroomSection::Timelapse | DarkroomSection::LegacyOther => {}
        }

        self.grading.preset = GradingPreset::Custom;
        self.current_preset = GradingPreset::Custom;
        self.needs_preview_update = true;
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
                            self.layout_config = AppLayoutConfig::default();
                            self.layout_config.save();
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
                            self.active_tool_family = family;
                            self.layout_config.active_tool_family = family;
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
                            self.active_tool_family = family;
                            self.layout_config.active_tool_family = family;
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
                    self.active_tools_only = !self.active_tools_only;
                    self.layout_config.active_tools_only = self.active_tools_only;
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
                                    self.active_tools_only = false;
                                    self.layout_config.active_tools_only = false;
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

                state
                    .show_header(ui, |ui| {
                        if is_active {
                            ui.label(
                                egui::RichText::new("●")
                                    .color(egui::Color32::from_rgb(100, 190, 255))
                                    .size(9.0),
                            )
                            .on_hover_text(
                                if cur_lang == Language::Fr {
                                    "Ce module contient des réglages actifs personnalisés"
                                } else {
                                    "This module contains active custom adjustments"
                                },
                            );
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

                        let out = match section {
                            DarkroomSection::ExposureTone => sections::exposure_tone::render(
                                ui,
                                &mut self.grading.tone,
                                &mut self.grading.smart_lighting,
                                cur_lang,
                            ),
                            DarkroomSection::WhiteBalanceColor => sections::white_balance::render(
                                ui,
                                &mut self.grading.color,
                                &mut self.grading.tone,
                                cur_lang,
                            ),
                            DarkroomSection::AiSemanticMasks => sections::masks::render(
                                ui,
                                &mut self.grading.local_masks,
                                &mut self.grading.show_local_mask_overlay,
                                &mut self.grading.active_brush_mask_idx,
                                &mut self.grading.semantic_masks,
                                &mut self.grading.show_semantic_mask_overlay,
                                cur_lang,
                            ),
                            DarkroomSection::SoftProofing => sections::soft_proofing::render(
                                ui,
                                &mut self.grading.soft_proof,
                                self.histogram.out_of_gamut_count,
                                cur_lang,
                            ),
                            DarkroomSection::ToneCurves => {
                                self.render_tone_curves_section(ui);
                                sections::SectionOutput::nothing()
                            }
                            DarkroomSection::Crop => {
                                let dims = self.loaded_photo.as_ref().map(|p| p.dimensions);
                                sections::crop::render(
                                    ui,
                                    &mut self.grading.geometry,
                                    &mut self.is_crop_editing,
                                    dims,
                                    cur_lang,
                                )
                            }
                            DarkroomSection::Straighten => sections::straighten::render(
                                ui,
                                &mut self.grading.geometry,
                                &mut self.horizon_tool_active,
                                &mut self.horizon_start_point,
                                &mut self.status_message,
                                cur_lang,
                            ),
                            DarkroomSection::Geometry => {
                                let mut tool_state = sections::geometry::ParallelLinesToolState {
                                    phase: &mut self.parallel_tool_phase,
                                    line1: &mut self.parallel_line1,
                                    line2: &mut self.parallel_line2,
                                    drag_start: &mut self.parallel_drag_start,
                                    status_message: &mut self.status_message,
                                };
                                sections::geometry::render(
                                    ui,
                                    &mut self.grading.geometry,
                                    &mut tool_state,
                                    cur_lang,
                                )
                            }
                            DarkroomSection::DetailDenoise => sections::detail_denoise::render(
                                ui,
                                &mut self.grading.denoise_sharpen,
                                cur_lang,
                            ),
                            DarkroomSection::OpticalCorrections => {
                                let lens_info = if let Some(base) = &self.base_preview {
                                    if let Some(matched) = &base.matched_lens {
                                        let meta_str = if let Some(photo) = &self.loaded_photo {
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
                                        Some(sections::optics::DetectedLensInfo {
                                            name: matched.profile_name.clone(),
                                            is_exact: matched.is_exact_match,
                                            extra_metadata: meta_str,
                                        })
                                    } else {
                                        None
                                    }
                                } else if let Some(photo) = &self.loaded_photo {
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
                                    if !photo.lens_model.is_empty() {
                                        Some(sections::optics::DetectedLensInfo {
                                            name: photo.lens_model.clone(),
                                            is_exact: true,
                                            extra_metadata: meta_str,
                                        })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                sections::optics::render(
                                    ui,
                                    &mut self.grading.optics,
                                    lens_info,
                                    cur_lang,
                                )
                            }
                            DarkroomSection::Timelapse => {
                                self.render_timelapse_controls_ui(ui, "inspector");
                                if sections::timelapse::render_export_button(ui, cur_lang) {
                                    self.export_target_mode = ExportTargetMode::TimelapseSequence;
                                    self.export_subfolder = "Timelapse".to_string();
                                    self.show_export_dialog = true;
                                    self.export_status = None;
                                }
                                sections::SectionOutput::nothing()
                            }
                            DarkroomSection::LegacyOther => sections::SectionOutput::nothing(),
                        };

                        if out.mark_custom_preset {
                            self.grading.preset = GradingPreset::Custom;
                            self.current_preset = GradingPreset::Custom;
                        }
                        if out.needs_preview_update {
                            self.needs_preview_update = true;
                            ui.ctx().request_repaint();
                        }
                        if out.save_sidecar {
                            self.save_current_photo_sidecar();
                        }
                    });
            });

        if self.needs_preview_update {
            if let Some(photo) = &self.loaded_photo {
                self.photo_gradings
                    .insert(photo.path.clone(), self.grading.clone());
                let rating = self
                    .photo_ratings
                    .get(&photo.path)
                    .copied()
                    .unwrap_or(photo.rating);
                save_photo_sidecars(&photo.path, &self.grading, rating);
            }
        }

        ui.add_space(4.0);
    }
}
