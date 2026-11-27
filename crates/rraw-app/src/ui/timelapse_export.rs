#![forbid(unsafe_code)]

//! Timelapse sequence export worker: 2-pass $O(1)$-RAM deflicker analysis and streaming frame rendering.

use std::path::{Path, PathBuf};

use super::export::ExportWriteConfig;
use crate::app::RrawApp;
use crate::preview::*;
use crate::state::*;

/// Destination directory, filename prefix, and extension for a Timelapse sequence.
struct TimelapseTarget<'a> {
    final_dest: &'a Path,
    seq_prefix: &'a str,
    ext: &'a str,
}

/// Snapshot of cached buffers and grading state for one Timelapse frame.
struct TimelapseFrameSnapshot {
    path: PathBuf,
    cached_base: Option<BaseRgbPreview>,
    cached_cfa: Option<rraw_core::SignedCfaBuffer>,
    cached_linear_rgb: Option<rraw_decode::LinearRgbBuffer>,
    grading: DarkroomGradingState,
}

impl RrawApp {
    /// Spawns a background streaming Timelapse export worker executing a 2-pass $O(1)$-RAM pipeline:
    /// - Pass 1: Measures lightweight sensor log-luminance per frame (dropping each preview buffer immediately) and computes keyframe ramping & temporal deflicker offsets.
    /// - Pass 2: Streams each frame one by one (`decode -> render_export_image_with_depth -> write -> drop`), emitting incremental progress updates over `export_sender`.
    pub(super) fn spawn_background_timelapse_export(&mut self) -> Result<(), String> {
        let sequence_paths = self.timelapse_candidate_paths();
        if sequence_paths.is_empty() {
            return Err("No photos available to assemble the Timelapse.".to_string());
        }

        if let Some(loaded) = &self.session.loaded_photo {
            self.library
                .photo_gradings
                .insert(loaded.path.clone(), self.session.grading.clone());
        }
        self.save_current_photo_sidecar();

        let snapshots = self.collect_timelapse_snapshots(&sequence_paths);
        let final_dest = self.resolve_timelapse_dest_dir(&sequence_paths[0]);
        let export_config = ExportWriteConfig {
            format: self.export.config.format,
            jpeg_quality: self.export.config.jpeg_quality,
            bit_depth: self.export.config.bit_depth,
            scale_mode: self.export.config.scale_mode,
            hdr_gain_map: self.export.config.hdr_gain_map,
        };
        let keyframe_ramping = self.export.config.timelapse.keyframe_ramping;
        let deflicker_cfg = self.export.config.timelapse.deflicker;
        let seq_prefix = self.export.config.timelapse.sequence_prefix.clone();
        let fps = self.export.config.timelapse.fps;
        let tx = self.workers.export_sender.clone();
        let cancel = self.workers.reset_export_cancel();

        std::thread::spawn(move || {
            if let Err(e) = std::fs::create_dir_all(&final_dest) {
                let msg = format!("Failed to create Timelapse directory: {e}");
                let _ = tx.send((1.0, msg.clone(), Some(Err(msg))));
                return;
            }

            let notify = |frac: f32, label: String, out: Option<Result<PathBuf, String>>| {
                let _ = tx.send((frac, label, out));
            };

            let Some((valid_snapshots, initial_gradings, sensor_log_lumas)) =
                Self::measure_timelapse_log_luminances(snapshots, &cancel, &notify)
            else {
                return;
            };

            let (smoothed_gradings, offsets_ev) = prepare_timelapse_gradings_from_lumas(
                &initial_gradings,
                &sensor_log_lumas,
                keyframe_ramping,
                &deflicker_cfg,
            );

            let ext = export_config.format.extension();
            let valid_total = valid_snapshots.len();
            let target = TimelapseTarget {
                final_dest: &final_dest,
                seq_prefix: &seq_prefix,
                ext,
            };
            let Some(last_path) = Self::stream_timelapse_frames(
                valid_snapshots,
                &smoothed_gradings,
                export_config,
                target,
                &cancel,
                &notify,
            ) else {
                return;
            };

            let manifest_res = rraw_export::write_timelapse_manifest(
                &final_dest,
                &seq_prefix,
                ext,
                fps,
                valid_total,
                &offsets_ev,
            )
            .map_err(|e| format!("Error writing Timelapse manifest: {e}"));

            match manifest_res {
                Ok(manifest_path) => {
                    let out_path = last_path.unwrap_or(manifest_path);
                    notify(1.0, "Timelapse completed".to_string(), Some(Ok(out_path)));
                }
                Err(e) => notify(1.0, format!("Error: {e}"), Some(Err(e))),
            }
        });

        Ok(())
    }

