use std::collections::{HashMap, VecDeque};

/// Cached entry in VRAM tracking allocation size.
#[derive(Debug, Clone)]
pub struct VramPhotoEntry {
    pub photo_id: String,
    pub size_bytes: u64,
}

/// LRU VRAM Cache enforcing a strict memory budget.
///
/// Prevents out-of-memory crashes by evicting older photos as users navigate the library.
#[derive(Debug)]
pub struct LruVramCache {
    max_vram_budget_bytes: u64,
    current_vram_usage: u64,
    lru_order: VecDeque<String>,
    entries: HashMap<String, VramPhotoEntry>,
}

impl LruVramCache {
    /// Creates a new LRU cache with the specified byte budget.
    #[must_use]
    pub fn new(max_vram_budget_bytes: u64) -> Self {
        Self {
            max_vram_budget_bytes,
            current_vram_usage: 0,
            lru_order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    /// Current allocated VRAM in bytes.
    #[must_use]
    pub fn current_vram_usage(&self) -> u64 {
        self.current_vram_usage
    }

    /// Maximum configured VRAM budget in bytes.
    #[must_use]
    pub fn max_vram_budget(&self) -> u64 {
        self.max_vram_budget_bytes
    }

    /// Checks whether a photo currently resides in VRAM cache.
    #[must_use]
    pub fn contains(&self, photo_id: &str) -> bool {
        self.entries.contains_key(photo_id)
    }

    /// Marks a photo as most recently accessed, refreshing its LRU priority.
    pub fn touch(&mut self, photo_id: &str) {
        if self.entries.contains_key(photo_id) {
            self.lru_order.retain(|id| id != photo_id);
            self.lru_order.push_back(photo_id.to_string());
        }
    }

    /// Inserts or updates a photo entry, evicting oldest photos if needed to stay within budget.
    pub fn insert_mock_photo(&mut self, photo_id: &str, size_bytes: u64) {
        if let Some(existing) = self.entries.remove(photo_id) {
            self.current_vram_usage = self.current_vram_usage.saturating_sub(existing.size_bytes);
            self.lru_order.retain(|id| id != photo_id);
        }

        // Evict oldest until new size fits within budget
        while self.current_vram_usage.saturating_add(size_bytes) > self.max_vram_budget_bytes
            && !self.lru_order.is_empty()
        {
            if let Some(oldest_id) = self.lru_order.pop_front() {
                if let Some(evicted) = self.entries.remove(&oldest_id) {
                    self.current_vram_usage =
                        self.current_vram_usage.saturating_sub(evicted.size_bytes);
                }
            }
        }

        self.entries.insert(
            photo_id.to_string(),
            VramPhotoEntry {
                photo_id: photo_id.to_string(),
                size_bytes,
            },
        );
        self.current_vram_usage = self.current_vram_usage.saturating_add(size_bytes);
        self.lru_order.push_back(photo_id.to_string());
    }

    /// Explicitly evicts a photo from VRAM.
    pub fn evict(&mut self, photo_id: &str) -> Option<VramPhotoEntry> {
        self.lru_order.retain(|id| id != photo_id);
        if let Some(entry) = self.entries.remove(photo_id) {
            self.current_vram_usage = self.current_vram_usage.saturating_sub(entry.size_bytes);
            Some(entry)
        } else {
            None
        }
    }
}
