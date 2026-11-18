use rraw_core::{DisplayHeadroomProvider, SurfaceFormat};
use rraw_macos_edr::{DualPassCompositor, MockDisplayHeadroomProvider};

#[test]
fn test_mock_headroom_provider_transition() {
    let provider = MockDisplayHeadroomProvider::new(4.0, 16.0);
    assert_eq!(provider.current_headroom().active_headroom(), 4.0);
    assert_eq!(provider.current_headroom().potential_headroom(), 16.0);

    provider.set_headroom(1.0, 1.0);
    assert_eq!(provider.current_headroom().active_headroom(), 1.0);
    assert_eq!(provider.current_headroom().potential_headroom(), 1.0);

    // Negative / NaN handling
    provider.set_headroom(f32::NAN, -4.0);
    assert_eq!(provider.current_headroom().active_headroom(), 1.0);
    assert_eq!(provider.current_headroom().potential_headroom(), 1.0);
}

#[test]
fn test_dual_pass_compositor_edr_to_sdr_transition() {
    let provider = MockDisplayHeadroomProvider::new(4.0, 16.0);
    let mut compositor = DualPassCompositor::new(&provider);

    // 1. Swapchain format invariant: permanently fixed Rgba16Float
    assert_eq!(compositor.surface_format(), SurfaceFormat::Rgba16Float);

    // 2. High headroom pass: EDR viewport renders peak highlights up to active headroom (H=4.0)
    let raw_viewport_pixel = [2.5_f32, 2.5_f32, 2.5_f32];
    let ui_pixel = [0.85_f32, 0.85_f32, 0.85_f32];

    let (viewport_out_edr, ui_out_edr) = compositor.composite_pixel(raw_viewport_pixel, ui_pixel);

    // Highlight is preserved in extended dynamic range (> 1.0)
    assert!(
        viewport_out_edr[0] > 1.0,
        "Viewport highlight must extend above 1.0 in EDR mode: {}",
        viewport_out_edr[0]
    );
    assert_eq!(ui_out_edr, ui_pixel);

    // 3. Dynamic transition: Display moves to SDR (H=1.0)
    provider.set_headroom(1.0, 1.0);
    compositor.sync_display_headroom();

    // Invariant: Swapchain format NEVER changes across display transitions
    assert_eq!(compositor.surface_format(), SurfaceFormat::Rgba16Float);

    let (viewport_out_sdr, ui_out_sdr) = compositor.composite_pixel(raw_viewport_pixel, ui_pixel);

    // Invariant: Viewport output is clamped to active headroom (1.0 in SDR)
    assert!(
        viewport_out_sdr[0] <= 1.0 + f32::EPSILON,
        "Viewport output must clamp to 1.0 in SDR mode: {}",
        viewport_out_sdr[0]
    );

    // Invariant: UI elements have identical luminance (zero color/brightness shift)
    assert_eq!(
        ui_out_sdr, ui_out_edr,
        "UI luminance must be invariant across display headroom transitions"
    );
    assert_eq!(ui_out_sdr, ui_pixel);
}
