#![deny(unsafe_code)]

//! Isolated macOS EDR display bridge implementing `DisplayHeadroomProvider`.
//!
//! This is the ONLY crate in the Rraw workspace configured with `#![deny(unsafe_code)]`
//! rather than `#![forbid(unsafe_code)]`, allowing minimal, `// SAFETY:`-audited
//! `#[allow(unsafe_code)]` `objc2` FFI blocks in Phase 2 without compiler error `E0453`.

use rraw_core::{DisplayHeadroomProvider, HeadroomSample, SurfaceFormat, SurfaceSpec};

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

/// Safe SDR fallback provider used before live `NSScreen` binding is attached.
#[derive(Debug, Default, Clone, Copy)]
pub struct SdrFallbackHeadroomProvider;

impl DisplayHeadroomProvider for SdrFallbackHeadroomProvider {
    fn current_headroom(&self) -> HeadroomSample {
        HeadroomSample::new(1.0, 1.0)
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
