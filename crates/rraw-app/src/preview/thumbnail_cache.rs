//! Bounded LRU cache of GPU texture handles for filmstrip thumbnails.
//!
//! Prevents GPU VRAM exhaustion when navigating directories with thousands of RAW photos
//! by evicting the least-recently-accessed texture handles once capacity is reached.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};

/// Bounded LRU cache of GPU texture handles for filmstrip thumbnails.
#[derive(Clone)]
pub struct LruThumbnailCache {
    capacity: usize,
    entries: HashMap<PathBuf, Option<egui::TextureHandle>>,
    order: VecDeque<PathBuf>,
}

impl fmt::Debug for LruThumbnailCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LruThumbnailCache")
            .field("capacity", &self.capacity)
            .field("len", &self.entries.len())
            .field("order", &self.order)
            .finish()
    }
}

impl Default for LruThumbnailCache {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY)
    }
}

impl LruThumbnailCache {
    /// Default maximum number of texture handles kept in VRAM (256 thumbnails ~ 64MB VRAM max).
    pub const DEFAULT_CAPACITY: usize = 256;

    /// Creates a new `LruThumbnailCache` with a fixed maximum capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            entries: HashMap::with_capacity(cap.min(Self::DEFAULT_CAPACITY)),
            order: VecDeque::with_capacity(cap.min(Self::DEFAULT_CAPACITY)),
        }
    }

    /// Returns the maximum capacity of the cache.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of cached thumbnail textures.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the cache contains no thumbnail textures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Checks if a texture handle exists for `path`. Does not alter access order.
    #[must_use]
    pub fn contains_key(&self, path: &Path) -> bool {
        self.entries.contains_key(path)
    }

    /// Retrieves an immutable reference to the texture handle for `path`.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<&egui::TextureHandle> {
        self.entries.get(path).and_then(Option::as_ref)
    }

    /// Retrieves an immutable reference to the texture handle for `path` and updates its LRU recency.
    pub fn get_and_touch(&mut self, path: &Path) -> Option<&egui::TextureHandle> {
        if self.entries.contains_key(path) {
            self.touch(path);
            self.entries.get(path).and_then(Option::as_ref)
        } else {
            None
        }
    }

    /// Retrieves a mutable reference to the texture handle for `path` and updates its LRU recency.
    pub fn get_mut(&mut self, path: &Path) -> Option<&mut egui::TextureHandle> {
        if self.entries.contains_key(path) {
            self.touch(path);
            self.entries.get_mut(path).and_then(Option::as_mut)
        } else {
            None
        }
    }

    /// Inserts or updates a real texture handle for `path`, evicting the oldest entry if capacity is exceeded.
    pub fn insert(&mut self, path: PathBuf, texture: egui::TextureHandle) {
        if self.entries.contains_key(&path) {
            self.touch(&path);
            self.entries.insert(path, Some(texture));
            return;
        }

        while self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }

        self.order.push_back(path.clone());
        self.entries.insert(path, Some(texture));
    }

    /// Testing helper inserting a mock entry without requiring an active egui GPU context.
    pub fn insert_mock(&mut self, path: PathBuf) {
        if self.entries.contains_key(&path) {
            self.touch(&path);
            self.entries.insert(path, None);
            return;
        }

        while self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }

        self.order.push_back(path.clone());
        self.entries.insert(path, None);
    }

    /// Removes a texture handle for `path`.
    pub fn remove(&mut self, path: &Path) -> Option<egui::TextureHandle> {
        if let Some(pos) = self.order.iter().position(|p| p == path) {
            self.order.remove(pos);
        }
        self.entries.remove(path).flatten()
    }

    /// Clears all textures from VRAM.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    /// Prunes entries not present in `active_entries` (e.g. on directory change).
    pub fn prune_not_in(&mut self, active_entries: &[PathBuf]) {
        let set: HashSet<&PathBuf> = active_entries.iter().collect();
        self.entries.retain(|k, _| set.contains(k));
        self.order.retain(|k| set.contains(k));
    }

    /// Updates the access order of `path` to be the most recently used (tail of `order`).
    pub fn touch(&mut self, path: &Path) {
        if let Some(pos) = self.order.iter().position(|p| p == path) {
            if pos + 1 < self.order.len() {
                if let Some(item) = self.order.remove(pos) {
                    self.order.push_back(item);
                }
            }
        }
    }
}
