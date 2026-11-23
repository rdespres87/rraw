#![forbid(unsafe_code)]

//! Image export execution engines: raster serialization, ISO 21496-1 Gain Map generation, and background batch workers.

use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ByteOrder};
use image::ImageEncoder;

use super::export::ExportWriteConfig;
use crate::app::RrawApp;
use crate::preview::*;
use crate::state::*;

impl RrawApp {
    pub fn write_exported_image_static(
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
        // Keep backward compatibility if export_batch_selection was toggled programmatically
        let mode = if self.export.export_target_mode == ExportTargetMode::CurrentPhoto
            && self.export.export_batch_selection
        {
            ExportTargetMode::BatchSelection
        } else {
            self.export.export_target_mode
        };

        if mode == ExportTargetMode::TimelapseSequence {
            return self.spawn_background_timelapse_export();
        }

        let photos_to_export: Vec<PathBuf> = match mode {
            ExportTargetMode::BatchSelection => {
                let matching_selected: Vec<PathBuf> = self
                    .library
                    .selected_photos
                    .iter()
                    .filter(|p| self.matches_active_filter(p))
                    .cloned()
                    .collect();

                if matching_selected.is_empty() {
                    if let Some(loaded) = &self.session.loaded_photo {
                        vec![loaded.path.clone()]
                    } else {
                        return Err("No photos selected for batch export.".to_string());
                    }
                } else {
                    let mut sorted = matching_selected;
                    sorted.sort();
                    sorted
                }
            }
            ExportTargetMode::CurrentPhoto | ExportTargetMode::TimelapseSequence => {
                if let Some(loaded) = &self.session.loaded_photo {
                    vec![loaded.path.clone()]
                } else {
                    return Err("Aucune photo chargée à exporter.".to_string());
                }
            }
        };

        // Prepare lightweight export tasks on UI thread (< 0.1 ms) with zero disk I/O
        struct ExportTask {
            path: PathBuf,
            target_path: PathBuf,
            cached_base: Option<BaseRgbPreview>,
            cached_cfa: Option<rraw_core::SignedCfaBuffer>,
            cached_grading: Option<DarkroomGradingState>,
        }

        let mut items = Vec::with_capacity(photos_to_export.len());

        let base_dest = self.export.export_custom_dir.clone().unwrap_or_else(|| {
            self.session
                .loaded_photo
                .as_ref()
                .and_then(|p| p.path.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
        });

        let final_dest = if self.export.export_use_subfolder
            && !self.export.export_subfolder.trim().is_empty()
        {
            base_dest.join(self.export.export_subfolder.trim())
        } else {
            base_dest
        };

        let _ = std::fs::create_dir_all(&final_dest);

        let ext = self.export.export_format.extension();
        let suffix = &self.export.export_suffix;

        for photo_path in &photos_to_export {
            let stem = photo_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let target_path = final_dest.join(format!("{stem}{suffix}.{ext}"));

            let is_loaded = self.session.loaded_photo.as_ref().map(|p| p.path.as_path())
                == Some(photo_path.as_path());
            let (cached_base, cached_cfa, cached_grading) = if is_loaded {
                (
                    self.session.base_preview.clone(),
                    self.session.cached_cfa.clone(),
                    Some(self.session.grading.clone()),
                )
            } else if let Some(cached) = self.photo_cache.iter().find(|e| &e.path == photo_path) {
                (
                    Some(cached.base_preview.clone()),
                    Some(cached.cfa.clone()),
                    self.library.photo_gradings.get(photo_path).cloned(),
                )
            } else {
                (
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
                cached_grading,
            });
        }

        if items.is_empty() {
            return Err("Aucune photo n'a pu être préparée pour l'export.".to_string());
        }

        self.save_current_photo_sidecar();

        let export_config = ExportWriteConfig {
            format: self.export.export_format,
            jpeg_quality: self.export.export_jpeg_quality,
            bit_depth: self.export.export_bit_depth,
            scale_mode: self.export.export_scale_mode,
            hdr_gain_map: self.export.export_hdr_gain_map,
        };
        let tx = self.workers.export_sender.clone();

        std::thread::spawn(move || {
            let total = items.len();
            let mut last_path = None;

            for (idx, item) in items.into_iter().enumerate() {
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
                    format!("Décodage {fname} ({}/{total})", idx + 1),
                    None,
                ));

                let (base_preview, cfa_opt) = match (item.cached_base, item.cached_cfa) {
                    (Some(base), cfa) => (base, cfa),
                    (None, _) => {
                        if let Some(payload) = decode_photo_file_payload(&item.path, false, 0) {
                            (payload.base_preview, Some(payload.cfa))
                        } else {
                            let _ = tx.send((
                                base_frac + step_frac,
                                format!("Failed decoding {fname}"),
                                Some(Err(format!("Failed to decode {}", item.path.display()))),
                            ));
                            continue;
                        }
                    }
                };

                let base = if let Some(cfa) = &cfa_opt {
                    Self::build_export_base_static(export_config.scale_mode, cfa, &base_preview)
                } else {
                    base_preview
                };

                let grading = item
                    .cached_grading
                    .unwrap_or_else(|| load_photo_sidecars(&item.path).0.unwrap_or_default());

                // Stage 2: Color grading, tone curves & geometry rendering (25% -> 60%)
                let _ = tx.send((
                    base_frac + step_frac * 0.25,
                    format!("Rendu {fname} ({}/{total})", idx + 1),
                    None,
                ));

                let effective_depth = match export_config.format {
                    ExportFormatChoice::Jpeg | ExportFormatChoice::Avif => ExportBitDepth::Bits8,
                    ExportFormatChoice::Png | ExportFormatChoice::Tiff => export_config.bit_depth,
                };
                let (out_w, out_h, raster_buf) =
                    render_export_image_with_depth(&base, &grading, 1.0, effective_depth);

                // Stage 3: Image compression & file writing (60% -> 98%)
                let _ = tx.send((
                    base_frac + step_frac * 0.60,
                    format!("Encodage {fname} ({}/{total})", idx + 1),
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
                            format!("Enregistré {fname} ({}/{total})", idx + 1),
                            None,
                        ));
                        last_path = Some(item.target_path);
                    }
                    Err(e) => {
                        let _ = tx.send((1.0, format!("Error: {e}"), Some(Err(e))));
                        return;
                    }
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

        Ok(())
    }

    /// Spawns a background streaming Timelapse export worker executing a 2-pass $O(1)$-RAM pipeline:
    /// - Pass 1: Measures lightweight sensor log-luminance per frame (dropping each preview buffer immediately) and computes keyframe ramping & temporal deflicker offsets.
    /// - Pass 2: Streams each frame one by one (`decode -> render_export_image_with_depth -> write -> drop`), emitting incremental progress updates over `export_sender`.
    fn spawn_background_timelapse_export(&mut self) -> Result<(), String> {
        let sequence_paths = self.timelapse_candidate_paths();
        if sequence_paths.is_empty() {
            return Err("Aucune photo disponible pour assembler le Timelapse.".to_string());
        }

        if let Some(loaded) = &self.session.loaded_photo {
            self.library
                .photo_gradings
                .insert(loaded.path.clone(), self.session.grading.clone());
        }
        self.save_current_photo_sidecar();

        struct TimelapseFrameSnapshot {
            path: PathBuf,
            cached_base: Option<BaseRgbPreview>,
            cached_cfa: Option<rraw_core::SignedCfaBuffer>,
            grading: DarkroomGradingState,
        }

        let mut snapshots = Vec::with_capacity(sequence_paths.len());
        for path in &sequence_paths {
            let is_loaded = self.session.loaded_photo.as_ref().map(|p| p.path.as_path())
                == Some(path.as_path());
            let (cached_base, cached_cfa, grading) = if is_loaded {
                (
                    self.session.base_preview.clone(),
                    self.session.cached_cfa.clone(),
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
                    g,
                )
            } else {
                let g = self
                    .library
                    .photo_gradings
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| load_photo_sidecars(path).0.unwrap_or_default());
                (None, None, g)
            };
            snapshots.push(TimelapseFrameSnapshot {
                path: path.clone(),
                cached_base,
                cached_cfa,
                grading,
            });
        }

        let first_path = sequence_paths[0].clone();
        let base_dest = self.export.export_custom_dir.clone().unwrap_or_else(|| {
            first_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        let final_dest = if self.export.export_use_subfolder
            && !self.export.export_subfolder.trim().is_empty()
        {
            base_dest.join(self.export.export_subfolder.trim())
        } else {
            base_dest
        };

        let export_config = ExportWriteConfig {
            format: self.export.export_format,
            jpeg_quality: self.export.export_jpeg_quality,
            bit_depth: self.export.export_bit_depth,
            scale_mode: self.export.export_scale_mode,
            hdr_gain_map: self.export.export_hdr_gain_map,
        };
        let keyframe_ramping = self.export.timelapse_keyframe_ramping;
        let deflicker_cfg = self.export.timelapse_deflicker;
        let seq_prefix = self.export.timelapse_sequence_prefix.clone();
        let fps = self.export.timelapse_fps;
        let tx = self.workers.export_sender.clone();

        std::thread::spawn(move || {
            if let Err(e) = std::fs::create_dir_all(&final_dest) {
                let msg = format!("Failed to create Timelapse directory: {e}");
                let _ = tx.send((1.0, msg.clone(), Some(Err(msg))));
                return;
            }

            let total = snapshots.len();
            let mut valid_snapshots = Vec::with_capacity(total);
            let mut initial_gradings = Vec::with_capacity(total);
            let mut sensor_log_lumas = Vec::with_capacity(total);

            // Pass 1: Measure sensor log-luminance per frame without retaining decoded buffers in RAM
            for (idx, snap) in snapshots.into_iter().enumerate() {
                let frac = 0.25 * ((idx + 1) as f32 / (total.max(1) as f32));
                let _ = tx.send((
                    frac,
                    format!("Analyse deflicker ({}/{total})", idx + 1),
                    None,
                ));

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
                let _ = tx.send((1.0, msg.clone(), Some(Err(msg))));
                return;
            }

            let (smoothed_gradings, offsets_ev) = prepare_timelapse_gradings_from_lumas(
                &initial_gradings,
                &sensor_log_lumas,
                keyframe_ramping,
                &deflicker_cfg,
            );

            let ext = export_config.format.extension();
            let effective_depth = match export_config.format {
                ExportFormatChoice::Jpeg | ExportFormatChoice::Avif => ExportBitDepth::Bits8,
                ExportFormatChoice::Png | ExportFormatChoice::Tiff => export_config.bit_depth,
            };
            let valid_total = valid_snapshots.len();
            let mut last_path = None;

            // Pass 2: Stream frame-by-frame (decode -> render -> write -> immediate drop)
            for (idx, (snap, grading)) in valid_snapshots
                .into_iter()
                .zip(smoothed_gradings.iter())
                .enumerate()
            {
                let frac = 0.25 + 0.73 * ((idx as f32 + 0.5) / (valid_total.max(1) as f32));
                let _ = tx.send((
                    frac,
                    format!("Rendu Timelapse ({}/{valid_total})", idx + 1),
                    None,
                ));

                let base = match (snap.cached_base, snap.cached_cfa) {
                    (Some(b), Some(cfa)) => {
                        Self::build_export_base_static(export_config.scale_mode, &cfa, &b)
                    }
                    (Some(b), None) => b,
                    (None, _) => {
                        let Some(payload) = decode_photo_file_payload(&snap.path, false, 0) else {
                            continue;
                        };
                        Self::build_export_base_static(
                            export_config.scale_mode,
                            &payload.cfa,
                            &payload.base_preview,
                        )
                    }
                };

                let (out_w, out_h, raster_buf) =
                    render_export_image_with_depth(&base, grading, 1.0, effective_depth);
                drop(base);

                let seq_filename = rraw_export::format_sequence_filename(&seq_prefix, idx + 1, ext);
                let target_path = final_dest.join(seq_filename);

                if let Err(e) = Self::write_exported_image_static(
                    export_config,
                    &target_path,
                    out_w,
                    out_h,
                    raster_buf.as_u8_slice().unwrap_or(&[]),
                    raster_buf.as_u16_slice().unwrap_or(&[]),
                ) {
                    let _ = tx.send((1.0, format!("Error: {e}"), Some(Err(e))));
                    return;
                }
                last_path = Some(target_path);
            }

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
                    let _ = tx.send((1.0, "Timelapse completed".to_string(), Some(Ok(out_path))));
                }
                Err(e) => {
                    let _ = tx.send((1.0, format!("Error: {e}"), Some(Err(e))));
                }
            }
        });

        Ok(())
    }
}
