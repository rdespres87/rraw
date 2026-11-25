#![forbid(unsafe_code)]

//! Library navigation, directory polling, DAM indexing, ratings, filtering, and RAW file opening for `RrawApp`.

use std::path::{Path, PathBuf};

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::preview::*;
use crate::state::*;

impl RrawApp {
    /// Opens (or creates) `.rraw/cache.db` for `abs_dir`, preloads all cached star ratings
    /// synchronously for Frame-0 filmstrip rendering, and spawns the background sidecar indexer.
    pub fn initialize_session_dam(&mut self, abs_dir: &Path) {
        match rraw_storage::SqliteDamIndex::open_session_database(abs_dir) {
            Ok(dam) => {
                if let Ok(ratings) = dam.load_all_ratings() {
                    for (path, rating) in ratings {
                        self.library.photo_ratings.insert(path, rating);
                    }
                }
                self.workers
                    .spawn_directory_indexer(dam.clone(), self.library.directory_entries.clone());
                self.library.session_dam = Some(dam);
            }
            Err(err) => {
                eprintln!("Failed to open session database: {err}");
                self.library.session_dam = None;
            }
        }
    }

    /// Dispatches a thumbnail generation request to background worker pool if not already loaded or pending.
    pub fn request_thumbnail(&mut self, path: PathBuf) {
        if self.workers.thumbnail_textures.contains_key(&path)
            || !self.workers.thumbnail_pending.insert(path.clone())
        {
            return;
        }

        let _ = self.workers.thumbnail_req_tx.send(path);
    }

