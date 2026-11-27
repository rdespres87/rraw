//! TDD integration tests for:
//! Concurrency Hardening, Thread Cancellation & VRAM Eviction Engine (`rraw-app`).

use rraw_app::preview::{DecodeCoordinator, LruThumbnailCache};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_lru_thumbnail_cache_capacity_bound() {
    let mut cache = LruThumbnailCache::new(3);
    assert_eq!(cache.capacity(), 3);
    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());

    // Allocate dummy egui texture handles in mock test
    // For unit testing LruThumbnailCache without full GPU context, we verify key retention
    let p1 = PathBuf::from("/photos/1.CR2");
    let p2 = PathBuf::from("/photos/2.CR2");
    let p3 = PathBuf::from("/photos/3.CR2");
    let p4 = PathBuf::from("/photos/4.CR2");

    cache.insert_mock(p1.clone());
    cache.insert_mock(p2.clone());
    cache.insert_mock(p3.clone());
    assert_eq!(cache.len(), 3);
    assert!(cache.contains_key(&p1));
    assert!(cache.contains_key(&p2));
    assert!(cache.contains_key(&p3));

    // Inserting 4th item must evict oldest (p1)
    cache.insert_mock(p4.clone());
    assert_eq!(cache.len(), 3);
    assert!(
        !cache.contains_key(&p1),
        "Oldest entry p1 should have been evicted"
    );
    assert!(cache.contains_key(&p2));
    assert!(cache.contains_key(&p3));
    assert!(cache.contains_key(&p4));
}

#[test]
fn test_lru_thumbnail_cache_touch_refreshes_recency() {
    let mut cache = LruThumbnailCache::new(3);
    let p1 = PathBuf::from("/photos/1.CR2");
    let p2 = PathBuf::from("/photos/2.CR2");
    let p3 = PathBuf::from("/photos/3.CR2");
    let p4 = PathBuf::from("/photos/4.CR2");

    cache.insert_mock(p1.clone());
    cache.insert_mock(p2.clone());
    cache.insert_mock(p3.clone());

    // Access p1 so it becomes the most recently used
    cache.touch(&p1);

    // Inserting p4 must now evict p2 (since p1 was refreshed and p2 is now oldest)
    cache.insert_mock(p4.clone());
    assert_eq!(cache.len(), 3);
    assert!(
        cache.contains_key(&p1),
        "p1 was touched so it should be retained"
    );
    assert!(
        !cache.contains_key(&p2),
        "p2 should be evicted as oldest untouched"
    );
    assert!(cache.contains_key(&p3));
    assert!(cache.contains_key(&p4));
}

#[test]
fn test_lru_thumbnail_cache_prune_on_directory_change() {
    let mut cache = LruThumbnailCache::new(10);
    let old_dir_p1 = PathBuf::from("/folder_a/img1.RAW");
    let old_dir_p2 = PathBuf::from("/folder_a/img2.RAW");
    let new_dir_p3 = PathBuf::from("/folder_b/img3.RAW");

    cache.insert_mock(old_dir_p1.clone());
    cache.insert_mock(old_dir_p2.clone());
    cache.insert_mock(new_dir_p3.clone());
    assert_eq!(cache.len(), 3);

    // Simulate switching to /folder_b where only new_dir_p3 exists
    let active_in_b = vec![new_dir_p3.clone()];
    cache.prune_not_in(&active_in_b);

    assert_eq!(cache.len(), 1);
    assert!(!cache.contains_key(&old_dir_p1));
    assert!(!cache.contains_key(&old_dir_p2));
    assert!(cache.contains_key(&new_dir_p3));
}

#[test]
fn test_decode_coordinator_atomic_epoch_invalidation() {
    let coord = DecodeCoordinator::new();
    assert_eq!(coord.current_epoch(), 0);

    // User navigates to photo 1
    let p1 = PathBuf::from("/test/photo1.dng");
    let req1 = coord.create_request(p1, false);
    assert_eq!(req1.epoch, 1);
    assert!(coord.is_current(req1.epoch, false));

    // User quickly navigates to photo 2 (superseding photo 1)
    let p2 = PathBuf::from("/test/photo2.dng");
    let req2 = coord.create_request(p2, false);
    assert_eq!(req2.epoch, 2);
    assert_eq!(coord.current_epoch(), 2);

    // req1 must now be recognized as canceled / stale
    assert!(!coord.is_current(req1.epoch, false), "req1 should be stale");
    assert!(
        coord.is_current(req2.epoch, false),
        "req2 should be current"
    );

    // Rapid scrubbing simulation (10 rapid key presses)
    for i in 3..=12 {
        let req = coord.create_request(PathBuf::from(format!("/test/photo{}.dng", i)), false);
        assert_eq!(req.epoch, i);
    }
    assert_eq!(coord.current_epoch(), 12);
    // Everything before 12 is stale
    for i in 1..=11 {
        assert!(!coord.is_current(i, false));
    }
    assert!(coord.is_current(12, false));
}

#[test]
fn test_decode_coordinator_prefetch_does_not_cancel_active_epoch() {
    let coord = DecodeCoordinator::new();
    let p1 = PathBuf::from("/test/active.dng");
    let active_req = coord.create_request(p1, false);
    let active_epoch = active_req.epoch;
    assert_eq!(active_epoch, 1);

    // Speculative prefetch for neighbor
    let p_next = PathBuf::from("/test/neighbor.dng");
    let prefetch_req = coord.create_request(p_next, true);

    // Prefetch must preserve the active epoch
    assert_eq!(prefetch_req.epoch, active_epoch);
    assert_eq!(coord.current_epoch(), active_epoch);
    assert!(coord.is_current(active_epoch, false));
    assert!(coord.is_current(prefetch_req.epoch, true));
}

#[test]
fn test_decode_worker_aborts_superseded_request_early() {
    let coord = DecodeCoordinator::new();
    let epoch_handle = coord.epoch_handle();

    let computed_count = Arc::new(AtomicUsize::new(0));
    let computed_clone = Arc::clone(&computed_count);

    // Simulate worker receiving request 1
    let req1 = coord.create_request(PathBuf::from("/test/heavy1.raw"), false);

    // User immediately triggers request 2 before worker starts decoding
    let req2 = coord.create_request(PathBuf::from("/test/heavy2.raw"), false);

    // Simulate worker logic on req1
    let is_stale_1 = req1.epoch != epoch_handle.load(Ordering::Acquire);
    assert!(is_stale_1, "req1 must be detected as stale before decoding");
    if !is_stale_1 {
        computed_clone.fetch_add(1, Ordering::SeqCst);
    }

    // Simulate worker logic on req2
    let is_stale_2 = req2.epoch != epoch_handle.load(Ordering::Acquire);
    assert!(!is_stale_2, "req2 must be detected as valid and processed");
    if !is_stale_2 {
        computed_clone.fetch_add(1, Ordering::SeqCst);
    }

    assert_eq!(
        computed_count.load(Ordering::SeqCst),
        1,
        "Only req2 should have been computed"
    );
}
