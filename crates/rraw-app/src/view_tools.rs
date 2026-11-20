#![forbid(unsafe_code)]

//! Pure, allocation-free viewport helpers: clipping overlays, before/after split geometry
//! and the shooting-parameter summary shown in the canvas HUD.

use egui::{Color32, Rect};

/// Overlay tint applied to fully crushed shadows.
pub const SHADOW_CLIP_TINT: Color32 = Color32::from_rgb(40, 110, 255);
/// Overlay tint applied to blown highlights.
pub const HIGHLIGHT_CLIP_TINT: Color32 = Color32::from_rgb(255, 40, 40);

/// Luma threshold under which a pixel is considered shadow-clipped (crushed black).
pub const SHADOW_CLIP_THRESHOLD: u8 = 1;
/// Luma threshold above which a pixel is considered highlight-clipped (blown white).
pub const HIGHLIGHT_CLIP_THRESHOLD: u8 = 250;
/// Per-channel threshold indicating color channel saturation.
pub const CHANNEL_CLIP_THRESHOLD: u8 = 253;

/// Which clipping warnings are painted on top of the rendered preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClippingOverlay {
    pub shadows: bool,
    pub highlights: bool,
}

impl ClippingOverlay {
    /// Returns `true` when at least one warning layer is enabled.
    #[must_use]
    pub const fn is_active(self) -> bool {
        self.shadows || self.highlights
    }
}

/// Integer Rec.709-weighted luma, identical to the histogram binning in `preview.rs`.
#[must_use]
pub fn display_luma(c: Color32) -> u8 {
    let l = (u32::from(c.r()) * 54 + u32::from(c.g()) * 183 + u32::from(c.b()) * 19) >> 8;
    // `l` is at most (255 * 256) >> 8 == 255, so the conversion never saturates.
    u8::try_from(l).unwrap_or(u8::MAX)
}

/// Checks whether a pixel is clipped in the dark tones.
#[must_use]
pub fn is_shadow_clipped(c: Color32) -> bool {
    display_luma(c) <= SHADOW_CLIP_THRESHOLD
}

/// Checks whether a pixel is clipped in the bright tones (overall luma >= 250 or any saturated channel >= 253).
#[must_use]
pub fn is_highlight_clipped(c: Color32) -> bool {
    display_luma(c) >= HIGHLIGHT_CLIP_THRESHOLD
        || c.r() >= CHANNEL_CLIP_THRESHOLD
        || c.g() >= CHANNEL_CLIP_THRESHOLD
        || c.b() >= CHANNEL_CLIP_THRESHOLD
}

/// Tints clipped pixels in place. Uses the exact same thresholds as the histogram counters,
/// so the number of tinted pixels always matches the clipping badges.
pub fn apply_clipping_overlay(pixels: &mut [Color32], overlay: ClippingOverlay) {
    if !overlay.is_active() {
        return;
    }
    pixels.iter_mut().for_each(|px| {
        if overlay.highlights && is_highlight_clipped(*px) {
            *px = HIGHLIGHT_CLIP_TINT;
        } else if overlay.shadows && is_shadow_clipped(*px) {
            *px = SHADOW_CLIP_TINT;
        }
    });
}

/// Screen and texture rectangles for one half of a split comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitHalf {
    pub screen: Rect,
    pub uv: Rect,
}

