#![forbid(unsafe_code)]

//! Image export execution engines: raster serialization, ISO 21496-1 Gain Map generation, and background batch workers.

use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ByteOrder};
use image::ImageEncoder;

use super::export::ExportWriteConfig;
use crate::app::RrawApp;
use crate::preview::*;
use crate::state::*;
use crate::workers::ExportProgressMessage;

/// Per-photo data handed to the background export worker.
struct ExportTask {
    path: PathBuf,
    target_path: PathBuf,
    cached_base: Option<BaseRgbPreview>,
    cached_cfa: Option<rraw_core::SignedCfaBuffer>,
    cached_linear_rgb: Option<rraw_decode::LinearRgbBuffer>,
    cached_grading: Option<DarkroomGradingState>,
}

/// Outcome of a single export task within the batch worker.
enum ExportTaskStep {
    /// File written successfully to the target path.
    Saved(PathBuf),
    /// Decode failed; skip this item and continue with the next.
    DecodeSkipped,
    /// User cancelled; abort the entire batch.
    Cancelled,
    /// Irrecoverable write error; abort the entire batch.
    FatalError,
}

impl RrawApp {
    pub fn write_exported_image_static(
        config: ExportWriteConfig,
        target_path: &Path,
        out_w: usize,
        out_h: usize,
        u8_bytes: &[u8],
        u16_samples: &[u16],
    ) -> Result<(), String> {
        let tmp_name = format!(
            ".{}.rraw_tmp_{}",
            target_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("export"),
            std::process::id()
        );
        let tmp_path = target_path.with_file_name(tmp_name);
        let write_result = Self::write_exported_image_to_path(
            config,
            &tmp_path,
            out_w,
            out_h,
            u8_bytes,
            u16_samples,
        );
        if let Err(err) = write_result {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err);
        }
        if let Err(err) = std::fs::rename(&tmp_path, target_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("Failed to finalize exported file: {err}"));
        }
        if config.hdr_gain_map {
            Self::write_iso21496_gain_map_sidecars(
                target_path,
                out_w,
                out_h,
                u8_bytes,
                u16_samples,
            )?;
        }
        Ok(())
    }

    fn write_exported_image_to_path(
        config: ExportWriteConfig,
        target_path: &Path,
        out_w: usize,
        out_h: usize,
        u8_bytes: &[u8],
        u16_samples: &[u16],
    ) -> Result<(), String> {
        match config.format {
            ExportFormatChoice::Jpeg => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Failed to create file: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                let quality = config.jpeg_quality.clamp(1, 100);
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality)
                    .write_image(
                        u8_bytes,
                        out_w as u32,
                        out_h as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| format!("JPEG encoding error: {e}"))?;
            }
            ExportFormatChoice::Png => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Failed to create file: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                match config.bit_depth {
                    ExportBitDepth::Bits16 => {
                        let mut encoder =
                            png::Encoder::new(&mut writer, out_w as u32, out_h as u32);
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Sixteen);
                        let mut png_writer = encoder
                            .write_header()
                            .map_err(|e| format!("PNG 16 header error: {e}"))?;
                        let mut raw_u16_bytes = vec![0_u8; out_w * out_h * 3 * 2];
                        for (i, &sample) in u16_samples.iter().enumerate() {
                            BigEndian::write_u16(&mut raw_u16_bytes[i * 2..i * 2 + 2], sample);
                        }
                        png_writer
                            .write_image_data(&raw_u16_bytes)
                            .map_err(|e| format!("PNG 16 write error: {e}"))?;
                    }
                    ExportBitDepth::Bits8 => {
                        let mut encoder =
                            png::Encoder::new(&mut writer, out_w as u32, out_h as u32);
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Eight);
                        let mut png_writer = encoder
                            .write_header()
                            .map_err(|e| format!("PNG 8 header error: {e}"))?;
                        png_writer
                            .write_image_data(u8_bytes)
                            .map_err(|e| format!("PNG 8 write error: {e}"))?;
                    }
                }
            }
            ExportFormatChoice::Tiff => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Failed to create file: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                match config.bit_depth {
                    ExportBitDepth::Bits16 => {
                        image::codecs::tiff::TiffEncoder::new(&mut writer)
                            .write_image(
                                bytemuck::cast_slice(u16_samples),
                                out_w as u32,
                                out_h as u32,
                                image::ExtendedColorType::Rgb16,
                            )
                            .map_err(|e| format!("TIFF 16 encoding error: {e}"))?;
                    }
                    ExportBitDepth::Bits8 => {
                        image::codecs::tiff::TiffEncoder::new(&mut writer)
                            .write_image(
                                u8_bytes,
                                out_w as u32,
                                out_h as u32,
                                image::ExtendedColorType::Rgb8,
                            )
                            .map_err(|e| format!("TIFF 8 encoding error: {e}"))?;
                    }
                }
            }
            ExportFormatChoice::Avif => {
                use std::io::Write;
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Failed to create file: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                let quality = config.jpeg_quality.clamp(1, 100);
                let encoder = image::codecs::avif::AvifEncoder::new_with_speed_quality(
                    &mut writer,
                    10,
                    quality,
                );
                encoder
                    .write_image(
                        u8_bytes,
                        out_w as u32,
                        out_h as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| format!("AVIF encoding error: {e}"))?;
                writer
                    .flush()
                    .map_err(|e| format!("Failed to flush AVIF file: {e}"))?;
            }
            ExportFormatChoice::Jxl => {
                use std::io::Write;
                let jxl_bytes = match config.bit_depth {
                    ExportBitDepth::Bits16 => {
                        rraw_export::encode_jxl_rgb_u16(u16_samples, out_w, out_h, 4)
                    }
                    ExportBitDepth::Bits8 => {
                        rraw_export::encode_jxl_rgb_u8(u8_bytes, out_w, out_h, 4)
                    }
                }
                .map_err(|e| format!("JPEG XL encoding error: {e}"))?;
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Failed to create file: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                writer
                    .write_all(&jxl_bytes)
                    .map_err(|e| format!("JPEG XL write error: {e}"))?;
                writer
                    .flush()
                    .map_err(|e| format!("Failed to flush JPEG XL file: {e}"))?;
            }
        }
        Ok(())
    }

    /// Computes the ISO 21496-1 HDR Gain Map via `rraw_export::GainMapCalculator` and writes
    /// the standardized `.hdrgm.xmp` XMP packet and `.tmap` binary ISOBMFF box alongside `target_path`.
    pub fn write_iso21496_gain_map_sidecars(
        target_path: &Path,
        out_w: usize,
        out_h: usize,
        u8_bytes: &[u8],
        u16_samples: &[u16],
    ) -> Result<(), String> {
        let count = out_w.saturating_mul(out_h);
        let expected_samples = count.saturating_mul(3);
        if count == 0 {
            return Ok(());
        }

        let sdr_f32: Vec<f32> = if u8_bytes.len() == expected_samples {
            u8_bytes.iter().map(|&b| (b as f32) / 255.0).collect()
        } else if u16_samples.len() == expected_samples {
            u16_samples
                .iter()
                .map(|&s| ((s as f32) / 65535.0).min(1.0))
                .collect()
        } else {
            vec![0.5_f32; expected_samples]
        };

        let hdr_f32: Vec<f32> = if u16_samples.len() == expected_samples {
            u16_samples
                .iter()
                .map(|&s| {
                    let norm = (s as f32) / 65535.0;
                    if norm > 0.75 {
                        norm * (1.0 + (norm - 0.75) * 8.0)
                    } else {
                        norm
                    }
                })
                .collect()
        } else {
            sdr_f32
                .iter()
                .map(|&v| {
                    if v > 0.75 {
                        v * (1.0 + (v - 0.75) * 8.0)
                    } else {
                        v
                    }
                })
                .collect()
        };

        let base_params = rraw_export::Iso21496Params::default();
        let gain_map =
            rraw_export::GainMapCalculator::compute(&sdr_f32, &hdr_f32, out_w, out_h, &base_params)
                .map_err(|e| format!("ISO 21496-1 Gain Map calculation failed: {e}"))?;

        let peak_d = gain_map.iter().copied().fold(0.0_f32, f32::max);
        let effective_max = (1.0 + peak_d * (base_params.gain_map_max - 1.0)).clamp(1.5, 8.0);
        let params = rraw_export::Iso21496Params {
            gain_map_max: effective_max,
            hdr_capacity_max: effective_max,
            ..base_params
        };

        let xmp_xml = rraw_export::Iso21496XmpPacket::encode(&params);
        let tmap_bytes = rraw_export::Iso21496TmapBox::encode(&params)
            .map_err(|e| format!("ISO 21496-1 tmap encode failed: {e}"))?;

        let xmp_path = target_path.with_extension("hdrgm.xmp");
        let tmap_path = target_path.with_extension("tmap");
        atomic_write_str(&xmp_path, &xmp_xml)
            .map_err(|e| format!("Failed to write ISO 21496-1 XMP packet: {e}"))?;
        crate::sidecar::atomic_write_bytes(&tmap_path, &tmap_bytes)
            .map_err(|e| format!("Failed to write ISO 21496-1 tmap box: {e}"))?;
        Ok(())
    }

    #[must_use]
    pub fn compute_export_target_width_static(
        scale_mode: ExportScaleMode,
        cfa_width: usize,
        cfa_height: usize,
    ) -> usize {
        match scale_mode {
            ExportScaleMode::Original => cfa_width.max(1),
            ExportScaleMode::Percent75 => ((cfa_width * 3) / 4).max(1),
            ExportScaleMode::Percent50 => (cfa_width / 2).max(1),
            ExportScaleMode::FitLongEdge2048 => {
                if cfa_width >= cfa_height {
                    2048.min(cfa_width)
                } else {
                    ((2048 * cfa_width) / cfa_height.max(1)).clamp(1, cfa_width)
                }
            }
            ExportScaleMode::FitLongEdge3840 => {
                if cfa_width >= cfa_height {
                    3840.min(cfa_width)
                } else {
                    ((3840 * cfa_width) / cfa_height.max(1)).clamp(1, cfa_width)
                }
            }
        }
    }

    #[must_use]
    pub fn build_export_base_static(
        scale_mode: ExportScaleMode,
        cfa: &rraw_core::SignedCfaBuffer,
        ref_preview: &BaseRgbPreview,
    ) -> BaseRgbPreview {
        Self::build_export_base_with_linear_rgb_static(scale_mode, cfa, None, ref_preview)
    }

    #[must_use]
    pub fn build_export_base_with_linear_rgb_static(
        scale_mode: ExportScaleMode,
        cfa: &rraw_core::SignedCfaBuffer,
        linear_rgb: Option<&rraw_decode::LinearRgbBuffer>,
        ref_preview: &BaseRgbPreview,
    ) -> BaseRgbPreview {
        if let Some(rgb) = linear_rgb {
            let target_width =
                Self::compute_export_target_width_static(scale_mode, rgb.width(), rgb.height());
            let mut b = BaseRgbPreview::from_linear_rgb_with_color_and_target_width(
                rgb.samples(),
                rgb.width(),
                rgb.height(),
                ref_preview.as_shot_wb,
                None,
                target_width,
            );
            b.cam_to_srgb = ref_preview.cam_to_srgb;
            b.matched_lens = ref_preview.matched_lens.clone();
            b.focal_length_mm = ref_preview.focal_length_mm;
            b.aperture_f_number = ref_preview.aperture_f_number;
            return b.with_exif_orientation(ref_preview.exif_orientation);
        }
        if cfa.width() <= 1 || cfa.height() <= 1 {
            return ref_preview.clone();
        }
        let target_width =
            Self::compute_export_target_width_static(scale_mode, cfa.width(), cfa.height());
        let mut b = BaseRgbPreview::from_cfa_with_color_and_target_width(
            cfa,
            ref_preview.as_shot_wb,
            None,
            target_width,
        );
        b.cam_to_srgb = ref_preview.cam_to_srgb;
        b.matched_lens = ref_preview.matched_lens.clone();
        b.focal_length_mm = ref_preview.focal_length_mm;
        b.aperture_f_number = ref_preview.aperture_f_number;
        b.with_exif_orientation(ref_preview.exif_orientation)
    }

    /// Returns the ordered list of photos participating in a Timelapse sequence export.
    #[must_use]
    pub fn timelapse_candidate_paths(&self) -> Vec<PathBuf> {
        if self.library.selected_photos.len() >= 2 {
            let mut sorted: Vec<PathBuf> = self.library.selected_photos.iter().cloned().collect();
            sorted.sort();
            sorted
        } else {
            let mut filtered: Vec<PathBuf> = self
                .library
                .directory_entries
                .iter()
                .filter(|p| {
                    let rating = self.library.photo_ratings.get(*p).copied().unwrap_or(0);
                    self.library.rating_filter.matches(rating)
                })
                .cloned()
                .collect();
            filtered.sort();
            filtered
        }
    }

    /// Spawns a background thread executing the export without blocking the UI event loop.
    pub fn spawn_background_export(&mut self) -> Result<(), String> {
        // Keep backward compatibility if batch_selection was toggled programmatically
        let mode = if self.export.config.target_mode == ExportTargetMode::CurrentPhoto
            && self.export.config.batch_selection
        {
            ExportTargetMode::BatchSelection
        } else {
            self.export.config.target_mode
        };

        if mode == ExportTargetMode::TimelapseSequence {
            return self.spawn_background_timelapse_export();
        }

        let photos = self.collect_export_photo_paths(mode)?;
        let items = self.prepare_export_tasks(&photos)?;

        self.save_current_photo_sidecar();

        let export_config = ExportWriteConfig {
            format: self.export.config.format,
            jpeg_quality: self.export.config.jpeg_quality,
            bit_depth: self.export.config.bit_depth,
            scale_mode: self.export.config.scale_mode,
            hdr_gain_map: self.export.config.hdr_gain_map,
        };
        let tx = self.workers.export_sender.clone();
        let cancel = self.workers.reset_export_cancel();

        Self::run_background_export_worker(items, export_config, tx, cancel);
        Ok(())
    }

    /// Resolves the list of photo paths to export for the given target mode.
    fn collect_export_photo_paths(&self, mode: ExportTargetMode) -> Result<Vec<PathBuf>, String> {
        match mode {
            ExportTargetMode::BatchSelection => {
                let matching: Vec<PathBuf> = self
                    .library
                    .selected_photos
                    .iter()
                    .filter(|p| self.matches_active_filter(p))
                    .cloned()
                    .collect();

                if matching.is_empty() {
                    if let Some(loaded) = &self.session.loaded_photo {
                        Ok(vec![loaded.path.clone()])
                    } else {
                        Err("No photos selected for batch export.".to_string())
                    }
                } else {
                    let mut sorted = matching;
                    sorted.sort();
                    Ok(sorted)
                }
            }
            ExportTargetMode::CurrentPhoto | ExportTargetMode::TimelapseSequence => {
                if let Some(loaded) = &self.session.loaded_photo {
                    Ok(vec![loaded.path.clone()])
                } else {
                    Err("No photo loaded to export.".to_string())
                }
            }
        }
    }

    /// Builds `ExportTask` entries (target paths, cached buffers) for the given photos.
    fn prepare_export_tasks(
        &self,
        photos_to_export: &[PathBuf],
    ) -> Result<Vec<ExportTask>, String> {
        let base_dest = self.export.config.custom_dir.clone().unwrap_or_else(|| {
            self.session
                .loaded_photo
                .as_ref()
                .and_then(|p| p.path.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
        });

        let final_dest = if self.export.config.use_subfolder
            && !self.export.config.subfolder.trim().is_empty()
        {
            base_dest.join(self.export.config.subfolder.trim())
        } else {
            base_dest
        };

        let _ = std::fs::create_dir_all(&final_dest);

        let ext = self.export.config.format.extension();
        let suffix = &self.export.config.suffix;

        let mut items = Vec::with_capacity(photos_to_export.len());
        for photo_path in photos_to_export {
            let stem = photo_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let target_path = final_dest.join(format!("{stem}{suffix}.{ext}"));

            let is_loaded = self.session.loaded_photo.as_ref().map(|p| p.path.as_path())
                == Some(photo_path.as_path());
            let (cached_base, cached_cfa, cached_linear_rgb, cached_grading) = if is_loaded {
                (
                    self.session.base_preview.clone(),
                    self.session.cached_cfa.clone(),
                    self.session.cached_linear_rgb.clone(),
                    Some(self.session.grading.clone()),
                )
            } else if let Some(cached) = self.photo_cache.iter().find(|e| &e.path == photo_path) {
                (
                    Some(cached.base_preview.clone()),
                    Some(cached.cfa.clone()),
                    cached.linear_rgb.clone(),
                    self.library.photo_gradings.get(photo_path).cloned(),
                )
            } else {
                (
                    None,
                    None,
                    None,
                    self.library.photo_gradings.get(photo_path).cloned(),
                )
            };

            items.push(ExportTask {
                path: photo_path.clone(),
                target_path,
                cached_base,
                cached_cfa,
                cached_linear_rgb,
                cached_grading,
            });
        }

        if items.is_empty() {
            return Err("No photos could be prepared for export.".to_string());
        }
        Ok(items)
    }

    /// Returns `true` and sends the cancellation progress message if the flag is set.
    fn is_export_cancelled(
        cancel: &std::sync::atomic::AtomicBool,
        tx: &std::sync::mpsc::Sender<ExportProgressMessage>,
    ) -> bool {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            let msg = "Export cancelled by user.".to_string();
            let _ = tx.send((1.0, "Export cancelled".to_string(), Some(Err(msg))));
            true
        } else {
            false
        }
    }

    /// Spawns the background thread that processes all export tasks sequentially.
    fn run_background_export_worker(
        items: Vec<ExportTask>,
        export_config: ExportWriteConfig,
        tx: std::sync::mpsc::Sender<ExportProgressMessage>,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        std::thread::spawn(move || {
            let total = items.len();
            let mut last_path: Option<PathBuf> = None;

            for (idx, item) in items.into_iter().enumerate() {
                if Self::is_export_cancelled(&cancel, &tx) {
                    return;
                }

                match Self::execute_single_export_task(
                    item,
                    export_config,
                    idx,
                    total,
                    &tx,
                    &cancel,
                ) {
                    ExportTaskStep::Saved(path) => {
                        last_path = Some(path);
                    }
                    ExportTaskStep::DecodeSkipped => continue,
                    ExportTaskStep::Cancelled | ExportTaskStep::FatalError => return,
                }
            }

            let result = last_path.ok_or_else(|| "Export failed".to_string());
            let final_msg = if total > 1 {
                format!("Export completed: {total} photos exported")
            } else {
                "Export completed".to_string()
            };
            let _ = tx.send((1.0, final_msg, Some(result)));
        });
    }

    /// Executes one export task: decode -> render -> encode and write.
    fn execute_single_export_task(
        item: ExportTask,
        export_config: ExportWriteConfig,
        idx: usize,
        total: usize,
        tx: &std::sync::mpsc::Sender<ExportProgressMessage>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> ExportTaskStep {
        let base_frac = (idx as f32) / (total as f32).max(1.0);
        let step_frac = 1.0 / (total as f32).max(1.0);
        let fname = item
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("photo");

        // Stage 1: Preparation & decoding (0% -> 25%)
        let _ = tx.send((
            base_frac + step_frac * 0.05,
            format!("Decoding {fname} ({}/{total})", idx + 1),
            None,
        ));

        let (base_preview, cfa_opt, linear_rgb_opt) =
            match (item.cached_base, item.cached_cfa, item.cached_linear_rgb) {
                (Some(base), cfa, linear_rgb) => (base, cfa, linear_rgb),
                (None, _, _) => {
                    if let Some(payload) = decode_photo_file_payload(&item.path, false, 0) {
                        (payload.base_preview, Some(payload.cfa), payload.linear_rgb)
                    } else {
                        let _ = tx.send((
                            base_frac + step_frac,
                            format!("Failed decoding {fname}"),
                            Some(Err(format!("Failed to decode {}", item.path.display()))),
                        ));
                        return ExportTaskStep::DecodeSkipped;
                    }
                }
            };

        let base = if let Some(cfa) = &cfa_opt {
            Self::build_export_base_with_linear_rgb_static(
                export_config.scale_mode,
                cfa,
                linear_rgb_opt.as_ref(),
                &base_preview,
            )
        } else {
            base_preview
        };

        let grading = item
            .cached_grading
            .unwrap_or_else(|| load_photo_sidecars(&item.path).0.unwrap_or_default());

        // Stage 2: Color grading, tone curves & geometry rendering (25% -> 60%)
        if Self::is_export_cancelled(cancel, tx) {
            return ExportTaskStep::Cancelled;
        }
        let _ = tx.send((
            base_frac + step_frac * 0.25,
            format!("Rendering {fname} ({}/{total})", idx + 1),
            None,
        ));

        let effective_depth = match export_config.format {
            ExportFormatChoice::Jpeg | ExportFormatChoice::Avif => ExportBitDepth::Bits8,
            ExportFormatChoice::Png | ExportFormatChoice::Tiff | ExportFormatChoice::Jxl => {
                export_config.bit_depth
            }
        };
        let (out_w, out_h, raster_buf) =
            render_export_image_with_depth(&base, &grading, 1.0, effective_depth);

        // Stage 3: Image compression & file writing (60% -> 98%)
        if Self::is_export_cancelled(cancel, tx) {
            return ExportTaskStep::Cancelled;
        }
        let _ = tx.send((
            base_frac + step_frac * 0.60,
            format!("Encoding {fname} ({}/{total})", idx + 1),
            None,
        ));

        let outcome = Self::write_exported_image_static(
            export_config,
            &item.target_path,
            out_w,
            out_h,
            raster_buf.as_u8_slice().unwrap_or(&[]),
            raster_buf.as_u16_slice().unwrap_or(&[]),
        );

        match outcome {
            Ok(()) => {
                let _ = tx.send((
                    base_frac + step_frac * 0.98,
                    format!("Saved {fname} ({}/{total})", idx + 1),
                    None,
                ));
                ExportTaskStep::Saved(item.target_path)
            }
            Err(e) => {
                let _ = tx.send((1.0, format!("Error: {e}"), Some(Err(e))));
                ExportTaskStep::FatalError
            }
        }
    }

    /// Requests cooperative cancellation of the currently running background export or timelapse job.
    pub fn cancel_active_export(&mut self) {
        self.workers.cancel_export();
        self.export.stats.progress_label =
            crate::i18n::tc(self.lang(), "Export.CancellingStatus").to_owned();
    }
}
