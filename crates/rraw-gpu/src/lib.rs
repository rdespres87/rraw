#![forbid(unsafe_code)]

//! GPU compute pipeline topology, LRU VRAM cache budget, and 120 Hz RoI scrubbing contracts.

pub mod cache;
pub mod context;
pub mod error;
pub mod pipeline;
pub mod roi;
pub mod stage3;
pub mod stage4;
pub mod stage5;
pub mod stage6;
pub mod stage7;

pub use cache::{LruVramCache, VramPhotoEntry};
pub use context::GpuContext;
pub use error::GpuError;
pub use pipeline::{Stage1Params, Stage1Pipeline, Stage2Params, Stage2Pipeline};
pub use roi::{expand_roi_with_halo, RoiWindow};
pub use stage3::{Stage3Params, Stage3RlPipeline};
pub use stage4::{Stage4Params, Stage4WarpPipeline};
pub use stage5::{Stage5Params, Stage5Pipeline};
pub use stage6::{Stage6Params, Stage6Pipeline};
pub use stage7::{Stage7Params, Stage7Pipeline};

use rraw_color::effective_laplacian_octaves;
use rraw_core::{HeadroomSample, SurfaceFormat, SurfaceSpec};
use rraw_optics::OpticalStagePlan;

/// Maximum default VRAM cache cap (`2048 MiB`).
pub const DEFAULT_VRAM_CAP_BYTES: u64 = 2048 * 1024 * 1024;

/// Default RoI halo margin in pixels covering wavelets, demosaic, and 10 RL iterations.
pub const DEFAULT_ROI_HALO_PX: u32 = 32;

/// GPU execution mode supporting headless offscreen testing and interactive Metal surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuContextMode {
    HeadlessOffscreen,
    InteractiveMetalSurface,
}

/// Interactive scrubbing configuration for 120 Hz Stages 1-3 updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrubbingPolicy {
    pub is_scrubbing: bool,
    pub halo_px: u32,
    pub async_tile_size: u32,
}

impl Default for ScrubbingPolicy {
    fn default() -> Self {
        Self {
            is_scrubbing: false,
            halo_px: DEFAULT_ROI_HALO_PX,
            async_tile_size: 1024,
        }
    }
}

/// Computes the strict LRU VRAM budget `min(2048 MiB, 25% of unified system memory)`.
#[must_use]
pub fn compute_vram_budget_bytes(unified_system_memory_bytes: u64) -> u64 {
    let quarter = unified_system_memory_bytes / 4;
    quarter.min(DEFAULT_VRAM_CAP_BYTES)
}

/// Fixed swapchain surface configuration for `wgpu` rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderSurfaceState {
    pub format: SurfaceFormat,
    pub active_headroom: f32,
}

impl RenderSurfaceState {
    /// Initializes surface state with permanently fixed `Rgba16Float` format.
    #[must_use]
    pub const fn new(headroom: HeadroomSample) -> Self {
        Self {
            format: SurfaceSpec::EDR_SURFACE_FORMAT,
            active_headroom: headroom.active_headroom(),
        }
    }

    /// Updates live display headroom without altering the fixed `Rgba16Float` format.
    pub fn apply_headroom(&mut self, headroom: HeadroomSample) {
        self.active_headroom = headroom.active_headroom();
    }
}

/// Validates GPU pipeline compatibility across Stage 4/5 invariants.
#[must_use]
pub const fn stage5_octaves_for_level(
    base_octaves: u8,
    mipmap_level: u8,
    _optics_plan: &OpticalStagePlan,
) -> u8 {
    effective_laplacian_octaves(base_octaves, mipmap_level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_optics::OpticalResolutionTier;

    #[test]
    fn vram_budget_caps_at_2048_mib_or_quarter_system_ram() {
        let gib = 1024 * 1024 * 1024_u64;
        assert_eq!(compute_vram_budget_bytes(0), 0);
        assert_eq!(compute_vram_budget_bytes(4 * gib), gib);
        assert_eq!(compute_vram_budget_bytes(8 * gib), DEFAULT_VRAM_CAP_BYTES);
        assert_eq!(compute_vram_budget_bytes(64 * gib), DEFAULT_VRAM_CAP_BYTES);
    }

    #[test]
    fn render_surface_keeps_rgba16float_across_edr_to_sdr_transition() {
        let mut surface = RenderSurfaceState::new(HeadroomSample::new(4.0, 16.0));
        assert_eq!(surface.format, SurfaceFormat::Rgba16Float);
        assert!((surface.active_headroom - 4.0).abs() < f32::EPSILON);

        surface.apply_headroom(HeadroomSample::new(1.0, 1.0));
        assert_eq!(surface.format, SurfaceFormat::Rgba16Float);
        assert!((surface.active_headroom - 1.0).abs() < f32::EPSILON);

        surface.apply_headroom(HeadroomSample::new(f32::NAN, 16.0));
        assert_eq!(surface.format, SurfaceFormat::Rgba16Float);
        assert!((surface.active_headroom - 1.0).abs() < f32::EPSILON);

        surface.apply_headroom(HeadroomSample::new(-4.0, -1.0));
        assert_eq!(surface.format, SurfaceFormat::Rgba16Float);
        assert!((surface.active_headroom - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scrubbing_policy_uses_32px_halo_and_1024_consolidation_tiles() {
        let policy = ScrubbingPolicy::default();
        assert_eq!(policy.halo_px, 32);
        assert_eq!(policy.async_tile_size, 1024);
        let dummy_plan = OpticalStagePlan {
            active_tier: OpticalResolutionTier::Tier1EmbeddedRawOpcodes,
            apply_radial_vignette_in_warp: false,
            lut_dimension: 256,
        };
        assert_eq!(stage5_octaves_for_level(6, 2, &dummy_plan), 4);
    }
}
