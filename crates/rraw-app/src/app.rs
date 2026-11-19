#![forbid(unsafe_code)]

//! Interactive `eframe` / `egui` native macOS application shell for Rraw.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use egui::Color32;
use rraw_core::{DisplayHeadroomProvider, SignedCfaBuffer, SurfaceFormat};
#[cfg(target_os = "macos")]
use rraw_macos_edr::MacOsDisplayHeadroomProvider;
#[cfg(not(target_os = "macos"))]
use rraw_macos_edr::SdrFallbackHeadroomProvider;

pub use crate::state::*;

pub use crate::preview::*;

pub struct RrawApp {
    /// Active macOS EDR headroom provider or SDR fallback.
    headroom_provider: Box<dyn DisplayHeadroomProvider>,
    /// Darkroom development parameters.
    pub grading: DarkroomGradingState,
    /// Currently active development preset.
    pub current_preset: GradingPreset,
    /// Persistent layout order for right inspector sections.
    pub layout_config: AppLayoutConfig,
    /// Currently loaded photograph metadata, if any.
    pub loaded_photo: Option<LoadedPhotoView>,
    /// Decoded signed CFA buffer in RAM.
    pub cached_cfa: Option<SignedCfaBuffer>,
    /// Pre-demosaiced linear RGB base preview for zero-latency slider scrubbing.
    pub base_preview: Option<BaseRgbPreview>,
    /// MRU in-memory cache of previously decoded photos for instant switching.
    pub photo_cache: Vec<CachedPhotoEntry>,
    /// Cached GPU texture handle for rendering in egui viewport.
    pub preview_texture: Option<egui::TextureHandle>,
    /// Cached GPU texture handle for rendering the untouched original photo.
    pub original_texture: Option<egui::TextureHandle>,
    /// Whether the user has toggled comparison mode to view the original untouched photo.
    pub is_comparing_original: bool,
    /// Pre-computed live RGB histogram data.
    pub histogram: HistogramData,
    /// Flag signaling that preview texture must be re-rendered.
    pub needs_preview_update: bool,
    /// Channel sender for dispatching background decode jobs.
    pub decode_sender: Sender<BackgroundDecodePayload>,
    /// Channel receiver for receiving background decode results on main UI thread.
    pub decode_receiver: Receiver<BackgroundDecodePayload>,
    /// True while a background decode task is in progress.
    pub is_decoding: bool,
    /// Currently selected directory in file explorer.
    pub current_directory: Option<PathBuf>,
    /// Image files discovered in `current_directory`.
    pub directory_entries: Vec<PathBuf>,
    /// Active inspector navigation tab.
    pub inspector_tab: InspectorTab,
    /// Active navigation tab in left panel.
    pub left_panel_tab: LeftPanelTab,
    /// Active tool family filter for categorized palette views.
    pub active_tool_family: ToolFamily,
    /// Whether only modified / active tools are displayed in the inspector.
    pub active_tools_only: bool,
    /// Whether the leveling line tool is active.
    pub horizon_tool_active: bool,
    /// Drag start point for leveling line tool.
    pub horizon_start_point: Option<egui::Pos2>,
    /// Last computed fit-to-viewport scale.
    pub last_fit_scale: f32,
    /// Active interactive handle being dragged for cropping.
    pub crop_drag_handle: CropDragHandle,
    /// Viewport zoom scale: 0.0 = Fit to viewport, 1.0 = 100% 1:1 pixel mapping, etc.
    pub zoom_level: f32,
    /// Viewport pan offset in logical points.
    pub pan_offset: egui::Vec2,
    /// Status message displayed on the bottom bar.
    pub status_message: String,
    /// Surface format invariant: always Rgba16Float.
    pub surface_format: SurfaceFormat,
    /// Per-photo darkroom grading state persisted by photo path.
    pub photo_gradings: HashMap<PathBuf, DarkroomGradingState>,
    /// Per-photo star ratings persisted by photo path (0 to 5).
    pub photo_ratings: HashMap<PathBuf, u8>,
    /// Active rating filter applied to bottom filmstrip.
    pub rating_filter: RatingFilter,
    /// Loaded GPU textures for filmstrip thumbnails.
    pub thumbnail_textures: HashMap<PathBuf, egui::TextureHandle>,
    /// Background sender for asynchronous thumbnail generation.
    pub thumbnail_sender: Sender<(PathBuf, egui::ColorImage)>,
    /// Background receiver for completed thumbnails.
    pub thumbnail_receiver: Receiver<(PathBuf, egui::ColorImage)>,
    /// Request sender to background thumbnail worker queue.
    pub thumbnail_req_tx: Sender<PathBuf>,
    /// Set of thumbnail paths currently being generated in background.
    pub thumbnail_pending: HashSet<PathBuf>,
    /// Whether the export modal dialog is open.
    pub show_export_dialog: bool,
    /// Selected output format for photo export.
    pub export_format: ExportFormatChoice,
    /// JPEG quality parameter (1-100).
    pub export_jpeg_quality: u8,
    /// Bit depth for raster export (8-bit or 16-bit).
    pub export_bit_depth: ExportBitDepth,
    /// Scaling mode applied during export.
    pub export_scale_mode: ExportScaleMode,
    /// Custom export destination folder, if chosen by user.
    pub export_custom_dir: Option<PathBuf>,
    /// Status message from latest export operation: (message, is_success).
    pub export_status: Option<(String, bool)>,
    /// Phase of the interactive 2-parallel-lines perspective correction tool.
    pub parallel_tool_phase: ParallelToolPhase,
    /// First user-drawn reference line for parallel alignment.
    pub parallel_line1: Option<(egui::Pos2, egui::Pos2)>,
    /// Second user-drawn reference line for parallel alignment.
    pub parallel_line2: Option<(egui::Pos2, egui::Pos2)>,
    /// Pointer start coordinate when dragging a parallel reference line.
    pub parallel_drag_start: Option<egui::Pos2>,
    /// Whether the crop frame is actively being manipulated in the central viewport.
    pub is_crop_editing: bool,
    /// Set of photos selected for batch operations in the filmstrip.
    pub selected_photos: HashSet<PathBuf>,
    /// Whether to create a subfolder in the photo directory upon export.
    pub export_use_subfolder: bool,
    /// Name of the subfolder for exported files.
    pub export_subfolder: String,
    /// Suffix appended to the exported filename.
    pub export_suffix: String,
    /// Whether export applies to all selected photos in batch or current photo.
    pub export_batch_selection: bool,
    /// Target export mode (Single photo, Batch selection, or Timelapse sequence).
    pub export_target_mode: ExportTargetMode,
    /// Frame rate for timelapse sequence export.
    pub timelapse_fps: rraw_export::TimelapseFrameRate,
    /// Temporal exposure deflicker configuration for timelapse export.
    pub timelapse_deflicker: rraw_export::TimelapseDeflickerConfig,
    /// Whether to interpolate exposure & white balance keyframes across the sequence.
    pub timelapse_keyframe_ramping: bool,
    /// Base filename prefix for numbered timelapse sequence output (`<prefix>_00001.<ext>`).
    pub timelapse_sequence_prefix: String,
    /// Last selected index in filmstrip for Shift-range selection.
    pub last_selected_index: Option<usize>,
    /// Search query for filtering EXIF metadata tags table.
    pub exif_search_query: String,
    /// Active color channel in the tone curves inspector.
    pub active_curve_channel: CurveChannel,
    /// Index of curve point currently being dragged, if any.
    pub dragging_curve_point: Option<usize>,
    /// Timestamp of last automatic filesystem check for new/deleted photos in `current_directory`.
    pub last_dir_poll: std::time::Instant,
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

