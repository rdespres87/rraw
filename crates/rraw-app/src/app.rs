#![forbid(unsafe_code)]

//! Interactive `eframe` / `egui` native macOS application shell for Rraw.

use std::path::{Path, PathBuf};

use egui::Color32;
use rraw_core::{DisplayHeadroomProvider, SurfaceFormat};
#[cfg(target_os = "macos")]
use rraw_macos_edr::MacOsDisplayHeadroomProvider;
#[cfg(not(target_os = "macos"))]
use rraw_macos_edr::SdrFallbackHeadroomProvider;

pub use crate::controllers::*;
pub use crate::i18n::{I18nEngine, Key, Language};
pub use crate::preview::*;
pub use crate::state::*;

/// Root interactive application state orchestrating the window shell, darkroom inspector and EDR viewport.
pub struct RrawApp {
    /// Active macOS EDR headroom provider or SDR fallback.
    headroom_provider: Box<dyn DisplayHeadroomProvider>,
    /// Persistent layout order for right inspector sections.
    pub layout_config: AppLayoutConfig,
    /// Active inspector navigation tab.
    pub inspector_tab: InspectorTab,
    /// Active navigation tab in left panel.
    pub left_panel_tab: LeftPanelTab,
    /// Active tool family filter for categorized palette views.
    pub active_tool_family: ToolFamily,
    /// Whether only modified / active tools are displayed in the inspector.
    pub active_tools_only: bool,
    /// Status message displayed on the bottom bar.
    pub status_message: String,
    /// Surface format invariant: always Rgba16Float.
    pub surface_format: SurfaceFormat,
    /// MRU in-memory cache of previously decoded photos for instant switching.
    pub photo_cache: Vec<CachedPhotoEntry>,

    // The 5 Domain Controllers:
    /// Active photograph session and development state.
    pub session: PhotoSession,
    /// Viewport, zoom, pan, split compare and interactive guides controller.
    pub viewport: ViewportController,
    /// Image library, navigation, ratings and multi-selection controller.
    pub library: LibraryController,
    /// Batch export, modal dialogs and timelapse sequence controller.
    pub export: ExportBatchController,
    /// Background worker threads and asynchronous channels controller.
    pub workers: BackgroundWorkers,
}

