#![forbid(unsafe_code)]

//! Phase 5 regression tests: horizontal filmstrip virtualization (10,000+ entries)
//! and full-resolution CPU demosaicing invalidation control during continuous UI interactions.

use std::path::PathBuf;
use std::sync::Mutex;

use rraw_app::preview::{
    full_res_demosaic_count, render_preview_from_base, BaseRgbPreview, PREVIEW_PROXY_MAX_WIDTH,
};
use rraw_app::state::{DarkroomGradingState, ExportScaleMode};
use rraw_app::ui::filmstrip::{
    filmstrip_card_x_offset, filmstrip_total_width, visible_filmstrip_range,
    FILMSTRIP_CARD_SPACING, FILMSTRIP_CARD_STRIDE, FILMSTRIP_CARD_WIDTH, FILMSTRIP_OVERSCAN_CARDS,
};
use rraw_app::ui::viewport::interaction::{clamp_pan_offset, compute_scroll_zoom};
use rraw_app::RrawApp;
use rraw_core::{BayerOrigin, CfaPattern, CoreError, SignedCfaBuffer};

static DEMOSAIC_COUNTER_LOCK: Mutex<()> = Mutex::new(());

fn make_large_synthetic_cfa(width: usize, height: usize) -> Result<SignedCfaBuffer, CoreError> {
    let mut samples = vec![0.18_f32; width * height];
    for (idx, sample) in samples.iter_mut().enumerate() {
        *sample = 0.1 + ((idx % 17) as f32) * 0.04;
    }
    SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)
}

#[test]
fn test_visible_filmstrip_range_edge_cases_and_10k_scale() {
    assert_eq!(visible_filmstrip_range(0, 0.0, 1280.0, 128.0, 2), 0..0);
    assert_eq!(visible_filmstrip_range(100, 0.0, 1280.0, 0.0, 2), 0..0);
    assert_eq!(visible_filmstrip_range(100, 0.0, 1280.0, -10.0, 2), 0..0);
    assert_eq!(visible_filmstrip_range(100, 500.0, 500.0, 128.0, 2), 0..0);
    assert_eq!(visible_filmstrip_range(100, -500.0, -10.0, 128.0, 2), 0..0);
    assert_eq!(
        visible_filmstrip_range(100, f32::NAN, 1280.0, 128.0, 2),
        0..0
    );

    // 10,000 RAW library: 1280px viewport at start renders only ~12 cards (10 visible + 2 overscan)
    let start_range = visible_filmstrip_range(
        10_000,
        0.0,
        1280.0,
        FILMSTRIP_CARD_STRIDE,
        FILMSTRIP_OVERSCAN_CARDS,
    );
    assert_eq!(start_range, 0..12);

    // Deep scroll to middle of 10,000-entry library (card 5,000)
    let mid_x = filmstrip_card_x_offset(5_000, FILMSTRIP_CARD_STRIDE);
    let mid_range = visible_filmstrip_range(
        10_000,
        mid_x,
        mid_x + 1280.0,
        FILMSTRIP_CARD_STRIDE,
        FILMSTRIP_OVERSCAN_CARDS,
    );
    assert_eq!(mid_range, 4_998..5_012);
    assert_eq!(mid_range.len(), 14);

    // Near end of library: clamped cleanly to 10,000
    let end_x = filmstrip_card_x_offset(9_995, FILMSTRIP_CARD_STRIDE);
    let end_range = visible_filmstrip_range(
        10_000,
        end_x,
        end_x + 1280.0,
        FILMSTRIP_CARD_STRIDE,
        FILMSTRIP_OVERSCAN_CARDS,
    );
    assert_eq!(end_range, 9_993..10_000);

    // Completely past end
    let past_end_x = filmstrip_card_x_offset(10_005, FILMSTRIP_CARD_STRIDE);
    assert_eq!(
        visible_filmstrip_range(
            10_000,
            past_end_x,
            past_end_x + 1280.0,
            FILMSTRIP_CARD_STRIDE,
            FILMSTRIP_OVERSCAN_CARDS,
        ),
        0..0
    );
}

#[test]
fn test_filmstrip_virtual_spacer_width_identity() {
    let total_cards = 10_000;
    let expected_total =
        filmstrip_total_width(total_cards, FILMSTRIP_CARD_WIDTH, FILMSTRIP_CARD_SPACING);
    assert_eq!(
        filmstrip_total_width(0, FILMSTRIP_CARD_WIDTH, FILMSTRIP_CARD_SPACING),
        0.0
    );

    let range = 4_998..5_012;
    let leading_w = (range.start as f32) * FILMSTRIP_CARD_STRIDE - FILMSTRIP_CARD_SPACING;
    let visible_w =
        filmstrip_total_width(range.len(), FILMSTRIP_CARD_WIDTH, FILMSTRIP_CARD_SPACING);
    let remaining = total_cards - range.end;
    let trailing_w = (remaining as f32) * FILMSTRIP_CARD_STRIDE - FILMSTRIP_CARD_SPACING;

    // Leading spacer + spacing + visible cards + spacing + trailing spacer == exact total width
    let reconstructed =
        leading_w + FILMSTRIP_CARD_SPACING + visible_w + FILMSTRIP_CARD_SPACING + trailing_w;
    assert!(
        (reconstructed - expected_total).abs() < 1e-2,
        "reconstructed={reconstructed} expected={expected_total}"
    );
}