    /// Resolves the destination directory for a Timelapse export sequence.
    fn resolve_timelapse_dest_dir(&self, first_path: &Path) -> PathBuf {
        let base_dest = self.export.config.custom_dir.clone().unwrap_or_else(|| {
            first_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        if self.export.config.use_subfolder && !self.export.config.subfolder.trim().is_empty() {
            base_dest.join(self.export.config.subfolder.trim())
        } else {
            base_dest
        }
    }

    /// Collects per-frame cached buffers and grading snapshots for a Timelapse sequence.
    fn collect_timelapse_snapshots(
        &self,
        sequence_paths: &[PathBuf],
    ) -> Vec<TimelapseFrameSnapshot> {
        let mut snapshots = Vec::with_capacity(sequence_paths.len());
        for path in sequence_paths {
            let is_loaded = self.session.loaded_photo.as_ref().map(|p| p.path.as_path())
                == Some(path.as_path());
            let (cached_base, cached_cfa, cached_linear_rgb, grading) = if is_loaded {
                (
                    self.session.base_preview.clone(),
                    self.session.cached_cfa.clone(),
                    self.session.cached_linear_rgb.clone(),
                    self.session.grading.clone(),
                )
            } else if let Some(cached) = self.photo_cache.iter().find(|e| &e.path == path) {
                let g = self
                    .library
                    .photo_gradings
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| load_photo_sidecars(path).0.unwrap_or_default());
                (
                    Some(cached.base_preview.clone()),
                    Some(cached.cfa.clone()),
                    cached.linear_rgb.clone(),
                    g,
                )
            } else {
                let g = self
                    .library
                    .photo_gradings
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| load_photo_sidecars(path).0.unwrap_or_default());
                (None, None, None, g)
            };
            snapshots.push(TimelapseFrameSnapshot {
                path: path.clone(),
                cached_base,
                cached_cfa,
                cached_linear_rgb,
                grading,
            });
        }
        snapshots
    }

    /// Pass 1: Measures sensor log-luminance per frame without retaining decoded buffers in RAM.
    fn measure_timelapse_log_luminances<F>(
        snapshots: Vec<TimelapseFrameSnapshot>,
        cancel: &std::sync::atomic::AtomicBool,
        notify: &F,
    ) -> Option<(
        Vec<TimelapseFrameSnapshot>,
        Vec<DarkroomGradingState>,
        Vec<f32>,
    )>
    where
        F: Fn(f32, String, Option<Result<PathBuf, String>>),
    {
        let total = snapshots.len();
        let mut valid_snapshots = Vec::with_capacity(total);
        let mut initial_gradings = Vec::with_capacity(total);
        let mut sensor_log_lumas = Vec::with_capacity(total);

        for (idx, snap) in snapshots.into_iter().enumerate() {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                let msg = "Timelapse export cancelled by user.".to_string();
                notify(1.0, "Export cancelled".to_string(), Some(Err(msg)));
                return None;
            }
            let frac = 0.25 * ((idx + 1) as f32 / (total.max(1) as f32));
            notify(
                frac,
                format!("Deflicker analysis ({}/{total})", idx + 1),
                None,
            );

            let log_y_opt = if let Some(ref base) = snap.cached_base {
                let flat_rgb: Vec<f32> = base
                    .samples_rgb
                    .iter()
                    .flat_map(|px| [px[0], px[1], px[2]])
                    .collect();
                Some(rraw_export::compute_frame_log_luminance(&flat_rgb))
            } else if let Some(payload) = decode_photo_file_payload(&snap.path, false, 0) {
                let flat_rgb: Vec<f32> = payload
                    .base_preview
                    .samples_rgb
                    .iter()
                    .flat_map(|px| [px[0], px[1], px[2]])
                    .collect();
                Some(rraw_export::compute_frame_log_luminance(&flat_rgb))
            } else {
                None
            };

            if let Some(log_y) = log_y_opt {
                sensor_log_lumas.push(log_y);
                initial_gradings.push(snap.grading.clone());
                valid_snapshots.push(snap);
            }
        }

