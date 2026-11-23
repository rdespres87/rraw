#![forbid(unsafe_code)]

//! Application shell state binding surface configuration to `DisplayHeadroomProvider`.

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
pub struct AppShellState<'a> {
    pub provider: &'a dyn DisplayHeadroomProvider,
    pub surface: RenderSurfaceState,
    pub edr_layer: EdrLayerConfig,
    pub scrubbing: ScrubbingPolicy,
    pub compositor: DualPassCompositor<'a>,
}

impl<'a> AppShellState<'a> {
    pub fn new(provider: &'a dyn DisplayHeadroomProvider) -> Self {
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
    pub fn refresh_display_headroom(&mut self) -> SurfaceFormat {
        let sample = self.provider.current_headroom();
        self.surface.apply_headroom(sample);
        self.compositor.sync_display_headroom();
        self.surface.format
    }
}

pub fn initialize_shell_summary() -> Result<String, ColorError> {
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
        "rraw-app ready | mode={:?} | format={:?} | edr_color={:?} | headroom={:.1} | sat_r={:.1} | halo={} | optics={:?} | export={:?} | token_ok={} | recipe={}",
        GpuContextMode::InteractiveMetalSurface,
        format,
        shell.edr_layer.color_space,
        shell.surface.active_headroom,
        sat.r(),
        shell.scrubbing.halo_px,
        OpticalResolutionTier::Tier2LensfunXml,
        export.format,
        is_token_current(token, token),
        sidecar.recipe_json.display(),
    ))
}
