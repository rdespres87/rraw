#![forbid(unsafe_code)]

//! Viewport Heads-Up Display (HUD) badges for active states, soft proofing and masks.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use rraw_color::{SoftProofConfig, SoftProofRenderingIntent};

use crate::state::{LocalMaskKind, LocalMaskLayer};

/// Live Original (Before/After) HUD Badge when holding reference view.
pub fn render_original_badge(painter: &Painter, available_rect: Rect, badge_text: &str) {
    let badge_rect = Rect::from_center_size(
        Pos2::new(available_rect.center().x, available_rect.top() + 24.0),
        Vec2::new(220.0, 28.0),
    );
    painter.rect_filled(badge_rect, 6.0, Color32::from_black_alpha(220));
    painter.rect_stroke(
        badge_rect,
        6.0,
        Stroke::new(1.5, Color32::from_rgb(255, 190, 60)),
        StrokeKind::Inside,
    );
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        badge_text,
        egui::FontId::proportional(13.0),
        Color32::from_rgb(255, 220, 110),
    );
}

/// Live Soft Proofing HUD Badge at top-left of viewport.
pub fn render_soft_proofing_hud(
    painter: &Painter,
    available_rect: Rect,
    soft_proof: &SoftProofConfig,
    out_of_gamut_count: u32,
) {
    let intent_str = match soft_proof.intent {
        SoftProofRenderingIntent::Perceptual => "Perceptuel",
        SoftProofRenderingIntent::RelativeColorimetric => "Relatif",
    };
    let paper_str = if soft_proof.simulate_paper_and_ink {
        " | Papier & Encre (Dmin/Dmax)"
    } else {
        ""
    };
    let oog_str = format!(" | Hors-Gamut: {out_of_gamut_count} px");
    let hud_text = format!(
        "🖨️ ÉPREUVAGE : {} ({intent_str}{paper_str}){oog_str}",
        soft_proof.profile.label()
    );
    let hud_pos = Pos2::new(available_rect.left() + 18.0, available_rect.top() + 14.0);
    let galley = painter.layout_no_wrap(
        hud_text,
        egui::FontId::proportional(12.5),
        Color32::from_rgb(235, 245, 240),
    );
    let badge_rect = Rect::from_min_size(hud_pos, galley.size() + Vec2::new(16.0, 8.0));
    painter.rect_filled(
        badge_rect,
        5.0,
        Color32::from_rgba_unmultiplied(18, 22, 26, 220),
    );
    painter.rect_stroke(
        badge_rect,
        5.0,
        Stroke::new(1.0, Color32::from_rgb(90, 210, 145)),
        StrokeKind::Outside,
    );
    painter.galley(hud_pos + Vec2::new(8.0, 4.0), galley, Color32::WHITE);
}

/// Active Local Mask Overlay HUD Badge.
pub fn render_local_mask_hud(painter: &Painter, available_rect: Rect, mask_name: &str) {
    let hud_text = format!("🎭 Incrustation : {mask_name} (Touche O ou 👁 pour masquer)");
    let hud_pos = Pos2::new(available_rect.left() + 18.0, available_rect.top() + 48.0);
    let galley = painter.layout_no_wrap(
        hud_text,
        egui::FontId::proportional(12.0),
        Color32::from_rgb(255, 235, 240),
    );
    let badge_rect = Rect::from_min_size(hud_pos, galley.size() + Vec2::new(16.0, 8.0));
    painter.rect_filled(
        badge_rect,
        5.0,
        Color32::from_rgba_unmultiplied(180, 25, 75, 230),
    );
    painter.galley(hud_pos + Vec2::new(8.0, 4.0), galley, Color32::WHITE);
}

/// Active Brush HUD Badge.
pub fn render_brush_hud(
    painter: &Painter,
    available_rect: Rect,
    mask: &LocalMaskLayer,
    has_mask_overlay: bool,
    zoom_level: f32,
) {
    let (mode_str, badge_color) = if let LocalMaskKind::FreehandBrush {
        erase_mode: true, ..
    } = mask.kind
    {
        (
            "Gomme 🧹",
            Color32::from_rgba_unmultiplied(160, 40, 40, 230),
        )
    } else {
        (
            "Peindre 🖌️",
            Color32::from_rgba_unmultiplied(30, 90, 160, 230),
        )
    };
    let top_y = if has_mask_overlay { 82.0 } else { 48.0 };
    let zoom_str = if zoom_level > 0.0 {
        format!(" | Zoom {:.0}% (Espace pour glisser)", zoom_level * 100.0)
    } else {
        String::new()
    };
    let hud_text = format!(
        "🖌️ Pinceau : {} [{}]{} (Glisser pour peindre, [ / ] taille, Échap pour quitter)",
        mask.name, mode_str, zoom_str
    );
    let hud_pos = Pos2::new(available_rect.left() + 18.0, available_rect.top() + top_y);
    let galley = painter.layout_no_wrap(
        hud_text,
        egui::FontId::proportional(12.0),
        Color32::from_rgb(240, 245, 255),
    );
    let badge_rect = Rect::from_min_size(hud_pos, galley.size() + Vec2::new(16.0, 8.0));
    painter.rect_filled(badge_rect, 5.0, badge_color);
    painter.galley(hud_pos + Vec2::new(8.0, 4.0), galley, Color32::WHITE);
}