#[test]
fn test_filmstrip_headless_render_10k_entries_is_virtualized_and_scrolls_to_offscreen_active() {
    let mut app = RrawApp::new_headless();
    app.library.directory_entries = (0..10_000)
        .map(|i| PathBuf::from(format!("/virtual/library/IMG_{i:05}.CR3")))
        .collect();
    // Set active photo far offscreen at index 7,500 and request scroll-to-active
    app.layout_config.last_open_photo = Some(app.library.directory_entries[7_500].clone());
    app.library.scroll_filmstrip_to_active = true;

    let ctx = egui::Context::default();
    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };

    let mut output = ctx.run_ui(raw_input, |ui| {
        app.render_filmstrip_panel(ui);
    });
    output.textures_delta.clear();

    // Virtualization guarantees scroll_filmstrip_to_active is handled even for offscreen index 7,500
    assert!(
        !app.library.scroll_filmstrip_to_active,
        "scroll_filmstrip_to_active must be resolved even when the active card starts offscreen"
    );
    // Only visible cards + overscan should have requested thumbnails, never all 10,000
    assert!(
        app.workers.thumbnail_pending.len() <= 32,
        "expected O(visible) thumbnail requests, got {}",
        app.workers.thumbnail_pending.len()
    );
}

#[test]
fn test_interactive_grading_pan_zoom_and_crop_never_trigger_full_res_demosaicing(
) -> Result<(), CoreError> {
    let _guard = DEMOSAIC_COUNTER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Sensor wider than PREVIEW_PROXY_MAX_WIDTH (1600px)
    let cfa = make_large_synthetic_cfa(1920, 1280)?;
    let before = full_res_demosaic_count();

    // 1. Interactive base preview build must use fast sub-sampled proxy (0 full-res demosaics)
    let base = BaseRgbPreview::from_cfa_with_color(&cfa, [2.0, 1.0, 1.5], None);
    assert!(base.is_interactive_proxy_for(&cfa));
    assert_eq!(base.width, PREVIEW_PROXY_MAX_WIDTH);
    assert_eq!(
        full_res_demosaic_count() - before,
        0,
        "interactive BaseRgbPreview must not invoke full-resolution CPU demosaicing"
    );

    // 2. Continuous slider scrubbing (20 frames) + live preview rendering uses cached base_preview
    let mut app = RrawApp::new_headless();
    app.session.cached_cfa = Some(cfa.clone());
    app.session.base_preview = Some(base.clone());

    for step in 0..20 {
        let pre = app.session.grading.clone();
        app.session.grading.tone.exposure_ev = -1.0 + (step as f32) * 0.1;
        app.session.notify_grading_edited(pre, true);
        let (_img, _hist) = render_preview_from_base(&base, &app.session.grading);
    }

    // 3. Continuous pan & zoom interactions never touch CFA or demosaic
    let available = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let (zoom, pan) = compute_scroll_zoom(
        1.0,
        0.5,
        120.0,
        Some(egui::pos2(640.0, 360.0)),
        available.center(),
        egui::Vec2::ZERO,
    );
    let _clamped = clamp_pan_offset(pan, egui::vec2(2560.0, 1440.0), available, 24.0, zoom);

    // 4. Continuous crop drag uses deferred preview update and zero demosaic
    for step in 0..10 {
        let pre = app.session.grading.clone();
        app.session.grading.geometry.crop_left = 0.01 * (step as f32);
        app.session
            .notify_grading_edited_deferred_preview(pre, true);
    }

    // 5. Undo / Redo cycles never re-demosaic
    let _ = app.session.undo();
    let _ = app.session.redo();

    assert_eq!(
        full_res_demosaic_count() - before,
        0,
        "continuous interactive grading, pan/zoom, crop drag, and undo/redo must trigger 0 full-res demosaics"
    );

    // 6. Explicit full-resolution export is the ONLY path that triggers 1:1 CPU demosaicing
    let full_res_export_base =
        RrawApp::build_export_base_static(ExportScaleMode::Original, &cfa, &base);
    assert_eq!(full_res_export_base.width, 1920);
    assert_eq!(
        full_res_demosaic_count() - before,
        1,
        "explicit Original export must invoke full-resolution CPU demosaicing exactly once"
    );

    // 7. Downscaled 50% export (960px < sensor width) remains sub-sampled (0 extra full-res demosaics)
    let web_export_base =
        RrawApp::build_export_base_static(ExportScaleMode::Percent50, &cfa, &base);
    assert_eq!(web_export_base.width, 960);
    assert_eq!(
        full_res_demosaic_count() - before,
        1,
        "downscaled export must not trigger full-resolution CPU demosaicing"
    );

    // Silence unused import warning if grading default is checked
    let _ = DarkroomGradingState::default();
    Ok(())
}