    /// Changes the active directory in the file explorer and preloads sidecars.
    pub fn open_directory(&mut self, dir: &Path) {
        let abs_dir = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(dir))
                .unwrap_or_else(|_| dir.to_path_buf())
        };

        self.library.current_directory = Some(abs_dir.clone());
        self.library.directory_entries = scan_directory_for_images(&abs_dir);
        self.library.refresh_subdirectories();

        // Evict GPU textures of thumbnails belonging to previously opened folders
        self.workers
            .thumbnail_textures
            .prune_not_in(&self.library.directory_entries);
        self.workers
            .thumbnail_pending
            .retain(|p| self.library.directory_entries.contains(p));

        let raw_count = self
            .library
            .directory_entries
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(crate::preview::is_raw_extension)
            })
            .count();
        self.library.raw_only_filter = self.layout_config.raw_only_filter.unwrap_or(raw_count > 0);

        // Track last opened directory in persistent layout configuration
        self.layout_config.last_open_directory = Some(abs_dir.clone());
        self.layout_config.save();

        // 1. Open SQLite session database (creates .rraw/cache.db) with WAL pragmas and preload Frame-0 ratings
        self.initialize_session_dam(&abs_dir);

        // Only preload sidecar synchronously for the currently active photo (if in this directory).
        // Other photos in the folder have their ratings streamed asynchronously by `spawn_directory_indexer`
        // and their full `.rraw.json` recipes loaded lazily on demand.
        if let Some(active_path) = self
            .session
            .loaded_photo
            .as_ref()
            .map(|p| p.path.clone())
            .or_else(|| self.layout_config.last_open_photo.clone())
        {
            if active_path.parent() == Some(abs_dir.as_path()) {
                let (grading_opt, rating_opt) = load_photo_sidecars(&active_path);
                if let Some(g) = grading_opt {
                    self.library.photo_gradings.insert(active_path.clone(), g);
                }
                if let Some(r) = rating_opt {
                    self.library.photo_ratings.insert(active_path, r);
                }
            }
        }

        self.status_message = format!(
            "Directory: {} ({} photos found)",
            abs_dir.display(),
            self.library.directory_entries.len()
        );
    }

    /// Polls the currently open directory for newly added or deleted image files without requiring a manual folder reopen.
    pub fn poll_current_directory_changes(&mut self) -> bool {
        let Some(dir) = self.library.current_directory.clone() else {
            return false;
        };
        let mut latest = self.library.directory_entries.clone();
        if !sync_directory_entries(&dir, &mut latest) {
            return false;
        }

        for path in &latest {
            if !self.library.directory_entries.contains(path) {
                let (grading_opt, rating_opt) = load_photo_sidecars(path);
                if let Some(g) = grading_opt {
                    self.library.photo_gradings.insert(path.clone(), g);
                }
                if let Some(r) = rating_opt {
                    self.library.photo_ratings.insert(path.clone(), r);
                }
            }
        }
        self.library.selected_photos.retain(|p| latest.contains(p));
        self.library.directory_entries = latest;
        self.library.refresh_subdirectories();
        true
    }

    /// Triggers background prefetching of immediate adjacent photos (+1, -1) for instant switching.
    pub fn trigger_prefetch(&self, around_path: &Path) {
        if let Some(pos) = self
            .library
            .directory_entries
            .iter()
            .position(|p| p == around_path)
        {
            let mut candidates = Vec::with_capacity(2);
            if pos + 1 < self.library.directory_entries.len() {
                candidates.push(self.library.directory_entries[pos + 1].clone());
            }
            if pos > 0 {
                candidates.push(self.library.directory_entries[pos - 1].clone());
            }

            for path in candidates {
                if !self.photo_cache.iter().any(|e| e.path == path) {
                    let req = self.workers.decode_coordinator.create_request(path, true);
                    let _ = self.workers.decode_req_tx.send(req);
                }
            }
        }
    }

    /// Returns the number of photos in `directory_entries` with rating >= `min_stars`.
    pub fn count_photos_with_min_rating(&self, min_stars: u8) -> usize {
        self.library
            .directory_entries
            .iter()
            .filter(|p| {
                let rating = self.library.photo_ratings.get(*p).copied().unwrap_or(0);
                rating >= min_stars
            })
            .count()
    }

    /// Returns the number of photos in `directory_entries` with 0 stars (unrated).
    pub fn count_photos_unrated(&self) -> usize {
        self.library
            .directory_entries
            .iter()
            .filter(|p| {
                let rating = self.library.photo_ratings.get(*p).copied().unwrap_or(0);
                rating == 0
            })
            .count()
    }

    /// Toggles both clipping warning layers together (`J` shortcut).
    pub fn toggle_clipping_overlay(&mut self) {
        let enable = !self.viewport.clipping_overlay.is_active();
        self.viewport.clipping_overlay = crate::view_tools::ClippingOverlay {
            shadows: enable,
            highlights: enable,
        };
        self.session.needs_preview_update = true;
    }

    /// Handles the Escape key press hierarchically:
    /// 1. Closes open modal dialogs (shortcuts cheat sheet, export dialog).
    /// 2. Deactivates active interactive viewport tools (crop editing, perspective lines, brush mask).
    /// 3. Clears the filmstrip batch selection if no dialog or tool is active.
    ///
    /// Returns `true` if an active dialog, tool, or selection was dismissed.
    pub fn handle_escape_key(&mut self) -> bool {
        if self.export.show_about_dialog {
            self.export
                .close_dialog(crate::controllers::export::ActiveModalDialog::About);
            true
        } else if self.export.show_shortcuts_dialog {
            self.export
                .close_dialog(crate::controllers::export::ActiveModalDialog::Shortcuts);
            true
        } else if self.export.show_export_dialog {
            self.export
                .close_dialog(crate::controllers::export::ActiveModalDialog::Export);
            true
        } else if self.viewport.is_crop_editing {
            self.viewport.is_crop_editing = false;
            self.viewport.crop_drag_handle = CropDragHandle::None;
            self.status_message = self.t(Key::EscapeCropDeactivated).to_string();
            true
        } else if self.viewport.horizon_tool_active {
            self.viewport.horizon_tool_active = false;
            self.viewport.horizon_start_point = None;
            self.status_message = self.t(Key::EscapeHorizonDeactivated).to_string();
            true
        } else if self.viewport.parallel_tool_phase != ParallelToolPhase::Inactive {
            self.viewport.parallel_tool_phase = ParallelToolPhase::Inactive;
            self.viewport.parallel_line1 = None;
            self.viewport.parallel_line2 = None;
            self.viewport.parallel_drag_start = None;
            self.status_message = self.t(Key::EscapePerspectiveDeactivated).to_string();
            true
        } else if self.session.grading.masks.active_brush_mask_idx.is_some() {
            self.session.grading.masks.active_brush_mask_idx = None;
            self.status_message = self.t(Key::EscapeBrushDeactivated).to_string();
            true
        } else if !self.library.selected_photos.is_empty() {
            let prev = self.library.selected_photos.len();
            self.library.selected_photos.clear();
            self.status_message = self.t(Key::EscapeSelectionReset).to_string();
            prev > 0
        } else {
            false
        }
    }

    /// Drains pending background DAM session indexing updates (bounded to max_events to preserve 120 FPS).
    pub fn drain_dam_indexing_events(&mut self, max_events: usize) -> usize {
        let mut drained = 0;
        while drained < max_events {
            match self.workers.dam_index_receiver.try_recv() {
                Ok((path, rating)) => {
                    self.library.photo_ratings.insert(path, rating);
                    drained += 1;
                }
                Err(_) => break,
            }
        }
        drained
    }

    /// Sets the rating (0 to 5) of the currently loaded photo and persists to `.rraw.json` and SQLite session DAM.
    pub fn set_current_photo_rating(&mut self, rating: u8) {
        if let Some(photo) = &mut self.session.loaded_photo {
            let path = photo.path.clone();
            self.set_photo_rating_for_path(&path, rating);
        }
    }

    /// Sets the rating of any photo in the library, persisting to `.rraw.json` and asynchronously
    /// enqueueing the SQLite session DAM update off the UI thread.
    pub fn set_photo_rating_for_path(&mut self, photo_path: &Path, rating: u8) {
        self.library
            .photo_ratings
            .insert(photo_path.to_path_buf(), rating);

        if let Some(photo) = &mut self.session.loaded_photo {
            if photo.path == photo_path {
                photo.rating = rating;
            }
        }
        if let Some(entry) = self.photo_cache.iter_mut().find(|e| e.path == photo_path) {
            entry.photo_view.rating = rating;
        }

        // Save immediately to adjacent .rraw/<photo>.rraw.json
        let grading = self
            .library
            .photo_gradings
            .get(photo_path)
            .cloned()
            .unwrap_or_else(|| self.session.grading.clone());
        save_photo_sidecars(photo_path, &grading, rating);

        // Bidirectional sync with SQLite session DAM
        if let Some(dam) = &self.library.session_dam {
            let _ = dam.update_ratings(
                photo_path,
                rating,
                rraw_core::ColorLabel::None,
                rraw_core::PickStatus::Unflagged,
            );
        }

        let star_text = if rating > 0 {
            "★".repeat(rating as usize)
        } else {
            "Unrated".to_string()
        };
        self.status_message = format!(
            "Rated {}: {} (Saved to .rraw.json & session DAM)",
            photo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("photo"),
            star_text
        );
    }

    /// Checks if a file path matches the currently active filmstrip filters (rating + raw-only).
    #[must_use]
    pub fn matches_active_filter(&self, path: &Path) -> bool {
        let rating = self.library.photo_ratings.get(path).copied().unwrap_or(0);
        let rating_match = self.library.rating_filter.matches(rating);
        let raw_match = !self.library.raw_only_filter
            || path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(crate::preview::is_raw_extension);
        rating_match && raw_match
    }

    /// Returns the first photo in `directory_entries` that satisfies active filmstrip filters.
    #[must_use]
    pub fn first_filtered_entry(&self) -> Option<PathBuf> {
        self.library
            .directory_entries
            .iter()
            .find(|p| self.matches_active_filter(p))
            .cloned()
    }

    /// Navigates relative to the current photo in the filtered filmstrip.
    pub fn navigate_relative(&mut self, delta: isize) {
        self.navigate_relative_with_shift(delta, false);
    }

    /// Navigates relative to current photo with optional Shift-range selection extension.
    pub fn navigate_relative_with_shift(&mut self, delta: isize, shift: bool) {
        let filtered: Vec<PathBuf> = self
            .library
            .directory_entries
            .iter()
            .filter(|p| self.matches_active_filter(p))
            .cloned()
            .collect();

        if filtered.is_empty() {
            return;
        }

        let current_path = self
            .session
            .loaded_photo
            .as_ref()
            .map(|p| &p.path)
            .or(self.layout_config.last_open_photo.as_ref());
        let current_idx = current_path
            .and_then(|cp| filtered.iter().position(|p| p == cp))
            .unwrap_or(0);

        let new_idx = (current_idx as isize + delta).rem_euclid(filtered.len() as isize) as usize;
        let target = filtered[new_idx].clone();

        if shift {
            let anchor = self.library.last_selected_index.unwrap_or(current_idx);
            let min_i = anchor.min(new_idx);
            let max_i = anchor.max(new_idx);
            for i in min_i..=max_i {
                if let Some(p) = filtered.get(i) {
                    self.library.selected_photos.insert(p.clone());
                }
            }
        } else {
            self.library.last_selected_index = Some(new_idx);
        }

        self.library.scroll_filmstrip_to_active = true;
        self.open_raw_file(&target);
    }

    /// Navigates directly to an absolute index in the filtered filmstrip (e.g. 0 for Home, usize::MAX for End).
    pub fn navigate_to_filtered_index(&mut self, index: usize, shift: bool) {
        let filtered: Vec<PathBuf> = self
            .library
            .directory_entries
            .iter()
            .filter(|p| self.matches_active_filter(p))
            .cloned()
            .collect();

        if filtered.is_empty() {
            return;
        }

        let new_idx = index.min(filtered.len() - 1);
        let target = filtered[new_idx].clone();

        if shift {
            let current_path = self
                .session
                .loaded_photo
                .as_ref()
                .map(|p| &p.path)
                .or(self.layout_config.last_open_photo.as_ref());
            let current_idx = current_path
                .and_then(|cp| filtered.iter().position(|p| p == cp))
                .unwrap_or(0);
            let anchor = self.library.last_selected_index.unwrap_or(current_idx);
            let min_i = anchor.min(new_idx);
            let max_i = anchor.max(new_idx);
            for i in min_i..=max_i {
                if let Some(p) = filtered.get(i) {
                    self.library.selected_photos.insert(p.clone());
                }
            }
        } else {
            self.library.last_selected_index = Some(new_idx);
        }

        self.library.scroll_filmstrip_to_active = true;
        self.open_raw_file(&target);
    }

    /// Inserts an entry into the LRU photo cache, enforcing both count and byte-budget limits.
    pub fn insert_cached_photo(&mut self, entry: CachedPhotoEntry) {
        if let Some(pos) = self.photo_cache.iter().position(|e| e.path == entry.path) {
            self.photo_cache.remove(pos);
        }

        // Memory budget: 800 MB default RAM budget for cached RAWs (~4-5x 45MP uncompressed images)
        const MAX_PHOTO_CACHE_BYTES: usize = 800_000_000;
        const MAX_PHOTO_CACHE_ENTRIES: usize = 10;

        self.photo_cache.insert(0, entry);

        while self.photo_cache.len() > MAX_PHOTO_CACHE_ENTRIES {
            self.photo_cache.pop();
        }

        let mut total_bytes: usize = self
            .photo_cache
            .iter()
            .map(|e| e.memory_footprint_bytes())
            .sum();
        while total_bytes > MAX_PHOTO_CACHE_BYTES && self.photo_cache.len() > 1 {
            if let Some(evicted) = self.photo_cache.pop() {
                total_bytes = total_bytes.saturating_sub(evicted.memory_footprint_bytes());
            } else {
                break;
            }
        }
    }

    /// Decodes and loads a RAW file at `path`, utilizing in-memory MRU cache or async worker thread.
    pub fn open_raw_file(&mut self, path: &Path) {
        self.library.scroll_filmstrip_to_active = true;
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        };

        // Automatically populate filmstrip with all photos in parent folder
        if let Some(parent) = abs_path.parent() {
            if !parent.as_os_str().is_empty()
                && self.library.current_directory.as_deref() != Some(parent)
            {
                self.open_directory(parent);
            }
        }

        // If explicitly opening a non-RAW image (e.g. PNG / JPG via dialog or CLI), disable raw-only filter so it appears in filmstrip
        let is_raw = abs_path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(crate::preview::is_raw_extension);
        if !is_raw && self.library.raw_only_filter {
            self.library.raw_only_filter = false;
            self.layout_config.raw_only_filter = Some(false);
            self.layout_config.save();
        }

        // Track last opened photo and directory in persistent layout configuration
        self.layout_config.last_open_photo = Some(abs_path.clone());
        if let Some(parent) = abs_path.parent() {
            if parent.is_dir() {
                self.layout_config.last_open_directory = Some(parent.to_path_buf());
            }
        }
        self.layout_config.save();

        // Flush current photo's grading and sidecars cleanly before switching photos
        self.save_current_photo_sidecar();
        self.session.clear_history();

        // Reset zoom & pan and active canvas tools on new photo open
        self.activate_canvas_tool(crate::controllers::viewport::ActiveCanvasTool::None);
        self.viewport.zoom_level = 0.0;
        self.viewport.pan_offset = egui::Vec2::ZERO;
        self.session.original_texture = None;
        self.session.is_comparing_original = false;

        // Check sidecars on disk first
        let (disk_grading, disk_rating) = load_photo_sidecars(&abs_path);
        if let Some(r) = disk_rating {
            self.library.photo_ratings.insert(abs_path.clone(), r);
        }
        if let Some(g) = disk_grading.as_ref() {
            self.library
                .photo_gradings
                .insert(abs_path.clone(), g.clone());
        }

        // Check if photo is already cached in memory for instantaneous 0 ms switching
        if let Some(pos) = self
            .photo_cache
            .iter()
            .position(|entry| entry.path == abs_path)
        {
            let mut entry = self.photo_cache.remove(pos);
            if let Some(&rating) = self.library.photo_ratings.get(&abs_path) {
                entry.photo_view.rating = rating;
            }
            self.session.loaded_photo = Some(entry.photo_view.clone());
            self.session.base_preview = Some(entry.base_preview.clone());
            self.session.cached_cfa = Some(entry.cfa.clone());
            self.session.original_texture = None;

            // Restore per-photo grading state (preferring disk sidecar if present)
            if let Some(g) = disk_grading {
                self.session.grading = g;
                self.session.current_preset = self.session.grading.preset;
            } else if let Some(saved) = self.library.photo_gradings.get(&abs_path) {
                self.session.grading = saved.clone();
                self.session.current_preset = self.session.grading.preset;
            } else {
                self.session.grading = DarkroomGradingState::default();
                self.session.current_preset = GradingPreset::AsShot;
            }

            self.status_message = format!("Loaded (cached) {}", entry.photo_view.camera_model);
            self.session.needs_preview_update = true;
            self.workers.is_decoding = false;
            self.insert_cached_photo(entry);
            self.trigger_prefetch(&abs_path);
            return;
        }

        let file_name = abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("RAW")
            .to_string();

        self.status_message = format!("Decoding {}...", file_name);
        self.workers.is_decoding = true;
        self.session.loaded_photo = None;
        self.session.base_preview = None;
        self.session.cached_cfa = None;
        self.session.preview_texture = None;
        self.session.original_texture = None;
        self.session.needs_preview_update = false;

        let req = self
            .workers
            .decode_coordinator
            .create_request(abs_path, false);
        let _ = self.workers.decode_req_tx.send(req);
    }
}
