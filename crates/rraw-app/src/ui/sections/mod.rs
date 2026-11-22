#![forbid(unsafe_code)]

//! Modular UI inspector sections decomposed from monolithic inspector view.

pub mod crop;
pub mod detail_denoise;
pub mod exposure_tone;
pub mod geometry;
pub mod masks;
pub mod optics;
pub mod soft_proofing;
pub mod straighten;
pub mod timelapse;
pub mod white_balance;

/// Standard return payload for UI darkroom sections.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SectionOutput {
    /// Indicates whether a parameter changed and requires marking custom preset.
    pub mark_custom_preset: bool,
    /// Indicates whether the active photo preview needs re-rendering.
    pub needs_preview_update: bool,
    /// Indicates whether the photo sidecar should be saved immediately.
    pub save_sidecar: bool,
}

impl SectionOutput {
    #[must_use]
    pub const fn nothing() -> Self {
        Self {
            mark_custom_preset: false,
            needs_preview_update: false,
            save_sidecar: false,
        }
    }

    #[must_use]
    pub const fn custom_and_preview() -> Self {
        Self {
            mark_custom_preset: true,
            needs_preview_update: true,
            save_sidecar: false,
        }
    }

    #[must_use]
    pub const fn preview_only() -> Self {
        Self {
            mark_custom_preset: false,
            needs_preview_update: true,
            save_sidecar: false,
        }
    }
}
