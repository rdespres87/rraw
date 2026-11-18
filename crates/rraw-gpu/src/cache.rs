use std::collections::{BTreeMap, HashMap};

/// Cached entry in VRAM tracking allocation size.
#[derive(Debug, Clone)]
pub struct VramPhotoEntry {
    pub photo_id: String,
    pub size_bytes: u64,
}

/// LRU VRAM Cache enforcing a strict memory budget.
///
/// Prevents out-of-memory crashes by evicting older photos as users navigate the library.
/// Uses an indexed sequence order for O(log N) touch and eviction without linear scans.
#[derive(Debug)]
pub struct LruVramCache {
    max_vram_budget_bytes: u64,
    current_vram_usage: u64,
    seq_counter: u64,
    access_order: BTreeMap<u64, String>,
    entries: HashMap<String, (VramPhotoEntry, u64)>,
}

impl LruVramCache {
    /// Creates a new LRU cache with the specified byte budget.
    #[must_use]
    pub fn new(max_vram_budget_bytes: u64) -> Self {
        Self {
            max_vram_budget_bytes,
            current_vram_usage: 0,
            seq_counter: 0,
            access_order: BTreeMap::new(),
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

    /// Marks a photo as most recently accessed, refreshing its LRU priority in O(log N).
    pub fn touch(&mut self, photo_id: &str) {
        if let Some((_, old_seq)) = self.entries.get_mut(photo_id) {
            self.access_order.remove(old_seq);
            self.seq_counter = self.seq_counter.saturating_add(1);
            *old_seq = self.seq_counter;
            self.access_order
                .insert(self.seq_counter, photo_id.to_string());
        }
    }

    /// Inserts or updates a photo entry, evicting oldest photos if needed to stay within budget.
    pub fn insert_mock_photo(&mut self, photo_id: &str, size_bytes: u64) {
        if let Some((existing, old_seq)) = self.entries.remove(photo_id) {
            self.current_vram_usage = self.current_vram_usage.saturating_sub(existing.size_bytes);
            self.access_order.remove(&old_seq);
        }

        // Evict oldest until new size fits within budget
        while self.current_vram_usage.saturating_add(size_bytes) > self.max_vram_budget_bytes
            && !self.access_order.is_empty()
        {
            if let Some((&oldest_seq, _)) = self.access_order.first_key_value() {
                if let Some(oldest_id) = self.access_order.remove(&oldest_seq) {
                    if let Some((evicted, _)) = self.entries.remove(&oldest_id) {
                        self.current_vram_usage =
                            self.current_vram_usage.saturating_sub(evicted.size_bytes);
                    }
                }
            } else {
                break;
            }
        }

        self.seq_counter = self.seq_counter.saturating_add(1);
        let seq = self.seq_counter;
        let entry = VramPhotoEntry {
            photo_id: photo_id.to_string(),
            size_bytes,
        };
        self.entries.insert(photo_id.to_string(), (entry, seq));
        self.access_order.insert(seq, photo_id.to_string());
        self.current_vram_usage = self.current_vram_usage.saturating_add(size_bytes);
    }

    /// Explicitly evicts a photo from VRAM in O(log N).
    pub fn evict(&mut self, photo_id: &str) -> Option<VramPhotoEntry> {
        if let Some((entry, seq)) = self.entries.remove(photo_id) {
            self.access_order.remove(&seq);
            self.current_vram_usage = self.current_vram_usage.saturating_sub(entry.size_bytes);
            Some(entry)
        } else {
            None
        }
    }
}
