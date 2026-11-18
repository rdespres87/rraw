#![forbid(unsafe_code)]

//! Central interactive GPU image viewport with zoom, pan, cropping, horizon, and perspective overlays.

use egui::Color32;

use crate::app::RrawApp;
use crate::state::*;

impl RrawApp {
    /// Renders the central viewport with interactive pan, zoom, and vector overlay tools.
    pub fn render_central_viewport(&mut self, ui: &mut egui::Ui) {
        // 6. Central Viewport Area with Interactive Zoom & Pan (Clean & Unobstructed)
        if self.needs_preview_update {
            self.update_preview_textures(ui.ctx());
        }

        egui::CentralPanel::default()
    .frame(
        egui::Frame::central_panel(ui.style()).fill(egui::Color32::from_rgb(16, 17, 20)),
    )
    .show(ui, |ui| {
        let available_rect = ui.available_rect_before_wrap();
        let painter = ui.painter().with_clip_rect(available_rect);

        if let Some(texture) = &self.preview_texture {
            let margin = 16.0;
            let max_w = (available_rect.width() - margin * 2.0).max(10.0);
            let max_h = (available_rect.height() - margin * 2.0).max(10.0);

            let img_size = texture.size_vec2();
            let is_cropped_view = self.grading.crop_enabled && !self.is_crop_editing;
            let crop_w = (self.grading.crop_right - self.grading.crop_left).clamp(0.05, 1.0);
            let crop_h = (self.grading.crop_bottom - self.grading.crop_top).clamp(0.05, 1.0);
            let effective_img_size = if is_cropped_view {
                egui::vec2(img_size.x * crop_w, img_size.y * crop_h)
            } else {
                img_size
            };

            let fit_scale = (max_w / effective_img_size.x).min(max_h / effective_img_size.y);
            self.last_fit_scale = fit_scale;

            let current_scale = if self.zoom_level <= 0.0 {
                fit_scale
            } else {
                self.zoom_level
            };

            let display_size = effective_img_size * current_scale;
            let center = available_rect.center() + self.pan_offset;
            let preview_rect = egui::Rect::from_center_size(center, display_size);

            let resp = ui.allocate_rect(available_rect, egui::Sense::click_and_drag());
            let is_tool_active = self.horizon_tool_active
                || self.parallel_tool_phase != ParallelToolPhase::Inactive;

            // Clamping pan offset to ensure image never moves out of viewport
            if self.zoom_level <= 0.0 {
                self.pan_offset = egui::Vec2::ZERO;
            } else {
                let max_pan_x =
                    ((display_size.x - available_rect.width() + margin * 2.0) * 0.5)
                        .max(0.0);
                let max_pan_y =
                    ((display_size.y - available_rect.height() + margin * 2.0) * 0.5)
                        .max(0.0);
                self.pan_offset.x = self.pan_offset.x.clamp(-max_pan_x, max_pan_x);
                self.pan_offset.y = self.pan_offset.y.clamp(-max_pan_y, max_pan_y);
            }

            if resp.dragged()
                && !is_tool_active
                && self.crop_drag_handle == CropDragHandle::None
                && self.zoom_level > 0.0
            {
                self.pan_offset += resp.drag_delta();
                let max_pan_x =
                    ((display_size.x - available_rect.width() + margin * 2.0) * 0.5)
                        .max(0.0);
                let max_pan_y =
                    ((display_size.y - available_rect.height() + margin * 2.0) * 0.5)
                        .max(0.0);
                self.pan_offset.x = self.pan_offset.x.clamp(-max_pan_x, max_pan_x);
                self.pan_offset.y = self.pan_offset.y.clamp(-max_pan_y, max_pan_y);
            }

            if resp.double_clicked() && !self.is_crop_editing {
                if self.zoom_level <= 0.0 {
                    self.zoom_level = 1.0;
                    self.pan_offset = egui::Vec2::ZERO;
                } else {
                    self.zoom_level = 0.0;
                    self.pan_offset = egui::Vec2::ZERO;
                }
            }

            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 && resp.hovered() && !is_tool_active && !self.is_crop_editing {
                let zoom_factor = (scroll * 0.003).exp();
                let min_scale = fit_scale;
                let max_scale = (fit_scale * 20.0).max(10.0);
                let new_scale = (current_scale * zoom_factor).clamp(min_scale, max_scale);

                if (new_scale - fit_scale).abs() / fit_scale < 0.015
                    || new_scale <= fit_scale
                {
                    self.zoom_level = 0.0;
                    self.pan_offset = egui::Vec2::ZERO;
                } else {
                    if let Some(mouse_pos) = resp.hover_pos() {
                        let old_offset = mouse_pos - center;
                        let scale_change = new_scale / current_scale;
                        self.pan_offset -= old_offset * (scale_change - 1.0);
                    }
                    self.zoom_level = new_scale;
                }
            }

            // Draw image with dark background frame and subtle shadow
            painter.rect_filled(
                preview_rect.expand(2.0),
                4.0,
                egui::Color32::from_rgb(10, 10, 12),
            );

            let uv = if is_cropped_view {
                egui::Rect::from_min_max(
                    egui::pos2(self.grading.crop_left, self.grading.crop_top),
                    egui::pos2(self.grading.crop_right, self.grading.crop_bottom),
                )
            } else {
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
            };

            painter.image(
                texture.id(),
                preview_rect,
                uv,
                egui::Color32::WHITE,
            );
            painter.rect_stroke(
                preview_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 65, 75)),
                egui::StrokeKind::Outside,
            );

            // Interactive mouse cropping with locked aspect ratio and dimmed mask
            if self.grading.crop_enabled && self.is_crop_editing {
                let pw = preview_rect.width();
                let ph = preview_rect.height();

                let crop_screen_min = egui::pos2(
                    preview_rect.min.x + self.grading.crop_left * pw,
                    preview_rect.min.y + self.grading.crop_top * ph,
                );
                let crop_screen_max = egui::pos2(
                    preview_rect.min.x + self.grading.crop_right * pw,
                    preview_rect.min.y + self.grading.crop_bottom * ph,
                );
                let crop_screen_rect =
                    egui::Rect::from_min_max(crop_screen_min, crop_screen_max);

                // 1. Dimmed outer mask outside crop box
                let mask_color = Color32::from_rgba_unmultiplied(0, 0, 0, 170);
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        preview_rect.min,
                        egui::pos2(preview_rect.max.x, crop_screen_rect.min.y),
                    ),
                    0.0,
                    mask_color,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(preview_rect.min.x, crop_screen_rect.max.y),
                        preview_rect.max,
                    ),
                    0.0,
                    mask_color,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(preview_rect.min.x, crop_screen_rect.min.y),
                        egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
                    ),
                    0.0,
                    mask_color,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
                        egui::pos2(preview_rect.max.x, crop_screen_rect.max.y),
                    ),
                    0.0,
                    mask_color,
                );

                // 2. Bright border around crop box
                painter.rect_stroke(
                    crop_screen_rect,
                    0.0,
                    egui::Stroke::new(1.5, Color32::WHITE),
                    egui::StrokeKind::Outside,
                );

                // 3. Rule-of-thirds composition grid
                if self.grading.crop_show_grid {
                    let grid_stroke = egui::Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 110),
                    );
                    let cw = crop_screen_rect.width();
                    let ch = crop_screen_rect.height();
                    painter.line_segment(
                        [
                            egui::pos2(crop_screen_rect.min.x + cw / 3.0, crop_screen_rect.min.y),
                            egui::pos2(crop_screen_rect.min.x + cw / 3.0, crop_screen_rect.max.y),
                        ],
                        grid_stroke,
                    );
                    painter.line_segment(
                        [
                            egui::pos2(crop_screen_rect.min.x + 2.0 * cw / 3.0, crop_screen_rect.min.y),
                            egui::pos2(crop_screen_rect.min.x + 2.0 * cw / 3.0, crop_screen_rect.max.y),
                        ],
                        grid_stroke,
                    );
                    painter.line_segment(
                        [
                            egui::pos2(crop_screen_rect.min.x, crop_screen_rect.min.y + ch / 3.0),
                            egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y + ch / 3.0),
                        ],
                        grid_stroke,
                    );
                    painter.line_segment(
                        [
                            egui::pos2(crop_screen_rect.min.x, crop_screen_rect.min.y + 2.0 * ch / 3.0),
                            egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y + 2.0 * ch / 3.0),
                        ],
                        grid_stroke,
                    );
                }

                // 4. Corner L-brackets & Edge handles
                let bracket_len = 16.0;
                let bracket_stroke = egui::Stroke::new(3.0, Color32::WHITE);
                painter.line_segment(
                    [
                        crop_screen_rect.min,
                        egui::pos2(crop_screen_rect.min.x + bracket_len, crop_screen_rect.min.y),
                    ],
                    bracket_stroke,
                );
                painter.line_segment(
                    [
                        crop_screen_rect.min,
                        egui::pos2(crop_screen_rect.min.x, crop_screen_rect.min.y + bracket_len),
                    ],
                    bracket_stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
                        egui::pos2(crop_screen_rect.max.x - bracket_len, crop_screen_rect.min.y),
                    ],
                    bracket_stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y),
                        egui::pos2(crop_screen_rect.max.x, crop_screen_rect.min.y + bracket_len),
                    ],
                    bracket_stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
                        egui::pos2(crop_screen_rect.min.x + bracket_len, crop_screen_rect.max.y),
                    ],
                    bracket_stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y),
                        egui::pos2(crop_screen_rect.min.x, crop_screen_rect.max.y - bracket_len),
                    ],
                    bracket_stroke,
                );
                painter.line_segment(
                    [
                        crop_screen_rect.max,
                        egui::pos2(crop_screen_rect.max.x - bracket_len, crop_screen_rect.max.y),
                    ],
                    bracket_stroke,
                );
                painter.line_segment(
                    [
                        crop_screen_rect.max,
                        egui::pos2(crop_screen_rect.max.x, crop_screen_rect.max.y - bracket_len),
                    ],
                    bracket_stroke,
                );

                // 5. Mouse Hit-Testing & Interactive Resizing
                if let Some(mouse_pos) = resp.hover_pos() {
                    let hit_dist = 14.0;
                    let mut hovered_handle = CropDragHandle::None;

                    if (mouse_pos - crop_screen_rect.left_top()).length() <= hit_dist {
                        hovered_handle = CropDragHandle::TopLeft;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                    } else if (mouse_pos - crop_screen_rect.right_top()).length() <= hit_dist {
                        hovered_handle = CropDragHandle::TopRight;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNeSw);
                    } else if (mouse_pos - crop_screen_rect.left_bottom()).length() <= hit_dist {
                        hovered_handle = CropDragHandle::BottomLeft;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNeSw);
                    } else if (mouse_pos - crop_screen_rect.right_bottom()).length() <= hit_dist {
                        hovered_handle = CropDragHandle::BottomRight;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                    } else if (mouse_pos.y - crop_screen_rect.top()).abs() <= hit_dist
                        && mouse_pos.x >= crop_screen_rect.left()
                        && mouse_pos.x <= crop_screen_rect.right()
                    {
                        hovered_handle = CropDragHandle::Top;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    } else if (mouse_pos.y - crop_screen_rect.bottom()).abs() <= hit_dist
                        && mouse_pos.x >= crop_screen_rect.left()
                        && mouse_pos.x <= crop_screen_rect.right()
                    {
                        hovered_handle = CropDragHandle::Bottom;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    } else if (mouse_pos.x - crop_screen_rect.left()).abs() <= hit_dist
                        && mouse_pos.y >= crop_screen_rect.top()
                        && mouse_pos.y <= crop_screen_rect.bottom()
                    {
                        hovered_handle = CropDragHandle::Left;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    } else if (mouse_pos.x - crop_screen_rect.right()).abs() <= hit_dist
                        && mouse_pos.y >= crop_screen_rect.top()
                        && mouse_pos.y <= crop_screen_rect.bottom()
                    {
                        hovered_handle = CropDragHandle::Right;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    } else if crop_screen_rect.contains(mouse_pos) {
                        hovered_handle = CropDragHandle::Center;
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }

                    if resp.drag_started() && hovered_handle != CropDragHandle::None {
                        self.crop_drag_handle = hovered_handle;
                    }
                }

                if resp.dragged() && self.crop_drag_handle != CropDragHandle::None {
                    let delta = resp.drag_delta();
                    let dx = delta.x / pw;
                    let dy = delta.y / ph;
                    let img_aspect = pw / ph.max(1.0);
                    let target_aspect = self.grading.crop_aspect.ratio(pw, ph);

                    match self.crop_drag_handle {
                        CropDragHandle::Center => {
                            let w = self.grading.crop_right - self.grading.crop_left;
                            let h = self.grading.crop_bottom - self.grading.crop_top;
                            let new_left = (self.grading.crop_left + dx).clamp(0.0, 1.0 - w);
                            let new_top = (self.grading.crop_top + dy).clamp(0.0, 1.0 - h);
                            self.grading.crop_left = new_left;
                            self.grading.crop_right = new_left + w;
                            self.grading.crop_top = new_top;
                            self.grading.crop_bottom = new_top + h;
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        CropDragHandle::BottomRight => {
                            let new_right = (self.grading.crop_right + dx)
                                .clamp(self.grading.crop_left + 0.05, 1.0);
                            if let Some(ratio) = target_aspect {
                                let w_norm = new_right - self.grading.crop_left;
                                let h_norm = w_norm * (img_aspect / ratio);
                                if self.grading.crop_top + h_norm <= 1.0 {
                                    self.grading.crop_bottom = self.grading.crop_top + h_norm;
                                    self.grading.crop_right = new_right;
                                }
                            } else {
                                self.grading.crop_right = new_right;
                                self.grading.crop_bottom = (self.grading.crop_bottom + dy)
                                    .clamp(self.grading.crop_top + 0.05, 1.0);
                            }
                        }
                        CropDragHandle::BottomLeft => {
                            let new_left = (self.grading.crop_left + dx)
                                .clamp(0.0, self.grading.crop_right - 0.05);
                            if let Some(ratio) = target_aspect {
                                let w_norm = self.grading.crop_right - new_left;
                                let h_norm = w_norm * (img_aspect / ratio);
                                if self.grading.crop_top + h_norm <= 1.0 {
                                    self.grading.crop_bottom = self.grading.crop_top + h_norm;
                                    self.grading.crop_left = new_left;
                                }
                            } else {
                                self.grading.crop_left = new_left;
                                self.grading.crop_bottom = (self.grading.crop_bottom + dy)
                                    .clamp(self.grading.crop_top + 0.05, 1.0);
                            }
                        }
                        CropDragHandle::TopRight => {
                            let new_right = (self.grading.crop_right + dx)
                                .clamp(self.grading.crop_left + 0.05, 1.0);
                            if let Some(ratio) = target_aspect {
                                let w_norm = new_right - self.grading.crop_left;
                                let h_norm = w_norm * (img_aspect / ratio);
                                if self.grading.crop_bottom >= h_norm {
                                    self.grading.crop_top = self.grading.crop_bottom - h_norm;
                                    self.grading.crop_right = new_right;
                                }
                            } else {
                                self.grading.crop_right = new_right;
                                self.grading.crop_top = (self.grading.crop_top + dy)
                                    .clamp(0.0, self.grading.crop_bottom - 0.05);
                            }
                        }
                        CropDragHandle::TopLeft => {
                            let new_left = (self.grading.crop_left + dx)
                                .clamp(0.0, self.grading.crop_right - 0.05);
                            if let Some(ratio) = target_aspect {
                                let w_norm = self.grading.crop_right - new_left;
                                let h_norm = w_norm * (img_aspect / ratio);
                                if self.grading.crop_bottom >= h_norm {
                                    self.grading.crop_top = self.grading.crop_bottom - h_norm;
                                    self.grading.crop_left = new_left;
                                }
                            } else {
                                self.grading.crop_left = new_left;
                                self.grading.crop_top = (self.grading.crop_top + dy)
                                    .clamp(0.0, self.grading.crop_bottom - 0.05);
                            }
                        }
                        CropDragHandle::Right => {
                            self.grading.crop_right = (self.grading.crop_right + dx)
                                .clamp(self.grading.crop_left + 0.05, 1.0);
                            if let Some(ratio) = target_aspect {
                                let w_norm = self.grading.crop_right - self.grading.crop_left;
                                let h_norm = (w_norm * (img_aspect / ratio)).min(1.0);
                                let cy = (self.grading.crop_top + self.grading.crop_bottom) * 0.5;
                                self.grading.crop_top = (cy - h_norm * 0.5).clamp(0.0, 1.0 - h_norm);
                                self.grading.crop_bottom = self.grading.crop_top + h_norm;
                            }
                        }
                        CropDragHandle::Left => {
                            self.grading.crop_left = (self.grading.crop_left + dx)
                                .clamp(0.0, self.grading.crop_right - 0.05);
                            if let Some(ratio) = target_aspect {
                                let w_norm = self.grading.crop_right - self.grading.crop_left;
                                let h_norm = (w_norm * (img_aspect / ratio)).min(1.0);
                                let cy = (self.grading.crop_top + self.grading.crop_bottom) * 0.5;
                                self.grading.crop_top = (cy - h_norm * 0.5).clamp(0.0, 1.0 - h_norm);
                                self.grading.crop_bottom = self.grading.crop_top + h_norm;
                            }
                        }
                        CropDragHandle::Top => {
                            self.grading.crop_top = (self.grading.crop_top + dy)
                                .clamp(0.0, self.grading.crop_bottom - 0.05);
                            if let Some(ratio) = target_aspect {
                                let h_norm = self.grading.crop_bottom - self.grading.crop_top;
                                let w_norm = (h_norm * (ratio / img_aspect)).min(1.0);
                                let cx = (self.grading.crop_left + self.grading.crop_right) * 0.5;
                                self.grading.crop_left = (cx - w_norm * 0.5).clamp(0.0, 1.0 - w_norm);
                                self.grading.crop_right = self.grading.crop_left + w_norm;
                            }
                        }
                        CropDragHandle::Bottom => {
                            self.grading.crop_bottom = (self.grading.crop_bottom + dy)
                                .clamp(self.grading.crop_top + 0.05, 1.0);
                            if let Some(ratio) = target_aspect {
                                let h_norm = self.grading.crop_bottom - self.grading.crop_top;
                                let w_norm = (h_norm * (ratio / img_aspect)).min(1.0);
                                let cx = (self.grading.crop_left + self.grading.crop_right) * 0.5;
                                self.grading.crop_left = (cx - w_norm * 0.5).clamp(0.0, 1.0 - w_norm);
                                self.grading.crop_right = self.grading.crop_left + w_norm;
                            }
                        }
                        CropDragHandle::None => {}
                    }
                    ui.ctx().request_repaint();
                }

                if resp.drag_stopped() {
                    self.crop_drag_handle = CropDragHandle::None;
                    self.save_current_photo_sidecar();
                    ui.ctx().request_repaint();
                }

                // Validate crop on Enter key or double-click inside crop box
                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let dbl_click_crop = resp.double_clicked()
                    && resp
                        .interact_pointer_pos()
                        .is_some_and(|p| crop_screen_rect.contains(p));

                if enter_pressed || dbl_click_crop {
                    self.is_crop_editing = false;
                    self.needs_preview_update = true;
                    self.save_current_photo_sidecar();
                    ui.ctx().request_repaint();
                }

                // Floating validation pill at top of viewport
                let banner_rect = egui::Rect::from_min_size(
                    available_rect.left_top() + egui::vec2(16.0, 16.0),
                    egui::vec2(380.0, 32.0),
                );
                painter.rect_filled(
                    banner_rect,
                    6.0,
                    Color32::from_rgba_unmultiplied(16, 20, 28, 230),
                );
                painter.rect_stroke(
                    banner_rect,
                    6.0,
                    egui::Stroke::new(1.0, Color32::from_rgb(60, 120, 200)),
                    egui::StrokeKind::Outside,
                );
                painter.text(
                    banner_rect.center() - egui::vec2(35.0, 0.0),
                    egui::Align2::CENTER_CENTER,
                    "✂️ Ajustez le cadre (Entrée pour valider)",
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );

                let val_btn_rect = egui::Rect::from_min_size(
                    egui::pos2(banner_rect.max.x - 74.0, banner_rect.min.y + 4.0),
                    egui::vec2(68.0, 24.0),
                );
                let val_btn_id = ui.auto_id_with("crop_validate_btn");
                let val_resp = ui.interact(val_btn_rect, val_btn_id, egui::Sense::click());
                if val_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                painter.rect_filled(
                    val_btn_rect,
                    4.0,
                    if val_resp.hovered() {
                        Color32::from_rgb(45, 150, 80)
                    } else {
                        Color32::from_rgb(35, 125, 65)
                    },
                );
                painter.text(
                    val_btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "✓ Valider",
                    egui::FontId::proportional(11.0),
                    Color32::WHITE,
                );
                if val_resp.clicked() {
                    self.is_crop_editing = false;
                    self.needs_preview_update = true;
                    self.save_current_photo_sidecar();
                    ui.ctx().request_repaint();
                }
            }

            // 2-parallel-lines perspective correction tool interaction
            if self.parallel_tool_phase != ParallelToolPhase::Inactive {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);

                let instruction = match self.parallel_tool_phase {
                    ParallelToolPhase::DrawingLine1 => {
                        "Glissez pour tracer la 1ère référence verticale (Ligne 1)"
                    }
                    ParallelToolPhase::WaitingLine2 => {
                        "Ligne 1 validée. Glissez maintenant pour tracer la 2ème référence (Ligne 2)"
                    }
                    ParallelToolPhase::DrawingLine2 => {
                        "Tracez la 2ème référence verticale..."
                    }
                    _ => "",
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        available_rect.left_top() + egui::vec2(16.0, 16.0),
                        egui::vec2(480.0, 32.0),
                    ),
                    6.0,
                    Color32::from_rgba_unmultiplied(16, 20, 28, 230),
                );
                painter.text(
                    available_rect.left_top() + egui::vec2(28.0, 32.0),
                    egui::Align2::LEFT_CENTER,
                    format!("📐 {instruction}"),
                    egui::FontId::proportional(14.0),
                    Color32::from_rgb(0, 240, 255),
                );

                if let Some((p1, p2)) = self.parallel_line1 {
                    painter.line_segment(
                        [p1, p2],
                        egui::Stroke::new(2.5, Color32::from_rgb(50, 230, 100)),
                    );
                    painter.circle_filled(p1, 5.0, Color32::from_rgb(50, 230, 100));
                    painter.circle_filled(p2, 5.0, Color32::from_rgb(50, 230, 100));
                    painter.text(
                        (p1 + p2.to_vec2()) * 0.5 + egui::vec2(10.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        "Ligne 1",
                        egui::FontId::proportional(13.0),
                        Color32::WHITE,
                    );
                }

                if resp.drag_started() {
                    self.parallel_drag_start = resp.interact_pointer_pos();
                }

                if resp.dragged() {
                    if let (Some(p1), Some(p2)) =
                        (self.parallel_drag_start, resp.interact_pointer_pos())
                    {
                        let stroke_col = match self.parallel_tool_phase {
                            ParallelToolPhase::DrawingLine1 => {
                                Color32::from_rgb(0, 240, 255)
                            }
                            _ => Color32::from_rgb(255, 220, 50),
                        };
                        painter.line_segment([p1, p2], egui::Stroke::new(2.5, stroke_col));
                        painter.circle_filled(p1, 5.0, stroke_col);
                        painter.circle_filled(p2, 5.0, stroke_col);
                    }
                }

                if resp.drag_stopped() {
                    if let (Some(p1), Some(p2)) =
                        (self.parallel_drag_start, resp.interact_pointer_pos())
                    {
                        let dist_sq = (p2.x - p1.x).powi(2) + (p2.y - p1.y).powi(2);
                        if dist_sq > 64.0 {
                            match self.parallel_tool_phase {
                                ParallelToolPhase::DrawingLine1 => {
                                    self.parallel_line1 = Some((p1, p2));
                                    self.parallel_tool_phase =
                                        ParallelToolPhase::WaitingLine2;
                                }
                                ParallelToolPhase::WaitingLine2
                                | ParallelToolPhase::DrawingLine2 => {
                                    self.parallel_line2 = Some((p1, p2));
                                    if let Some((l1_a, l1_b)) = self.parallel_line1 {
                                        let (a1, b1) = if l1_b.y < l1_a.y {
                                            (l1_a, l1_b)
                                        } else {
                                            (l1_b, l1_a)
                                        };
                                        let (a2, b2) = if p2.y < p1.y {
                                            (p1, p2)
                                        } else {
                                            (p2, p1)
                                        };

                                        let dx1 = b1.x - a1.x;
                                        let dy1 = a1.y - b1.y;
                                        let dx2 = b2.x - a2.x;
                                        let dy2 = a2.y - b2.y;

                                        let slope1 = dx1 / dy1.max(1.0);
                                        let slope2 = dx2 / dy2.max(1.0);

                                        let (left_slope, right_slope) = if a1.x < a2.x {
                                            (slope1, slope2)
                                        } else {
                                            (slope2, slope1)
                                        };

                                        let convergence = right_slope - left_slope;
                                        let avg_tilt = (slope1 + slope2) * 0.5;

                                        let keystone =
                                            (-convergence * 65.0).clamp(-50.0, 50.0);
                                        let rotation =
                                            (-avg_tilt * 57.29578).clamp(-45.0, 45.0);

                                        self.grading.keystone_vertical = keystone;
                                        self.grading.rotation_degrees = rotation;
                                        self.grading.preset = GradingPreset::Custom;
                                        self.current_preset = GradingPreset::Custom;
                                        self.needs_preview_update = true;
                                        self.save_current_photo_sidecar();
                                        self.status_message = format!(
                                            "Perspective redressée : Keystone V {:.1}, Rotation {:.1}°",
                                            keystone, rotation
                                        );
                                    }
                                    self.parallel_tool_phase = ParallelToolPhase::Inactive;
                                    self.parallel_line1 = None;
                                    self.parallel_line2 = None;
                                }
                                _ => {}
                            }
                        }
                    }
                    self.parallel_drag_start = None;
                }
            }

            // Leveling horizon line interaction
            if self.horizon_tool_active {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                if resp.drag_started() {
                    self.horizon_start_point = resp.interact_pointer_pos();
                }
                if resp.dragged() {
                    if let (Some(p1), Some(p2)) =
                        (self.horizon_start_point, resp.interact_pointer_pos())
                    {
                        let dx = p2.x - p1.x;
                        let dy = p2.y - p1.y;
                        let angle = -dy.atan2(dx).to_degrees();
                        painter.line_segment(
                            [p1, p2],
                            egui::Stroke::new(2.5, Color32::from_rgb(0, 240, 255)),
                        );
                        painter.circle_filled(p1, 4.0, Color32::from_rgb(0, 240, 255));
                        painter.circle_filled(p2, 4.0, Color32::from_rgb(0, 240, 255));
                        painter.text(
                            (p1 + p2.to_vec2()) * 0.5 + egui::vec2(0.0, -18.0),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.1}°", angle),
                            egui::FontId::proportional(14.0),
                            Color32::WHITE,
                        );
                    }
                }
                if resp.drag_stopped() {
                    if let (Some(p1), Some(p2)) =
                        (self.horizon_start_point, resp.interact_pointer_pos())
                    {
                        let dx = p2.x - p1.x;
                        let dy = p2.y - p1.y;
                        if (dx * dx + dy * dy) > 36.0 {
                            let angle = -dy.atan2(dx).to_degrees();
                            self.grading.rotation_degrees = angle.clamp(-45.0, 45.0);
                            self.grading.preset = GradingPreset::Custom;
                            self.current_preset = GradingPreset::Custom;
                            self.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                        }
                    }
                    self.horizon_start_point = None;
                    self.horizon_tool_active = false;
                }
            }
        } else if self.is_decoding {
            painter.text(
                available_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Decoding RAW file in background...",
                egui::FontId::proportional(18.0),
                egui::Color32::from_rgb(100, 180, 255),
            );
        } else {
            // Empty state prompt
            painter.text(
                available_rect.center(),
                egui::Align2::CENTER_CENTER,
                "No RAW image loaded.\nClick 'Open RAW...' or pick a file in Library & Files.",
                egui::FontId::proportional(16.0),
                egui::Color32::from_rgb(140, 145, 155),
            );
        }
    });
    }
}
