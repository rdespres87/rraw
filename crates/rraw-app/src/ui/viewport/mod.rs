#![forbid(unsafe_code)]

//! Central interactive GPU image viewport with zoom, pan, cropping, horizon, and perspective overlays.

pub mod brush_overlay;
pub mod crop_overlay;
pub mod hud;
pub mod perspective_tools;
pub mod split_compare;

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::state::*;
use crate::ui::sections::geometry::ParallelLinesToolState;
impl RrawApp {
    /// Renders the central viewport with interactive pan, zoom, and vector overlay tools.
    pub fn render_central_viewport(&mut self, ui: &mut egui::Ui) {
        // 6. Central Viewport Area with Interactive Zoom & Pan (Clean & Unobstructed)
        if self.session.needs_preview_update {
            self.update_preview_textures(ui.ctx());
        }

        egui::CentralPanel::default()
    .frame(
        egui::Frame::central_panel(ui.style()).fill(egui::Color32::from_rgb(16, 17, 20)),
    )
    .show(ui, |ui| {
        let available_rect = ui.available_rect_before_wrap();
        let painter = ui.painter().with_clip_rect(available_rect);
        let mut split_divider_rect: Option<egui::Rect> = None;

        if let Some(texture) = &self.session.preview_texture {
            let margin = 16.0;
            let max_w = (available_rect.width() - margin * 2.0).max(10.0);
            let max_h = (available_rect.height() - margin * 2.0).max(10.0);

            let img_size = texture.size_vec2();
            let is_cropped_view = self.session.grading.geometry.crop_enabled && !self.viewport.is_crop_editing;
            let crop_w = (self.session.grading.geometry.crop_right - self.session.grading.geometry.crop_left).clamp(0.05, 1.0);
            let crop_h = (self.session.grading.geometry.crop_bottom - self.session.grading.geometry.crop_top).clamp(0.05, 1.0);
            let effective_img_size = if is_cropped_view {
                egui::vec2(img_size.x * crop_w, img_size.y * crop_h)
            } else {
                img_size
            };

            let fit_scale = (max_w / effective_img_size.x).min(max_h / effective_img_size.y);
            self.viewport.last_fit_scale = fit_scale;

            let current_scale = if self.viewport.zoom_level <= 0.0 {
                fit_scale
            } else {
                self.viewport.zoom_level
            };

            let display_size = effective_img_size * current_scale;
            let center = available_rect.center() + self.viewport.pan_offset;
            let preview_rect = egui::Rect::from_center_size(center, display_size);

            let resp = ui.allocate_rect(available_rect, egui::Sense::click_and_drag());
            let is_brush_active = self.session.grading.active_brush_mask_idx.is_some_and(|idx| {
                self.session.grading.local_masks.get(idx).is_some_and(|m| {
                    matches!(m.kind, LocalMaskKind::FreehandBrush { .. }) && m.enabled
                })
            });
            let is_tool_active = self.viewport.horizon_tool_active
                || self.viewport.parallel_tool_phase != ParallelToolPhase::Inactive
                || is_brush_active;

            if is_brush_active {
                if ui.input(|i| i.key_pressed(egui::Key::OpenBracket)) {
                    if let Some(mask_idx) = self.session.grading.active_brush_mask_idx {
                        if let Some(mask) = self.session.grading.local_masks.get_mut(mask_idx) {
                            if let LocalMaskKind::FreehandBrush { brush_radius, .. } =
                                &mut mask.kind
                            {
                                let step = if *brush_radius > 0.05 {
                                    0.01
                                } else if *brush_radius > 0.01 {
                                    0.002
                                } else {
                                    0.001
                                };
                                *brush_radius = (*brush_radius - step).max(0.001);
                                ui.ctx().request_repaint();
                            }
                        }
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::CloseBracket)) {
                    if let Some(mask_idx) = self.session.grading.active_brush_mask_idx {
                        if let Some(mask) = self.session.grading.local_masks.get_mut(mask_idx) {
                            if let LocalMaskKind::FreehandBrush { brush_radius, .. } =
                                &mut mask.kind
                            {
                                let step = if *brush_radius >= 0.05 {
                                    0.01
                                } else if *brush_radius >= 0.01 {
                                    0.002
                                } else {
                                    0.001
                                };
                                *brush_radius = (*brush_radius + step).min(0.5);
                                ui.ctx().request_repaint();
                            }
                        }
                    }
                }
            }

            let is_holding_space = ui.input(|i| i.key_down(egui::Key::Space));
            let is_middle_drag = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
            let is_panning_override =
                (is_holding_space || is_middle_drag) && self.viewport.zoom_level > 0.0;

            // Clamping pan offset to ensure image never moves out of viewport
            if self.viewport.zoom_level <= 0.0 {
                self.viewport.pan_offset = egui::Vec2::ZERO;
            } else {
                let max_pan_x =
                    ((display_size.x - available_rect.width() + margin * 2.0) * 0.5)
                        .max(0.0);
                let max_pan_y =
                    ((display_size.y - available_rect.height() + margin * 2.0) * 0.5)
                        .max(0.0);
                self.viewport.pan_offset.x = self.viewport.pan_offset.x.clamp(-max_pan_x, max_pan_x);
                self.viewport.pan_offset.y = self.viewport.pan_offset.y.clamp(-max_pan_y, max_pan_y);
            }

            if resp.dragged()
                && (!is_tool_active || is_panning_override)
                && self.viewport.crop_drag_handle == CropDragHandle::None
                && self.viewport.zoom_level > 0.0
            {
                self.viewport.pan_offset += resp.drag_delta();
                let max_pan_x =
                    ((display_size.x - available_rect.width() + margin * 2.0) * 0.5)
                        .max(0.0);
                let max_pan_y =
                    ((display_size.y - available_rect.height() + margin * 2.0) * 0.5)
                        .max(0.0);
                self.viewport.pan_offset.x = self.viewport.pan_offset.x.clamp(-max_pan_x, max_pan_x);
                self.viewport.pan_offset.y = self.viewport.pan_offset.y.clamp(-max_pan_y, max_pan_y);
            }

            if resp.double_clicked() && !self.viewport.is_crop_editing && !is_tool_active {
                if self.viewport.zoom_level <= 0.0 {
                    self.viewport.zoom_level = 1.0;
                    self.viewport.pan_offset = egui::Vec2::ZERO;
                } else {
                    self.viewport.zoom_level = 0.0;
                    self.viewport.pan_offset = egui::Vec2::ZERO;
                }
            }

            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0
                && resp.hovered()
                && (!is_tool_active || is_brush_active)
                && !self.viewport.is_crop_editing
            {
                let zoom_factor = (scroll * 0.003).exp();
                let min_scale = fit_scale;
                let max_scale = (fit_scale * 20.0).max(10.0);
                let new_scale = (current_scale * zoom_factor).clamp(min_scale, max_scale);

                if (new_scale - fit_scale).abs() / fit_scale < 0.015
                    || new_scale <= fit_scale
                {
                    self.viewport.zoom_level = 0.0;
                    self.viewport.pan_offset = egui::Vec2::ZERO;
                } else {
                    if let Some(mouse_pos) = resp.hover_pos() {
                        let old_offset = mouse_pos - center;
                        let scale_change = new_scale / current_scale;
                        self.viewport.pan_offset -= old_offset * (scale_change - 1.0);
                    }
                    self.viewport.zoom_level = new_scale;
                }
            }

            // Draw image with background frame (paper-white pass-partout frame when Soft Proofing paper simulation is active)
            if self.session.grading.soft_proof.enabled && self.session.grading.soft_proof.simulate_paper_and_ink {
                let paper_col = match self.session.grading.soft_proof.profile {
                    rraw_color::SoftProofProfile::FineArtMattePaper => {
                        egui::Color32::from_rgb(244, 240, 230)
                    }
                    rraw_color::SoftProofProfile::GlossyBarytaPaper => {
                        egui::Color32::from_rgb(249, 247, 243)
                    }
                    _ => egui::Color32::from_rgb(248, 248, 246),
                };
                painter.rect_filled(preview_rect.expand(10.0), 3.0, paper_col);
            } else {
                painter.rect_filled(
                    preview_rect.expand(2.0),
                    4.0,
                    egui::Color32::from_rgb(10, 10, 12),
                );
            }

            let uv = if is_cropped_view {
                egui::Rect::from_min_max(
                    egui::pos2(self.session.grading.geometry.crop_left, self.session.grading.geometry.crop_top),
                    egui::pos2(self.session.grading.geometry.crop_right, self.session.grading.geometry.crop_bottom),
                )
            } else {
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
            };

            let is_holding_right_click = ui.input(|i| {
                i.pointer.button_down(egui::PointerButton::Secondary)
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| available_rect.contains(p))
            });

            let is_holding_shortcut_key = !ui.ctx().egui_wants_keyboard_input()
                && ui.input(|i| {
                    i.key_down(egui::Key::P)
                        || i.key_down(egui::Key::Backslash)
                        || (i.key_down(egui::Key::D) && !i.modifiers.command && !i.modifiers.ctrl)
                });

            if ui.input(|i| i.key_pressed(egui::Key::O)) {
                if self.session.grading.show_local_mask_overlay.is_some() {
                    self.session.grading.show_local_mask_overlay = None;
                    self.session.needs_preview_update = true;
                    ui.ctx().request_repaint();
                } else if !self.session.grading.local_masks.is_empty() {
                    self.session.grading.show_local_mask_overlay = Some(0);
                    self.session.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }

            let is_holding_left_fit = !self.viewport.split_compare
                && !self.export.is_any_dialog_open()
                && !ui.ctx().egui_wants_pointer_input()
                && ui.input(|i| {
                    i.pointer.button_down(egui::PointerButton::Primary)
                        && i.pointer
                            .interact_pos()
                            .is_some_and(|p| preview_rect.contains(p))
                })
                && !is_tool_active
                && self.viewport.crop_drag_handle == CropDragHandle::None
                && self.viewport.zoom_level <= 0.0;

            let show_original = self.session.is_comparing_original
                || is_holding_left_fit
                || is_holding_right_click
                || is_holding_shortcut_key;

            let active_texture = if show_original {
                self.session.original_texture.as_ref().unwrap_or(texture)
            } else {
                texture
            };

            match (&self.session.original_texture, self.viewport.split_compare && !show_original) {
                (Some(orig), true) => {
                    let (before, after) =
                        crate::view_tools::split_comparison(preview_rect, uv, self.viewport.split_position);
                    if before.screen.width() >= 1.0 {
                        painter.image(orig.id(), before.screen, before.uv, egui::Color32::WHITE);
                    }
                    if after.screen.width() >= 1.0 {
                        painter.image(texture.id(), after.screen, after.uv, egui::Color32::WHITE);
                    }
                }
                _ => {
                    painter.image(
                        active_texture.id(),
                        preview_rect,
                        uv,
                        egui::Color32::WHITE,
                    );
                }
            }
            painter.rect_stroke(
                preview_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 65, 75)),
                egui::StrokeKind::Outside,
            );

            let cur_lang = self.lang();

            // Live Original HUD Badge
            if show_original {
                hud::render_original_badge(&painter, available_rect, self.t(Key::ViewportOriginalBadge));
            }

            // Live Soft Proofing HUD Badge
            if self.session.grading.soft_proof.enabled || self.session.grading.soft_proof.show_gamut_warning {
                hud::render_soft_proofing_hud(
                    &painter,
                    available_rect,
                    &self.session.grading.soft_proof,
                    self.viewport.histogram.out_of_gamut_count,
                    cur_lang,
                );
            }

            // Active Local Mask Overlay HUD Badge
            if let Some(mask_idx) = self.session.grading.show_local_mask_overlay {
                let default_mask_name = if cur_lang == crate::i18n::Language::Fr {
                    "Masque"
                } else {
                    "Mask"
                };
                let mask_name = self
                    .session
                    .grading
                    .local_masks
                    .get(mask_idx)
                    .map_or(default_mask_name, |m| m.name.as_str());
                hud::render_local_mask_hud(&painter, available_rect, mask_name, cur_lang);
            }

            let brush_ctx = brush_overlay::BrushViewportContext {
                available_rect,
                preview_rect,
                is_panning_override,
                is_cropped_view,
            };

            // Interactive freehand brush painting / erasing
            if is_brush_active {
                let (needs_upd, save_sidecar) = brush_overlay::handle_brush_painting(
                    ui,
                    &resp,
                    &brush_ctx,
                    &self.session.grading.geometry,
                    &mut self.session.grading.local_masks,
                    self.session.grading.active_brush_mask_idx,
                );
                if needs_upd {
                    self.session.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
                if save_sidecar {
                    self.save_current_photo_sidecar();
                }

                // Active Brush HUD Badge
                if let Some(mask_idx) = self.session.grading.active_brush_mask_idx {
                    if let Some(mask) = self.session.grading.local_masks.get(mask_idx) {
                        let has_mask_overlay = self.session.grading.show_local_mask_overlay.is_some();
                        hud::render_brush_hud(
                            &painter,
                            available_rect,
                            mask,
                            has_mask_overlay,
                            self.viewport.zoom_level,
                            cur_lang,
                        );
                    }
                }
            }

            // Interactive mouse cropping
            if self.session.grading.geometry.crop_enabled && self.viewport.is_crop_editing {
                let mut crop_state = crop_overlay::CropToolState {
                    geometry: &mut self.session.grading.geometry,
                    is_crop_editing: &mut self.viewport.is_crop_editing,
                    crop_drag_handle: &mut self.viewport.crop_drag_handle,
                };
                let crop_out = crop_overlay::handle_crop_overlay(
                    ui,
                    &resp,
                    &painter,
                    available_rect,
                    preview_rect,
                    &mut crop_state,
                    cur_lang,
                );
                if crop_out.needs_preview_update {
                    self.session.needs_preview_update = true;
                }
                if crop_out.save_sidecar {
                    self.save_current_photo_sidecar();
                }
            }

            // 2-parallel-lines perspective correction tool interaction
            if self.viewport.parallel_tool_phase != ParallelToolPhase::Inactive {
                let mut tool_state = ParallelLinesToolState {
                    phase: &mut self.viewport.parallel_tool_phase,
                    line1: &mut self.viewport.parallel_line1,
                    line2: &mut self.viewport.parallel_line2,
                    drag_start: &mut self.viewport.parallel_drag_start,
                    status_message: &mut self.status_message,
                };
                let p_out = perspective_tools::handle_parallel_lines_tool(
                    ui,
                    &resp,
                    &painter,
                    available_rect,
                    &mut tool_state,
                    &mut self.session.grading.geometry,
                    cur_lang,
                );
                if p_out.mark_custom_preset {
                    self.session.grading.preset = GradingPreset::Custom;
                    self.session.current_preset = GradingPreset::Custom;
                }
                if p_out.needs_preview_update {
                    self.session.needs_preview_update = true;
                }
                if p_out.save_sidecar {
                    self.save_current_photo_sidecar();
                }
            }

            // Leveling horizon line interaction
            if self.viewport.horizon_tool_active {
                let h_out = perspective_tools::handle_horizon_tool(
                    ui,
                    &resp,
                    &painter,
                    &mut self.viewport.horizon_tool_active,
                    &mut self.viewport.horizon_start_point,
                    &mut self.session.grading.geometry,
                );
                if h_out.mark_custom_preset {
                    self.session.grading.preset = GradingPreset::Custom;
                    self.session.current_preset = GradingPreset::Custom;
                }
                if h_out.needs_preview_update {
                    self.session.needs_preview_update = true;
                }
                if h_out.save_sidecar {
                    self.save_current_photo_sidecar();
                }
            }

            // Brush cursor ring indicator preview
            if is_brush_active {
                brush_overlay::render_brush_cursor_ring(
                    ui,
                    &resp,
                    &painter,
                    &brush_ctx,
                    &self.session.grading.local_masks,
                    self.session.grading.active_brush_mask_idx,
                );
            }
            if self.viewport.split_compare && !show_original && self.session.original_texture.is_some() {
                split_divider_rect = Some(preview_rect);
            }
        } else if self.workers.is_decoding {
            let cur_lang = self.lang();
            let is_es = cur_lang.is_spanish();
            let msg = if cur_lang == crate::i18n::Language::Fr {
                "Décodage du fichier RAW en arrière-plan..."
            } else if is_es {
                "Decodificando archivo RAW en segundo plano..."
            } else {
                "Decoding RAW file in background..."
            };
            painter.text(
                available_rect.center(),
                egui::Align2::CENTER_CENTER,
                msg,
                egui::FontId::proportional(18.0),
                egui::Color32::from_rgb(100, 180, 255),
            );
        } else {
            let cur_lang = self.lang();
            let is_es = cur_lang.is_spanish();
            let msg = if cur_lang == crate::i18n::Language::Fr {
                "Aucune image RAW chargée.\nCliquez sur « Ouvrir image... » ou choisissez un fichier dans l'explorateur."
            } else if is_es {
                "Ninguna imagen RAW cargada.\nHaga clic en « Abrir imagen... » o elija un archivo en el explorador."
            } else {
                "No RAW image loaded.\nClick \"Open image...\" or select a file in the explorer."
            };
            painter.text(
                available_rect.center(),
                egui::Align2::CENTER_CENTER,
                msg,
                egui::FontId::proportional(16.0),
                egui::Color32::from_rgb(140, 145, 155),
            );
        }

        if let Some(image_rect) = split_divider_rect {
            self.render_split_divider(ui, image_rect);
        }
        if let Some(rating) = self.render_canvas_hud(ui, available_rect) {
            self.set_current_photo_rating(rating);
        }
    });
    }

    /// Paints the draggable Before/After divider and updates `split_position` while dragging.
    fn render_split_divider(&mut self, ui: &egui::Ui, image_rect: egui::Rect) {
        let label_before = self.t(Key::ViewportBefore);
        let label_after = self.t(Key::ViewportAfter);
        split_compare::render_split_divider(
            ui,
            image_rect,
            &mut self.viewport.split_position,
            label_before,
            label_after,
        );
    }
}
