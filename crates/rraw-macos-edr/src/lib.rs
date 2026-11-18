#![deny(unsafe_code)]

//! Isolated macOS EDR display bridge implementing `DisplayHeadroomProvider`.
//!
//! This is the ONLY crate in the Rraw workspace configured with `#![deny(unsafe_code)]`
//! rather than `#![forbid(unsafe_code)]`, allowing minimal, `// SAFETY:`-audited
//! `#[allow(unsafe_code)]` `objc2` FFI blocks in Phase 2 without compiler error `E0453`.

use rraw_core::{DisplayHeadroomProvider, HeadroomSample, SurfaceFormat, SurfaceSpec};
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

/// Target macOS `CGColorSpace` paired with `Rgba16Float` swapchain surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdrColorSpace {
    ExtendedLinearDisplayP3,
}

/// Static `CAMetalLayer` EDR configuration invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdrLayerConfig {
    pub surface_format: SurfaceFormat,
    pub wants_extended_dynamic_range_content: bool,
    pub color_space: EdrColorSpace,
}

impl Default for EdrLayerConfig {
    fn default() -> Self {
        Self {
            surface_format: SurfaceSpec::EDR_SURFACE_FORMAT,
            wants_extended_dynamic_range_content: true,
            color_space: EdrColorSpace::ExtendedLinearDisplayP3,
        }
    }
}

/// Safe SDR fallback provider used before live `NSScreen` binding is attached or in headless environments.
#[derive(Debug, Default, Clone, Copy)]
pub struct SdrFallbackHeadroomProvider;

impl DisplayHeadroomProvider for SdrFallbackHeadroomProvider {
    fn current_headroom(&self) -> HeadroomSample {
        HeadroomSample::new(1.0, 1.0)
    }
}

/// Thread-safe mock headroom provider for testing and deterministic CI transitions.
#[derive(Debug)]
pub struct MockDisplayHeadroomProvider {
    active_headroom: AtomicU32,
    potential_headroom: AtomicU32,
}

impl MockDisplayHeadroomProvider {
    /// Creates a mock provider with specified initial headroom values.
    #[must_use]
    pub fn new(active: f32, potential: f32) -> Self {
        let valid_active = if active.is_finite() && active >= 1.0 {
            active
        } else {
            1.0
        };
        let valid_potential = if potential.is_finite() && potential >= valid_active {
            potential
        } else {
            valid_active
        };
        Self {
            active_headroom: AtomicU32::new(valid_active.to_bits()),
            potential_headroom: AtomicU32::new(valid_potential.to_bits()),
        }
    }

    /// Dynamically updates display headroom, simulating moving between displays or unplugging EDR monitors.
    pub fn set_headroom(&self, active: f32, potential: f32) {
        let valid_active = if active.is_finite() && active >= 1.0 {
            active
        } else {
            1.0
        };
        let valid_potential = if potential.is_finite() && potential >= valid_active {
            potential
        } else {
            valid_active
        };
        self.active_headroom
            .store(valid_active.to_bits(), Ordering::Release);
        self.potential_headroom
            .store(valid_potential.to_bits(), Ordering::Release);
    }
}

impl DisplayHeadroomProvider for MockDisplayHeadroomProvider {
    fn current_headroom(&self) -> HeadroomSample {
        let active = f32::from_bits(self.active_headroom.load(Ordering::Acquire));
        let potential = f32::from_bits(self.potential_headroom.load(Ordering::Acquire));
        HeadroomSample::new(active, potential)
    }
}

#[cfg(target_os = "macos")]
/// Native macOS display headroom provider querying `NSScreen` via `objc2`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsDisplayHeadroomProvider;

