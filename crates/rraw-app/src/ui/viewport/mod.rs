#![forbid(unsafe_code)]

//! Central interactive GPU image viewport with zoom, pan, cropping, horizon, and perspective overlays.

pub mod brush_overlay;
pub mod canvas;
pub mod crop_overlay;
pub mod hud;
pub mod interaction;
pub mod perspective_tools;
pub mod split_compare;

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::state::*;
use crate::ui::sections::geometry::ParallelLinesToolState;

/// Shared read-only viewport canvas context passed to interactive overlay tools.
pub struct ViewportToolContext<'a> {
    pub resp: &'a egui::Response,
    pub painter: &'a egui::Painter,
    pub available_rect: egui::Rect,
    pub preview_rect: egui::Rect,
    pub language: Language,
}

impl RrawApp {
    /// Renders the central viewport with interactive pan, zoom, and vector overlay tools.
    pub fn render_central_viewport(&mut self, ui: &mut egui::Ui) {
        if self.session.needs_preview_update {
            self.update_preview_textures(ui.ctx());
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).fill(egui::Color32::from_rgb(16, 17, 20)))
            .show(ui, |ui| {
                let available_rect = ui.available_rect_before_wrap();
                let painter = ui.painter().with_clip_rect(available_rect);
                let mut split_divider_rect: Option<egui::Rect> = None;

                if let Some(texture) = self.session.preview_texture.clone() {
                    let layout = canvas::compute_viewport_layout(
                        available_rect,
                        texture.size_vec2(),
                        &self.session.grading.geometry,
                        self.viewport.crop.is_editing,
                        self.viewport.camera.zoom_level,
                        self.viewport.camera.pan_offset,
                    );
                    self.viewport.camera.last_fit_scale = layout.fit_scale;

                    let resp = ui.allocate_rect(available_rect, egui::Sense::click_and_drag());
                    let is_brush_active = self.is_freehand_brush_active();
                    let is_tool_active = self.viewport.geometry_tools.horizon_active
                        || self.viewport.geometry_tools.parallel_phase
                            != ParallelToolPhase::Inactive
                        || is_brush_active;

                    if is_brush_active {
                        interaction::handle_brush_radius_shortcuts(ui, &mut self.session.grading);
                    }

                    let pan_zoom_ctx = interaction::PanZoomContext {
                        available_rect,
                        layout: &layout,
                        is_tool_active,
                        is_brush_active,
                    };
                    let is_panning_override = interaction::handle_pan_and_zoom(
                        ui,
                        &resp,
                        &pan_zoom_ctx,
                        &mut self.viewport,
                    );

                    self.handle_mask_overlay_shortcut(ui);

                    let orig_ctx = interaction::OriginalPreviewContext {
                        available_rect,
                        preview_rect: layout.preview_rect,
                        is_comparing_original: self.session.is_comparing_original,
                        is_any_dialog_open: self.export.is_any_dialog_open(),
                        is_tool_active,
                    };
                    let show_original =
                        interaction::resolve_show_original(ui, &self.viewport, &orig_ctx);

                    let split_pos = self
                        .viewport
                        .split
                        .active
                        .then_some(self.viewport.split.position);
                    let paint_params = canvas::CanvasPaintParams {
                        layout: &layout,
                        soft_proof: &self.session.grading.soft_proof,
                        texture: &texture,
                        original_texture: self.session.original_texture.as_ref(),
                        show_original,
                        split_position: split_pos,
                    };
                    canvas::paint_canvas_image(&painter, &paint_params);

                    self.render_viewport_huds(&painter, available_rect, show_original);
                    let brush_ctx = brush_overlay::BrushViewportContext {
                        available_rect,
                        preview_rect: layout.preview_rect,
                        is_panning_override,
                        is_cropped_view: layout.is_cropped_view,
                    };
                    self.handle_viewport_overlays(ui, &resp, &painter, &brush_ctx, is_brush_active);

                    if self.viewport.split.active
                        && !show_original
                        && self.session.original_texture.is_some()
                    {
                        split_divider_rect = Some(layout.preview_rect);
                    }
                } else {
                    canvas::paint_status_placeholder(
                        &painter,
                        available_rect,
                        self.workers.is_decoding,
                        self.lang(),
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

    fn is_freehand_brush_active(&self) -> bool {
        self.session
            .grading
            .masks
            .active_brush_mask_idx
            .is_some_and(|idx| {
                self.session
                    .grading
                    .masks
                    .local_masks
                    .get(idx)
                    .is_some_and(|m| {
                        matches!(m.kind, LocalMaskKind::FreehandBrush { .. }) && m.enabled
                    })
            })
    }

    fn handle_mask_overlay_shortcut(&mut self, ui: &egui::Ui) {
        if ui.input(|i| i.key_pressed(egui::Key::O)) {
            if self.session.grading.masks.show_local_mask_overlay.is_some() {
                self.session.grading.masks.show_local_mask_overlay = None;
                self.session.needs_preview_update = true;
                ui.ctx().request_repaint();
            } else if !self.session.grading.masks.local_masks.is_empty() {
                self.session.grading.masks.show_local_mask_overlay = Some(0);
                self.session.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        }
    }

    fn render_viewport_huds(
        &self,
        painter: &egui::Painter,
        available_rect: egui::Rect,
        show_original: bool,
    ) {
        let cur_lang = self.lang();
        if show_original {
            hud::render_original_badge(painter, available_rect, self.t(Key::ViewportOriginalBadge));
        }

        if self.session.grading.soft_proof.enabled
            || self.session.grading.soft_proof.show_gamut_warning
        {
            hud::render_soft_proofing_hud(
                painter,
                available_rect,
                &self.session.grading.soft_proof,
                self.viewport.histogram.out_of_gamut_count,
                cur_lang,
            );
        }

        if let Some(mask_idx) = self.session.grading.masks.show_local_mask_overlay {
            let default_mask_name = crate::i18n::tc(cur_lang, "Viewport.DefaultMaskName");
            let mask_name = self
                .session
                .grading
                .masks
                .local_masks
                .get(mask_idx)
                .map_or(default_mask_name, |m| m.name.as_str());
            hud::render_local_mask_hud(painter, available_rect, mask_name, cur_lang);
        }
    }

    fn handle_viewport_overlays(
        &mut self,
        ui: &mut egui::Ui,
        resp: &egui::Response,
        painter: &egui::Painter,
        brush_ctx: &brush_overlay::BrushViewportContext,
        is_brush_active: bool,
    ) {
        let cur_lang = self.lang();
        let available_rect = brush_ctx.available_rect;
        let preview_rect = brush_ctx.preview_rect;

        let pre_viewport_grading = self.session.grading.clone();
        let pointer_down = ui.input(|i| i.pointer.any_down());

        if is_brush_active {
            let (needs_upd, save_sidecar) = brush_overlay::handle_brush_painting(
                ui,
                resp,
                brush_ctx,
                &self.session.grading.geometry,
                &mut self.session.grading.masks.local_masks,
                self.session.grading.masks.active_brush_mask_idx,
            );
            if needs_upd {
                self.session.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
            if save_sidecar {
                self.session.sidecar_dirty = true;
            }
            if let Some(mask_idx) = self.session.grading.masks.active_brush_mask_idx {
                if let Some(mask) = self.session.grading.masks.local_masks.get(mask_idx) {
                    let has_mask_overlay =
                        self.session.grading.masks.show_local_mask_overlay.is_some();
                    hud::render_brush_hud(
                        painter,
                        available_rect,
                        mask,
                        has_mask_overlay,
                        self.viewport.camera.zoom_level,
                        cur_lang,
                    );
                }
            }
        }

        let tool_ctx = ViewportToolContext {
            resp,
            painter,
            available_rect,
            preview_rect,
            language: cur_lang,
        };

        if self.session.grading.geometry.crop_enabled && self.viewport.crop.is_editing {
            let mut crop_state = crop_overlay::CropToolState {
                geometry: &mut self.session.grading.geometry,
                is_crop_editing: &mut self.viewport.crop.is_editing,
                crop_drag_handle: &mut self.viewport.crop.drag_handle,
                crop_show_grid: self.viewport.crop.show_grid,
            };
            let crop_out = crop_overlay::handle_crop_overlay(ui, &tool_ctx, &mut crop_state);
            if crop_out.needs_preview_update {
                self.session.needs_preview_update = true;
            }
            if crop_out.save_sidecar {
                self.session.sidecar_dirty = true;
            }
            if !self.viewport.crop.is_editing {
                self.activate_canvas_tool(crate::controllers::viewport::ActiveCanvasTool::None);
            }
        }

        if self.viewport.geometry_tools.parallel_phase != ParallelToolPhase::Inactive {
            let mut tool_state = ParallelLinesToolState {
                phase: &mut self.viewport.geometry_tools.parallel_phase,
                line1: &mut self.viewport.geometry_tools.parallel_line1,
                line2: &mut self.viewport.geometry_tools.parallel_line2,
                drag_start: &mut self.viewport.geometry_tools.parallel_drag_start,
                status_message: &mut self.status_message,
            };
            let p_out = perspective_tools::handle_parallel_lines_tool(
                ui,
                &tool_ctx,
                &mut tool_state,
                &mut self.session.grading.geometry,
            );
            if p_out.mark_custom_preset {
                self.session.grading.preset = GradingPreset::Custom;
                self.session.current_preset = GradingPreset::Custom;
            }
            if p_out.needs_preview_update {
                self.session.needs_preview_update = true;
            }
            if p_out.save_sidecar {
                self.session.sidecar_dirty = true;
            }
            if self.viewport.geometry_tools.parallel_phase == ParallelToolPhase::Inactive {
                self.activate_canvas_tool(crate::controllers::viewport::ActiveCanvasTool::None);
            }
        }

        if self.viewport.geometry_tools.horizon_active {
            let h_out = perspective_tools::handle_horizon_tool(
                ui,
                &tool_ctx,
                &mut self.viewport.geometry_tools.horizon_active,
                &mut self.viewport.geometry_tools.horizon_start_point,
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
                self.session.sidecar_dirty = true;
            }
            if !self.viewport.geometry_tools.horizon_active {
                self.activate_canvas_tool(crate::controllers::viewport::ActiveCanvasTool::None);
            }
        }

        if self.session.grading != pre_viewport_grading {
            if self.viewport.crop.is_editing {
                self.notify_grading_edited_deferred_preview(pre_viewport_grading, pointer_down);
            } else {
                self.notify_grading_edited(pre_viewport_grading, pointer_down);
            }
            ui.ctx().request_repaint();
        }

        if is_brush_active {
            brush_overlay::render_brush_cursor_ring(
                ui,
                resp,
                painter,
                brush_ctx,
                &self.session.grading.masks.local_masks,
                self.session.grading.masks.active_brush_mask_idx,
            );
        }
    }

    /// Paints the draggable Before/After divider and updates `split_position` while dragging.
    fn render_split_divider(&mut self, ui: &egui::Ui, image_rect: egui::Rect) {
        let label_before = self.t(Key::ViewportBefore);
        let label_after = self.t(Key::ViewportAfter);
        split_compare::render_split_divider(
            ui,
            image_rect,
            &mut self.viewport.split.position,
            label_before,
            label_after,
        );
    }
}
