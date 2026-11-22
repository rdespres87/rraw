//! Dedicated domain controller managing the active photograph session and its development state.

use crate::preview::{BaseRgbPreview, LoadedPhotoView};
use crate::state::{DarkroomGradingState, GradingPreset};
use rraw_core::SignedCfaBuffer;
use std::path::Path;

/// Active photo session state: raw sensor buffers, pre-demosaiced preview, and grading state.
pub struct PhotoSession {
    /// Currently loaded photograph metadata, if any.
    pub loaded_photo: Option<LoadedPhotoView>,
    /// Decoded signed CFA buffer in RAM.
    pub cached_cfa: Option<SignedCfaBuffer>,
    /// Pre-demosaiced linear RGB base preview for zero-latency slider scrubbing.
    pub base_preview: Option<BaseRgbPreview>,
    /// Darkroom development parameters.
    pub grading: DarkroomGradingState,
    /// Currently active development preset.
    pub current_preset: GradingPreset,
    /// Cached GPU texture handle for rendering in egui viewport.
    pub preview_texture: Option<egui::TextureHandle>,
    /// Cached GPU texture handle for rendering the untouched original photo.
    pub original_texture: Option<egui::TextureHandle>,
    /// Whether the user has toggled comparison mode to view the original untouched photo.
    pub is_comparing_original: bool,
    /// Flag signaling that preview texture must be re-rendered.
    pub needs_preview_update: bool,
}

impl Default for PhotoSession {
    fn default() -> Self {
        Self::new()
    }
}

impl PhotoSession {
    /// Creates an empty initial photo session with default grading and no loaded photo.
    #[must_use]
    pub fn new() -> Self {
        Self {
            loaded_photo: None,
            cached_cfa: None,
            base_preview: None,
            grading: DarkroomGradingState::default(),
            current_preset: GradingPreset::AsShot,
            preview_texture: None,
            original_texture: None,
            is_comparing_original: false,
            needs_preview_update: false,
        }
    }

    /// Returns the absolute path of the currently loaded photo, if present.
    #[must_use]
    pub fn active_path(&self) -> Option<&Path> {
        self.loaded_photo.as_ref().map(|p| p.path.as_path())
    }

    /// Returns true if a photo is loaded in the active session.
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.loaded_photo.is_some()
    }

    /// Signals that the active grading has changed and the preview texture must be regenerated.
    pub fn mark_dirty(&mut self) {
        self.needs_preview_update = true;
    }

    /// Resets the active grading parameters to the untouched raw sensor reference state.
    pub fn reset_to_untouched(&mut self) {
        self.grading = DarkroomGradingState::untouched_raw();
        self.current_preset = GradingPreset::AsShot;
        self.mark_dirty();
    }
}
