//! Dedicated domain controller managing background decoding threads, thumbnail generators and thread pools.

use crate::preview::decoder::generate_fast_thumbnail;
use crate::preview::types::BackgroundDecodePayload;
use crate::preview::{DecodeCoordinator, DecodeRequest, LruThumbnailCache};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Background worker threads and asynchronous communication channels.
pub struct BackgroundWorkers {
    /// Channel sender for dispatching background decode jobs.
    pub decode_sender: Sender<BackgroundDecodePayload>,
    /// Channel receiver for receiving background decode results on main UI thread.
    pub decode_receiver: Receiver<BackgroundDecodePayload>,
    /// True while a background decode task is in progress.
    pub is_decoding: bool,
    /// Epoch/generation coordinator for atomic cancellation of obsolete decode tasks.
    pub decode_coordinator: DecodeCoordinator,
    /// Request sender to the sequential background decode worker.
    pub decode_req_tx: Sender<DecodeRequest>,
    /// Background sender for asynchronous thumbnail generation.
    pub thumbnail_sender: Sender<(PathBuf, egui::ColorImage)>,
    /// Background receiver for completed thumbnails.
    pub thumbnail_receiver: Receiver<(PathBuf, egui::ColorImage)>,
    /// Request sender to background thumbnail worker queue.
    pub thumbnail_req_tx: Sender<PathBuf>,
    /// Set of thumbnail paths currently being generated in background.
    pub thumbnail_pending: HashSet<PathBuf>,
    /// Loaded GPU textures for filmstrip thumbnails bounded by LRU eviction.
    pub thumbnail_textures: LruThumbnailCache,
}

impl Default for BackgroundWorkers {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundWorkers {
    /// Initializes background workers and spawns background decode & thumbnail threads.
    #[must_use]
    pub fn new() -> Self {
        let (decode_sender, decode_receiver) = channel();
        let (thumbnail_sender, thumbnail_receiver) = channel();
        let (thumbnail_req_tx, thumbnail_req_rx) = channel::<PathBuf>();

        let decode_coordinator = DecodeCoordinator::new();
        let (decode_req_tx, decode_req_rx) = channel::<DecodeRequest>();

        // Dedicated background decode worker sequentially processing requests with cancellation
        {
            let rx = decode_req_rx;
            let tx = decode_sender.clone();
            let epoch_handle = decode_coordinator.epoch_handle();
            std::thread::spawn(move || {
                crate::preview::run_decode_worker(rx, tx, epoch_handle);
            });
        }

        // Dedicated background thumbnail generator workers (pooled to avoid CPU saturation)
        let req_rx_arc = Arc::new(Mutex::new(thumbnail_req_rx));
        for _ in 0..2 {
            let rx = Arc::clone(&req_rx_arc);
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

        Self {
            decode_sender,
            decode_receiver,
            is_decoding: false,
            decode_coordinator,
            decode_req_tx,
            thumbnail_sender,
            thumbnail_receiver,
            thumbnail_req_tx,
            thumbnail_pending: HashSet::new(),
            thumbnail_textures: LruThumbnailCache::default(),
        }
    }

    /// Creates a headless background worker configuration without starting background threads.
    #[must_use]
    pub fn new_headless() -> Self {
        let (decode_sender, decode_receiver) = channel();
        let (thumbnail_sender, thumbnail_receiver) = channel();
        let (thumbnail_req_tx, _thumbnail_req_rx) = channel::<PathBuf>();
        let decode_coordinator = DecodeCoordinator::new();
        let (decode_req_tx, _decode_req_rx) = channel::<DecodeRequest>();

        Self {
            decode_sender,
            decode_receiver,
            is_decoding: false,
            decode_coordinator,
            decode_req_tx,
            thumbnail_sender,
            thumbnail_receiver,
            thumbnail_req_tx,
            thumbnail_pending: HashSet::new(),
            thumbnail_textures: LruThumbnailCache::default(),
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

    /// Prunes thumbnail textures and pending tasks not belonging to `active_entries`.
    pub fn prune_thumbnails(&mut self, active_entries: &[PathBuf]) {
        self.thumbnail_textures.prune_not_in(active_entries);
        self.thumbnail_pending
            .retain(|p| active_entries.contains(p));
    }
}
