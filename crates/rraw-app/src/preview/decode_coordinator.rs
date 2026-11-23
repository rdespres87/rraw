//! Coordination and cancellation tokens for background RAW decoding tasks.
//!
//! Provides generation/epoch counters ensuring that obsolete decodes are instantly
//! dropped when scrubbing or navigating quickly across photos in the filmstrip.

use crate::preview::decoder::decode_photo_file_payload;
use crate::preview::types::BackgroundDecodePayload;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

/// Request dispatched to the background decode worker.
#[derive(Debug, Clone)]
pub struct DecodeRequest {
    /// Incremental epoch counter. If `epoch != active_epoch`, foreground jobs are canceled.
    pub epoch: u64,
    /// Absolute path to the photo file to decode.
    pub path: PathBuf,
    /// Whether this decode is a speculative prefetch or active foreground request.
    pub is_prefetch: bool,
}

/// Thread-safe generation/epoch coordinator ensuring atomic cancellation of superseded decodes.
#[derive(Debug, Clone)]
pub struct DecodeCoordinator {
    active_epoch: Arc<AtomicU64>,
}

impl Default for DecodeCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl DecodeCoordinator {
    /// Creates a new coordinator starting at epoch 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Increments the epoch and returns a new `DecodeRequest` for `path`.
    pub fn create_request(&self, path: PathBuf, is_prefetch: bool) -> DecodeRequest {
        let epoch = if is_prefetch {
            self.active_epoch.load(Ordering::Acquire)
        } else {
            self.active_epoch.fetch_add(1, Ordering::SeqCst) + 1
        };
        DecodeRequest {
            epoch,
            path,
            is_prefetch,
        }
    }

    /// Checks whether `epoch` is still current for foreground and prefetch requests.
    #[must_use]
    pub fn is_current(&self, epoch: u64, _is_prefetch: bool) -> bool {
        epoch == self.active_epoch.load(Ordering::Acquire)
    }

    /// Returns the current active epoch.
    #[must_use]
    pub fn current_epoch(&self) -> u64 {
        self.active_epoch.load(Ordering::Acquire)
    }

    /// Shared atomic epoch handle for passing into worker threads.
    #[must_use]
    pub fn epoch_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.active_epoch)
    }
}

/// Dedicated decode worker loop processing requests sequentially from `rx` and canceling stale jobs.
pub fn run_decode_worker(
    rx: Receiver<DecodeRequest>,
    tx: Sender<BackgroundDecodePayload>,
    active_epoch: Arc<AtomicU64>,
) {
    while let Ok(req) = rx.recv() {
        // Fast abort: check if job was already superseded before touching disk or CPU
        if req.epoch != active_epoch.load(Ordering::Acquire) {
            continue;
        }

        if let Some(payload) = decode_photo_file_payload(&req.path, req.is_prefetch, req.epoch) {
            // Post-decode abort: verify again before shipping payload over channel
            if req.epoch != active_epoch.load(Ordering::Acquire) {
                continue;
            }
            let _ = tx.send(payload);
        }
    }
}
