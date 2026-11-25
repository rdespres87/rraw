//! Dedicated domain controller managing background decoding threads, thumbnail generators and thread pools.

use crate::preview::decoder::generate_fast_thumbnail;
use crate::preview::types::BackgroundDecodePayload;
use crate::preview::{DecodeCoordinator, DecodeRequest, LruThumbnailCache};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Bounded capacity for non-blocking background SQLite DAM rating updates.
const DAM_WRITE_QUEUE_CAPACITY: usize = 256;

/// Commands processed sequentially by the background SQLite DAM write worker.
#[derive(Debug)]
pub enum DamWriteRequest {
    UpdateRating {
        dam: rraw_storage::SqliteDamIndex,
        photo_path: PathBuf,
        rating: u8,
        color_label: rraw_core::ColorLabel,
        pick_status: rraw_core::PickStatus,
    },
    Flush(Sender<()>),
}

/// Progress event emitted by background export tasks: `(progress_fraction, label, final_result)`.
pub type ExportProgressMessage = (f32, String, Option<Result<PathBuf, String>>);

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
    /// Channel sender for dispatching background export outcomes.
    pub export_sender: Sender<ExportProgressMessage>,
    /// Channel receiver for reading background export progress and results on UI thread.
    pub export_receiver: Receiver<ExportProgressMessage>,
    /// Cooperative cancellation flag shared with active background export/timelapse workers.
    pub export_cancel: Arc<AtomicBool>,
    /// Channel sender for background DAM session indexing updates.
    pub dam_index_sender: Sender<(PathBuf, u8)>,
    /// Channel receiver for background DAM session indexing updates.
    pub dam_index_receiver: Receiver<(PathBuf, u8)>,
    /// Dedicated epoch counter cancelling only superseded directory indexers (independent of photo decoding).
    dam_index_epoch: Arc<AtomicU64>,
    /// Bounded non-blocking channel sender for background SQLite DAM rating writes.
    pub dam_write_tx: SyncSender<DamWriteRequest>,
}

impl Drop for BackgroundWorkers {
    fn drop(&mut self) {
        self.flush_dam_writes();
    }
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
        let (export_sender, export_receiver) = channel();
        let (dam_index_sender, dam_index_receiver) = channel();

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