impl RrawApp {
    /// Initializes a new interactive application instance.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.all_styles_mut(|style| {
            style.spacing.scroll.kinetic.rubber_band = false;
        });

        #[cfg(target_os = "macos")]
        let headroom_provider: Box<dyn DisplayHeadroomProvider> =
            Box::new(MacOsDisplayHeadroomProvider);
        #[cfg(not(target_os = "macos"))]
        let headroom_provider: Box<dyn DisplayHeadroomProvider> =
            Box::new(SdrFallbackHeadroomProvider);

        let workers = BackgroundWorkers::new();

        // Discover external translation catalogs from assets/i18n and ~/.config/rraw/i18n
        let i18n_engine = crate::i18n::I18nEngine::global();
        let _ = i18n_engine.load_from_directory(std::path::Path::new("assets/i18n"));
        if let Some(mut user_config_i18n) = AppLayoutConfig::config_path() {
            user_config_i18n.pop();
            user_config_i18n.push("i18n");
            let _ = i18n_engine.load_from_directory(&user_config_i18n);
        }

        let layout_config = AppLayoutConfig::load();
        let active_tool_family = layout_config.active_tool_family;
        let active_tools_only = layout_config.active_tools_only;

        // Restore previous working directory if valid, otherwise fallback to user's Desktop
        let initial_dir = match layout_config
            .last_open_directory
            .as_ref()
            .filter(|p| p.is_dir())
            .cloned()
        {
            Some(dir) => dir,
            None => {
                let desktop_opt = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|h| h.join("Desktop"))
                    .filter(|p| p.is_dir());
                match desktop_opt {
                    Some(desk) => desk,
                    None => match std::env::current_dir() {
                        Ok(cwd) => cwd,
                        Err(_) => PathBuf::from("."),
                    },
                }
            }
        };
        let initial_entries = scan_directory_for_images(&initial_dir);
        let raw_count = initial_entries
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(crate::preview::is_raw_extension)
            })
            .count();
        let raw_only_filter = layout_config.raw_only_filter.unwrap_or(raw_count > 0);

        let mut library = LibraryController::new();
        library.current_directory = Some(initial_dir);
        library.directory_entries = initial_entries;
        library.raw_only_filter = raw_only_filter;
        let left_panel_tab = layout_config.left_panel_tab;

        let mut app = Self {
            headroom_provider,
            layout_config,
            inspector_tab: InspectorTab::Develop,
            left_panel_tab,
            active_tool_family,
            active_tools_only,
            status_message: "Ready | 120 Hz Metal EDR Surface | Format: Rgba16Float".to_string(),
            surface_format: SurfaceFormat::Rgba16Float,
            photo_cache: Vec::new(),
            session: PhotoSession::new(),
            viewport: ViewportController::new(),
            library,
            export: ExportBatchController::new(),
            workers,
        };

        // Restore previous photo if available, valid and matching active filters, otherwise open first filtered photo in directory
        let photo_to_open = app
            .layout_config
            .last_open_photo
            .as_ref()
            .filter(|p| p.is_file() && app.matches_active_filter(p))
            .cloned()
            .or_else(|| app.first_filtered_entry())
            .or_else(|| app.library.directory_entries.first().cloned());

        if let Some(photo) = photo_to_open {
            app.open_raw_file(&photo);
        }

        app
    }

    /// Creates a headless `RrawApp` instance without window context for unit & integration testing.
    #[must_use]
    pub fn new_headless() -> Self {
        #[cfg(target_os = "macos")]
        let headroom_provider: Box<dyn DisplayHeadroomProvider> =
            Box::new(MacOsDisplayHeadroomProvider);
        #[cfg(not(target_os = "macos"))]
        let headroom_provider: Box<dyn DisplayHeadroomProvider> =
            Box::new(SdrFallbackHeadroomProvider);
        let layout_config = AppLayoutConfig::default();

        Self {
            headroom_provider,
            layout_config,
            inspector_tab: InspectorTab::Develop,
            left_panel_tab: LeftPanelTab::Explorer,
            active_tool_family: ToolFamily::All,
            active_tools_only: false,
            status_message: "Test headless instance".to_string(),
            surface_format: SurfaceFormat::Rgba16Float,
            photo_cache: Vec::new(),
            session: PhotoSession::new(),
            viewport: ViewportController::new(),
            library: LibraryController::new(),
            export: ExportBatchController::new(),
            workers: BackgroundWorkers::new_headless(),
        }
    }

    /// Returns the currently active application UI language.
    #[must_use]
    pub fn lang(&self) -> Language {
        self.layout_config.language
    }

    /// Translates a UI text key into the currently active language.
    #[must_use]
    pub fn t(&self, key: Key) -> &'static str {
        key.translate(self.layout_config.language)
    }

    /// Queries the current active display headroom (e.g. 1.0 on SDR, up to 4.0 or 16.0 on EDR).
    #[must_use]
    pub fn current_headroom(&self) -> f32 {
        self.headroom_provider.current_headroom().active_headroom()
    }

    /// Updates the main viewport texture and histogram from the active base preview.
    pub fn update_preview_textures(&mut self, ctx: &egui::Context) {
        if let Some(base) = &self.session.base_preview {
            let (mut preview, hist) = render_preview_from_base(base, &self.session.grading);

            // Dynamically update the active filmstrip thumbnail with live grading and crop
            if let Some(photo) = &self.session.loaded_photo {
                let thumb_img =
                    create_thumbnail_from_preview(&preview, &self.session.grading, 140, 100);
                if let Some(tex) = self.workers.thumbnail_textures.get_mut(&photo.path) {
                    tex.set(thumb_img, egui::TextureOptions::LINEAR);
                } else {
                    let tex = ctx.load_texture(
                        format!("thumb_{}", photo.path.display()),
                        thumb_img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.workers
                        .thumbnail_textures
                        .insert(photo.path.clone(), tex);
                }
            }

            // Clipping warnings are painted after the thumbnail so the filmstrip stays clean.
            crate::view_tools::apply_clipping_overlay(
                &mut preview.pixels,
                self.viewport.clipping_overlay,
            );

            if let Some(tex) = &mut self.session.preview_texture {
                if tex.name() == "raw_preview" {
                    tex.set(preview, egui::TextureOptions::LINEAR);
                } else {
                    self.session.preview_texture = Some(ctx.load_texture(
                        "raw_preview",
                        preview,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            } else {
                self.session.preview_texture =
                    Some(ctx.load_texture("raw_preview", preview, egui::TextureOptions::LINEAR));
            }
            self.viewport.histogram = hist;

            if self.session.original_texture.is_none() {
                let untouched = DarkroomGradingState::untouched_raw();
                let (orig_preview, _) = render_preview_from_base(base, &untouched);
                self.session.original_texture = Some(ctx.load_texture(
                    "raw_original",
                    orig_preview,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        self.session.needs_preview_update = false;
    }

    /// Persists current photo grading and rating to both `.rraw.json` and `.xmp` sidecars and memory cache.
    pub fn save_current_photo_sidecar(&mut self) {
        if let Some(photo) = &self.session.loaded_photo {
            let rating = self
                .library
                .photo_ratings
                .get(&photo.path)
                .copied()
                .unwrap_or(photo.rating);
            self.library
                .photo_gradings
                .insert(photo.path.clone(), self.session.grading.clone());
            save_photo_sidecars(&photo.path, &self.session.grading, rating);
        }
    }

    /// Copies the active photo's color & tone grading parameters into the in-memory clipboard.
    pub fn copy_active_recipe(&mut self) {
        if self.session.loaded_photo.is_some() {
            let clipboard = GradingRecipeClipboard::from_grading(&self.session.grading);
            let summary = clipboard.summary();
            self.library.recipe_clipboard = Some(clipboard);
            self.status_message = format!("📋 Rendu copié dans le presse-papiers ({summary})");
        } else {
            self.status_message = "Presse-papiers : aucune photo active à copier".to_string();
        }
    }

    /// Copies the color & tone grading parameters for a specific photograph path.
    pub fn copy_recipe_for_path(&mut self, path: &Path) {
        if self.session.loaded_photo.as_ref().map(|p| p.path.as_path()) == Some(path) {
            self.copy_active_recipe();
            return;
        }

        let grading = if let Some(g) = self.library.photo_gradings.get(path) {
            g.clone()
        } else if let (Some(g), _) = load_photo_sidecars(path) {
            g
        } else {
            DarkroomGradingState::default()
        };

        let clipboard = GradingRecipeClipboard::from_grading(&grading);
        let summary = clipboard.summary();
        self.library.recipe_clipboard = Some(clipboard);
        self.status_message = format!("📋 Rendu copié dans le presse-papiers ({summary})");
    }

    /// Pastes the in-memory clipboard recipe onto a single target photograph.
    pub fn paste_recipe_to_path(&mut self, target_path: &Path, ctx: &egui::Context) {
        let clipboard = match self.library.recipe_clipboard.clone() {
            Some(c) => c,
            None => {
                self.status_message =
                    "Presse-papiers vide : copiez un rendu d'abord (Cmd+Shift+C)".to_string();
                return;
            }
        };

        self.apply_clipboard_to_single_photo(target_path, &clipboard, ctx);
        self.status_message = "✓ Rendu collé avec succès sur la photo".to_string();
    }

    /// Pastes the in-memory clipboard recipe onto all selected photos in the filmstrip,
    /// or onto the currently active photo if no batch selection is active.
    pub fn paste_recipe_to_selection(&mut self, ctx: &egui::Context) {
        let clipboard = match self.library.recipe_clipboard.clone() {
            Some(c) => c,
            None => {
                self.status_message =
                    "Presse-papiers vide : copiez un rendu d'abord (Cmd+Shift+C)".to_string();
                return;
            }
        };

        let targets: Vec<PathBuf> = if !self.library.selected_photos.is_empty() {
            self.library.selected_photos.iter().cloned().collect()
        } else if let Some(photo) = &self.session.loaded_photo {
            vec![photo.path.clone()]
        } else {
            return;
        };

        let count = targets.len();
        for target_path in &targets {
            self.apply_clipboard_to_single_photo(target_path, &clipboard, ctx);
        }

        self.status_message = format!(
            "✓ Rendu collé avec succès sur {} photo{}",
            count,
            if count > 1 { "s" } else { "" }
        );
    }

    /// Helper applying a clipboard recipe to a single photograph, saving sidecars and updating thumbnails.
    fn apply_clipboard_to_single_photo(
        &mut self,
        target_path: &Path,
        clipboard: &GradingRecipeClipboard,
        ctx: &egui::Context,
    ) {
        let is_loaded =
            self.session.loaded_photo.as_ref().map(|p| p.path.as_path()) == Some(target_path);

        if is_loaded {
            clipboard.apply_to_grading(&mut self.session.grading);
            self.session.current_preset = self.session.grading.preset;
            self.library
                .photo_gradings
                .insert(target_path.to_path_buf(), self.session.grading.clone());
            self.session.needs_preview_update = true;
            let rating = self
                .library
                .photo_ratings
                .get(target_path)
                .copied()
                .or_else(|| self.session.loaded_photo.as_ref().map(|p| p.rating))
                .unwrap_or(0);
            save_photo_sidecars(target_path, &self.session.grading, rating);
        } else {
            let mut target_grading = if let Some(g) = self.library.photo_gradings.get(target_path) {
                g.clone()
            } else if let (Some(g), _) = load_photo_sidecars(target_path) {
                g
            } else {
                DarkroomGradingState::default()
            };

            clipboard.apply_to_grading(&mut target_grading);
            self.library
                .photo_gradings
                .insert(target_path.to_path_buf(), target_grading.clone());
            let rating = self
                .library
                .photo_ratings
                .get(target_path)
                .copied()
                .unwrap_or(0);
            save_photo_sidecars(target_path, &target_grading, rating);

            // If the photo is cached in memory, re-render its thumbnail immediately
            if let Some(entry) = self.photo_cache.iter_mut().find(|e| e.path == target_path) {
                let (preview, _) = render_preview_from_base(&entry.base_preview, &target_grading);
                let thumb_img = create_thumbnail_from_preview(&preview, &target_grading, 140, 100);
                if let Some(tex) = self.workers.thumbnail_textures.get_mut(target_path) {
                    tex.set(thumb_img, egui::TextureOptions::LINEAR);
                } else {
                    let tex = ctx.load_texture(
                        format!("thumb_{}", target_path.display()),
                        thumb_img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.workers
                        .thumbnail_textures
                        .insert(target_path.to_path_buf(), tex);
                }
            } else {
                // Invalidate texture so background thumbnail worker reloads it with updated sidecar
                self.workers.thumbnail_textures.remove(target_path);
                self.workers.thumbnail_pending.remove(target_path);
                let _ = self
                    .workers
                    .thumbnail_req_tx
                    .send(target_path.to_path_buf());
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

        // 1. Open SQLite session database (creates .rraw/cache.db) with WAL pragmas
        match rraw_storage::SqliteDamIndex::open_session_database(&abs_dir) {
            Ok(dam) => {
                // Background worker asynchronously preloads cached ratings and indexes directory photos without freezing the UI thread
                self.workers
                    .spawn_directory_indexer(dam.clone(), self.library.directory_entries.clone());
                self.library.session_dam = Some(dam);
            }
            Err(err) => {
                eprintln!("Failed to open session database: {err}");
                self.library.session_dam = None;
            }
        }

        // Preload sidecars (.rraw.json & .xmp) without stalling CPU
        let entries = self.library.directory_entries.clone();
        for path in &entries {
            let (grading_opt, rating_opt) = load_photo_sidecars(path);
            if let Some(g) = grading_opt {
                self.library.photo_gradings.insert(path.clone(), g);
            }
            if let Some(r) = rating_opt {
                self.library.photo_ratings.insert(path.clone(), r);
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
            self.export.show_about_dialog = false;
            true
        } else if self.export.show_shortcuts_dialog {
            self.export.show_shortcuts_dialog = false;
            true
        } else if self.export.show_export_dialog {
            self.export.show_export_dialog = false;
            true
        } else if self.viewport.is_crop_editing {
            self.viewport.is_crop_editing = false;
            self.viewport.crop_drag_handle = CropDragHandle::None;
            self.status_message = "Outil de recadrage désactivé".to_string();
            true
        } else if self.viewport.parallel_tool_phase != ParallelToolPhase::Inactive {
            self.viewport.parallel_tool_phase = ParallelToolPhase::Inactive;
            self.viewport.parallel_line1 = None;
            self.viewport.parallel_line2 = None;
            self.viewport.parallel_drag_start = None;
            self.status_message = "Outil de perspective désactivé".to_string();
            true
        } else if self.session.grading.active_brush_mask_idx.is_some() {
            self.session.grading.active_brush_mask_idx = None;
            self.status_message = "Outil pinceau désactivé".to_string();
            true
        } else if !self.library.selected_photos.is_empty() {
            let prev = self.library.selected_photos.len();
            self.library.selected_photos.clear();
            self.status_message = "Sélection d'exportation réinitialisée (0 photo)".to_string();
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

    /// Sets the rating (0 to 5) of the currently loaded photo and persists to `.rraw.json`, `.xmp`, and SQLite session DAM.
    pub fn set_current_photo_rating(&mut self, rating: u8) {
        if let Some(photo) = &mut self.session.loaded_photo {
            let path = photo.path.clone();
            self.set_photo_rating_for_path(&path, rating);
        }
    }

    /// Sets the rating of any photo in the library, persisting to sidecars and SQLite session DAM.
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

        // Save immediately to adjacent <photo>.rraw.json and <photo>.xmp
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

        // Save current photo's grading and sidecars in background before switching
        if let Some(loaded) = &self.session.loaded_photo {
            self.library
                .photo_gradings
                .insert(loaded.path.clone(), self.session.grading.clone());
            let rating = self
                .library
                .photo_ratings
                .get(&loaded.path)
                .copied()
                .unwrap_or(loaded.rating);
            let path_to_save = loaded.path.clone();
            let grading_to_save = self.session.grading.clone();
            std::thread::spawn(move || {
                save_photo_sidecars(&path_to_save, &grading_to_save, rating);
            });
        }

        // Reset zoom & pan to Fit on new photo open
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

impl eframe::App for RrawApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.style_mut().spacing.scroll.kinetic.rubber_band = false;

        let wants_keyboard = ui.ctx().egui_wants_keyboard_input();

        // Keyboard navigation and multi-selection (only when text input doesn't have focus)
        if !wants_keyboard {
            let shift = ui.input(|i| i.modifiers.shift);
            let nav_prev = ui
                .input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp));
            let nav_next = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageDown)
            });
            let nav_home = ui.input(|i| i.key_pressed(egui::Key::Home));
            let nav_end = ui.input(|i| i.key_pressed(egui::Key::End));

            if nav_prev {
                self.navigate_relative_with_shift(-1, shift);
            } else if nav_next {
                self.navigate_relative_with_shift(1, shift);
            } else if nav_home {
                self.navigate_to_filtered_index(0, shift);
            } else if nav_end {
                self.navigate_to_filtered_index(usize::MAX, shift);
            }
        }

        // Batch Recipe Clipboard shortcuts: Cmd/Ctrl + Shift + C (Copy) / Cmd/Ctrl + Shift + V (Paste)
        let (copy_recipe, paste_recipe) = ui.input(|i| {
            let cmd = i.modifiers.command || i.modifiers.ctrl;
            (
                cmd && i.modifiers.shift && i.key_pressed(egui::Key::C),
                cmd && i.modifiers.shift && i.key_pressed(egui::Key::V),
            )
        });
        if copy_recipe {
            self.copy_active_recipe();
        } else if paste_recipe {
            let ctx = ui.ctx().clone();
            self.paste_recipe_to_selection(&ctx);
        }

        // Global Escape key handling:
        // - If a modal/dialog is open (shortcuts guide, export window), Escape always dismisses it, even if a child widget has focus.
        // - If no dialog is open and a text field has focus, Escape is left for egui to unfocus the active input.
        // - If no text field is focused, Escape hierarchically cancels the active tool or batch selection.
        if ui.input(|i| i.key_pressed(egui::Key::Escape))
            && (self.export.show_shortcuts_dialog
                || self.export.show_export_dialog
                || self.export.show_about_dialog
                || !wants_keyboard)
        {
            self.handle_escape_key();
        }

        // Classical keyboard selection shortcuts: Cmd/Ctrl+A, Cmd/Ctrl+D, Space (when no text field focused)
        if !wants_keyboard {
            // Top-level file & dialog shortcuts (Cmd/Ctrl + O / Shift+O / E / 1 / 2 / 3)
            let (
                cmd_open_file,
                cmd_open_folder,
                cmd_export,
                switch_tab_1,
                switch_tab_2,
                switch_tab_3,
            ) = ui.input(|i| {
                let cmd = i.modifiers.command || i.modifiers.ctrl;
                let mut tab1 = false;
                let mut tab2 = false;
                let mut tab3 = false;

                if cmd {
                    if i.key_pressed(egui::Key::Num1) {
                        tab1 = true;
                    }
                    if i.key_pressed(egui::Key::Num2) {
                        tab2 = true;
                    }
                    if i.key_pressed(egui::Key::Num3) {
                        tab3 = true;
                    }

                    // Inspect events to support both physical keys (AZERTY Mac) and logical keys (QWERTY)
                    for event in &i.events {
                        if let egui::Event::Key {
                            key,
                            physical_key,
                            pressed: true,
                            modifiers,
                            ..
                        } = event
                        {
                            if modifiers.command || modifiers.ctrl {
                                if *key == egui::Key::Num1 || *physical_key == Some(egui::Key::Num1)
                                {
                                    tab1 = true;
                                }
                                if *key == egui::Key::Num2 || *physical_key == Some(egui::Key::Num2)
                                {
                                    tab2 = true;
                                }
                                if *key == egui::Key::Num3 || *physical_key == Some(egui::Key::Num3)
                                {
                                    tab3 = true;
                                }
                            }
                        }
                    }
                }

                (
                    cmd && !i.modifiers.shift && i.key_pressed(egui::Key::O),
                    cmd && i.modifiers.shift && i.key_pressed(egui::Key::O),
                    cmd && i.key_pressed(egui::Key::E),
                    tab1,
                    tab2,
                    tab3,
                )
            });

            if cmd_open_file {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Supported Images",
                        &[
                            "arw", "cr2", "cr3", "dng", "nef", "raf", "rw2", "orf", "jpg", "jpeg",
                            "png", "tif", "tiff",
                        ],
                    )
                    .pick_file()
                {
                    self.open_raw_file(&path);
                }
            } else if cmd_open_folder {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.open_directory(&folder);
                    if let Some(first_file) = self
                        .first_filtered_entry()
                        .or_else(|| self.library.directory_entries.first().cloned())
                    {
                        self.open_raw_file(&first_file);
                    }
                }
            } else if cmd_export {
                self.export.show_export_dialog = true;
                self.export.export_status = None;
            } else if switch_tab_1 {
                self.left_panel_tab = LeftPanelTab::Explorer;
                self.layout_config.left_panel_tab = LeftPanelTab::Explorer;
                self.layout_config.save();
                ui.ctx().request_repaint();
            } else if switch_tab_2 {
                self.left_panel_tab = LeftPanelTab::Presets;
                self.layout_config.left_panel_tab = LeftPanelTab::Presets;
                self.layout_config.save();
                ui.ctx().request_repaint();
            } else if switch_tab_3 {
                self.left_panel_tab = LeftPanelTab::Exif;
                self.layout_config.left_panel_tab = LeftPanelTab::Exif;
                self.layout_config.save();
                ui.ctx().request_repaint();
            }

            ui.input(|i| {
                if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::A) {
                    for entry in &self.library.directory_entries {
                        if self.matches_active_filter(entry) {
                            self.library.selected_photos.insert(entry.clone());
                        }
                    }
                    let sel_count = self
                        .library
                        .selected_photos
                        .iter()
                        .filter(|p| self.matches_active_filter(p))
                        .count();
                    self.status_message = format!(
                        "Sélection : {sel_count} photo{} cochée{} pour l'export",
                        if sel_count > 1 { "s" } else { "" },
                        if sel_count > 1 { "s" } else { "" }
                    );
                }
                if i.key_pressed(egui::Key::Backslash) {
                    self.session.is_comparing_original = !self.session.is_comparing_original;
                }
                if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::D) {
                    let prev = self.library.selected_photos.len();
                    self.library.selected_photos.clear();
                    self.session.grading.active_brush_mask_idx = None;
                    self.status_message = if prev > 0 {
                        format!(
                            "Désélection : {prev} photo{} retirée{} du lot d'export",
                            if prev > 1 { "s" } else { "" },
                            if prev > 1 { "s" } else { "" }
                        )
                    } else {
                        "Sélection d'exportation : aucune photo cochée".to_string()
                    };
                }
                if i.key_pressed(egui::Key::Space) {
                    // In Darkroom with brush active, Space is reserved for panning zoomed image
                    if self.session.grading.active_brush_mask_idx.is_none() {
                        if let Some(photo) = &self.session.loaded_photo {
                            if self.library.selected_photos.contains(&photo.path) {
                                self.library.selected_photos.remove(&photo.path);
                                self.status_message = format!(
                                    "Photo retirée du lot d'export (reste {})",
                                    self.library.selected_photos.len()
                                );
                            } else {
                                self.library.selected_photos.insert(photo.path.clone());
                                self.status_message = format!(
                                    "Photo ajoutée au lot d'export ({} au total)",
                                    self.library.selected_photos.len()
                                );
                            }
                        }
                    }
                }
            });

            // Viewport display shortcuts (ignored while a text field has keyboard focus)
            let (toggle_filmstrip, toggle_hud, toggle_split, toggle_clipping, toggle_shortcuts) =
                ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::F9),
                        i.key_pressed(egui::Key::I) && !i.modifiers.any(),
                        i.key_pressed(egui::Key::Y) && !i.modifiers.any(),
                        i.key_pressed(egui::Key::J) && !i.modifiers.any(),
                        i.key_pressed(egui::Key::F1),
                    )
                });
            if toggle_filmstrip {
                self.layout_config.filmstrip_visible = !self.layout_config.filmstrip_visible;
                self.layout_config.save();
            }
            if toggle_hud {
                self.layout_config.show_canvas_hud = !self.layout_config.show_canvas_hud;
                self.layout_config.save();
            }
            if toggle_split {
                self.viewport.split_compare = !self.viewport.split_compare;
            }
            if toggle_clipping {
                self.toggle_clipping_overlay();
            }
            if toggle_shortcuts {
                self.export.show_shortcuts_dialog = !self.export.show_shortcuts_dialog;
            }

            let has_cmd_ctrl_alt =
                ui.input(|i| i.modifiers.command || i.modifiers.ctrl || i.modifiers.alt);
            if !has_cmd_ctrl_alt {
                for digit in 0..=5 {
                    let key = match digit {
                        0 => egui::Key::Num0,
                        1 => egui::Key::Num1,
                        2 => egui::Key::Num2,
                        3 => egui::Key::Num3,
                        4 => egui::Key::Num4,
                        5 => egui::Key::Num5,
                        _ => unreachable!(),
                    };
                    if ui.input(|i| i.key_pressed(key)) {
                        self.set_current_photo_rating(digit);
                    }
                }
            }
        }

        // Collect results from background decoding worker without blocking the UI
        while let Ok(payload) = self.workers.decode_receiver.try_recv() {
            if payload.is_prefetch {
                if !self.photo_cache.iter().any(|e| e.path == payload.path) {
                    self.insert_cached_photo(CachedPhotoEntry {
                        path: payload.path,
                        photo_view: payload.photo_view,
                        base_preview: payload.base_preview,
                        cfa: payload.cfa,
                    });
                }
                continue;
            }

            let file_name = payload
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("photo")
                .to_string();

            self.status_message = format!(
                "Loaded {} | {}x{} | ISO {} | {}",
                file_name,
                payload.photo_view.dimensions.0,
                payload.photo_view.dimensions.1,
                payload.photo_view.iso,
                payload.photo_view.camera_model
            );

            // Record rating if extracted from EXIF and not already tracked
            if payload.photo_view.rating > 0
                && !self.library.photo_ratings.contains_key(&payload.path)
            {
                self.library
                    .photo_ratings
                    .insert(payload.path.clone(), payload.photo_view.rating);
            }

            let is_still_active_target = self
                .workers
                .decode_coordinator
                .is_current(payload.epoch, payload.is_prefetch)
                && self
                    .layout_config
                    .last_open_photo
                    .as_ref()
                    .is_some_and(|target| target == &payload.path);

            if !is_still_active_target {
                // If the user already switched to another photo while this background thread was running,
                // cache the result in memory but do NOT hijack the active viewport.
                if !self.photo_cache.iter().any(|e| e.path == payload.path) {
                    self.insert_cached_photo(CachedPhotoEntry {
                        path: payload.path,
                        photo_view: payload.photo_view,
                        base_preview: payload.base_preview,
                        cfa: payload.cfa,
                    });
                }
                continue;
            }

            let mut view = payload.photo_view.clone();
            if let Some(&rating) = self.library.photo_ratings.get(&payload.path) {
                view.rating = rating;
            }

            self.session.loaded_photo = Some(view);
            self.session.base_preview = Some(payload.base_preview.clone());
            self.session.cached_cfa = Some(payload.cfa.clone());
            self.session.original_texture = None;

            // Restore per-photo grading state
            if let Some(saved) = self.library.photo_gradings.get(&payload.path) {
                self.session.grading = saved.clone();
                self.session.current_preset = self.session.grading.preset;
            } else {
                self.session.grading = DarkroomGradingState::default();
                self.session.current_preset = GradingPreset::AsShot;
            }

            self.insert_cached_photo(CachedPhotoEntry {
                path: payload.path.clone(),
                photo_view: payload.photo_view,
                base_preview: payload.base_preview,
                cfa: payload.cfa,
            });

            self.workers.is_decoding = false;
            self.session.needs_preview_update = true;

            // Trigger background prefetching for adjacent photos
            self.trigger_prefetch(&payload.path);
        }

        // Drain completed thumbnail images from background workers
        while let Ok((path, img)) = self.workers.thumbnail_receiver.try_recv() {
            self.workers.thumbnail_pending.remove(&path);
            let is_currently_active = self
                .session
                .loaded_photo
                .as_ref()
                .is_some_and(|p| p.path == path);
            if !is_currently_active {
                if let Some(tex) = self.workers.thumbnail_textures.get_mut(&path) {
                    tex.set(img, egui::TextureOptions::LINEAR);
                } else {
                    let tex = ui.ctx().load_texture(
                        format!("thumb_{}", path.display()),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.workers.thumbnail_textures.insert(path, tex);
                }
            }
        }

        if !self.workers.thumbnail_pending.is_empty() {
            ui.ctx().request_repaint();
        }

        // Drain background DAM session indexing updates (bounded to 128 per frame to preserve 120 FPS)
        let dam_drained = self.drain_dam_indexing_events(128);
        if dam_drained == 128 {
            ui.ctx().request_repaint();
        }

        // Drain background export status & completion
        while let Ok((progress, label, outcome)) = self.workers.export_receiver.try_recv() {
            self.export.export_progress = progress;
            self.export.export_progress_label = label;
            if let Some(res) = outcome {
                self.export.is_exporting = false;
                match res {
                    Ok(path) => {
                        let file_name =
                            path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
                        let status_txt = match self.export.export_target_mode {
                            crate::state::ExportTargetMode::CurrentPhoto => {
                                format!(
                                    "{}{file_name}",
                                    self.t(crate::i18n::Key::ExportSuccessSinglePrefix)
                                )
                            }
                            crate::state::ExportTargetMode::BatchSelection => {
                                let count = self.library.selected_photos.len().max(1);
                                if self.lang() == crate::i18n::Language::Fr {
                                    format!("✅ Export par lot réussi ({count} fichier(s))")
                                } else {
                                    format!("✅ Batch export successful ({count} file(s))")
                                }
                            }
                            crate::state::ExportTargetMode::TimelapseSequence => {
                                let frames = self.timelapse_candidate_paths().len();
                                let fps = self.export.timelapse_fps.fps();
                                if self.lang() == crate::i18n::Language::Fr {
                                    format!("✅ Séquence Timelapse exportée ({frames} images @ {fps} fps + manifeste JSON)")
                                } else {
                                    format!("✅ Timelapse sequence exported ({frames} frames @ {fps} fps + JSON manifest)")
                                }
                            }
                        };
                        self.status_message = status_txt.clone();
                        self.export.export_status = Some((status_txt, true));
                        self.export.show_export_dialog = false;
                    }
                    Err(err) => {
                        self.export.export_status = Some((
                            format!("{}{err}", self.t(crate::i18n::Key::ExportFailedPrefix)),
                            false,
                        ));
                    }
                }
            }
        }

        if self.export.is_exporting {
            ui.ctx().request_repaint();
        }

        // Automatically detect newly added or removed photos in the active folder every 1.5s
        if self.library.last_dir_poll.elapsed() >= std::time::Duration::from_millis(1500) {
            self.library.last_dir_poll = std::time::Instant::now();
            if self.poll_current_directory_changes() {
                ui.ctx().request_repaint();
            }
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1500));

        // Sub-millisecond zero-latency preview texture update
        if self.session.needs_preview_update {
            self.update_preview_textures(ui.ctx());
        }

        if self.workers.is_decoding {
            ui.ctx().request_repaint();
        }

        let headroom = self.current_headroom();

        // 1. Top Control Bar
        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(22, 24, 28)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(self.t(Key::TopBarOpenImage)).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                "Supported Images",
                                &[
                                    "arw", "cr2", "cr3", "dng", "nef", "raf", "rw2", "orf", "jpg",
                                    "jpeg", "png", "tif", "tiff",
                                ],
                            )
                            .pick_file()
                        {
                            self.open_raw_file(&path);
                        }
                    }

                    if ui.button(self.t(Key::TopBarOpenFolder)).clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.open_directory(&folder);
                            if let Some(first_file) = self
                                .first_filtered_entry()
                                .or_else(|| self.library.directory_entries.first().cloned())
                            {
                                self.open_raw_file(&first_file);
                            }
                        }
                    }

                    if ui.button(self.t(Key::TopBarExport)).clicked() {
                        self.export.show_export_dialog = true;
                        self.export.export_status = None;
                    }

                    if ui
                        .button(self.t(Key::TopBarTimelapse))
                        .on_hover_text(self.t(Key::TopBarTimelapseTooltip))
                        .clicked()
                    {
                        self.export.export_target_mode = ExportTargetMode::TimelapseSequence;
                        self.export.export_subfolder = "Timelapse".to_string();
                        self.export.show_export_dialog = true;
                        self.export.export_status = None;
                    }

                    ui.separator();

                    // Live EDR Headroom Badge (compact; details on hover)
                    let (headroom_text, headroom_col, headroom_tip) = if headroom > 1.05 {
                        (
                            format!("{} {headroom:.1}x", self.t(Key::TopBarEdrHeadroom)),
                            egui::Color32::from_rgb(255, 200, 50),
                            self.t(Key::TopBarEdrTooltip),
                        )
                    } else {
                        (
                            format!("{} {headroom:.1}x", self.t(Key::TopBarSdrHeadroom)),
                            egui::Color32::LIGHT_GRAY,
                            self.t(Key::TopBarSdrTooltip),
                        )
                    };
                    ui.label(egui::RichText::new(headroom_text).color(headroom_col))
                        .on_hover_text(headroom_tip);

                    ui.separator();
                    self.render_view_toggles(ui);

                    ui.separator();
                    ui.label(egui::RichText::new(self.t(Key::TopBarZoom)).size(12.0));
                    if ui
                        .selectable_label(
                            self.viewport.zoom_level <= 0.0,
                            self.t(Key::TopBarZoomFit),
                        )
                        .clicked()
                    {
                        self.viewport.zoom_level = 0.0;
                        self.viewport.pan_offset = egui::Vec2::ZERO;
                    }
                    if ui
                        .selectable_label(
                            (self.viewport.zoom_level - 1.0).abs() < 0.01,
                            self.t(Key::TopBarZoom11),
                        )
                        .clicked()
                    {
                        self.viewport.zoom_level = 1.0;
                        self.viewport.pan_offset = egui::Vec2::ZERO;
                    }
                    let fit_s = self.viewport.last_fit_scale.max(0.001);
                    if ui.small_button("➖").clicked() {
                        let cur_s = if self.viewport.zoom_level <= 0.0 {
                            fit_s
                        } else {
                            self.viewport.zoom_level
                        };
                        let new_s = (cur_s / 1.25).max(fit_s);
                        if (new_s - fit_s).abs() / fit_s < 0.015 || new_s <= fit_s {
                            self.viewport.zoom_level = 0.0;
                            self.viewport.pan_offset = egui::Vec2::ZERO;
                        } else {
                            self.viewport.zoom_level = new_s;
                        }
                    }
                    let zoom_pct_str = if self.viewport.zoom_level <= 0.0 {
                        self.t(Key::TopBarZoomFitted).to_string()
                    } else {
                        format!(
                            "{}%",
                            ((self.viewport.zoom_level / fit_s) * 100.0).round() as u32
                        )
                    };
                    ui.label(egui::RichText::new(zoom_pct_str).monospace().strong());
                    if ui.small_button("➕").clicked() {
                        let cur_s = if self.viewport.zoom_level <= 0.0 {
                            fit_s
                        } else {
                            self.viewport.zoom_level
                        };
                        let new_s = (cur_s * 1.25).min(fit_s * 20.0);
                        self.viewport.zoom_level = new_s;
                    }
                    if ui
                        .button(self.t(Key::TopBarCenter))
                        .on_hover_text(self.t(Key::TopBarCenterTooltip))
                        .clicked()
                    {
                        self.viewport.pan_offset = egui::Vec2::ZERO;
                    }

                    ui.separator();
                    let sp_active = self.session.grading.soft_proof.enabled;
                    let sp_btn_label = if sp_active {
                        egui::RichText::new(self.t(Key::TopBarSoftProofActive))
                            .strong()
                            .color(Color32::from_rgb(120, 230, 165))
                    } else {
                        egui::RichText::new(self.t(Key::TopBarSoftProofInactive))
                    };
                    if ui
                        .selectable_label(sp_active, sp_btn_label)
                        .on_hover_text(self.t(Key::TopBarSoftProofTooltip))
                        .clicked()
                    {
                        self.session.grading.soft_proof.enabled =
                            !self.session.grading.soft_proof.enabled;
                        if !self.session.grading.soft_proof.enabled {
                            self.session.grading.soft_proof.show_gamut_warning = false;
                        }
                        self.session.needs_preview_update = true;
                        self.save_current_photo_sidecar();
                        ui.ctx().request_repaint();
                    }

                    if sp_active {
                        egui::ComboBox::from_id_salt("top_bar_soft_proof_profile")
                            .selected_text(self.session.grading.soft_proof.profile.short_label())
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for prof in rraw_color::SoftProofProfile::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.session.grading.soft_proof.profile,
                                            prof,
                                            prof.label(),
                                        )
                                        .changed()
                                    {
                                        self.session.grading.soft_proof.simulate_paper_and_ink =
                                            prof.is_print_medium();
                                        self.session.needs_preview_update = true;
                                        self.save_current_photo_sidecar();
                                        ui.ctx().request_repaint();
                                    }
                                }
                            });

                        egui::ComboBox::from_id_salt("top_bar_soft_proof_intent")
                            .selected_text(match self.session.grading.soft_proof.intent {
                                rraw_color::SoftProofRenderingIntent::Perceptual => "Perceptuel",
                                rraw_color::SoftProofRenderingIntent::RelativeColorimetric => {
                                    "Relatif"
                                }
                            })
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                for intent in rraw_color::SoftProofRenderingIntent::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.session.grading.soft_proof.intent,
                                            intent,
                                            intent.label(),
                                        )
                                        .changed()
                                    {
                                        self.session.needs_preview_update = true;
                                        self.save_current_photo_sidecar();
                                        ui.ctx().request_repaint();
                                    }
                                }
                            });

                        let paper_ink_label = self.t(Key::TopBarPaperInk);
                        let paper_ink_tip = self.t(Key::TopBarPaperInkTooltip);
                        if ui
                            .checkbox(
                                &mut self.session.grading.soft_proof.simulate_paper_and_ink,
                                paper_ink_label,
                            )
                            .on_hover_text(paper_ink_tip)
                            .changed()
                        {
                            self.session.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }

                        let oog_active = self.session.grading.soft_proof.show_gamut_warning;
                        let oog_count = self.viewport.histogram.out_of_gamut_count;
                        let oog_btn_label = if oog_active {
                            if oog_count > 0 {
                                egui::RichText::new(format!("✓ ⚠️ Hors-Gamut ({oog_count})"))
                                    .strong()
                                    .color(Color32::from_rgb(255, 110, 230))
                            } else {
                                egui::RichText::new(self.t(Key::TopBarGamutOk))
                                    .strong()
                                    .color(Color32::from_rgb(120, 230, 165))
                            }
                        } else {
                            egui::RichText::new(self.t(Key::TopBarGamutAlert))
                        };
                        if ui
                            .selectable_label(oog_active, oog_btn_label)
                            .on_hover_text(self.t(Key::TopBarGamutTooltip))
                            .clicked()
                        {
                            self.session.grading.soft_proof.show_gamut_warning =
                                !self.session.grading.soft_proof.show_gamut_warning;
                            self.session.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }
                    }
                });
            });

        // 2. Bottom Status Bar (At the very bottom of the window)
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(18, 19, 22)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.export.is_exporting {
                        ui.spinner();
                        ui.label(egui::RichText::new(&self.export.export_progress_label).strong());
                        ui.add(
                            egui::ProgressBar::new(self.export.export_progress)
                                .desired_width(120.0)
                                .animate(true),
                        );
                        ui.label(egui::RichText::new("·").weak());
                    } else if self.workers.is_decoding {
                        ui.spinner();
                    }
                    ui.label(&self.status_message);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .link(egui::RichText::new(rraw_core::build_info::VERSION_TAG).weak())
                            .on_hover_text(rraw_core::build_info::BuildInfo::about_text())
                            .clicked()
                        {
                            self.export.show_about_dialog = true;
                        }
                        ui.label(egui::RichText::new("·").weak());
                        ui.label(self.t(Key::StatusSurfaceInfo));
                        ui.label(egui::RichText::new("·").weak());

                        // Dynamic language selector from all registered catalogs
                        let cur_lang = self.layout_config.language;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                            let available = I18nEngine::global().available_languages();
                            for (i, &lang) in available.iter().enumerate() {
                                if i > 0 {
                                    ui.label(egui::RichText::new("|").weak());
                                }
                                if ui
                                    .selectable_label(cur_lang == lang, lang.code())
                                    .on_hover_text(lang.name())
                                    .clicked()
                                    && cur_lang != lang
                                {
                                    self.layout_config.language = lang;
                                    self.layout_config.save();
                                }
                            }
                        });
                    });
                });
            });

        // 3. Bottom Filmstrip Panel
        if self.layout_config.filmstrip_visible {
            self.render_filmstrip_panel(ui);
        }

        // 4. Left Sidebar Panel (Histogram + Explorer + Presets + EXIF)
        self.render_left_panel(ui);

        // 5. Right Darkroom Inspector Panel (Categorized Palettes + Active Filter + Collapsible Cards)
        self.render_darkroom_panel(ui);

        // 6. Central Viewport Area
        self.render_central_viewport(ui);

        // 7. Render interactive Export modal dialog if active
        self.render_export_dialog(ui.ctx());

        // 8. Render shortcuts cheat sheet dialog if active
        self.render_shortcuts_dialog(ui.ctx());

        // 9. Render "About" version dialog if active
        self.render_about_dialog(ui.ctx());
    }

    fn on_exit(&mut self) {
        if let Some(loaded) = &self.session.loaded_photo {
            self.layout_config.last_open_photo = Some(loaded.path.clone());
            let rating = match self.library.photo_ratings.get(&loaded.path).copied() {
                Some(r) => r,
                None => loaded.rating,
            };
            save_photo_sidecars(&loaded.path, &self.session.grading, rating);
        }
        if let Some(dir) = &self.library.current_directory {
            self.layout_config.last_open_directory = Some(dir.clone());
        }
        self.layout_config.save();
    }
}
