#![forbid(unsafe_code)]

//! Right-hand darkroom development inspector panel and collapsible sections.

use egui::Color32;
use rraw_color::DisplayTransform;

use crate::app::RrawApp;
use crate::state::*;

impl RrawApp {
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

        ui.horizontal(|ui| {
            ui.strong(section.title());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !is_last && ui.small_button("▼").on_hover_text("Move down").clicked() {
                    self.move_section(idx, false);
                }
                if !is_first && ui.small_button("▲").on_hover_text("Move up").clicked() {
                    self.move_section(idx, true);
                }
            });
        });

        match section {
            DarkroomSection::ExposureTone => {
                ui.label("Exposure (EV)");
                let r1 = ui.add(
                    egui::Slider::new(&mut self.grading.exposure_ev, -5.0..=5.0).show_value(true),
                );
                if r1.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Contrast");
                let r2 = ui.add(
                    egui::Slider::new(&mut self.grading.contrast, -100.0..=100.0).show_value(true),
                );
                if r2.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Highlights");
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

                ui.label("Shadows");
                let r4 = ui.add(
                    egui::Slider::new(&mut self.grading.shadows, -100.0..=100.0).show_value(true),
                );
                if r4.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Noirs / Blacks");
                let r5 = ui.add(
                    egui::Slider::new(&mut self.grading.blacks, -100.0..=100.0).show_value(true),
                );
                if r5.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button("Reset Tones").clicked() {
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
                ui.label("Temperature (Kelvin)");
                let r_k = ui.add(
                    egui::Slider::new(&mut self.grading.kelvin, 2000.0..=12000.0).show_value(true),
                );
                if r_k.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Tint");
                let r_t = ui
                    .add(egui::Slider::new(&mut self.grading.tint, -50.0..=50.0).show_value(true));
                if r_t.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Vibrance");
                let r_vib = ui.add(
                    egui::Slider::new(&mut self.grading.vibrance, -100.0..=100.0).show_value(true),
                );
                if r_vib.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Saturation");
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

                ui.horizontal(|ui| {
                    ui.label("Transform:");
                    egui::ComboBox::from_id_salt("drt_select")
                        .selected_text(format!("{:?}", self.grading.display_transform))
                        .show_ui(ui, |ui| {
                            let c1 = ui.selectable_value(
                                &mut self.grading.display_transform,
                                DisplayTransform::AgX,
                                "AgX (Default)",
                            );
                            let c2 = ui.selectable_value(
                                &mut self.grading.display_transform,
                                DisplayTransform::OpenDrt,
                                "OpenDRT",
                            );
                            if c1.changed() || c2.changed() {
                                self.grading.preset = GradingPreset::Custom;
                                self.current_preset = GradingPreset::Custom;
                                self.needs_preview_update = true;
                                ui.ctx().request_repaint();
                            }
                        });
                });

                let c_mono = ui.checkbox(&mut self.grading.is_monochrome, "Monochrome (B&W)");
                if c_mono.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button("Reset Color").clicked() {
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
                ui.label(
                    egui::RichText::new(
                        "Segmentation sémantique IA CoreML (Sujet, Ciel, Arrière-plan, Portrait)",
                    )
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
                        let label = SemanticMaskLayer::category_label(cat);
                        let btn_text = if exists {
                            egui::RichText::new(format!("☑ {label}"))
                                .strong()
                                .color(Color32::from_rgb(110, 225, 145))
                        } else {
                            egui::RichText::new(format!("☐ {label}"))
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
                let mut changed = false;
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
                                        egui::RichText::new(SemanticMaskLayer::category_label(
                                            layer.category,
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
                                    .on_hover_text("Afficher l'overlay sémantique rouge/magenta")
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
                                    .on_hover_text("Inverser la carte de probabilité (1 - p)")
                                    .changed()
                                {
                                    changed = true;
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("✖")
                                            .on_hover_text("Supprimer ce masque IA")
                                            .clicked()
                                        {
                                            category_to_remove = Some(layer.category);
                                        }
                                    },
                                );
                            });

                            if layer.enabled {
                                ui.horizontal(|ui| {
                                    ui.label("Exposition (EV)");
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
                                    ui.label("Contraste");
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
                                    ui.label("Saturation");
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
                                    ui.label("Température");
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
                                    ui.label("Contour (Feather)");
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut layer.feather, 0.02..=0.50)
                                                .show_value(true),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                    ui.label("Opacité");
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
                    && ui.button("↺ Réinitialiser Masques IA").clicked()
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
            DarkroomSection::ToneCurves => {
                self.render_tone_curves_section(ui);
            }
            DarkroomSection::Crop => {
                let c_crop = ui.checkbox(&mut self.grading.crop_enabled, "Activer le recadrage");
                if c_crop.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if self.grading.crop_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Format / Ratio :");
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
                                    egui::RichText::new("✅ Valider le recadrage")
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
                                egui::RichText::new("✂️ Ajuster le recadrage (Souris)").strong(),
                            )
                            .clicked()
                        {
                            self.is_crop_editing = true;
                            ui.ctx().request_repaint();
                        }

                        if ui.button("↺ Réinitialiser").clicked() {
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
                        "Afficher la grille des tiers",
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
                ui.horizontal(|ui| {
                    ui.label("Rotation:");
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
                    if ui.button("↺ 0°").clicked() {
                        self.grading.rotation_degrees = 0.0;
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                });

                let horizon_btn_text = if self.horizon_tool_active {
                    "📍 Horizon actif (Tracez sur la photo)"
                } else {
                    "📐 Tracer une ligne d'horizon"
                };
                let horizon_btn = ui.selectable_label(self.horizon_tool_active, horizon_btn_text);
                if horizon_btn.clicked() {
                    self.horizon_tool_active = !self.horizon_tool_active;
                    self.horizon_start_point = None;
                    if self.horizon_tool_active {
                        self.status_message =
                            "Glissez sur la photo pour aligner l'horizon".to_string();
                    }
                }

                ui.horizontal(|ui| {
                    if ui.button("↺ 90° CCW").clicked() {
                        self.grading.rotation_degrees =
                            (self.grading.rotation_degrees - 90.0).clamp(-45.0, 45.0);
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                    if ui.button("↻ 90° CW").clicked() {
                        self.grading.rotation_degrees =
                            (self.grading.rotation_degrees + 90.0).clamp(-45.0, 45.0);
                        self.grading.preset = GradingPreset::Custom;
                        self.current_preset = GradingPreset::Custom;
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }
                    let c_fx = ui.checkbox(&mut self.grading.flip_horizontal, "↔ Miroir H");
                    let c_fy = ui.checkbox(&mut self.grading.flip_vertical, "↕ Miroir V");
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
                    ui.checkbox(&mut self.grading.geometry_enabled, "Activer la géométrie");
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

                    ui.label("Keystone Vertical (Perspective verticale)");
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

                    ui.label("Keystone Horizontal (Perspective horizontale)");
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
                        "Recadrage automatique anti-bords",
                    );
                    if c_ac.changed() {
                        self.needs_preview_update = true;
                        ui.ctx().request_repaint();
                    }

                    if ui.button("Réinitialiser Géométrie").clicked() {
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
                ui.label("Wavelet Denoise (σ)");
                let r_w = ui.add(
                    egui::Slider::new(&mut self.grading.wavelet_denoise_sigma, 0.0..=5.0)
                        .show_value(true),
                );
                if r_w.changed() {
                    self.grading.preset = GradingPreset::Custom;
                    self.current_preset = GradingPreset::Custom;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Richardson-Lucy Sharpening");
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
            }
            DarkroomSection::OpticalCorrections => {
                let c_dist = ui.checkbox(
                    &mut self.grading.distortion_correction,
                    "Geometric Distortion (Lensfun/MakerNote)",
                );
                if c_dist.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Manual Distortion");
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
                    "Lateral Chromatic Aberration",
                );
                if c_ca.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.separator();

                let c_vig = ui.checkbox(&mut self.grading.vignette_correction, "Radial Vignetting");
                if c_vig.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                ui.label("Vignette Correction Strength");
                let r_va = ui.add(
                    egui::Slider::new(&mut self.grading.vignette_amount, 0.0..=100.0)
                        .show_value(true),
                );
                if r_va.changed() {
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }

                if ui.button("Reset Optics").clicked() {
                    self.grading.distortion_correction = true;
                    self.grading.distortion_amount = 0.0;
                    self.grading.vignette_correction = true;
                    self.grading.vignette_amount = 50.0;
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            DarkroomSection::LegacyOther => {}
        }

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

        ui.separator();
    }
}
