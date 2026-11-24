#![forbid(unsafe_code)]

//! Contract integration tests for `rraw-macos-edr`.
//!
//! Validates EDR layer invariants, headroom provider clamping and thread safety,
//! `DualPassCompositor` HDR clamping, and SDR UI luminance invariance across
//! display headroom transitions.

use rraw_core::{DisplayHeadroomProvider, SurfaceFormat, SurfaceSpec};
use rraw_macos_edr::{
    DualPassCompositor, EdrColorSpace, EdrLayerConfig, MockDisplayHeadroomProvider,
    SdrFallbackHeadroomProvider,
};

#[cfg(target_os = "macos")]
use rraw_macos_edr::MacOsDisplayHeadroomProvider;

#[test]
fn edr_layer_config_invariants() {
    let cfg = EdrLayerConfig::default();
    assert_eq!(cfg.surface_format, SurfaceFormat::Rgba16Float);
    assert_eq!(cfg.surface_format, SurfaceSpec::EDR_SURFACE_FORMAT);
    assert!(cfg.wants_extended_dynamic_range_content);
    assert_eq!(cfg.color_space, EdrColorSpace::ExtendedLinearDisplayP3);
}

#[test]
fn sdr_fallback_provider_returns_nominal_headroom() {
    let provider = SdrFallbackHeadroomProvider;
    let sample = provider.current_headroom();
    assert!((sample.active_headroom() - 1.0).abs() < f32::EPSILON);
    assert!((sample.potential_headroom() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn mock_provider_clamps_invalid_and_inverted_headroom() {
    for (active, potential, expected_active, expected_potential) in [
        (f32::NAN, 2.5, 1.0, 2.5),
        (-2.0, -5.0, 1.0, 1.0),
        (0.5, 0.25, 1.0, 1.0),
        (f32::INFINITY, f32::INFINITY, 1.0, 1.0),
        (3.5, 1.5, 3.5, 3.5),
    ] {
        let provider = MockDisplayHeadroomProvider::new(active, potential);
        let sample = provider.current_headroom();
        assert!((sample.active_headroom() - expected_active).abs() < f32::EPSILON);
        assert!((sample.potential_headroom() - expected_potential).abs() < f32::EPSILON);
    }

    let provider = MockDisplayHeadroomProvider::new(1.0, 1.0);
    provider.set_headroom(f32::NAN, 0.5);
    let sample = provider.current_headroom();
    assert!((sample.active_headroom() - 1.0).abs() < f32::EPSILON);
    assert!((sample.potential_headroom() - 1.0).abs() < f32::EPSILON);

    provider.set_headroom(4.0, 2.0);
    let inverted = provider.current_headroom();
    assert!((inverted.active_headroom() - 4.0).abs() < f32::EPSILON);
    assert!((inverted.potential_headroom() - 4.0).abs() < f32::EPSILON);
}

#[test]
fn compositor_pins_rgba16float_across_headroom_transitions() {
    let provider = MockDisplayHeadroomProvider::new(1.0, 4.0);
    let mut compositor = DualPassCompositor::new(&provider);
    assert_eq!(compositor.surface_format(), SurfaceFormat::Rgba16Float);
    assert!((compositor.active_headroom() - 1.0).abs() < f32::EPSILON);

    provider.set_headroom(4.0, 4.0);
    compositor.sync_display_headroom();
    assert_eq!(compositor.surface_format(), SurfaceFormat::Rgba16Float);
    assert!((compositor.active_headroom() - 4.0).abs() < f32::EPSILON);

    provider.set_headroom(1.0, 1.0);
    compositor.sync_display_headroom();
    assert_eq!(compositor.surface_format(), SurfaceFormat::Rgba16Float);
    assert!((compositor.active_headroom() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn compositor_clamps_hdr_viewport_to_active_headroom_and_non_negative() {
    let provider = MockDisplayHeadroomProvider::new(2.5, 4.0);
    let compositor = DualPassCompositor::new(&provider);

    let (viewport, _) = compositor.composite_pixel([-0.5, 1.75, 5.0], [0.2, 0.4, 0.6]);
    assert!((viewport[0] - 0.0).abs() < f32::EPSILON);
    assert!((viewport[1] - 1.75).abs() < f32::EPSILON);
    assert!((viewport[2] - 2.5).abs() < f32::EPSILON);
}

#[test]
fn compositor_preserves_sdr_ui_luminance_invariance_across_edr_changes() {
    let provider = MockDisplayHeadroomProvider::new(1.0, 4.0);
    let mut compositor = DualPassCompositor::new(&provider);
    let ui_in = [0.18, 0.5, 0.92];

    let (_, ui_sdr) = compositor.composite_pixel([3.0, 3.0, 3.0], ui_in);

    provider.set_headroom(3.5, 4.0);
    compositor.sync_display_headroom();
    let (_, ui_xdr) = compositor.composite_pixel([3.0, 3.0, 3.0], ui_in);

    provider.set_headroom(1.0, 1.0);
    compositor.sync_display_headroom();
    let (_, ui_back) = compositor.composite_pixel([3.0, 3.0, 3.0], ui_in);

    for ch in 0..3 {
        assert_eq!(ui_sdr[ch].to_bits(), ui_xdr[ch].to_bits());
        assert_eq!(ui_sdr[ch].to_bits(), ui_back[ch].to_bits());
    }

    let (_, clamped_ui) = compositor.composite_pixel([1.0, 1.0, 1.0], [-0.25, 0.5, 2.5]);
    assert!((clamped_ui[0] - 0.0).abs() < f32::EPSILON);
    assert!((clamped_ui[1] - 0.5).abs() < f32::EPSILON);
    assert!((clamped_ui[2] - 1.0).abs() < f32::EPSILON);
}

#[test]
fn mock_provider_is_thread_safe_under_concurrent_updates() {
    let provider = MockDisplayHeadroomProvider::new(1.0, 4.0);

    std::thread::scope(|scope| {
        scope.spawn(|| {
            for step in 0..200_u32 {
                let active = 1.0 + (step % 4) as f32;
                provider.set_headroom(active, 4.0);
            }
        });

        for _ in 0..2 {
            scope.spawn(|| {
                for _ in 0..200 {
                    let sample = provider.current_headroom();
                    assert!(sample.active_headroom().is_finite());
                    assert!(sample.potential_headroom().is_finite());
                    assert!(sample.active_headroom() >= 1.0);
                    assert!(sample.potential_headroom() >= sample.active_headroom());
                }
            });
        }
    });
}

#[test]
fn compositor_debug_formatting_is_non_empty() {
    let provider = SdrFallbackHeadroomProvider;
    let compositor = DualPassCompositor::new(&provider);
    let formatted = format!("{compositor:?}");
    assert!(formatted.contains("DualPassCompositor"));
    assert!(formatted.contains("Rgba16Float"));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_provider_off_main_thread_falls_back_to_sdr() {
    let result = std::thread::scope(|scope| {
        let handle = scope.spawn(|| MacOsDisplayHeadroomProvider.current_headroom());
        handle.join()
    });
    assert!(result.is_ok());
    if let Ok(sample) = result {
        assert!(sample.active_headroom().is_finite());
        assert!(sample.potential_headroom().is_finite());
        assert!((sample.active_headroom() - 1.0).abs() < f32::EPSILON);
        assert!((sample.potential_headroom() - 1.0).abs() < f32::EPSILON);
    }
}
