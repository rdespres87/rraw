//! Dedicated domain controller managing the local image library, navigation, selection and ratings.

use crate::state::{DarkroomGradingState, GradingRecipeClipboard, RatingFilter};
use rraw_storage::SqliteDamIndex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Image library navigation, ratings and multi-selection controller.
#[derive(Debug, Clone)]
pub struct LibraryController {
    /// Currently selected directory in file explorer.
    pub current_directory: Option<PathBuf>,
    /// Active SQLite DAM session index for currently opened directory.
    pub session_dam: Option<SqliteDamIndex>,
    /// Image files discovered in `current_directory`.
    pub directory_entries: Vec<PathBuf>,
    /// Filter displaying only genuine RAW files in the filmstrip.
    pub raw_only_filter: bool,
    /// Active rating filter applied to bottom filmstrip.
    pub rating_filter: RatingFilter,
    /// Per-photo darkroom grading state persisted by photo path.
    pub photo_gradings: HashMap<PathBuf, DarkroomGradingState>,
    /// Per-photo star ratings persisted by photo path (0 to 5).
    pub photo_ratings: HashMap<PathBuf, u8>,
    /// Set of photos selected for batch operations in the filmstrip.
    pub selected_photos: HashSet<PathBuf>,
    /// Last selected index in filmstrip for Shift-range selection.
    pub last_selected_index: Option<usize>,
    /// In-memory clipboard holding a snapshot of color & tone grading parameters for batch copy/paste.
    pub recipe_clipboard: Option<GradingRecipeClipboard>,
    /// Search query for filtering EXIF metadata tags table.
    pub exif_search_query: String,
    /// Timestamp of last automatic filesystem check for new/deleted photos in `current_directory`.
    pub last_dir_poll: std::time::Instant,
    /// Whether to scroll the bottom filmstrip so the active thumbnail card is brought into view.
    pub scroll_filmstrip_to_active: bool,
}

impl Default for LibraryController {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryController {
    /// Creates an empty initial library controller.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_directory: None,
            session_dam: None,
            directory_entries: Vec::new(),
            raw_only_filter: false,
            rating_filter: RatingFilter::All,
            photo_gradings: HashMap::new(),
            photo_ratings: HashMap::new(),
            selected_photos: HashSet::new(),
            last_selected_index: None,
            recipe_clipboard: None,
            exif_search_query: String::new(),
            last_dir_poll: std::time::Instant::now(),
            scroll_filmstrip_to_active: false,
        }
    }

    /// Returns the number of photos in `directory_entries` with rating >= `min_stars`.
    #[must_use]
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
    #[must_use]
    pub fn count_photos_unrated(&self) -> usize {
        self.directory_entries
            .iter()
            .filter(|p| {
                let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                rating == 0
            })
            .count()
    }

    /// Returns true if `path` is part of the active multi-selection.
    #[must_use]
    pub fn is_selected(&self, path: &Path) -> bool {
        self.selected_photos.contains(path)
    }

    /// Clears the multi-selection set.
    pub fn clear_selection(&mut self) {
        self.selected_photos.clear();
        self.last_selected_index = None;
    }

    /// Returns the number of selected photos that currently match the active filter.
    #[must_use]
    pub fn active_selection_count(&self, filter_fn: impl Fn(&Path) -> bool) -> usize {
        self.selected_photos
            .iter()
            .filter(|p| filter_fn(p.as_path()))
            .count()
    }
}