        let (decode_sender, decode_receiver) = channel();
        let (thumbnail_sender, thumbnail_receiver) = channel();
        let (thumbnail_req_tx, thumbnail_req_rx) = channel::<PathBuf>();

        // Dedicated background thumbnail generator workers (pooled to avoid CPU saturation)
        let req_rx_arc = std::sync::Arc::new(std::sync::Mutex::new(thumbnail_req_rx));
        for _ in 0..2 {
            let rx = std::sync::Arc::clone(&req_rx_arc);
            let tx = thumbnail_sender.clone();
            std::thread::spawn(move || loop {
                let path = {
                    let Ok(guard) = rx.lock() else { break };
                    match guard.recv() {
                        Ok(p) => p,
                        Err(_) => break,
                    }
                };
                if let Some(img) = generate_fast_thumbnail(&path) {
                    let _ = tx.send((path, img));
                }
            });
        }

        // Default folder to user's Desktop as requested
        let default_dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join("Desktop"))
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let default_entries = scan_directory_for_images(&default_dir);

        let layout_config = AppLayoutConfig::load();
        let active_tool_family = layout_config.active_tool_family;
        let active_tools_only = layout_config.active_tools_only;

        let mut app = Self {
            headroom_provider,
            grading: DarkroomGradingState::default(),
            current_preset: GradingPreset::AsShot,
            layout_config,
            loaded_photo: None,
            cached_cfa: None,
            base_preview: None,
            photo_cache: Vec::new(),
            preview_texture: None,
            original_texture: None,
            is_comparing_original: false,
            histogram: HistogramData::default(),
            needs_preview_update: false,
            decode_sender,
            decode_receiver,
            is_decoding: false,
            current_directory: Some(default_dir),
            directory_entries: default_entries,
            inspector_tab: InspectorTab::Develop,
            left_panel_tab: LeftPanelTab::Explorer,
            active_tool_family,
            active_tools_only,
            horizon_tool_active: false,
            horizon_start_point: None,
            last_fit_scale: 1.0,
            crop_drag_handle: CropDragHandle::None,
            zoom_level: 0.0,
            pan_offset: egui::Vec2::ZERO,
            status_message: "Ready | 120 Hz Metal EDR Surface | Format: Rgba16Float".to_string(),
            surface_format: SurfaceFormat::Rgba16Float,
            photo_gradings: HashMap::new(),
            photo_ratings: HashMap::new(),
            rating_filter: RatingFilter::All,
            thumbnail_textures: HashMap::new(),
            thumbnail_sender,
            thumbnail_receiver,
            thumbnail_req_tx,
            thumbnail_pending: HashSet::new(),
            show_export_dialog: false,
            export_format: ExportFormatChoice::Jpeg,
            export_jpeg_quality: 92,
            export_bit_depth: ExportBitDepth::Bits8,
            export_scale_mode: ExportScaleMode::Original,
            export_custom_dir: None,
            export_status: None,
            parallel_tool_phase: ParallelToolPhase::Inactive,
            parallel_line1: None,
            parallel_line2: None,
            parallel_drag_start: None,
            is_crop_editing: false,
            selected_photos: HashSet::new(),
            export_use_subfolder: true,
            export_subfolder: "Export".to_string(),
            export_suffix: "_rraw".to_string(),
            export_batch_selection: false,
            export_target_mode: ExportTargetMode::CurrentPhoto,
            timelapse_fps: rraw_export::TimelapseFrameRate::Fps24,
            timelapse_deflicker: rraw_export::TimelapseDeflickerConfig::default(),
            timelapse_keyframe_ramping: true,
            timelapse_sequence_prefix: "timelapse".to_string(),
            last_selected_index: None,
            exif_search_query: String::new(),
            active_curve_channel: CurveChannel::CombinedRgb,
            dragging_curve_point: None,
            last_dir_poll: std::time::Instant::now(),
        };

        // Open the first photo in default directory if available
        if let Some(first) = app.directory_entries.first().cloned() {
            app.open_raw_file(&first);
        }

        app
    }

    /// Queries the current active display headroom (e.g. 1.0 on SDR, up to 4.0 or 16.0 on EDR).
    #[must_use]
    pub fn current_headroom(&self) -> f32 {
        self.headroom_provider.current_headroom().active_headroom()
    }

    /// Updates the main viewport texture and histogram from the active base preview.
    pub fn update_preview_textures(&mut self, ctx: &egui::Context) {
        if let Some(base) = &self.base_preview {
            let (preview, hist) = render_preview_from_base(base, &self.grading);

            // Dynamically update the active filmstrip thumbnail with live grading and crop
            if let Some(photo) = &self.loaded_photo {
                let thumb_img = create_thumbnail_from_preview(&preview, &self.grading, 140, 100);
                if let Some(tex) = self.thumbnail_textures.get_mut(&photo.path) {
                    tex.set(thumb_img, egui::TextureOptions::LINEAR);
                } else {
                    let tex = ctx.load_texture(
                        format!("thumb_{}", photo.path.display()),
                        thumb_img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.thumbnail_textures.insert(photo.path.clone(), tex);
                }
            }

            if let Some(tex) = &mut self.preview_texture {
                tex.set(preview, egui::TextureOptions::LINEAR);
            } else {
                self.preview_texture =
                    Some(ctx.load_texture("raw_preview", preview, egui::TextureOptions::LINEAR));
            }
            self.histogram = hist;

            if self.original_texture.is_none() {
                let (orig_preview, _) =
                    render_preview_from_base(base, &DarkroomGradingState::default());
                self.original_texture = Some(ctx.load_texture(
                    "raw_original",
                    orig_preview,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        self.needs_preview_update = false;
    }

    /// Persists current photo grading and rating to both `.rraw.json` and `.xmp` sidecars.
    pub fn save_current_photo_sidecar(&self) {
        if let Some(photo) = &self.loaded_photo {
            let rating = self
                .photo_ratings
                .get(&photo.path)
                .copied()
                .unwrap_or(photo.rating);
            save_photo_sidecars(&photo.path, &self.grading, rating);
        }
    }

    /// Dispatches a thumbnail generation request to background worker pool if not already loaded or pending.
    pub fn request_thumbnail(&mut self, path: PathBuf) {
        if self.thumbnail_textures.contains_key(&path)
            || !self.thumbnail_pending.insert(path.clone())
        {
            return;
        }

        let _ = self.thumbnail_req_tx.send(path);
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

        self.current_directory = Some(abs_dir.clone());
        self.directory_entries = scan_directory_for_images(&abs_dir);

        // Preload sidecars (.rraw.json & .xmp) without stalling CPU
        let entries = self.directory_entries.clone();
        for path in &entries {
            let (grading_opt, rating_opt) = load_photo_sidecars(path);
            if let Some(g) = grading_opt {
                self.photo_gradings.insert(path.clone(), g);
            }
            if let Some(r) = rating_opt {
                self.photo_ratings.insert(path.clone(), r);
            }
        }

        self.status_message = format!(
            "Directory: {} ({} photos found)",
            abs_dir.display(),
            self.directory_entries.len()
        );
    }

    /// Polls the currently open directory for newly added or deleted image files without requiring a manual folder reopen.
    pub fn poll_current_directory_changes(&mut self) -> bool {
        let Some(dir) = self.current_directory.clone() else {
            return false;
        };
        let mut latest = self.directory_entries.clone();
        if !sync_directory_entries(&dir, &mut latest) {
            return false;
        }

        for path in &latest {
            if !self.directory_entries.contains(path) {
                let (grading_opt, rating_opt) = load_photo_sidecars(path);
                if let Some(g) = grading_opt {
                    self.photo_gradings.insert(path.clone(), g);
                }
                if let Some(r) = rating_opt {
                    self.photo_ratings.insert(path.clone(), r);
                }
            }
        }
        self.selected_photos.retain(|p| latest.contains(p));
        self.directory_entries = latest;
        true
    }

    /// Triggers background prefetching of adjacent photos (+1, +2, -1, -2) for instant switching.
    pub fn trigger_prefetch(&self, around_path: &Path) {
        if let Some(pos) = self.directory_entries.iter().position(|p| p == around_path) {
            let mut candidates = Vec::new();
            if pos + 1 < self.directory_entries.len() {
                candidates.push(self.directory_entries[pos + 1].clone());
            }
            if pos + 2 < self.directory_entries.len() {
                candidates.push(self.directory_entries[pos + 2].clone());
            }
            if pos > 0 {
                candidates.push(self.directory_entries[pos - 1].clone());
            }
            if pos > 1 {
                candidates.push(self.directory_entries[pos - 2].clone());
            }

            for path in candidates {
                if !self.photo_cache.iter().any(|e| e.path == path) {
                    let tx = self.decode_sender.clone();
                    std::thread::spawn(move || {
                        if let Some(payload) = decode_photo_file_payload(&path, true) {
                            let _ = tx.send(payload);
                        }
                    });
                }
            }
        }
    }

    /// Returns the number of photos in `directory_entries` with rating >= `min_stars`.
    pub fn count_photos_with_min_rating(&self, min_stars: u8) -> usize {
        self.directory_entries
            .iter()
            .filter(|p| {
                let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                rating >= min_stars
            })
            .count()
    }

    /// Returns the number of photos in `directory_entries` with 0 stars (unrated).
    pub fn count_photos_unrated(&self) -> usize {
        self.directory_entries
            .iter()
            .filter(|p| {
                let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                rating == 0
            })
            .count()
    }

    /// Sets the rating (0 to 5) of the currently loaded photo and persists to `.rraw.json` and `.xmp`.
    pub fn set_current_photo_rating(&mut self, rating: u8) {
        if let Some(photo) = &mut self.loaded_photo {
            photo.rating = rating;
            self.photo_ratings.insert(photo.path.clone(), rating);
            if let Some(entry) = self.photo_cache.iter_mut().find(|e| e.path == photo.path) {
                entry.photo_view.rating = rating;
            }

            // Save immediately to adjacent <photo>.rraw.json and <photo>.xmp
            save_photo_sidecars(&photo.path, &self.grading, rating);

            let star_text = if rating > 0 {
                "★".repeat(rating as usize)
            } else {
                "Unrated".to_string()
            };
            self.status_message = format!(
                "Rated {}: {} (Saved to .rraw.json & .xmp)",
                photo
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("photo"),
                star_text
            );
        }
    }

    /// Navigates relative to the current photo in the filtered filmstrip.
    pub fn navigate_relative(&mut self, delta: isize) {
        self.navigate_relative_with_shift(delta, false);
    }

    /// Navigates relative to current photo with optional Shift-range selection extension.
    pub fn navigate_relative_with_shift(&mut self, delta: isize, shift: bool) {
        let filtered: Vec<PathBuf> = self
            .directory_entries
            .iter()
            .filter(|p| {
                let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                self.rating_filter.matches(rating)
            })
            .cloned()
            .collect();

        if filtered.is_empty() {
            return;
        }

        let current_path = self.loaded_photo.as_ref().map(|p| &p.path);
        let current_idx = current_path
            .and_then(|cp| filtered.iter().position(|p| p == cp))
            .unwrap_or(0);

        let new_idx = (current_idx as isize + delta).rem_euclid(filtered.len() as isize) as usize;
        let target = filtered[new_idx].clone();

        if shift {
            let anchor = self.last_selected_index.unwrap_or(current_idx);
            let min_i = anchor.min(new_idx);
            let max_i = anchor.max(new_idx);
            for i in min_i..=max_i {
                if let Some(p) = filtered.get(i) {
                    self.selected_photos.insert(p.clone());
                }
            }
        } else {
            self.last_selected_index = Some(new_idx);
        }

        self.open_raw_file(&target);
    }

    /// Decodes and loads a RAW file at `path`, utilizing in-memory MRU cache or async worker thread.
    pub fn open_raw_file(&mut self, path: &Path) {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        };

        // Automatically populate filmstrip with all photos in parent folder
        if let Some(parent) = abs_path.parent() {
            if !parent.as_os_str().is_empty() && self.current_directory.as_deref() != Some(parent) {
                self.open_directory(parent);
            }
        }

        // Save current photo's grading and sidecars in background before switching
        if let Some(loaded) = &self.loaded_photo {
            self.photo_gradings
                .insert(loaded.path.clone(), self.grading.clone());
            let rating = self
                .photo_ratings
                .get(&loaded.path)
                .copied()
                .unwrap_or(loaded.rating);
            let path_to_save = loaded.path.clone();
            let grading_to_save = self.grading.clone();
            std::thread::spawn(move || {
                save_photo_sidecars(&path_to_save, &grading_to_save, rating);
            });
        }

        // Reset zoom & pan to Fit on new photo open
        self.zoom_level = 0.0;
        self.pan_offset = egui::Vec2::ZERO;
        self.original_texture = None;
        self.is_comparing_original = false;

        // Check sidecars on disk first
        let (disk_grading, disk_rating) = load_photo_sidecars(&abs_path);
        if let Some(r) = disk_rating {
            self.photo_ratings.insert(abs_path.clone(), r);
        }
        if let Some(g) = disk_grading.as_ref() {
            self.photo_gradings.insert(abs_path.clone(), g.clone());
        }

        // Check if photo is already cached in memory for instantaneous 0 ms switching
        if let Some(pos) = self
            .photo_cache
            .iter()
            .position(|entry| entry.path == abs_path)
        {
            let mut entry = self.photo_cache.remove(pos);
            if let Some(&rating) = self.photo_ratings.get(&abs_path) {
                entry.photo_view.rating = rating;
            }
            self.loaded_photo = Some(entry.photo_view.clone());
            self.base_preview = Some(entry.base_preview.clone());
            self.cached_cfa = Some(entry.cfa.clone());

            // Restore per-photo grading state (preferring disk sidecar if present)
            if let Some(g) = disk_grading {
                self.grading = g;
                self.current_preset = self.grading.preset;
            } else if let Some(saved) = self.photo_gradings.get(&abs_path) {
                self.grading = saved.clone();
                self.current_preset = self.grading.preset;
            } else {
                self.grading = DarkroomGradingState::default();
                self.current_preset = GradingPreset::AsShot;
            }

            self.status_message = format!("Loaded (cached) {}", entry.photo_view.camera_model);
            self.needs_preview_update = true;
            self.is_decoding = false;
            self.photo_cache.insert(0, entry);
            self.trigger_prefetch(&abs_path);
            return;
        }

        let file_name = abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("RAW")
            .to_string();

        self.status_message = format!("Decoding {}...", file_name);
        self.is_decoding = true;

        // Instant visual feedback: if thumbnail texture is already in memory, display it immediately!
        if let Some(thumb_tex) = self.thumbnail_textures.get(&abs_path) {
            self.preview_texture = Some(thumb_tex.clone());
        }

        let tx = self.decode_sender.clone();
        let target_path = abs_path;

        std::thread::spawn(move || {
            if let Some(payload) = decode_photo_file_payload(&target_path, false) {
                let _ = tx.send(payload);
            }
        });
    }
}

impl eframe::App for RrawApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.style_mut().spacing.scroll.kinetic.rubber_band = false;

        // Keyboard navigation and multi-selection
        let shift = ui.input(|i| i.modifiers.shift);
        let nav_prev =
            ui.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp));
        let nav_next = ui
            .input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageDown));
        if nav_prev {
            self.navigate_relative_with_shift(-1, shift);
        } else if nav_next {
            self.navigate_relative_with_shift(1, shift);
        }

        // Classical keyboard selection shortcuts: Cmd/Ctrl+A, Cmd/Ctrl+D, Escape, Space
        ui.input(|i| {
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::A) {
                for entry in &self.directory_entries {
                    self.selected_photos.insert(entry.clone());
                }
            }
            if i.key_pressed(egui::Key::Backslash) {
                self.is_comparing_original = !self.is_comparing_original;
            }
            if i.key_pressed(egui::Key::Escape) {
                if self.grading.active_brush_mask_idx.is_some() {
                    self.grading.active_brush_mask_idx = None;
                } else if !(i.modifiers.command || i.modifiers.ctrl) {
                    self.selected_photos.clear();
                }
            }
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::D) {
                self.selected_photos.clear();
            }
            if i.key_pressed(egui::Key::Space) {
                // In Darkroom with brush active, Space is reserved for panning zoomed image
                if self.grading.active_brush_mask_idx.is_none() {
                    if let Some(photo) = &self.loaded_photo {
                        if self.selected_photos.contains(&photo.path) {
                            self.selected_photos.remove(&photo.path);
                        } else {
                            self.selected_photos.insert(photo.path.clone());
                        }
                    }
                }
            }
        });

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

        // Collect results from background decoding worker without blocking the UI
        while let Ok(payload) = self.decode_receiver.try_recv() {
            if payload.is_prefetch {
                if self.photo_cache.len() >= 30 {
                    self.photo_cache.pop();
                }
                if !self.photo_cache.iter().any(|e| e.path == payload.path) {
                    self.photo_cache.insert(
                        0,
                        CachedPhotoEntry {
                            path: payload.path,
                            photo_view: payload.photo_view,
                            base_preview: payload.base_preview,
                            cfa: payload.cfa,
                        },
                    );
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
            if payload.photo_view.rating > 0 && !self.photo_ratings.contains_key(&payload.path) {
                self.photo_ratings
                    .insert(payload.path.clone(), payload.photo_view.rating);
            }

            let mut view = payload.photo_view.clone();
            if let Some(&rating) = self.photo_ratings.get(&payload.path) {
                view.rating = rating;
            }

            self.loaded_photo = Some(view);
            self.base_preview = Some(payload.base_preview.clone());
            self.cached_cfa = Some(payload.cfa.clone());

            // Restore per-photo grading state
            if let Some(saved) = self.photo_gradings.get(&payload.path) {
                self.grading = saved.clone();
                self.current_preset = self.grading.preset;
            } else {
                self.grading = DarkroomGradingState::default();
                self.current_preset = GradingPreset::AsShot;
            }

            if self.photo_cache.len() >= 30 {
                self.photo_cache.pop();
            }
            self.photo_cache.insert(
                0,
                CachedPhotoEntry {
                    path: payload.path.clone(),
                    photo_view: payload.photo_view,
                    base_preview: payload.base_preview,
                    cfa: payload.cfa,
                },
            );

            self.is_decoding = false;
            self.needs_preview_update = true;

            // Trigger background prefetching for adjacent photos
            self.trigger_prefetch(&payload.path);
        }

        // Drain completed thumbnail images from background workers
        while let Ok((path, img)) = self.thumbnail_receiver.try_recv() {
            self.thumbnail_pending.remove(&path);
            let is_currently_active = self.loaded_photo.as_ref().is_some_and(|p| p.path == path);
            if !is_currently_active {
                if let Some(tex) = self.thumbnail_textures.get_mut(&path) {
                    tex.set(img, egui::TextureOptions::LINEAR);
                } else {
                    let tex = ui.ctx().load_texture(
                        format!("thumb_{}", path.display()),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.thumbnail_textures.insert(path, tex);
                }
            }
        }

        if !self.thumbnail_pending.is_empty() {
            ui.ctx().request_repaint();
        }

        // Automatically detect newly added or removed photos in the active folder every 1.5s
        if self.last_dir_poll.elapsed() >= std::time::Duration::from_millis(1500) {
            self.last_dir_poll = std::time::Instant::now();
            if self.poll_current_directory_changes() {
                ui.ctx().request_repaint();
            }
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1500));

        // Sub-millisecond zero-latency preview texture update
        if self.needs_preview_update {
            self.update_preview_textures(ui.ctx());
        }

        if self.is_decoding {
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
                    ui.heading("RRAW");
                    ui.label(
                        egui::RichText::new("Chambre noire 120Hz EDR")
                            .color(egui::Color32::from_rgb(100, 180, 255)),
                    );

                    ui.separator();

                    if ui.button("📂 Ouvrir image...").clicked() {
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

                    if ui.button("📁 Ouvrir dossier...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.open_directory(&folder);
                            if let Some(first_file) = self.directory_entries.first().cloned() {
                                self.open_raw_file(&first_file);
                            }
                        }
                    }

                    if ui.button("💾 Exporter...").clicked() {
                        self.show_export_dialog = true;
                        self.export_status = None;
                    }

                    if ui
                        .button("🎬 Timelapse...")
                        .on_hover_text(
                            "Assembler une séquence RAW en Timelapse avec anti-scintillement (Deflicker) et interpolation Jour→Nuit",
                        )
                        .clicked()
                    {
                        self.export_target_mode = ExportTargetMode::TimelapseSequence;
                        self.export_subfolder = "Timelapse".to_string();
                        self.show_export_dialog = true;
                        self.export_status = None;
                    }

                    ui.separator();

                    // Live EDR Headroom Badge
                    let headroom_label = if headroom > 1.05 {
                        egui::RichText::new(format!(
                            "⚡ Headroom EDR : {:.1}x (Apple XDR)",
                            headroom
                        ))
                        .color(egui::Color32::from_rgb(255, 200, 50))
                    } else {
                        egui::RichText::new(format!("📺 Écran SDR : {:.1}x", headroom))
                            .color(egui::Color32::LIGHT_GRAY)
                    };
                    ui.label(headroom_label);

                    if let Some(photo) = &self.loaded_photo {
                        ui.separator();
                        if let Some(file_name) = photo.path.file_name().and_then(|n| n.to_str()) {
                            ui.label(egui::RichText::new(file_name).strong());
                        }
                        let fl_info = if !photo.focal_length_str.is_empty() {
                            format!(" | {}", photo.focal_length_str)
                        } else {
                            String::new()
                        };
                        ui.label(format!(
                            "{}x{} | ISO {} | {} | {}{fl_info} | {}",
                            photo.dimensions.0,
                            photo.dimensions.1,
                            photo.iso,
                            photo.shutter_str,
                            photo.aperture_str,
                            photo.camera_model
                        ));

                        ui.separator();
                        let current_r = photo.rating;
                        let mut new_top_r = None;
                        ui.horizontal(|ui| {
                            for s in 1..=5 {
                                let is_lit = s <= current_r;
                                let star_char = if is_lit { "★" } else { "☆" };
                                let star_col = if is_lit {
                                    Color32::from_rgb(255, 200, 50)
                                } else {
                                    Color32::from_rgb(100, 105, 115)
                                };
                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(star_char).color(star_col).size(14.0),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                let tip = format!(
                                    "Noter {} étoile{} (0 pour effacer)",
                                    s,
                                    if s > 1 { "s" } else { "" }
                                );
                                let resp = resp.on_hover_text(tip);
                                if resp.clicked() {
                                    let target_r = if current_r == s { 0 } else { s };
                                    new_top_r = Some(target_r);
                                }
                            }
                        });
                        if let Some(r) = new_top_r {
                            self.set_current_photo_rating(r);
                        }
                    }

                    ui.separator();
                    ui.label(egui::RichText::new("Zoom :").size(12.0));
                    if ui.selectable_label(self.zoom_level <= 0.0, "Ajuster").clicked() {
                        self.zoom_level = 0.0;
                        self.pan_offset = egui::Vec2::ZERO;
                    }
                    if ui
                        .selectable_label((self.zoom_level - 1.0).abs() < 0.01, "1:1")
                        .clicked()
                    {
                        self.zoom_level = 1.0;
                        self.pan_offset = egui::Vec2::ZERO;
                    }
                    let fit_s = self.last_fit_scale.max(0.001);
                    if ui.small_button("➖").clicked() {
                        let cur_s = if self.zoom_level <= 0.0 {
                            fit_s
                        } else {
                            self.zoom_level
                        };
                        let new_s = (cur_s / 1.25).max(fit_s);
                        if (new_s - fit_s).abs() / fit_s < 0.015 || new_s <= fit_s {
                            self.zoom_level = 0.0;
                            self.pan_offset = egui::Vec2::ZERO;
                        } else {
                            self.zoom_level = new_s;
                        }
                    }
                    let zoom_pct_str = if self.zoom_level <= 0.0 {
                        "Ajusté".to_string()
                    } else {
                        format!("{}%", ((self.zoom_level / fit_s) * 100.0).round() as u32)
                    };
                    ui.label(egui::RichText::new(zoom_pct_str).monospace().strong());
                    if ui.small_button("➕").clicked() {
                        let cur_s = if self.zoom_level <= 0.0 {
                            fit_s
                        } else {
                            self.zoom_level
                        };
                        let new_s = (cur_s * 1.25).min(fit_s * 20.0);
                        self.zoom_level = new_s;
                    }
                    if ui
                        .button("↺ Centrer")
                        .on_hover_text("Réinitialiser le panoramique")
                        .clicked()
                    {
                        self.pan_offset = egui::Vec2::ZERO;
                    }

                    ui.separator();
                    let sp_active = self.grading.soft_proof.enabled;
                    let sp_btn_label = if sp_active {
                        egui::RichText::new("✓ 🖨️ Épreuvage")
                            .strong()
                            .color(Color32::from_rgb(120, 230, 165))
                    } else {
                        egui::RichText::new("🖨️ Épreuvage")
                    };
                    if ui
                        .selectable_label(sp_active, sp_btn_label)
                        .on_hover_text("Activer / Désactiver l'épreuvage écran (Soft Proofing ICC)")
                        .clicked()
                    {
                        self.grading.soft_proof.enabled = !self.grading.soft_proof.enabled;
                        if !self.grading.soft_proof.enabled {
                            self.grading.soft_proof.show_gamut_warning = false;
                        }
                        self.needs_preview_update = true;
                        self.save_current_photo_sidecar();
                        ui.ctx().request_repaint();
                    }

                    if sp_active {
                        egui::ComboBox::from_id_salt("top_bar_soft_proof_profile")
                            .selected_text(self.grading.soft_proof.profile.short_label())
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for prof in rraw_color::SoftProofProfile::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.grading.soft_proof.profile,
                                            prof,
                                            prof.label(),
                                        )
                                        .changed()
                                    {
                                        self.grading.soft_proof.simulate_paper_and_ink =
                                            prof.is_print_medium();
                                        self.needs_preview_update = true;
                                        self.save_current_photo_sidecar();
                                        ui.ctx().request_repaint();
                                    }
                                }
                            });

                        egui::ComboBox::from_id_salt("top_bar_soft_proof_intent")
                            .selected_text(match self.grading.soft_proof.intent {
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
                                            &mut self.grading.soft_proof.intent,
                                            intent,
                                            intent.label(),
                                        )
                                        .changed()
                                    {
                                        self.needs_preview_update = true;
                                        self.save_current_photo_sidecar();
                                        ui.ctx().request_repaint();
                                    }
                                }
                            });

                        if ui
                            .checkbox(
                                &mut self.grading.soft_proof.simulate_paper_and_ink,
                                "Papier/Encre",
                            )
                            .on_hover_text(
                                "Simuler le blanc chaud du papier (Dmin) et la densité de l'encre noire (Dmax)",
                            )
                            .changed()
                        {
                            self.needs_preview_update = true;
                            self.save_current_photo_sidecar();
                            ui.ctx().request_repaint();
                        }

                        let oog_active = self.grading.soft_proof.show_gamut_warning;
                        let oog_count = self.histogram.out_of_gamut_count;
                        let oog_btn_label = if oog_active {
                            if oog_count > 0 {
                                egui::RichText::new(format!("✓ ⚠️ Hors-Gamut ({oog_count})"))
                                    .strong()
                                    .color(Color32::from_rgb(255, 110, 230))
                            } else {
                                egui::RichText::new("✓ Gamut OK")
                                    .strong()
                                    .color(Color32::from_rgb(120, 230, 165))
                            }
                        } else {
                            egui::RichText::new("Alerte Gamut")
                        };
                        if ui
                            .selectable_label(oog_active, oog_btn_label)
                            .on_hover_text(
                                "Afficher / Masquer l'alerte de dépassement de gamut (Zébrures Magenta/Cyan)",
                            )
                            .clicked()
                        {
                            self.grading.soft_proof.show_gamut_warning =
                                !self.grading.soft_proof.show_gamut_warning;
                            self.needs_preview_update = true;
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
                    if self.is_decoding {
                        ui.spinner();
                    }
                    ui.label(&self.status_message);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            "Surface: Rgba16Float | Color: ExtendedLinearDisplayP3 | 120 Hz Metal",
                        );
                    });
                });
            });

        // 3. Bottom Filmstrip Panel
        self.render_filmstrip_panel(ui);

        // 4. Left Sidebar Panel (Histogram + Explorer + Presets + EXIF)
        self.render_left_panel(ui);

        // 5. Right Darkroom Inspector Panel (Categorized Palettes + Active Filter + Collapsible Cards)
        self.render_darkroom_panel(ui);

        // 6. Central Viewport Area
        self.render_central_viewport(ui);

        // 7. Render interactive Export modal dialog if active
        self.render_export_dialog(ui.ctx());
    }
}
