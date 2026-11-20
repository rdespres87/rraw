#![forbid(unsafe_code)]

//! Right-hand darkroom development inspector panel and collapsible sections.

use egui::Color32;
use rraw_color::DisplayTransform;

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::state::*;

impl RrawApp {
    /// Returns true if a darkroom section contains non-default adjustment values.
    #[must_use]
    pub fn is_section_active(&self, section: DarkroomSection) -> bool {
        match section {
            DarkroomSection::ExposureTone => {
                self.grading.exposure_ev != 0.0
                    || self.grading.contrast != 0.0
                    || self.grading.highlights != 0.0
                    || self.grading.shadows != 0.0
                    || self.grading.blacks != 0.0
                    || self.grading.saturation != 0.0
                    || self.grading.vibrance != 0.0
            }
            DarkroomSection::ToneCurves => !self.grading.tone_curves.is_identity(),
            DarkroomSection::WhiteBalanceColor => {
                (self.grading.kelvin - 5500.0).abs() > 1.0
                    || self.grading.tint != 0.0
                    || self.grading.custom_clut.is_some()
                    || self.grading.is_monochrome
            }
            DarkroomSection::SoftProofing => self.grading.soft_proof.enabled,
            DarkroomSection::DetailDenoise => {
                (self.grading.wavelet_denoise_sigma - 1.0).abs() > 0.01
                    || (self.grading.color_denoise - 25.0).abs() > 0.01
                    || self.grading.luma_denoise > 0.0
                    || (self.grading.capture_sharpening_amount - 0.5).abs() > 0.01
            }
            DarkroomSection::Crop => self.grading.crop_enabled,
            DarkroomSection::Straighten => {
                self.grading.rotation_degrees != 0.0
                    || self.grading.flip_horizontal
                    || self.grading.flip_vertical
            }
            DarkroomSection::Geometry => {
                self.grading.keystone_vertical != 0.0 || self.grading.keystone_horizontal != 0.0
            }
            DarkroomSection::OpticalCorrections => {
                self.grading.distortion_amount != 0.0
                    || self.grading.defringe_purple_amount != 0.0
                    || self.grading.defringe_green_amount != 0.0
                    || self.grading.creative_vignette_amount != 0.0
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
                self.grading.exposure_ev = 0.0;
                self.grading.contrast = 0.0;
                self.grading.highlights = 0.0;
                self.grading.shadows = 0.0;
                self.grading.blacks = 0.0;
                self.grading.saturation = 0.0;
                self.grading.vibrance = 0.0;
            }
            DarkroomSection::ToneCurves => {
                self.grading.tone_curves = rraw_core::curves::ToneCurvesRecipe::default();
            }
            DarkroomSection::WhiteBalanceColor => {
                self.grading.kelvin = 5500.0;
                self.grading.tint = 0.0;
                self.grading.custom_clut = None;
                self.grading.is_monochrome = false;
                self.grading.display_transform = rraw_color::DisplayTransform::AgX;
            }
            DarkroomSection::SoftProofing => {
                self.grading.soft_proof.enabled = false;
                self.grading.soft_proof.show_gamut_warning = false;
            }
            DarkroomSection::DetailDenoise => {
                self.grading.wavelet_denoise_sigma = 1.0;
                self.grading.color_denoise = 25.0;
                self.grading.luma_denoise = 0.0;
                self.grading.capture_sharpening_amount = 0.5;
            }
            DarkroomSection::Crop => {
                self.grading.crop_enabled = false;
                self.grading.crop_left = 0.0;
                self.grading.crop_top = 0.0;
                self.grading.crop_right = 1.0;
                self.grading.crop_bottom = 1.0;
            }
            DarkroomSection::Straighten => {
                self.grading.rotation_degrees = 0.0;
                self.grading.flip_horizontal = false;
                self.grading.flip_vertical = false;
            }
            DarkroomSection::Geometry => {
                self.grading.geometry_enabled = true;
                self.grading.keystone_vertical = 0.0;
                self.grading.keystone_horizontal = 0.0;
                self.grading.auto_crop_geometry = true;
            }
            DarkroomSection::OpticalCorrections => {
                self.grading.distortion_correction = true;
                self.grading.distortion_amount = 0.0;
                self.grading.chromatic_aberration_correction = true;
                self.grading.defringe_purple_amount = 0.0;
                self.grading.defringe_green_amount = 0.0;
                self.grading.vignette_correction = true;
                self.grading.vignette_amount = 50.0;
                self.grading.creative_vignette_amount = 0.0;
                self.grading.creative_vignette_midpoint = 50.0;
                self.grading.creative_vignette_feather = 50.0;
                self.grading.creative_vignette_roundness = 0.0;
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
                            .on_hover_text(if cur_lang == Language::Fr {
                                "Ce module contient des réglages actifs personnalisés"
                            } else {
                                "This module contains active custom adjustments"
                            });
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

        match section {
            DarkroomSection::ExposureTone => {
                ui.label(t(Key::InspectorParamExposure));
                let r1 = ui.add(
                    egui::Slider::new(&mut self.grading.exposure_ev, -5.0..=5.0).show_value(true),
                );
                if r1.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamContrast));
                let r2 = ui.add(
                    egui::Slider::new(&mut self.grading.contrast, -100.0..=100.0).show_value(true),
                );
                if r2.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamHighlights));
                let r3 = ui.add(
                    egui::Slider::new(&mut self.grading.highlights, -100.0..=100.0)
                        .show_value(true),
                );
                if r3.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamShadows));
                let r4 = ui.add(
                    egui::Slider::new(&mut self.grading.shadows, -100.0..=100.0).show_value(true),
                );
                if r4.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamBlacks));
                let r5 = ui.add(
                    egui::Slider::new(&mut self.grading.blacks, -100.0..=100.0).show_value(true),
                );
                if r5.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button(t(Key::InspectorResetTone)).clicked() {
                    self.grading.exposure_ev = 0.0;
                    self.grading.contrast = 0.0;
                    self.grading.highlights = 0.0;
                    self.grading.shadows = 0.0;
                    self.grading.blacks = 0.0;
                    self.grading.preset = GradingPreset::detect_preset(&self.grading);
                    self.current_preset = self.grading.preset;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::WhiteBalanceColor => {
                ui.label(t(Key::InspectorParamKelvin));
                let r_k = ui.add(
                    egui::Slider::new(&mut self.grading.kelvin, 2000.0..=12000.0).show_value(true),
                );
                if r_k.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamTint));
                let r_t = ui
                    .add(egui::Slider::new(&mut self.grading.tint, -50.0..=50.0).show_value(true));
                if r_t.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamVibrance));
                let r_vib = ui.add(
                    egui::Slider::new(&mut self.grading.vibrance, -100.0..=100.0).show_value(true),
                );
                if r_vib.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamSaturation));
                let r_sat = ui.add(
                    egui::Slider::new(&mut self.grading.saturation, -100.0..=100.0)
                        .show_value(true),
                );
                if r_sat.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamDrt));
                let drt_w = (ui.available_width() - 8.0).max(100.0);
                egui::ComboBox::from_id_salt("drt_select")
                    .selected_text(format!("{:?}", self.grading.display_transform))
                    .width(drt_w)
                    .show_ui(ui, |ui| {
                        let c1 = ui.selectable_value(
                            &mut self.grading.display_transform,
                            DisplayTransform::AgX,
                            t(Key::InspectorDrtAgx),
                        );
                        let c2 = ui.selectable_value(
                            &mut self.grading.display_transform,
                            DisplayTransform::OpenDrt,
                            t(Key::InspectorDrtOpenDrt),
                        );
                        if c1.changed() || c2.changed() {
                            self.grading.preset = GradingPreset::Custom;
                            self.current_preset = GradingPreset::Custom;
                            self.needs_preview_update = true;
                            ui.ctx().request_repaint();
                        }
                    });

                let c_mono = ui.checkbox(
                    &mut self.grading.is_monochrome,
                    t(Key::InspectorParamMonochrome),
                );
                if c_mono.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button(t(Key::InspectorResetColor)).clicked() {
                    self.grading.kelvin = 5500.0;
                    self.grading.tint = 0.0;
                    self.grading.vibrance = 0.0;
                    self.grading.saturation = 0.0;
                    self.grading.is_monochrome = false;
                    self.grading.preset = GradingPreset::detect_preset(&self.grading);
                    self.current_preset = self.grading.preset;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::AiSemanticMasks => {
                let mut changed = false;

                ui.label(egui::RichText::new(t(Key::InspectorGeometricMasksTitle)).strong());
                ui.vertical(|ui| {
                    if ui
                        .button(t(Key::InspectorParamAddLinear))
                        .on_hover_text(t(Key::InspectorAddLinearTooltip))
                        .clicked()
                    {
                        let count = self.grading.local_masks.len() + 1;
                        self.grading.local_masks.push(LocalMaskLayer::new_linear(
                            format!("Linéaire {count}"),
                            [0.5, 0.0],
                            [0.5, 0.45],
                        ));
                        self.grading.show_local_mask_overlay = None;
                        changed = true;
                    }
                    if ui
                        .button(t(Key::InspectorParamAddRadial))
                        .on_hover_text(t(Key::InspectorAddRadialTooltip))
                        .clicked()
                    {
                        let count = self.grading.local_masks.len() + 1;
                        self.grading.local_masks.push(LocalMaskLayer::new_radial(
                            format!("Radial {count}"),
                            [0.5, 0.5],
                            [0.3, 0.3],
                        ));
                        self.grading.show_local_mask_overlay = None;
                        self.grading.active_brush_mask_idx = None;
                        changed = true;
                    }
                    if ui
                        .button(t(Key::InspectorParamAddBrush))
                        .on_hover_text(t(Key::InspectorAddBrushTooltip))
                        .clicked()
                    {
                        let count = self.grading.local_masks.len() + 1;
                        self.grading.local_masks.push(LocalMaskLayer::new_brush(
                            format!("Pinceau {count}"),
                            0.08,
                            0.65,
                        ));
                        let new_idx = self.grading.local_masks.len() - 1;
                        self.grading.show_local_mask_overlay = None;
                        self.grading.active_brush_mask_idx = Some(new_idx);
                        changed = true;
                    }
                });

                let mut local_mask_to_remove: Option<usize> = None;
                let mut active_local_overlay = self.grading.show_local_mask_overlay;
                let mut active_brush_idx = self.grading.active_brush_mask_idx;

                for (idx, mask) in self.grading.local_masks.iter_mut().enumerate() {
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
                                        egui::RichText::new(format!("{icon} {}", mask.name))
                                            .strong(),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }

                                if matches!(mask.kind, LocalMaskKind::FreehandBrush { .. }) {
                                    let is_painting = active_brush_idx == Some(idx);
                                    if ui
                                        .selectable_label(is_painting, t(Key::InspectorPaintButton))
                                        .on_hover_text(t(Key::InspectorPaintTooltip))
                                        .clicked()
                                    {
                                        active_brush_idx =
                                            if is_painting { None } else { Some(idx) };
                                    }
                                }

                                let is_overlay = active_local_overlay == Some(idx);
                                if ui
                                    .selectable_label(is_overlay, "👁")
                                    .on_hover_text(t(Key::InspectorOverlayTooltip))
                                    .clicked()
                                {
                                    active_local_overlay =
                                        if is_overlay { None } else { Some(idx) };
                                    changed = true;
                                }

                                if ui
                                    .checkbox(&mut mask.invert, t(Key::InspectorParamInvertMask))
                                    .on_hover_text(t(Key::InspectorInvertTooltip))
                                    .changed()
                                {
                                    changed = true;
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("✖")
                                            .on_hover_text(t(Key::InspectorDeleteMaskTooltip))
                                            .clicked()
                                        {
                                            local_mask_to_remove = Some(idx);
                                        }
                                    },
                                );
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
                                            egui::Slider::new(
                                                &mut mask.temperature_shift,
                                                -100.0..=100.0,
                                            )
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
                                        .add(
                                            egui::Slider::new(&mut mask.feather, 0.05..=1.0)
                                                .show_value(true),
                                        )
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
                                                .add(
                                                    egui::Slider::new(&mut end[1], 0.0..=1.0)
                                                        .show_value(true),
                                                )
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
                                                .on_hover_text(
                                                    t(Key::InspectorBrushRadiusTooltip),
                                                )
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
                                                        .custom_formatter(|n, _| {
                                                            format!("{:.0}%", n * 100.0)
                                                        }),
                                                )
                                                .on_hover_text(
                                                    "Intensité de chaque coup de pinceau",
                                                )
                                                .changed()
                                            {
                                                changed = true;
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            let is_erase = *erase_mode;
                                            if ui
                                                .selectable_label(!is_erase, "🖌️ Peindre")
                                                .on_hover_text(
                                                    "Mode application : ajoute de la matière au masque",
                                                )
                                                .clicked()
                                            {
                                                *erase_mode = false;
                                            }
                                            if ui
                                                .selectable_label(is_erase, "🧹 Gommer")
                                                .on_hover_text(
                                                    "Mode gomme : soustrait du masque",
                                                )
                                                .clicked()
                                            {
                                                *erase_mode = true;
                                            }
                                            ui.add_space(8.0);
                                            let dab_count = dabs.len();
                                            ui.label(format!("({dab_count} traits)"));
                                            if dab_count > 0
                                                && ui
                                                    .small_button("🗑️ Vider")
                                                    .on_hover_text(
                                                        "Effacer tous les traits de ce pinceau",
                                                    )
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
                    if remove_idx < self.grading.local_masks.len() {
                        self.grading.local_masks.remove(remove_idx);
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
                self.grading.show_local_mask_overlay = active_local_overlay;
                self.grading.active_brush_mask_idx = active_brush_idx;

                ui.separator();
                ui.label(egui::RichText::new(t(Key::InspectorAiMasksTitle)).strong());
                ui.label(
                    egui::RichText::new(t(Key::InspectorAiMasksSubtitle))
                        .small()
                        .color(Color32::GRAY),
                );
                ui.horizontal_wrapped(|ui| {
                    for cat in rraw_core::SemanticMaskCategory::ALL {
                        let exists = self
                            .grading
                            .semantic_masks
                            .iter()
                            .any(|m| m.category == cat);
                        let label = SemanticMaskLayer::category_label_lang(cat, cur_lang);
                        let btn_text = if exists {
                            egui::RichText::new(format!("✓ {label}"))
                                .strong()
                                .color(Color32::from_rgb(110, 225, 145))
                        } else {
                            egui::RichText::new(label.to_string())
                        };
                        let tooltip = if exists {
                            "Cliquer pour décocher et retirer ce masque IA"
                        } else {
                            "Cliquer pour cocher et activer ce masque IA"
                        };
                        if ui
                            .selectable_label(exists, btn_text)
                            .on_hover_text(tooltip)
                            .clicked()
                        {
                            if exists {
                                self.grading.remove_semantic_mask(cat);
                            } else {
                                let _ = self.grading.upsert_semantic_mask(cat);
                            }
                            self.grading.preset = GradingPreset::Custom;
                            self.current_preset = GradingPreset::Custom;
                            self.needs_preview_update = true;
                            ui.ctx().request_repaint();
                        }
                    }
                });

                let mut category_to_remove: Option<rraw_core::SemanticMaskCategory> = None;
                let mut active_overlay = self.grading.show_semantic_mask_overlay;

                for layer in &mut self.grading.semantic_masks {
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style())
                        .fill(Color32::from_rgb(33, 36, 42))
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(
                                        &mut layer.enabled,
                                        egui::RichText::new(SemanticMaskLayer::category_label_lang(layer.category, cur_lang))
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
                                    .checkbox(&mut layer.invert, "Inverser")
                                    .on_hover_text(t(Key::InspectorAiInvertTooltip))
                                    .changed()
                                {
                                    changed = true;
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("✖")
                                            .on_hover_text(t(Key::InspectorAiDeleteTooltip))
                                            .clicked()
                                        {
                                            category_to_remove = Some(layer.category);
                                        }
                                    },
                                );
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
                                            egui::Slider::new(
                                                &mut layer.saturation,
                                                -100.0..=100.0,
                                            )
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
                                            egui::Slider::new(
                                                &mut layer.temperature_shift,
                                                -100.0..=100.0,
                                            )
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
                                            egui::Slider::new(&mut layer.feather, 0.02..=0.50)
                                                .show_value(true),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(t(Key::InspectorParamMaskOpacity));
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut layer.opacity, 0.0..=1.0)
                                                .show_value(true),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });
                            }
                        });
                }

                self.grading.show_semantic_mask_overlay = active_overlay;
                if let Some(cat) = category_to_remove {
                    self.grading.remove_semantic_mask(cat);
                    changed = true;
                }

                if !self.grading.semantic_masks.is_empty()
                    && ui.button(t(Key::InspectorResetAiMasks)).clicked()
                {
                    self.grading.semantic_masks.clear();
                    self.grading.show_semantic_mask_overlay = None;
                    changed = true;
                }

                if changed {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::SoftProofing => {
                let mut sp_changed = false;

                if ui
                    .checkbox(
                        &mut self.grading.soft_proof.enabled,
                        egui::RichText::new(t(Key::InspectorEnableSoftProof)).strong(),
                    )
                    .changed()
                {
                    sp_changed = true;
                }

                ui.add_space(2.0);
                ui.label(t(Key::InspectorParamSoftProofProfile));
                let combo_w = (ui.available_width() - 8.0).max(100.0);
                egui::ComboBox::from_id_salt("soft_proof_profile_combo")
                    .selected_text(self.grading.soft_proof.profile.label())
                    .width(combo_w)
                    .show_ui(ui, |ui| {
                        for prof in rraw_color::SoftProofProfile::ALL {
                            if ui
                                .selectable_value(
                                    &mut self.grading.soft_proof.profile,
                                    prof,
                                    prof.label(),
                                )
                                .changed()
                            {
                                // Auto-enable paper & ink simulation when choosing a physical paper profile
                                if prof.is_print_medium() {
                                    self.grading.soft_proof.simulate_paper_and_ink = true;
                                }
                                sp_changed = true;
                            }
                        }
                    });

                ui.add_space(2.0);
                ui.label(t(Key::InspectorParamSoftProofIntent));
                let intent_w = (ui.available_width() - 8.0).max(100.0);
                egui::ComboBox::from_id_salt("soft_proof_intent_combo")
                    .selected_text(self.grading.soft_proof.intent.label())
                    .width(intent_w)
                    .show_ui(ui, |ui| {
                        for intent in rraw_color::SoftProofRenderingIntent::ALL {
                            if ui
                                .selectable_value(
                                    &mut self.grading.soft_proof.intent,
                                    intent,
                                    intent.label(),
                                )
                                .changed()
                            {
                                sp_changed = true;
                            }
                        }
                    });

                ui.add_space(2.0);
                if ui
                    .checkbox(
                        &mut self.grading.soft_proof.simulate_paper_and_ink,
                        "Simuler papier et encre",
                    )
                    .on_hover_text(
                        "Simule la teinte chaude du papier coton/baryté (Dmin) et la réflectance minimale de l'encre noire (Dmax)",
                    )
                    .changed()
                {
                    sp_changed = true;
                }

                let alert_label = if self.grading.soft_proof.show_gamut_warning {
                    egui::RichText::new(t(Key::InspectorGamutAlertStrong))
                        .color(Color32::from_rgb(255, 110, 230))
                } else {
                    egui::RichText::new(t(Key::InspectorGamutAlert))
                };
                if ui
                    .checkbox(&mut self.grading.soft_proof.show_gamut_warning, alert_label)
                    .on_hover_text(
                        t(Key::InspectorGamutAlertTooltip),
                    )
                    .changed()
                {
                    sp_changed = true;
                }

                let oog_count = self.histogram.out_of_gamut_count;
                let status_text = if oog_count > 0 {
                    egui::RichText::new(format!(
                        "⚠️ {oog_count} pixels hors-gamut ({})",
                        self.grading.soft_proof.profile.short_label()
                    ))
                    .small()
                    .color(Color32::from_rgb(255, 130, 220))
                } else {
                    egui::RichText::new(format!(
                        "✓ 100% dans le gamut ({})",
                        self.grading.soft_proof.profile.short_label()
                    ))
                    .small()
                    .color(Color32::from_rgb(110, 220, 145))
                };
                ui.label(status_text);

                if (self.grading.soft_proof.enabled
                    || self.grading.soft_proof.show_gamut_warning
                    || self.grading.soft_proof.simulate_paper_and_ink)
                    && ui.button(t(Key::InspectorResetSoftProof)).clicked()
                {
                    self.grading.soft_proof = rraw_color::SoftProofConfig::default();
                    sp_changed = true;
                }

                if sp_changed {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::ToneCurves => {
                self.render_tone_curves_section(ui);
            }
            DarkroomSection::Crop => {
                let c_crop = ui.checkbox(&mut self.grading.crop_enabled, t(Key::InspectorEnableCrop));
                if c_crop.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if self.grading.crop_enabled {
                    ui.horizontal(|ui| {
                        ui.label(t(Key::InspectorParamCropRatio));
                        egui::ComboBox::from_id_salt("crop_ratio_select")
                            .selected_text(self.grading.crop_aspect.label())
                            .show_ui(ui, |ui| {
                                for ratio in CropAspectRatio::ALL {
                                    let is_sel = self.grading.crop_aspect == ratio;
                                    if ui.selectable_label(is_sel, ratio.label()).clicked() {
                                        self.grading.crop_aspect = ratio;
                                        let (orig_w, orig_h) = if let Some(photo) =
                                            &self.loaded_photo
                                        {
                                            (photo.dimensions.0 as f32, photo.dimensions.1 as f32)
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
                                            self.grading.crop_left =
                                                ((1.0 - crop_w) * 0.5).clamp(0.0, 0.45);
                                            self.grading.crop_right =
                                                (self.grading.crop_left + crop_w).clamp(0.55, 1.0);
                                            self.grading.crop_top =
                                                ((1.0 - crop_h) * 0.5).clamp(0.0, 0.45);
                                            self.grading.crop_bottom =
                                                (self.grading.crop_top + crop_h).clamp(0.55, 1.0);
                                        }
                                        self.is_crop_editing = true;
                                        ui.ctx().request_repaint();
                                    }
                                }
                            });
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if self.is_crop_editing {
                            if ui
                                .button(
                                    egui::RichText::new(t(Key::InspectorValidateCrop))
                                        .strong()
                                        .color(Color32::from_rgb(100, 220, 120)),
                                )
                                .clicked()
                            {
                                self.is_crop_editing = false;
                                self.needs_preview_update = true;
                                self.save_current_photo_sidecar();
                                ui.ctx().request_repaint();
                            }
                        } else if ui
                            .button(
                                egui::RichText::new(t(Key::InspectorAdjustCrop)).strong(),
                            )
                            .clicked()
                        {
                            self.is_crop_editing = true;
                            ui.ctx().request_repaint();
                        }

                        if ui.button(t(Key::InspectorResetCrop)).clicked() {
                            self.grading.crop_left = 0.0;
                            self.grading.crop_top = 0.0;
                            self.grading.crop_right = 1.0;
                            self.grading.crop_bottom = 1.0;
                            self.grading.crop_aspect = CropAspectRatio::Free;
                            self.is_crop_editing = false;
                            self.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }
                    });

                    let c_grid = ui.checkbox(
                        &mut self.grading.crop_show_grid,
                        t(Key::InspectorShowCropGrid),
                    );
                    if c_grid.changed() {
                        ui.ctx().request_repaint();
                    }

                    ui.label(
                        egui::RichText::new(
                            "💡 Ajustez le cadre à la souris sur la photo puis cliquez sur Valider.",
                        )
                        .small()
                        .color(Color32::GRAY),
                    );
                }
            }
            DarkroomSection::Straighten => {
                ui.label(t(Key::InspectorParamStraightenAngle));
                let r_rot = ui.add(
                    egui::Slider::new(&mut self.grading.rotation_degrees, -45.0..=45.0)
                        .step_by(0.1)
                        .suffix("°")
                        .show_value(true),
                );
                if r_rot.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.horizontal(|ui| {
                    if ui.button(t(Key::InspectorResetAngle)).clicked() {
                        self.grading.rotation_degrees = 0.0;
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                    if ui.button(t(Key::InspectorRotate90Left)).clicked() {
                        self.grading.rotation_degrees =
                            (self.grading.rotation_degrees - 90.0).clamp(-45.0, 45.0);
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                    if ui.button(t(Key::InspectorRotate90Right)).clicked() {
                        self.grading.rotation_degrees =
                            (self.grading.rotation_degrees + 90.0).clamp(-45.0, 45.0);
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                });

                let horizon_btn_text = if self.horizon_tool_active {
                    t(Key::StraightenHorizonActive)
                } else {
                    t(Key::StraightenHorizonDraw)
                };
                let horizon_btn = ui.selectable_label(self.horizon_tool_active, horizon_btn_text);
                if horizon_btn.clicked() {
                    self.horizon_tool_active = !self.horizon_tool_active;
                    self.horizon_start_point = None;
                    if self.horizon_tool_active {
                        self.status_message =
                            t(Key::StraightenHorizonStatus).to_string();
                    }
                }

                ui.horizontal(|ui| {
                    let c_fx = ui.checkbox(&mut self.grading.flip_horizontal, t(Key::InspectorParamFlipH));
                    let c_fy = ui.checkbox(&mut self.grading.flip_vertical, t(Key::InspectorParamFlipV));
                    if c_fx.changed() || c_fy.changed() {
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                });
            }
            DarkroomSection::Geometry => {
                let c_geom =
                    ui.checkbox(&mut self.grading.geometry_enabled, t(Key::InspectorEnableGeometry));
                if c_geom.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if self.grading.geometry_enabled {
                    let is_parallel_active =
                        self.parallel_tool_phase != ParallelToolPhase::Inactive;
                    let parallel_btn_label = if is_parallel_active {
                        "📐 2 Lignes Parallèles (Actif - Glissez sur la photo)"
                    } else {
                        "📐 Tracer 2 lignes parallèles (Perspective)"
                    };
                    let btn = ui.selectable_label(is_parallel_active, parallel_btn_label);
                    if btn.clicked() {
                        if is_parallel_active {
                            self.parallel_tool_phase = ParallelToolPhase::Inactive;
                            self.parallel_line1 = None;
                            self.parallel_line2 = None;
                            self.parallel_drag_start = None;
                        } else {
                            self.parallel_tool_phase = ParallelToolPhase::DrawingLine1;
                            self.parallel_line1 = None;
                            self.parallel_line2 = None;
                            self.parallel_drag_start = None;
                            self.status_message =
                                "Glissez sur la photo pour tracer la 1ère référence verticale"
                                    .to_string();
                        }
                    }

                    ui.label(t(Key::InspectorParamKeystoneV));
                    let r_kv = ui.add(
                        egui::Slider::new(&mut self.grading.keystone_vertical, -50.0..=50.0)
                            .step_by(0.5)
                            .show_value(true),
                    );
                    if r_kv.changed() {
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    ui.label(t(Key::InspectorParamKeystoneH));
                    let r_kh = ui.add(
                        egui::Slider::new(&mut self.grading.keystone_horizontal, -50.0..=50.0)
                            .step_by(0.5)
                            .show_value(true),
                    );
                    if r_kh.changed() {
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    let c_ac = ui.checkbox(
                        &mut self.grading.auto_crop_geometry,
                        t(Key::InspectorAutoCrop),
                    );
                    if c_ac.changed() {
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    if ui.button(t(Key::InspectorResetGeometry)).clicked() {
                        self.grading.keystone_vertical = 0.0;
                        self.grading.keystone_horizontal = 0.0;
                        self.grading.rotation_degrees = 0.0;
                        self.grading.flip_horizontal = false;
                        self.grading.flip_vertical = false;
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                }
            }
            DarkroomSection::DetailDenoise => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t(Key::InspectorParamChromaDenoise)).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if (self.grading.color_denoise - 25.0).abs() > 0.1
                            && ui
                                .small_button("Auto (25)")
                                .on_hover_text("Rétablir la valeur automatique recommandée")
                                .clicked()
                        {
                            self.grading.color_denoise = 25.0;
                            self.needs_preview_update = true;
                            ui.ctx().request_repaint();
                        }
                    });
                });
                let r_cd = ui
                    .add(
                        egui::Slider::new(&mut self.grading.color_denoise, 0.0..=100.0)
                            .show_value(true)
                            .suffix("%"),
                    )
                    .on_hover_text("Supprime les marbrures et tavelures colorées (magenta/vertes) sans flouter les détails nets");
                if r_cd.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.add_space(4.0);
                ui.label(egui::RichText::new(t(Key::InspectorParamLumaDenoise)).strong());
                let r_ld = ui
                    .add(
                        egui::Slider::new(&mut self.grading.luma_denoise, 0.0..=100.0)
                            .show_value(true)
                            .suffix("%"),
                    )
                    .on_hover_text("Lisse le grain noir et blanc dans les aplats tout en préservant le micro-contraste des contours");
                if r_ld.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.add_space(4.0);
                ui.label(t(Key::InspectorParamCaptureSharpening));
                let r_s = ui.add(
                    egui::Slider::new(&mut self.grading.capture_sharpening_amount, 0.0..=2.0)
                        .show_value(true),
                );
                if r_s.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.add_space(4.0);
                if ui.button(t(Key::InspectorResetDetailDenoise)).clicked() {
                    self.grading.color_denoise = 25.0;
                    self.grading.luma_denoise = 0.0;
                    self.grading.capture_sharpening_amount = 0.5;
                    self.grading.preset = GradingPreset::detect_preset(&self.grading);
                    self.current_preset = self.grading.preset;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::OpticalCorrections => {
                let c_dist = ui.checkbox(
                    &mut self.grading.distortion_correction,
                    t(Key::InspectorLensDistortion),
                );
                if c_dist.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamDistortion));
                let r_da = ui.add(
                    egui::Slider::new(&mut self.grading.distortion_amount, -50.0..=50.0)
                        .show_value(true),
                );
                if r_da.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.separator();

                let c_ca = ui.checkbox(
                    &mut self.grading.chromatic_aberration_correction,
                    t(Key::InspectorLensCa),
                );
                if c_ca.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamDefringePurple));
                let r_dp = ui.add(
                    egui::Slider::new(&mut self.grading.defringe_purple_amount, 0.0..=100.0)
                        .show_value(true)
                        .suffix(" %"),
                );
                if r_dp.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorParamDefringeGreen));
                let r_dg = ui.add(
                    egui::Slider::new(&mut self.grading.defringe_green_amount, 0.0..=100.0)
                        .show_value(true)
                        .suffix(" %"),
                );
                if r_dg.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.separator();

                ui.label(egui::RichText::new(t(Key::InspectorLensVignetteTitle)).strong());
                let c_vig = ui.checkbox(
                    &mut self.grading.vignette_correction,
                    t(Key::InspectorLensVignette),
                );
                if c_vig.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label(t(Key::InspectorCorrectionStrength));
                let r_va = ui.add(
                    egui::Slider::new(&mut self.grading.vignette_amount, 0.0..=100.0)
                        .show_value(true),
                );
                if r_va.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.separator();

                ui.label(egui::RichText::new(t(Key::InspectorCreativeVignetteTitle)).strong());
                ui.label(t(Key::InspectorParamVignetteAmount));
                let r_cva = ui.add(
                    egui::Slider::new(&mut self.grading.creative_vignette_amount, -100.0..=100.0)
                        .show_value(true),
                );
                if r_cva.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if self.grading.creative_vignette_amount.abs() > 0.1 {
                    ui.label(t(Key::InspectorParamVignetteMidpoint));
                    let r_cvm = ui.add(
                        egui::Slider::new(
                            &mut self.grading.creative_vignette_midpoint,
                            10.0..=90.0,
                        )
                        .show_value(true),
                    );
                    if r_cvm.changed() {
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    ui.label(t(Key::InspectorParamVignetteFeather));
                    let r_cvf = ui.add(
                        egui::Slider::new(&mut self.grading.creative_vignette_feather, 10.0..=90.0)
                            .show_value(true),
                    );
                    if r_cvf.changed() {
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    ui.label(t(Key::InspectorParamVignetteRoundness));
                    let r_cvr = ui.add(
                        egui::Slider::new(
                            &mut self.grading.creative_vignette_roundness,
                            -100.0..=100.0,
                        )
                        .show_value(true),
                    );
                    if r_cvr.changed() {
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                }

                if ui.button(t(Key::InspectorResetOptical)).clicked() {
                    self.grading.distortion_correction = true;
                    self.grading.distortion_amount = 0.0;
                    self.grading.defringe_purple_amount = 0.0;
                    self.grading.defringe_green_amount = 0.0;
                    self.grading.vignette_correction = true;
                    self.grading.vignette_amount = 50.0;
                    self.grading.creative_vignette_amount = 0.0;
                    self.grading.creative_vignette_midpoint = 50.0;
                    self.grading.creative_vignette_feather = 50.0;
                    self.grading.creative_vignette_roundness = 0.0;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::Timelapse => {
                self.render_timelapse_controls_ui(ui, "inspector");
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(t(Key::InspectorTimelapseExport)).strong(),
                        )
                        .fill(Color32::from_rgb(30, 110, 210)),
                    )
                    .clicked()
                {
                    self.export_target_mode = ExportTargetMode::TimelapseSequence;
                    self.export_subfolder = "Timelapse".to_string();
                    self.show_export_dialog = true;
                    self.export_status = None;
                }
            }
            DarkroomSection::LegacyOther => {}
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