        let (dam_write_tx, dam_write_rx) =
            sync_channel::<DamWriteRequest>(DAM_WRITE_QUEUE_CAPACITY);
        std::thread::spawn(move || run_dam_write_worker(dam_write_rx));

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
            export_sender,
            export_receiver,
            export_cancel: Arc::new(AtomicBool::new(false)),
            dam_index_sender,
            dam_index_receiver,
            dam_index_epoch: Arc::new(AtomicU64::new(0)),
            dam_write_tx,
        }
    }

    /// Creates a headless background worker configuration without starting decode/thumbnail threads.
    #[must_use]
    pub fn new_headless() -> Self {
        let (decode_sender, decode_receiver) = channel();
        let (thumbnail_sender, thumbnail_receiver) = channel();
        let (thumbnail_req_tx, _thumbnail_req_rx) = channel::<PathBuf>();
        let (export_sender, export_receiver) = channel();
        let (dam_index_sender, dam_index_receiver) = channel();
        let decode_coordinator = DecodeCoordinator::new();
        let (decode_req_tx, _decode_req_rx) = channel::<DecodeRequest>();
        let (dam_write_tx, dam_write_rx) =
            sync_channel::<DamWriteRequest>(DAM_WRITE_QUEUE_CAPACITY);
        std::thread::spawn(move || run_dam_write_worker(dam_write_rx));

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
            export_sender,
            export_receiver,
            export_cancel: Arc::new(AtomicBool::new(false)),
            dam_index_sender,
            dam_index_receiver,
            dam_index_epoch: Arc::new(AtomicU64::new(0)),
            dam_write_tx,
        }
    }

    /// Enqueues a SQLite DAM rating update without blocking the UI thread.
    pub fn enqueue_dam_rating_update(
        &self,
        dam: rraw_storage::SqliteDamIndex,
        photo_path: PathBuf,
        rating: u8,
    ) {
        let req = DamWriteRequest::UpdateRating {
            dam,
            photo_path,
            rating,
            color_label: rraw_core::ColorLabel::None,
            pick_status: rraw_core::PickStatus::Unflagged,
        };
        let _ = self.dam_write_tx.try_send(req);
    }

    /// Deterministically waits until all previously enqueued DAM writes have completed
    /// (zero `thread::sleep`, bounded by a 5-second timeout).
    pub fn flush_dam_writes(&self) {
        let (ack_tx, ack_rx) = channel();
        if self
            .dam_write_tx
            .send(DamWriteRequest::Flush(ack_tx))
            .is_ok()
        {
            let _ = ack_rx.recv_timeout(Duration::from_secs(5));
        }
    }

    /// Resets the cooperative export cancellation flag to `false` and returns a shared handle
    /// for a newly spawned background export or timelapse worker.
    #[must_use]
    pub fn reset_export_cancel(&self) -> Arc<AtomicBool> {
        self.export_cancel.store(false, Ordering::Release);
        Arc::clone(&self.export_cancel)
    }

    /// Signals any active background export or timelapse worker to cancel cooperatively.
    pub fn cancel_export(&self) {
        self.export_cancel.store(true, Ordering::Release);
    }

    /// Returns `true` if cancellation has been requested for the active export worker.
    #[must_use]
    pub fn is_export_cancelled(&self) -> bool {
        self.export_cancel.load(Ordering::Acquire)
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

    /// Spawns a background worker thread to incrementally index directory photos into the session DAM.
    pub fn spawn_directory_indexer(
        &mut self,
        dam: rraw_storage::SqliteDamIndex,
        entries: Vec<PathBuf>,
    ) {
        let epoch_handle = Arc::clone(&self.dam_index_epoch);
        let current_epoch = epoch_handle.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let tx = self.dam_index_sender.clone();

        std::thread::spawn(move || {
            // First stream pre-existing cached ratings immediately from SQLite (background)
            if let Ok(existing_ratings) = dam.load_all_ratings() {
                for (path, rating) in existing_ratings {
                    if epoch_handle.load(Ordering::Acquire) != current_epoch {
                        return;
                    }
                    let _ = tx.send((path, rating));
                }
            }

            for path in entries {
                if epoch_handle.load(Ordering::Acquire) != current_epoch {
                    break;
                }

                // 1. Fast sidecar sync if photo already in DB
                if let Ok(true) = dam.sync_sidecar(&path) {
                    if let Ok(Some(record)) = dam.get_photo(&path) {
                        let _ = tx.send((path, record.rating));
                        continue;
                    }
                }

                // 2. Read sidecars and insert/update
                let (_, rating_opt) = crate::state::load_photo_sidecars(&path);
                let rating = rating_opt.unwrap_or(0);

                if let Ok(None) = dam.get_photo(&path) {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("RAW");
                    let noise_profile = match rraw_core::NoiseProfile::new(
                        [0.0001; 3],
                        [0.0001; 3],
                        rraw_core::NoiseProfileSource::CameraTable,
                    ) {
                        Ok(np) => np,
                        Err(_) => continue,
                    };
                    let meta = rraw_core::PhotoMetadata {
                        camera_make: "Digital Camera".to_string(),
                        camera_model: ext.to_ascii_uppercase(),
                        lens_model: None,
                        focal_length_mm: None,
                        aperture_f_number: None,
                        iso: None,
                        width: 0,
                        height: 0,
                        black_levels: [0.0; 4],
                        white_levels: [1.0; 4],
                        cfa_pattern: rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
                        noise_profile,
                        vignette_handled_by_gainmap: false,
                        dng_warp_rectilinear: None,
                        rating,
                        pick: rraw_core::PickStatus::Unflagged,
                        label: rraw_core::ColorLabel::None,
                        keywords: Vec::new(),
                    };
                    let _ = dam.sync_photo(&path, &meta);
                }

                let _ = tx.send((path, rating));
            }
        });
    }
}

/// Sequential background worker executing SQLite DAM rating updates off the UI thread.
fn run_dam_write_worker(rx: Receiver<DamWriteRequest>) {
    while let Ok(req) = rx.recv() {
        match req {
            DamWriteRequest::UpdateRating {
                dam,
                photo_path,
                rating,
                color_label,
                pick_status,
            } => {
                let _ = dam.update_ratings(&photo_path, rating, color_label, pick_status);
            }
            DamWriteRequest::Flush(ack) => {
                let _ = ack.send(());
            }
        }
    }
}