        if valid_snapshots.is_empty() {
            let msg = "Failed to decode images in Timelapse sequence.".to_string();
            notify(1.0, msg.clone(), Some(Err(msg)));
            return None;
        }

        Some((valid_snapshots, initial_gradings, sensor_log_lumas))
    }

    /// Pass 2: Streams each frame one by one (`decode -> render -> write -> immediate drop`).
    fn stream_timelapse_frames<F>(
        valid_snapshots: Vec<TimelapseFrameSnapshot>,
        smoothed_gradings: &[DarkroomGradingState],
        export_config: ExportWriteConfig,
        target: TimelapseTarget<'_>,
        cancel: &std::sync::atomic::AtomicBool,
        notify: &F,
    ) -> Option<Option<PathBuf>>
    where
        F: Fn(f32, String, Option<Result<PathBuf, String>>),
    {
        let effective_depth = match export_config.format {
            ExportFormatChoice::Jpeg | ExportFormatChoice::Avif => ExportBitDepth::Bits8,
            ExportFormatChoice::Png | ExportFormatChoice::Tiff | ExportFormatChoice::Jxl => {
                export_config.bit_depth
            }
        };
        let valid_total = valid_snapshots.len();
        let mut last_path = None;

        for (idx, (snap, grading)) in valid_snapshots
            .into_iter()
            .zip(smoothed_gradings.iter())
            .enumerate()
        {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                let msg = "Timelapse export cancelled by user.".to_string();
                notify(1.0, "Export cancelled".to_string(), Some(Err(msg)));
                return None;
            }
            let frac = 0.25 + 0.73 * ((idx as f32 + 0.5) / (valid_total.max(1) as f32));
            notify(
                frac,
                format!("Rendering Timelapse ({}/{valid_total})", idx + 1),
                None,
            );

            let base = match (snap.cached_base, snap.cached_cfa, snap.cached_linear_rgb) {
                (Some(b), Some(cfa), linear_rgb) => Self::build_export_base_with_linear_rgb_static(
                    export_config.scale_mode,
                    &cfa,
                    linear_rgb.as_ref(),
                    &b,
                ),
                (Some(b), None, _) => b,
                (None, _, _) => {
                    let Some(payload) = decode_photo_file_payload(&snap.path, false, 0) else {
                        continue;
                    };
                    Self::build_export_base_with_linear_rgb_static(
                        export_config.scale_mode,
                        &payload.cfa,
                        payload.linear_rgb.as_ref(),
                        &payload.base_preview,
                    )
                }
            };

            let (out_w, out_h, raster_buf) =
                render_export_image_with_depth(&base, grading, 1.0, effective_depth);
            drop(base);

            let seq_filename =
                rraw_export::format_sequence_filename(target.seq_prefix, idx + 1, target.ext);
            let target_path = target.final_dest.join(seq_filename);

            if let Err(e) = Self::write_exported_image_static(
                export_config,
                &target_path,
                out_w,
                out_h,
                raster_buf.as_u8_slice().unwrap_or(&[]),
                raster_buf.as_u16_slice().unwrap_or(&[]),
            ) {
                notify(1.0, format!("Error: {e}"), Some(Err(e)));
                return None;
            }
            last_path = Some(target_path);
        }

        Some(last_path)
    }
}
