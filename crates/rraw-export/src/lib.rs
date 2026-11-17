#![forbid(unsafe_code)]

//! Multi-threaded batch export specifications and ISO 21496-1 Gain Map contracts.

use rraw_color::DisplayTransform;
use rraw_core::HeadroomSample;
use rraw_gpu::RenderSurfaceState;

/// Supported raster export output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Tiff16,
    Png16,
    Jpeg8Chroma444,
    Avif10,
    Avif12,
    JpegXl10,
    JpegXl16,
}

/// ISO 21496-1 Gain Map metadata emission mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GainMapMode {
    Disabled,
    /// Emits BOTH standardized binary payload (`tmap` ISOBMFF / `APP2` MPF) AND XMP `hdrgm:*`.
    Iso21496BinaryAndXmp,
}

/// Export configuration passed to the asynchronous batch queue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportSpec {
    pub format: ExportFormat,
    pub display_transform: DisplayTransform,
    pub gain_map: GainMapMode,
    pub target_headroom: HeadroomSample,
}

impl ExportSpec {
    /// Creates an SDR 16-bit PNG export spec without Gain Map.
    #[must_use]
    pub fn sdr_png16(display_transform: DisplayTransform) -> Self {
        Self {
            format: ExportFormat::Png16,
            display_transform,
            gain_map: GainMapMode::Disabled,
            target_headroom: HeadroomSample::new(1.0, 1.0),
        }
    }

    /// Creates a compatible `RenderSurfaceState` matching export target headroom.
    #[must_use]
    pub const fn preview_surface_state(&self) -> RenderSurfaceState {
        RenderSurfaceState::new(self.target_headroom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_core::SurfaceFormat;

    #[test]
    fn hdr_export_spec_supports_dual_iso21496_gain_map_and_rgba16float_preview() {
        let spec = ExportSpec {
            format: ExportFormat::Avif10,
            display_transform: DisplayTransform::AgX,
            gain_map: GainMapMode::Iso21496BinaryAndXmp,
            target_headroom: HeadroomSample::new(8.0, 16.0),
        };
        assert_eq!(spec.gain_map, GainMapMode::Iso21496BinaryAndXmp);
        let preview = spec.preview_surface_state();
        assert_eq!(preview.format, SurfaceFormat::Rgba16Float);
        assert!((preview.active_headroom - 8.0).abs() < f32::EPSILON);
    }
}
