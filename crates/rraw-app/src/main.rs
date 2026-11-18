#![forbid(unsafe_code)]

//! Native macOS Rraw application entry point wiring all 9 workspace library crates.

use std::path::Path;

use rraw_ai::is_token_current;
use rraw_color::{stage2_saturation_ceilings, ColorError, DisplayTransform};
use rraw_core::{
    DisplayHeadroomProvider, PhotoId, Stage0Epoch, Stage0Token, SurfaceFormat,
    WhiteBalanceMultipliers,
};
use rraw_decode::DecodeError;
use rraw_export::ExportSpec;
use rraw_gpu::{GpuContextMode, RenderSurfaceState, ScrubbingPolicy};
use rraw_macos_edr::{DualPassCompositor, EdrLayerConfig, SdrFallbackHeadroomProvider};
use rraw_optics::OpticalResolutionTier;
use rraw_storage::SidecarPaths;

/// Application shell state binding surface configuration to `DisplayHeadroomProvider`.
struct AppShellState<'a> {
    provider: &'a dyn DisplayHeadroomProvider,
    surface: RenderSurfaceState,
    edr_layer: EdrLayerConfig,
    scrubbing: ScrubbingPolicy,
    compositor: DualPassCompositor<'a>,
}

impl<'a> AppShellState<'a> {
    fn new(provider: &'a dyn DisplayHeadroomProvider) -> Self {
        let headroom = provider.current_headroom();
        Self {
            provider,
            surface: RenderSurfaceState::new(headroom),
            edr_layer: EdrLayerConfig::default(),
            scrubbing: ScrubbingPolicy::default(),
            compositor: DualPassCompositor::new(provider),
        }
    }

    /// Refreshes active display headroom while keeping the surface format fixed at `Rgba16Float`.
    fn refresh_display_headroom(&mut self) -> SurfaceFormat {
        let sample = self.provider.current_headroom();
        self.surface.apply_headroom(sample);
        self.compositor.sync_display_headroom();
        self.surface.format
    }
}

fn initialize_shell_summary() -> Result<String, ColorError> {
    let provider = SdrFallbackHeadroomProvider;
    let mut shell = AppShellState::new(&provider);
    let format = shell.refresh_display_headroom();
    let wb = WhiteBalanceMultipliers::new(2.0, 1.0, 1.5)?;
    let sat = stage2_saturation_ceilings(wb)?;
    let token = Stage0Token {
        photo_id: PhotoId(1),
        epoch: Stage0Epoch(1),
    };
    let sidecar = SidecarPaths::from_raw_path(Path::new("preview.raf"));
    let export = ExportSpec::sdr_png16(DisplayTransform::AgX);
    let _decode_err_marker = std::any::type_name::<DecodeError>();

    Ok(format!(
        "rraw-app ready | mode={:?} | format={:?} | edr_color={:?} | headroom={:.1} | sat_r={:.1} | halo={} | optics={:?} | export={:?} | token_ok={} | xmp={}",
        GpuContextMode::InteractiveMetalSurface,
        format,
        shell.edr_layer.color_space,
        shell.surface.active_headroom,
        sat.r(),
        shell.scrubbing.halo_px,
        OpticalResolutionTier::Tier2LensfunXml,
        export.format,
        is_token_current(token, token),
        sidecar.xmp_sidecar.display(),
    ))
}

fn main() -> std::process::ExitCode {
    match initialize_shell_summary() {
        Ok(summary) => {
            println!("{summary}");
            std::process::ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("rraw-app initialization failed: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use rraw_core::HeadroomSample;

    struct MockHeadroomProvider {
        /// Stores encoded headroom * 10 as integer for lock-free interior mutability.
        active_tenths: AtomicU32,
    }

    impl DisplayHeadroomProvider for MockHeadroomProvider {
        fn current_headroom(&self) -> HeadroomSample {
            let active = self.active_tenths.load(Ordering::Acquire) as f32 / 10.0;
            HeadroomSample::new(active, 16.0)
        }
    }

    #[test]
    fn xdr_to_sdr_display_switch_keeps_rgba16float_fixed_and_clamps_headroom() {
        let provider = MockHeadroomProvider {
            active_tenths: AtomicU32::new(40),
        };
        let mut shell = AppShellState::new(&provider);
        assert_eq!(shell.surface.format, SurfaceFormat::Rgba16Float);
        assert!((shell.surface.active_headroom - 4.0).abs() < 1e-5);

        // Simulate dragging window from XDR display (4.0) to external SDR monitor (1.0).
        provider.active_tenths.store(10, Ordering::Release);
        let format_after = shell.refresh_display_headroom();

        assert_eq!(format_after, SurfaceFormat::Rgba16Float);
        assert_eq!(shell.surface.format, SurfaceFormat::Rgba16Float);
        assert!((shell.surface.active_headroom - 1.0).abs() < 1e-5);
    }

    #[test]
    fn shell_initialization_succeeds() -> Result<(), ColorError> {
        let summary = initialize_shell_summary()?;
        assert!(summary.contains("Rgba16Float"));
        assert!(summary.contains("ExtendedLinearDisplayP3"));
        Ok(())
    }
}
