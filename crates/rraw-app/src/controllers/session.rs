#![forbid(unsafe_code)]

//! Dedicated domain controller managing the active photograph session and its development state.

use crate::preview::{BaseRgbPreview, LoadedPhotoView};
use crate::state::{DarkroomGradingState, GradingPreset};
use rraw_core::SignedCfaBuffer;
use std::collections::VecDeque;
use std::path::Path;

/// Active photo session state: raw sensor buffers, pre-demosaiced preview, grading state, and bounded Undo/Redo stack.
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
    /// Bounded history stack of prior grading states for `Cmd+Z` Undo.
    pub undo_stack: VecDeque<DarkroomGradingState>,
    /// History stack of undone grading states for `Cmd+Shift+Z` Redo.
    pub redo_stack: Vec<DarkroomGradingState>,
    /// Indicates whether `grading` has been modified in memory and needs to be flushed to disk sidecars when pointer interaction ends.
    pub sidecar_dirty: bool,
    /// Tracks whether a continuous slider/curve drag is currently active so intermediate drag frames coalesce into a single Undo snapshot.
    pub is_continuous_drag_active: bool,
}

impl Default for PhotoSession {
    fn default() -> Self {
        Self::new()
    }
}

impl PhotoSession {
    /// Maximum number of Undo snapshots retained per active photograph session.
    pub const MAX_UNDO_HISTORY: usize = 64;

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
            undo_stack: VecDeque::with_capacity(Self::MAX_UNDO_HISTORY),
            redo_stack: Vec::new(),
            sidecar_dirty: false,
            is_continuous_drag_active: false,
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

    /// Records the current grading state onto the bounded Undo stack before a discrete mutation,
    /// deduplicating identical consecutive snapshots and clearing Redo history.
    pub fn record_undo_snapshot(&mut self) {
        if self.undo_stack.back() == Some(&self.grading) {
            return;
        }
        if self.undo_stack.len() >= Self::MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(self.grading.clone());
        self.redo_stack.clear();
    }

    /// Pushes an explicit pre-edit snapshot onto the Undo stack if it differs from the current grading state.
    pub fn push_pre_edit_snapshot(&mut self, pre_edit: DarkroomGradingState) {
        if pre_edit == self.grading {
            return;
        }
        if self.undo_stack.back() != Some(&pre_edit) {
            if self.undo_stack.len() >= Self::MAX_UNDO_HISTORY {
                self.undo_stack.pop_front();
            }
            self.undo_stack.push_back(pre_edit);
        }
        self.redo_stack.clear();
    }

    /// Notifies the session that grading parameters were edited during a UI frame.
    /// When `pointer_down` is true across consecutive frames (continuous slider or curve scrubbing),
    /// only the initial pre-drag snapshot is pushed onto `undo_stack`, and `sidecar_dirty` is set
    /// without blocking the UI thread on disk `fsync`.
    pub fn notify_grading_edited(&mut self, pre_edit: DarkroomGradingState, pointer_down: bool) {
        if self.record_grading_edit_without_preview(pre_edit, pointer_down) {
            self.needs_preview_update = true;
        }
    }

    /// Notifies the session that overlay-only geometry parameters (such as the interactive crop box)
    /// were edited, coalescing Undo snapshots and marking sidecars dirty without forcing a full
    /// preview texture recomputation on every drag frame.
    pub fn notify_grading_edited_deferred_preview(
        &mut self,
        pre_edit: DarkroomGradingState,
        pointer_down: bool,
    ) {
        let _ = self.record_grading_edit_without_preview(pre_edit, pointer_down);
    }

    fn record_grading_edit_without_preview(
        &mut self,
        pre_edit: DarkroomGradingState,
        pointer_down: bool,
    ) -> bool {
        if pre_edit == self.grading {
            if !pointer_down {
                self.is_continuous_drag_active = false;
            }
            return false;
        }

        if pointer_down {
            if !self.is_continuous_drag_active {
                self.push_pre_edit_snapshot(pre_edit);
                self.is_continuous_drag_active = true;
            }
        } else {
            self.is_continuous_drag_active = false;
            self.push_pre_edit_snapshot(pre_edit);
        }

        self.sidecar_dirty = true;
        true
    }

    /// Returns true if at least one Undo snapshot is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns true if at least one Redo snapshot is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Returns the number of available Undo snapshots.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Returns the number of available Redo snapshots.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Restores the most recent Undo snapshot, moving the current state to the Redo stack.
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo_stack.pop_back() else {
            return false;
        };
        let current = std::mem::replace(&mut self.grading, prev);
        self.redo_stack.push(current);
        self.current_preset = self.grading.preset;
        self.needs_preview_update = true;
        self.sidecar_dirty = true;
        self.is_continuous_drag_active = false;
        true
    }

    /// Re-applies the most recent Redo snapshot, pushing the prior state back onto the Undo stack.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        if self.undo_stack.len() >= Self::MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
        let current = std::mem::replace(&mut self.grading, next);
        self.undo_stack.push_back(current);
        self.current_preset = self.grading.preset;
        self.needs_preview_update = true;
        self.sidecar_dirty = true;
        self.is_continuous_drag_active = false;
        true
    }

    /// Clears all Undo/Redo history and resets continuous drag and dirty sidecar flags (called on photo switch).
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_continuous_drag_active = false;
        self.sidecar_dirty = false;
    }

    /// Resets the active grading parameters to the untouched raw sensor reference state, recording an Undo snapshot.
    pub fn reset_to_untouched(&mut self) {
        self.record_undo_snapshot();
        self.grading = DarkroomGradingState::untouched_raw();
        self.current_preset = GradingPreset::AsShot;
        self.sidecar_dirty = true;
        self.mark_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deferred_preview_edit_preserves_undo_without_forcing_preview_rerender() {
        let mut session = PhotoSession {
            needs_preview_update: false,
            ..Default::default()
        };

        let initial = session.grading.clone();
        session.grading.geometry.crop_left = 0.1;
        session.notify_grading_edited_deferred_preview(initial.clone(), true);

        assert!(!session.needs_preview_update);
        assert!(session.sidecar_dirty);
        assert_eq!(session.undo_len(), 1);

        // Second continuous drag frame coalesces into the same Undo snapshot and still skips preview rerender
        let mid_drag = session.grading.clone();
        session.grading.geometry.crop_left = 0.25;
        session.notify_grading_edited_deferred_preview(mid_drag, true);

        assert!(!session.needs_preview_update);
        assert_eq!(session.undo_len(), 1);

        // Undo restores the initial pre-drag crop rectangle
        assert!(session.undo());
        assert_eq!(session.grading, initial);
    }
}
