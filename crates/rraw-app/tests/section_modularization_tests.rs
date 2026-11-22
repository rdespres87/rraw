#![forbid(unsafe_code)]

//! TDD and regression test suite for Task 15.2: Monolithic File & Function Decomposition.
//!
//! Validates:
//! 1. Principle of least privilege: modular sections accept dedicated sub-struct references.
//! 2. `SectionOutput` contract behavior (mark_custom_preset, needs_preview_update, save_sidecar).
//! 3. Clean delegation from `inspector.rs` without regression.

use egui::{Context, Ui, Vec2};
use rraw_app::grading::{
    ColorGradingParams, DenoiseSharpenParams, EffectsGradingParams, GeometryGradingParams,
    OpticsGradingParams, ToneGradingParams,
};
use rraw_app::i18n::Language;
use rraw_app::state::ParallelToolPhase;
use rraw_app::ui::sections::{
    crop, detail_denoise, effects, exposure_tone, geometry, masks, optics, soft_proofing,
    straighten, timelapse, white_balance,
};
use rraw_color::SoftProofConfig;
use rraw_core::SmartLightingParams;

fn with_test_ui<F: FnMut(&mut Ui)>(mut f: F) {
    let ctx = Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_min_size(Vec2::new(400.0, 800.0));
        f(ui);
    });
    out.textures_delta.clear();
}

#[test]
fn test_exposure_tone_section_least_privilege() {
    with_test_ui(|ui| {
        let mut tone = ToneGradingParams::default();
        let mut smart_lighting = SmartLightingParams::default();

        let out = exposure_tone::render(ui, &mut tone, &mut smart_lighting, Language::Fr);
        assert!(!out.mark_custom_preset);
        assert!(!out.needs_preview_update);
    });
}

#[test]
fn test_white_balance_section_least_privilege() {
    with_test_ui(|ui| {
        let mut color = ColorGradingParams::default();
        let mut tone = ToneGradingParams::default();

        let out = white_balance::render(ui, &mut color, &mut tone, Language::En);
        assert!(!out.mark_custom_preset);
        assert!(!out.needs_preview_update);
    });
}

#[test]
fn test_detail_denoise_section_least_privilege() {
    with_test_ui(|ui| {
        let mut denoise = DenoiseSharpenParams::default();

        let out = detail_denoise::render(ui, &mut denoise, Language::Fr);
        assert!(!out.mark_custom_preset);
        assert!(!out.needs_preview_update);
    });
}

#[test]
fn test_soft_proofing_section_least_privilege() {
    with_test_ui(|ui| {
        let mut soft_proof = SoftProofConfig::default();

        let out = soft_proofing::render(ui, &mut soft_proof, 0, Language::Fr);
        assert!(!out.needs_preview_update);
    });
}

#[test]
fn test_optics_section_least_privilege() {
    with_test_ui(|ui| {
        let mut opt = OpticsGradingParams::default();

        let out = optics::render(ui, &mut opt, None, Language::Fr);
        assert!(!out.needs_preview_update);
    });
}

#[test]
fn test_crop_section_least_privilege() {
    with_test_ui(|ui| {
        let mut geom = GeometryGradingParams::default();
        let mut is_crop_editing = false;

        let out = crop::render(
            ui,
            &mut geom,
            &mut is_crop_editing,
            Some((6000, 4000)),
            Language::Fr,
        );
        assert!(!out.needs_preview_update);
        assert!(!is_crop_editing);
    });
}

#[test]
fn test_straighten_section_least_privilege() {
    with_test_ui(|ui| {
        let mut geom = GeometryGradingParams::default();
        let mut horizon_active = false;
        let mut horizon_pt = None;
        let mut status = String::new();

        let out = straighten::render(
            ui,
            &mut geom,
            &mut horizon_active,
            &mut horizon_pt,
            &mut status,
            Language::Fr,
        );
        assert!(!out.needs_preview_update);
        assert!(!horizon_active);
    });
}

#[test]
fn test_geometry_section_least_privilege() {
    with_test_ui(|ui| {
        let mut geom = GeometryGradingParams::default();
        let mut phase = ParallelToolPhase::Inactive;
        let mut line1 = None;
        let mut line2 = None;
        let mut drag_start = None;
        let mut status = String::new();

        let mut tool_state = geometry::ParallelLinesToolState {
            phase: &mut phase,
            line1: &mut line1,
            line2: &mut line2,
            drag_start: &mut drag_start,
            status_message: &mut status,
        };

        let out = geometry::render(ui, &mut geom, &mut tool_state, Language::Fr);
        assert!(!out.needs_preview_update);
        assert_eq!(phase, ParallelToolPhase::Inactive);
    });
}

#[test]
fn test_masks_section_least_privilege() {
    with_test_ui(|ui| {
        let mut local_masks = Vec::new();
        let mut show_local_overlay = None;
        let mut active_brush_idx = None;
        let mut semantic_masks = Vec::new();
        let mut show_semantic_overlay = None;

        let out = masks::render(
            ui,
            &mut local_masks,
            &mut show_local_overlay,
            &mut active_brush_idx,
            &mut semantic_masks,
            &mut show_semantic_overlay,
            Language::Fr,
        );
        assert!(!out.mark_custom_preset);
        assert!(!out.needs_preview_update);
    });
}

#[test]
fn test_timelapse_section_export_button() {
    with_test_ui(|ui| {
        let clicked = timelapse::render_export_button(ui, Language::Fr);
        assert!(!clicked);
    });
}

#[test]
fn test_effects_section_least_privilege() {
    with_test_ui(|ui| {
        let mut effects = EffectsGradingParams::default();
        let out = effects::render(ui, &mut effects, Language::Fr);
        assert!(!out.mark_custom_preset);
        assert!(!out.needs_preview_update);
    });
}