/// Splits an image drawn in `screen` (sampling `uv`) at the normalized horizontal
/// position `split` (clamped to `[0, 1]`). Returns `(before, after)`: the left half shows
/// the original, the right half the graded result. UVs are interpolated linearly so both
/// halves stay pixel-aligned with the full image.
#[must_use]
pub fn split_comparison(screen: Rect, uv: Rect, split: f32) -> (SplitHalf, SplitHalf) {
    let t = if split.is_finite() {
        split.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let x_screen = egui::lerp(screen.min.x..=screen.max.x, t);
    let x_uv = egui::lerp(uv.min.x..=uv.max.x, t);

    let before = SplitHalf {
        screen: Rect::from_min_max(screen.min, egui::pos2(x_screen, screen.max.y)),
        uv: Rect::from_min_max(uv.min, egui::pos2(x_uv, uv.max.y)),
    };
    let after = SplitHalf {
        screen: Rect::from_min_max(egui::pos2(x_screen, screen.min.y), screen.max),
        uv: Rect::from_min_max(egui::pos2(x_uv, uv.min.y), uv.max),
    };
    (before, after)
}

/// Converts a pointer x coordinate into a normalized split position inside `screen`.
#[must_use]
pub fn split_from_pointer(screen: Rect, pointer_x: f32) -> f32 {
    let w = screen.width();
    if w <= f32::EPSILON || !pointer_x.is_finite() {
        return 0.5;
    }
    ((pointer_x - screen.min.x) / w).clamp(0.0, 1.0)
}

/// Builds the compact `24 mm · f/2.8 · 1/500 s · ISO 100` HUD line, skipping empty fields.
#[must_use]
pub fn shooting_summary(focal: &str, aperture: &str, shutter: &str, iso: u32) -> String {
    let iso_str = (iso > 0).then(|| format!("ISO {iso}"));
    [focal, aperture, shutter]
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .chain(iso_str)
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
    }

    #[test]
    fn luma_extremes_match_histogram_bins() {
        assert_eq!(display_luma(Color32::BLACK), 0);
        assert_eq!(display_luma(Color32::WHITE), 255);
        assert_eq!(display_luma(Color32::from_rgb(128, 128, 128)), 128);
    }

    #[test]
    fn inactive_overlay_leaves_pixels_untouched() {
        let mut px = [Color32::BLACK, Color32::WHITE, Color32::from_gray(100)];
        let before = px;
        apply_clipping_overlay(&mut px, ClippingOverlay::default());
        assert_eq!(px, before);
    }

    #[test]
    fn overlay_tints_only_requested_clipping_layers() {
        let mid = Color32::from_gray(100);
        let mut px = [Color32::BLACK, Color32::WHITE, mid];
        apply_clipping_overlay(
            &mut px,
            ClippingOverlay {
                shadows: true,
                highlights: false,
            },
        );
        assert_eq!(px, [SHADOW_CLIP_TINT, Color32::WHITE, mid]);

        let mut px = [Color32::BLACK, Color32::WHITE, mid];
        apply_clipping_overlay(
            &mut px,
            ClippingOverlay {
                shadows: true,
                highlights: true,
            },
        );
        assert_eq!(px, [SHADOW_CLIP_TINT, HIGHLIGHT_CLIP_TINT, mid]);
    }

    #[test]
    fn near_clipped_pixels_are_not_tinted() {
        let mut px = [Color32::from_gray(10), Color32::from_gray(240)];
        let before = px;
        apply_clipping_overlay(
            &mut px,
            ClippingOverlay {
                shadows: true,
                highlights: true,
            },
        );
        assert_eq!(px, before);
    }

    #[test]
    fn split_halves_tile_screen_and_uv_exactly() {
        let screen = rect(100.0, 50.0, 500.0, 350.0);
        let uv = rect(0.2, 0.1, 0.8, 0.9);
        let (b, a) = split_comparison(screen, uv, 0.25);
        assert_eq!(b.screen, rect(100.0, 50.0, 200.0, 350.0));
        assert_eq!(a.screen, rect(200.0, 50.0, 500.0, 350.0));
        assert!((b.uv.max.x - 0.35).abs() < 1e-6);
        assert!((a.uv.min.x - 0.35).abs() < 1e-6);
        assert_eq!(b.uv.min, uv.min);
        assert_eq!(a.uv.max, uv.max);
    }

    #[test]
    fn split_position_is_clamped_and_nan_safe() {
        let screen = rect(0.0, 0.0, 100.0, 100.0);
        let uv = rect(0.0, 0.0, 1.0, 1.0);
        let (b, _) = split_comparison(screen, uv, 2.0);
        assert_eq!(b.screen, screen);
        let (b, a) = split_comparison(screen, uv, -1.0);
        assert_eq!(b.screen.width(), 0.0);
        assert_eq!(a.screen, screen);
        let (b, _) = split_comparison(screen, uv, f32::NAN);
        assert_eq!(b.screen.max.x, 50.0);
    }

    #[test]
    fn pointer_maps_to_normalized_split() {
        let screen = rect(100.0, 0.0, 300.0, 10.0);
        assert_eq!(split_from_pointer(screen, 200.0), 0.5);
        assert_eq!(split_from_pointer(screen, 0.0), 0.0);
        assert_eq!(split_from_pointer(screen, 999.0), 1.0);
        assert_eq!(split_from_pointer(rect(5.0, 0.0, 5.0, 1.0), 5.0), 0.5);
    }

    #[test]
    fn shooting_summary_skips_missing_fields() {
        assert_eq!(
            shooting_summary("24 mm", "f/2.8", "1/500 s", 100),
            "24 mm · f/2.8 · 1/500 s · ISO 100"
        );
        assert_eq!(shooting_summary("", "f/4", "  ", 0), "f/4");
        assert_eq!(shooting_summary("", "", "", 0), "");
    }
}