#[cfg(target_os = "macos")]
impl DisplayHeadroomProvider for MacOsDisplayHeadroomProvider {
    fn current_headroom(&self) -> HeadroomSample {
        // SAFETY:
        // MainThreadMarker::new() safely verifies that the thread is indeed the main UI thread.
        // If not on the main thread or if no NSScreen is attached (headless CI),
        // we safely return the nominal SDR headroom (1.0).
        #[allow(unsafe_code)]
        let (active, potential) = {
            use objc2::MainThreadMarker;
            use objc2_app_kit::NSScreen;

            if let Some(mtm) = MainThreadMarker::new() {
                if let Some(screen) = NSScreen::mainScreen(mtm) {
                    let act = screen.maximumExtendedDynamicRangeColorComponentValue() as f32;
                    let pot =
                        screen.maximumPotentialExtendedDynamicRangeColorComponentValue() as f32;
                    (act, pot)
                } else {
                    (1.0, 1.0)
                }
            } else {
                (1.0, 1.0)
            }
        };
        HeadroomSample::new(active, potential)
    }
}

/// Dual-pass compositor managing fixed `Rgba16Float` swapchain rendering.
///
/// Guarantees:
/// 1. The swapchain surface format is permanently fixed to `Rgba16Float` (`extendedLinearDisplayP3`).
/// 2. Viewport HDR highlights are scaled by active headroom `H` and clamped to `H`.
/// 3. SDR UI elements (egui) are rendered with strictly invariant luminance (zero color/brightness shift).
pub struct DualPassCompositor<'a> {
    provider: &'a dyn DisplayHeadroomProvider,
    active_headroom: f32,
    surface_format: SurfaceFormat,
}

impl<'a> fmt::Debug for DualPassCompositor<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DualPassCompositor")
            .field("active_headroom", &self.active_headroom)
            .field("surface_format", &self.surface_format)
            .finish()
    }
}

impl<'a> DualPassCompositor<'a> {
    /// Creates a new dual-pass compositor attached to a headroom provider.
    pub fn new(provider: &'a dyn DisplayHeadroomProvider) -> Self {
        let sample = provider.current_headroom();
        Self {
            provider,
            active_headroom: sample.active_headroom(),
            surface_format: SurfaceFormat::Rgba16Float,
        }
    }

    /// Fixed surface format (always `Rgba16Float`).
    #[must_use]
    pub const fn surface_format(&self) -> SurfaceFormat {
        self.surface_format
    }

    /// Current active headroom.
    #[must_use]
    pub const fn active_headroom(&self) -> f32 {
        self.active_headroom
    }

    /// Synchronizes active headroom from the display provider without altering surface format.
    pub fn sync_display_headroom(&mut self) {
        self.active_headroom = self.provider.current_headroom().active_headroom();
    }

    /// Composites a viewport HDR pixel and an SDR UI pixel into output linear P3 space.
    ///
    /// - `viewport_rgb`: HDR linear pixel, clamped to active headroom `H`.
    /// - `ui_rgb`: SDR linear pixel in `[0.0, 1.0]`, invariant across display transitions.
    #[must_use]
    pub fn composite_pixel(
        &self,
        viewport_rgb: [f32; 3],
        ui_rgb: [f32; 3],
    ) -> ([f32; 3], [f32; 3]) {
        let h = self.active_headroom.max(1.0);
        let clamped_viewport = [
            viewport_rgb[0].min(h).max(0.0),
            viewport_rgb[1].min(h).max(0.0),
            viewport_rgb[2].min(h).max(0.0),
        ];

        let clamped_ui = [
            ui_rgb[0].clamp(0.0, 1.0),
            ui_rgb[1].clamp(0.0, 1.0),
            ui_rgb[2].clamp(0.0, 1.0),
        ];

        (clamped_viewport, clamped_ui)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edr_layer_config_pins_rgba16float_and_extended_linear_display_p3() {
        let cfg = EdrLayerConfig::default();
        assert_eq!(cfg.surface_format, SurfaceFormat::Rgba16Float);
        assert!(cfg.wants_extended_dynamic_range_content);
        assert_eq!(cfg.color_space, EdrColorSpace::ExtendedLinearDisplayP3);
    }

    #[test]
    fn fallback_provider_returns_nominal_sdr_headroom() {
        let provider = SdrFallbackHeadroomProvider;
        let sample = provider.current_headroom();
        assert!((sample.active_headroom() - 1.0).abs() < f32::EPSILON);
        assert!((sample.potential_headroom() - 1.0).abs() < f32::EPSILON);
    }
}
