#![forbid(unsafe_code)]

//! GPU preview texture layout calculation and canvas frame painting for the central viewport.

use egui::Color32;

use crate::i18n::Language;
use crate::state::GeometryGradingParams;

/// Computed screen-space geometry for the viewport preview canvas.
#[derive(Debug, Clone, Copy)]
pub struct ViewportLayout {
    pub fit_scale: f32,
    pub current_scale: f32,
    pub display_size: egui::Vec2,
    pub center: egui::Pos2,
    pub preview_rect: egui::Rect,
    pub uv: egui::Rect,
    pub is_cropped_view: bool,
    pub margin: f32,
}

/// Computes the fitted screen-space preview rectangle, scale, and UV coordinates.
#[must_use]
pub fn compute_viewport_layout(
    available_rect: egui::Rect,
    img_size: egui::Vec2,
    geometry: &GeometryGradingParams,
    is_crop_editing: bool,
    zoom_level: f32,
    pan_offset: egui::Vec2,
) -> ViewportLayout {
    let margin = 16.0;
    let max_w = (available_rect.width() - margin * 2.0).max(10.0);
    let max_h = (available_rect.height() - margin * 2.0).max(10.0);

    let is_cropped_view = geometry.crop_enabled && !is_crop_editing;
    let crop_w = (geometry.crop_right - geometry.crop_left).clamp(0.05, 1.0);
    let crop_h = (geometry.crop_bottom - geometry.crop_top).clamp(0.05, 1.0);
    let effective_img_size = if is_cropped_view {
        egui::vec2(img_size.x * crop_w, img_size.y * crop_h)
    } else {
        img_size
    };

    let fit_scale = (max_w / effective_img_size.x).min(max_h / effective_img_size.y);
    let current_scale = if zoom_level <= 0.0 {
        fit_scale
    } else {
        zoom_level
    };

    let display_size = effective_img_size * current_scale;
    let center = available_rect.center() + pan_offset;
    let preview_rect = egui::Rect::from_center_size(center, display_size);

    let uv = if is_cropped_view {
        egui::Rect::from_min_max(
            egui::pos2(geometry.crop_left, geometry.crop_top),
            egui::pos2(geometry.crop_right, geometry.crop_bottom),
        )
    } else {
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
    };

    ViewportLayout {
        fit_scale,
        current_scale,
        display_size,
        center,
        preview_rect,
        uv,
        is_cropped_view,
        margin,
    }
}

/// Borrowed parameters for painting the central canvas image and passe-partout frame.
pub struct CanvasPaintParams<'a> {
    pub layout: &'a ViewportLayout,
    pub soft_proof: &'a rraw_color::SoftProofConfig,
    pub texture: &'a egui::TextureHandle,
    pub original_texture: Option<&'a egui::TextureHandle>,
    pub show_original: bool,
    pub split_position: Option<f32>,
}

/// Paints the background pass-partout frame, the GPU preview texture (or split-compare pair), and border.
pub fn paint_canvas_image(painter: &egui::Painter, params: &CanvasPaintParams<'_>) {
    let layout = params.layout;
    let soft_proof = params.soft_proof;
    let texture = params.texture;
    let original_texture = params.original_texture;
    let show_original = params.show_original;
    let split_position = params.split_position;

    if soft_proof.enabled && soft_proof.simulate_paper_and_ink {
        let paper_col = match soft_proof.profile {
            rraw_color::SoftProofProfile::FineArtMattePaper => Color32::from_rgb(244, 240, 230),
            rraw_color::SoftProofProfile::GlossyBarytaPaper => Color32::from_rgb(249, 247, 243),
            _ => Color32::from_rgb(248, 248, 246),
        };
        painter.rect_filled(layout.preview_rect.expand(10.0), 3.0, paper_col);
    } else {
        painter.rect_filled(
            layout.preview_rect.expand(2.0),
            4.0,
            Color32::from_rgb(10, 10, 12),
        );
    }

    let active_texture = if show_original {
        original_texture.unwrap_or(texture)
    } else {
        texture
    };

    match (original_texture, split_position) {
        (Some(orig), Some(split_pos)) if !show_original => {
            let (before, after) =
                crate::view_tools::split_comparison(layout.preview_rect, layout.uv, split_pos);
            if before.screen.width() >= 1.0 {
                painter.image(orig.id(), before.screen, before.uv, Color32::WHITE);
            }
            if after.screen.width() >= 1.0 {
                painter.image(texture.id(), after.screen, after.uv, Color32::WHITE);
            }
        }
        _ => {
            painter.image(
                active_texture.id(),
                layout.preview_rect,
                layout.uv,
                Color32::WHITE,
            );
        }
    }

    painter.rect_stroke(
        layout.preview_rect,
        0.0,
        egui::Stroke::new(1.0, Color32::from_rgb(60, 65, 75)),
        egui::StrokeKind::Outside,
    );
}

/// Paints the centered decoding or empty-library placeholder message when no preview texture is active.
pub fn paint_status_placeholder(
    painter: &egui::Painter,
    available_rect: egui::Rect,
    is_decoding: bool,
    lang: Language,
) {
    if is_decoding {
        let msg = crate::i18n::tc(lang, "Viewport.DecodingStatus");
        painter.text(
            available_rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(18.0),
            Color32::from_rgb(100, 180, 255),
        );
    } else {
        let msg = crate::i18n::tc(lang, "Viewport.EmptyStatus");
        painter.text(
            available_rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(16.0),
            Color32::from_rgb(140, 145, 155),
        );
    }
}
