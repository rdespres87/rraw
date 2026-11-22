#![forbid(unsafe_code)]

//! Image export dialog, batch processor, and RAW Timelapse & Temporal Deflicker sequence exporter.

use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ByteOrder};
use egui::Color32;
use image::ImageEncoder;

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::preview::*;
use crate::state::*;

/// Configuration parameters for single image encoding and disk serialization.
#[derive(Debug, Clone, Copy)]
pub struct ExportWriteConfig {
    pub format: ExportFormatChoice,
    pub jpeg_quality: u8,
    pub bit_depth: ExportBitDepth,
    pub scale_mode: ExportScaleMode,
}

impl RrawApp {
    fn write_exported_image(
        &self,
        target_path: &Path,
        out_w: usize,
        out_h: usize,
        u8_bytes: &[u8],
        u16_samples: &[u16],
    ) -> Result<(), String> {
        let cfg = ExportWriteConfig {
            format: self.export.export_format,
            jpeg_quality: self.export.export_jpeg_quality,
            bit_depth: self.export.export_bit_depth,
            scale_mode: self.export.export_scale_mode,
        };
        Self::write_exported_image_static(cfg, target_path, out_w, out_h, u8_bytes, u16_samples)
    }

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
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                let quality = config.jpeg_quality.clamp(1, 100);
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality)
                    .write_image(
                        u8_bytes,
                        out_w as u32,
                        out_h as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| format!("Erreur d'encodage JPEG: {e}"))?;
            }
            ExportFormatChoice::Png => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                match config.bit_depth {
                    ExportBitDepth::Bits16 => {
                        let mut encoder =
                            png::Encoder::new(&mut writer, out_w as u32, out_h as u32);
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Sixteen);
                        let mut png_writer = encoder
                            .write_header()
                            .map_err(|e| format!("Erreur header PNG 16: {e}"))?;
                        let mut raw_u16_bytes = vec![0_u8; out_w * out_h * 3 * 2];
                        for (i, &sample) in u16_samples.iter().enumerate() {
                            BigEndian::write_u16(&mut raw_u16_bytes[i * 2..i * 2 + 2], sample);
                        }
                        png_writer
                            .write_image_data(&raw_u16_bytes)
                            .map_err(|e| format!("Erreur d'écriture PNG 16: {e}"))?;
                    }
                    ExportBitDepth::Bits8 => {
                        let mut encoder =
                            png::Encoder::new(&mut writer, out_w as u32, out_h as u32);
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Eight);
                        let mut png_writer = encoder
                            .write_header()
                            .map_err(|e| format!("Erreur header PNG 8: {e}"))?;
                        png_writer
                            .write_image_data(u8_bytes)
                            .map_err(|e| format!("Erreur d'écriture PNG 8: {e}"))?;
                    }
                }
            }
            ExportFormatChoice::Tiff => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
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
                            .map_err(|e| format!("Erreur d'encodage TIFF 16: {e}"))?;
                    }
                    ExportBitDepth::Bits8 => {
                        image::codecs::tiff::TiffEncoder::new(&mut writer)
                            .write_image(
                                u8_bytes,
                                out_w as u32,
                                out_h as u32,
                                image::ExtendedColorType::Rgb8,
                            )
                            .map_err(|e| format!("Erreur d'encodage TIFF 8: {e}"))?;
                    }
                }
            }
            ExportFormatChoice::Avif => {
                use std::io::Write;
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
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
                    .map_err(|e| format!("Erreur d'encodage AVIF: {e}"))?;
                writer
                    .flush()
                    .map_err(|e| format!("Erreur lors de la finalisation du fichier AVIF: {e}"))?;
            }
        }
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
        b.with_exif_orientation(ref_preview.exif_orientation)
    }

    fn build_export_base_from_cfa(
        &self,
        cfa: &rraw_core::SignedCfaBuffer,
        ref_preview: &BaseRgbPreview,
    ) -> BaseRgbPreview {
        Self::build_export_base_static(self.export.export_scale_mode, cfa, ref_preview)
    }

    fn resolve_photo_preview_and_grading(
        &self,
        photo_path: &Path,
    ) -> Option<(BaseRgbPreview, DarkroomGradingState)> {
        let is_loaded =
            self.session.loaded_photo.as_ref().map(|p| p.path.as_path()) == Some(photo_path);
        if is_loaded {
            let base = match (&self.session.cached_cfa, &self.session.base_preview) {
                (Some(cfa), Some(prev)) => self.build_export_base_from_cfa(cfa, prev),
                (_, Some(prev)) => prev.clone(),
                _ => return None,
            };
            Some((base, self.session.grading.clone()))
        } else if let Some(cached) = self.photo_cache.iter().find(|e| e.path == photo_path) {
            let grading = self
                .library
                .photo_gradings
                .get(photo_path)
                .cloned()
                .unwrap_or_else(|| load_photo_sidecars(photo_path).0.unwrap_or_default());
            let base = self.build_export_base_from_cfa(&cached.cfa, &cached.base_preview);
            Some((base, grading))
        } else if let Some(payload) = decode_photo_file_payload(photo_path, false, 0) {
            let grading = self
                .library
                .photo_gradings
                .get(photo_path)
                .cloned()
                .unwrap_or_else(|| load_photo_sidecars(photo_path).0.unwrap_or_default());
            let base = self.build_export_base_from_cfa(&payload.cfa, &payload.base_preview);
            Some((base, grading))
        } else {
            None
        }
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
            // For timelapse, use existing synchronous call or background worker
            let res = self.execute_timelapse_export();
            let _ =
                self.workers
                    .export_sender
                    .send((1.0, "Timelapse terminé".to_string(), Some(res)));
            return Ok(());
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
                        return Err("Aucune photo sélectionnée pour l'export par lot.".to_string());
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
                                format!("Échec du décodage de {fname}"),
                                Some(Err(format!(
                                    "Impossible de décoder {}",
                                    item.path.display()
                                ))),
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

                let (out_w, out_h, u8_bytes, u16_samples) =
                    render_export_image(&base, &grading, 1.0);

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
                    &u8_bytes,
                    &u16_samples,
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
                        let _ = tx.send((1.0, format!("Erreur: {e}"), Some(Err(e))));
                        return;
                    }
                }
            }

            let result = last_path.ok_or_else(|| "Erreur lors de l'exportation".to_string());
            let final_msg = if total > 1 {
                format!("Export terminé : {total} photos exportées")
            } else {
                "Export terminé".to_string()
            };
            let _ = tx.send((1.0, final_msg, Some(result)));
        });

        Ok(())
    }

    /// Executes photographic export (Single photo, Batch selection, or Timelapse sequence with
    /// temporal anti-flicker and keyframe ramping).
    pub fn execute_export(&mut self) -> Result<PathBuf, String> {
        // Keep backward compatibility if export_batch_selection was toggled programmatically
        let mode = if self.export.export_target_mode == ExportTargetMode::CurrentPhoto
            && self.export.export_batch_selection
        {
            ExportTargetMode::BatchSelection
        } else {
            self.export.export_target_mode
        };

        if mode == ExportTargetMode::TimelapseSequence {
            return self.execute_timelapse_export();
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
                        return Err("Aucune photo sélectionnée pour l'export par lot.".to_string());
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

        let mut last_exported_path = None;
        let mut count = 0;

        for photo_path in &photos_to_export {
            let Some((base_preview, grading)) = self.resolve_photo_preview_and_grading(photo_path)
            else {
                continue;
            };

            let (out_w, out_h, u8_bytes, u16_samples) =
                render_export_image(&base_preview, &grading, 1.0);

            let base_dest = self.export.export_custom_dir.clone().unwrap_or_else(|| {
                photo_path
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

            let _ = std::fs::create_dir_all(&final_dest);

            let stem = photo_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let ext = self.export.export_format.extension();
            let suffix = &self.export.export_suffix;
            let target_path = final_dest.join(format!("{stem}{suffix}.{ext}"));

            self.write_exported_image(&target_path, out_w, out_h, &u8_bytes, &u16_samples)?;
            last_exported_path = Some(target_path);
            count += 1;
        }

        if count == 0 {
            return Err("Aucune photo n'a pu être exportée.".to_string());
        }

        self.save_current_photo_sidecar();

        last_exported_path.ok_or_else(|| "Erreur lors de l'export".to_string())
    }

    fn execute_timelapse_export(&mut self) -> Result<PathBuf, String> {
        let sequence_paths = self.timelapse_candidate_paths();
        if sequence_paths.is_empty() {
            return Err("Aucune photo disponible pour assembler le Timelapse.".to_string());
        }

        // Persist current photo grading in memory map before assembling sequence
        if let Some(loaded) = &self.session.loaded_photo {
            self.library
                .photo_gradings
                .insert(loaded.path.clone(), self.session.grading.clone());
        }

        let mut decoded_frames: Vec<(PathBuf, BaseRgbPreview, DarkroomGradingState)> =
            Vec::with_capacity(sequence_paths.len());

        for path in &sequence_paths {
            if let Some((base, grading)) = self.resolve_photo_preview_and_grading(path) {
                decoded_frames.push((path.clone(), base, grading));
            }
        }

        if decoded_frames.is_empty() {
            return Err("Impossible de décoder les images de la séquence Timelapse.".to_string());
        }

        let frame_refs: Vec<(&BaseRgbPreview, DarkroomGradingState)> = decoded_frames
            .iter()
            .map(|(_, b, g)| (b, g.clone()))
            .collect();

        let (smoothed_gradings, offsets_ev) = prepare_timelapse_frame_gradings(
            &frame_refs,
            self.export.timelapse_keyframe_ramping,
            &self.export.timelapse_deflicker,
        );

        let first_path = &decoded_frames[0].0;
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

        std::fs::create_dir_all(&final_dest)
            .map_err(|e| format!("Impossible de créer le dossier Timelapse: {e}"))?;

        let ext = self.export.export_format.extension();
        let mut last_path = None;

        for (idx, ((_, base, _), grading)) in decoded_frames
            .iter()
            .zip(smoothed_gradings.iter())
            .enumerate()
        {
            let (out_w, out_h, u8_bytes, u16_samples) = render_export_image(base, grading, 1.0);
            let seq_filename = rraw_export::format_sequence_filename(
                &self.export.timelapse_sequence_prefix,
                idx + 1,
                ext,
            );
            let target_path = final_dest.join(seq_filename);
            self.write_exported_image(&target_path, out_w, out_h, &u8_bytes, &u16_samples)?;
            last_path = Some(target_path);
        }

        let manifest_path = rraw_export::write_timelapse_manifest(
            &final_dest,
            &self.export.timelapse_sequence_prefix,
            ext,
            self.export.timelapse_fps,
            decoded_frames.len(),
            &offsets_ev,
        )
        .map_err(|e| format!("Erreur lors de l'écriture du manifeste Timelapse: {e}"))?;

        self.save_current_photo_sidecar();

        Ok(last_path.unwrap_or(manifest_path))
    }

    /// Renders the shared Timelapse & Holy Grail keyframe editor UI (used both in the right
    /// inspector side panel and inside the floating export window).
    pub fn render_timelapse_controls_ui(&mut self, ui: &mut egui::Ui, id_salt_prefix: &str) {
        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let tl_paths = self.timelapse_candidate_paths();
        let frame_count = tl_paths.len();
        let dur_s = self.export.timelapse_fps.duration_seconds(frame_count);

        let seq_text = if lang == Language::Fr {
            format!("🎞️ Séquence : {} images", frame_count)
        } else {
            format!("🎞️ Sequence: {} frames", frame_count)
        };
        let dur_text = if lang == Language::Fr {
            format!(
                "Durée : {:.2} s @ {} fps",
                dur_s,
                self.export.timelapse_fps.fps()
            )
        } else {
            format!(
                "Duration: {:.2} s @ {} fps",
                dur_s,
                self.export.timelapse_fps.fps()
            )
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(seq_text)
                    .strong()
                    .color(Color32::from_rgb(120, 220, 255)),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(dur_text)
                    .strong()
                    .color(Color32::from_rgb(130, 235, 165)),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(t(Key::TimelapseFps));
            egui::ComboBox::from_id_salt(format!("{id_salt_prefix}_timelapse_fps_combo"))
                .selected_text(self.export.timelapse_fps.label())
                .show_ui(ui, |ui| {
                    for fps in rraw_export::TimelapseFrameRate::ALL {
                        ui.selectable_value(&mut self.export.timelapse_fps, fps, fps.label());
                    }
                });
        });

        ui.separator();
        ui.checkbox(
            &mut self.export.timelapse_deflicker.enabled,
            t(Key::TimelapseDeflickerTitle),
        )
        .on_hover_text(t(Key::TimelapseDeflickerTooltip));

        if self.export.timelapse_deflicker.enabled {
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseWindowSize));
                let mut rad = self.export.timelapse_deflicker.window_radius as u32;
                if ui
                    .add(
                        egui::Slider::new(&mut rad, 1..=15).suffix(if lang == Language::Fr {
                            " images"
                        } else {
                            " frames"
                        }),
                    )
                    .changed()
                {
                    self.export.timelapse_deflicker.window_radius = rad as usize;
                }
            });
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseIntensity));
                let mut pct = (self.export.timelapse_deflicker.strength * 100.0).round();
                if ui
                    .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%"))
                    .changed()
                {
                    self.export.timelapse_deflicker.strength = (pct / 100.0).clamp(0.0, 1.0);
                }
            });
        }

        ui.separator();
        ui.checkbox(
            &mut self.export.timelapse_keyframe_ramping,
            t(Key::TimelapseHolyGrailTitle),
        )
        .on_hover_text(t(Key::TimelapseHolyGrailTooltip));

        if self.export.timelapse_keyframe_ramping && frame_count >= 2 {
            let first_path = tl_paths[0].clone();
            let last_path = tl_paths[frame_count - 1].clone();

            if let Some(loaded) = &self.session.loaded_photo {
                self.library
                    .photo_gradings
                    .insert(loaded.path.clone(), self.session.grading.clone());
            }
            if !self.library.photo_gradings.contains_key(&first_path) {
                let g = load_photo_sidecars(&first_path).0.unwrap_or_default();
                self.library.photo_gradings.insert(first_path.clone(), g);
            }
            if !self.library.photo_gradings.contains_key(&last_path) {
                let g = load_photo_sidecars(&last_path).0.unwrap_or_default();
                self.library.photo_gradings.insert(last_path.clone(), g);
            }

            let first_name = first_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Image 1")
                .to_string();
            let last_name = last_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Image N")
                .to_string();

            let mut open_preview_target: Option<PathBuf> = None;

            ui.add_space(4.0);
            // Keyframe 1 (Start)
            let mut g_first = self
                .library
                .photo_gradings
                .get(&first_path)
                .cloned()
                .unwrap_or_default();
            let mut first_changed = false;
            let mut first_commit_disk = false;
            ui.horizontal(|ui| {
                let start_title = format!("{} (#1 : {first_name})", t(Key::TimelapseStartLabel));
                ui.label(
                    egui::RichText::new(start_title)
                        .small()
                        .strong()
                        .color(Color32::from_rgb(255, 215, 110)),
                );
                if ui
                    .small_button(t(Key::TimelapseViewButton))
                    .on_hover_text(t(Key::TimelapseViewStartTooltip))
                    .clicked()
                {
                    open_preview_target = Some(first_path.clone());
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t(Key::TimelapseExpShort)).small());
                let r_exp = ui.add(
                    egui::Slider::new(&mut g_first.tone.exposure_ev, -4.0..=4.0)
                        .step_by(0.05)
                        .suffix(" EV"),
                );
                if r_exp.changed() {
                    first_changed = true;
                }
                if r_exp.drag_stopped() || r_exp.lost_focus() {
                    first_commit_disk = true;
                }
                ui.label(egui::RichText::new(t(Key::TimelapseTempShort)).small());
                let r_tmp = ui.add(
                    egui::Slider::new(&mut g_first.color.kelvin, 2000.0..=12000.0)
                        .step_by(50.0)
                        .suffix(" K"),
                );
                if r_tmp.changed() {
                    first_changed = true;
                }
                if r_tmp.drag_stopped() || r_tmp.lost_focus() {
                    first_commit_disk = true;
                }
            });
            if first_changed {
                self.library
                    .photo_gradings
                    .insert(first_path.clone(), g_first.clone());
                if self
                    .session
                    .loaded_photo
                    .as_ref()
                    .is_some_and(|p| p.path == first_path)
                {
                    self.session.grading = g_first.clone();
                    self.session.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            if first_commit_disk {
                let r = self
                    .library
                    .photo_ratings
                    .get(&first_path)
                    .copied()
                    .unwrap_or(0);
                save_photo_sidecars(&first_path, &g_first, r);
            }

            ui.add_space(2.0);
            // Keyframe N (End)
            let mut g_last = self
                .library
                .photo_gradings
                .get(&last_path)
                .cloned()
                .unwrap_or_default();
            let mut last_changed = false;
            let mut last_commit_disk = false;
            ui.horizontal(|ui| {
                let end_title = format!(
                    "{} (#{frame_count} : {last_name})",
                    t(Key::TimelapseEndLabel)
                );
                ui.label(
                    egui::RichText::new(end_title)
                        .small()
                        .strong()
                        .color(Color32::from_rgb(155, 195, 255)),
                );
                if ui
                    .small_button(t(Key::TimelapseViewButton))
                    .on_hover_text(t(Key::TimelapseViewEndTooltip))
                    .clicked()
                {
                    open_preview_target = Some(last_path.clone());
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t(Key::TimelapseExpShort)).small());
                let r_exp = ui.add(
                    egui::Slider::new(&mut g_last.tone.exposure_ev, -4.0..=4.0)
                        .step_by(0.05)
                        .suffix(" EV"),
                );
                if r_exp.changed() {
                    last_changed = true;
                }
                if r_exp.drag_stopped() || r_exp.lost_focus() {
                    last_commit_disk = true;
                }
                ui.label(egui::RichText::new(t(Key::TimelapseTempShort)).small());
                let r_tmp = ui.add(
                    egui::Slider::new(&mut g_last.color.kelvin, 2000.0..=12000.0)
                        .step_by(50.0)
                        .suffix(" K"),
                );
                if r_tmp.changed() {
                    last_changed = true;
                }
                if r_tmp.drag_stopped() || r_tmp.lost_focus() {
                    last_commit_disk = true;
                }
            });
            if last_changed {
                self.library
                    .photo_gradings
                    .insert(last_path.clone(), g_last.clone());
                if self
                    .session
                    .loaded_photo
                    .as_ref()
                    .is_some_and(|p| p.path == last_path)
                {
                    self.session.grading = g_last.clone();
                    self.session.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            if last_commit_disk {
                let r = self
                    .library
                    .photo_ratings
                    .get(&last_path)
                    .copied()
                    .unwrap_or(0);
                save_photo_sidecars(&last_path, &g_last, r);
            }

            if let Some(target) = open_preview_target {
                self.open_raw_file(&target);
            }
        }
    }

    /// Renders the export configuration dialog window.
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.export.show_export_dialog {
            return;
        }

        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let mut is_open = self.export.show_export_dialog;
        let mut trigger_export = false;
        let mut close_dialog = false;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close_dialog = true;
        }

        egui::Window::new(t(Key::ExportWindowTitle))
            .id(egui::Id::NULL.with("export_timelapse_modal_window"))
            .open(&mut is_open)
            .movable(true)
            .collapsible(true)
            .resizable(false)
            .default_width(470.0)
            .default_pos(egui::pos2(780.0, 52.0))
            .show(ctx, |ui| {
                ui.heading(t(Key::ExportTargetHeading));
                let sel_count = self
                    .library
                    .selected_photos
                    .iter()
                    .filter(|p| self.matches_active_filter(p))
                    .count();

                ui.horizontal_wrapped(|ui| {
                    if ui
                        .radio_value(
                            &mut self.export.export_target_mode,
                            ExportTargetMode::CurrentPhoto,
                            t(Key::ExportTargetCurrent),
                        )
                        .clicked()
                    {
                        self.export.export_batch_selection = false;
                    }
                    let batch_word = t(Key::ExportTargetBatch);
                    let s_suffix = if sel_count > 1 { "s" } else { "" };
                    let batch_label = format!("{batch_word} ({sel_count} photo{s_suffix})");
                    if ui
                        .radio_value(
                            &mut self.export.export_target_mode,
                            ExportTargetMode::BatchSelection,
                            batch_label,
                        )
                        .clicked()
                    {
                        self.export.export_batch_selection = true;
                    }
                    if ui
                        .radio_value(
                            &mut self.export.export_target_mode,
                            ExportTargetMode::TimelapseSequence,
                            t(Key::ExportTargetTimelapse),
                        )
                        .clicked()
                    {
                        self.export.export_batch_selection = true;
                        if self.export.export_subfolder == "Export" {
                            self.export.export_subfolder = "Timelapse".to_string();
                        }
                    }
                });

                match self.export.export_target_mode {
                    ExportTargetMode::CurrentPhoto => {
                        if let Some(photo) = &self.session.loaded_photo {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(t(Key::ExportActivePhoto)).strong());
                                if let Some(name) = photo.path.file_name().and_then(|n| n.to_str())
                                {
                                    ui.label(
                                        egui::RichText::new(name)
                                            .color(Color32::from_rgb(100, 180, 255)),
                                    );
                                }
                            });
                            let crop_text = if self.session.grading.geometry.crop_enabled {
                                t(Key::ExportCropApplied)
                            } else {
                                ""
                            };
                            let sensor_title = t(Key::ExportSensorResolution);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{sensor_title} {} × {} px{crop_text}",
                                    photo.dimensions.0, photo.dimensions.1
                                ))
                                .small()
                                .color(Color32::LIGHT_GRAY),
                            );
                        }
                    }
                    ExportTargetMode::BatchSelection => {
                        if sel_count == 0 {
                            ui.label(
                                egui::RichText::new(t(Key::ExportNoBatchWarning))
                                    .small()
                                    .color(Color32::from_rgb(255, 180, 50)),
                            );
                        }
                    }
                    ExportTargetMode::TimelapseSequence => {
                        ui.add_space(4.0);
                        egui::Frame::group(ui.style())
                            .fill(Color32::from_rgb(24, 28, 36))
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                self.render_timelapse_controls_ui(ui, "dialog");
                            });
                    }
                }

                ui.separator();
                ui.heading(t(Key::ExportFormatHeading));
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Jpeg,
                        "JPEG",
                    );
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Png,
                        "PNG",
                    );
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Tiff,
                        "TIFF",
                    );
                    ui.selectable_value(
                        &mut self.export.export_format,
                        ExportFormatChoice::Avif,
                        "AVIF",
                    );
                });

                ui.add_space(4.0);
                match self.export.export_format {
                    ExportFormatChoice::Jpeg => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportJpegQuality));
                            ui.add(
                                egui::Slider::new(&mut self.export.export_jpeg_quality, 1..=100)
                                    .suffix("%"),
                            );
                        });
                        let quality_desc = if self.export.export_jpeg_quality >= 92 {
                            t(Key::ExportJpegQualityMax)
                        } else if self.export.export_jpeg_quality >= 80 {
                            t(Key::ExportJpegQualityBalanced)
                        } else {
                            t(Key::ExportJpegQualityLow)
                        };
                        ui.label(
                            egui::RichText::new(quality_desc)
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    ExportFormatChoice::Avif => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportJpegQuality));
                            ui.add(
                                egui::Slider::new(&mut self.export.export_jpeg_quality, 1..=100)
                                    .suffix("%"),
                            );
                        });
                        let quality_desc = if self.export.export_jpeg_quality >= 92 {
                            t(Key::ExportJpegQualityMax)
                        } else if self.export.export_jpeg_quality >= 80 {
                            t(Key::ExportJpegQualityBalanced)
                        } else {
                            t(Key::ExportJpegQualityLow)
                        };
                        ui.label(
                            egui::RichText::new(quality_desc)
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    ExportFormatChoice::Png => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportBitDepth));
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits8,
                                t(Key::ExportBitDepth8Standard),
                            );
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits16,
                                t(Key::ExportBitDepth16Lossless),
                            );
                        });
                    }
                    ExportFormatChoice::Tiff => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportBitDepth));
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits8,
                                t(Key::ExportBitDepth8),
                            );
                            ui.radio_value(
                                &mut self.export.export_bit_depth,
                                ExportBitDepth::Bits16,
                                t(Key::ExportBitDepth16Studio),
                            );
                        });
                    }
                }

                ui.separator();
                ui.heading(t(Key::ExportResizeHeading));
                egui::ComboBox::from_id_salt("export_scale_combo")
                    .selected_text(self.export.export_scale_mode.label_lang(lang))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::Original,
                            ExportScaleMode::Original.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::Percent75,
                            ExportScaleMode::Percent75.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::Percent50,
                            ExportScaleMode::Percent50.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::FitLongEdge2048,
                            ExportScaleMode::FitLongEdge2048.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export.export_scale_mode,
                            ExportScaleMode::FitLongEdge3840,
                            ExportScaleMode::FitLongEdge3840.label_lang(lang),
                        );
                    });

                ui.separator();
                ui.heading(t(Key::ExportDestinationHeading));
                ui.horizontal(|ui| {
                    let dest_label = if let Some(dir) = &self.export.export_custom_dir {
                        dir.to_string_lossy().to_string()
                    } else if let Some(photo) = &self.session.loaded_photo {
                        photo
                            .path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| t(Key::ExportSourceFolder).to_string())
                    } else {
                        t(Key::ExportSourceFolder).to_string()
                    };
                    ui.label(egui::RichText::new(dest_label).small().monospace());
                    if ui.button(t(Key::SidebarBrowseButton)).clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.export.export_custom_dir = Some(folder);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut self.export.export_use_subfolder,
                        t(Key::ExportUseSubfolder),
                    );
                    if self.export.export_use_subfolder {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export.export_subfolder)
                                .desired_width(120.0),
                        );
                    }
                });

                if self.export.export_target_mode == ExportTargetMode::TimelapseSequence {
                    ui.horizontal(|ui| {
                        ui.label(t(Key::ExportSequencePrefix));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export.timelapse_sequence_prefix)
                                .desired_width(120.0),
                        );
                        let ext = self.export.export_format.extension();
                        let sample = rraw_export::format_sequence_filename(
                            &self.export.timelapse_sequence_prefix,
                            1,
                            ext,
                        );
                        ui.label(
                            egui::RichText::new(format!("→ {sample}"))
                                .small()
                                .monospace(),
                        );
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(t(Key::ExportFileSuffix));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export.export_suffix)
                                .desired_width(120.0),
                        );
                    });
                }

                if self.export.is_exporting {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new(&self.export.export_progress_label).strong());
                    });
                    ui.add(egui::ProgressBar::new(self.export.export_progress).animate(true));
                }

                if let Some((msg, success)) = &self.export.export_status {
                    ui.add_space(6.0);
                    let col = if *success {
                        Color32::from_rgb(100, 220, 120)
                    } else {
                        Color32::from_rgb(255, 100, 100)
                    };
                    ui.label(egui::RichText::new(msg).color(col));
                }

                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.export.is_exporting,
                            egui::Button::new(t(Key::ExportCancel)),
                        )
                        .clicked()
                    {
                        close_dialog = true;
                    }
                    let export_btn_text = match self.export.export_target_mode {
                        ExportTargetMode::CurrentPhoto => t(Key::ExportBtnSingle),
                        ExportTargetMode::BatchSelection => t(Key::ExportBtnBatch),
                        ExportTargetMode::TimelapseSequence => t(Key::ExportBtnTimelapse),
                    };
                    if ui
                        .add_enabled(
                            !self.export.is_exporting,
                            egui::Button::new(egui::RichText::new(export_btn_text).strong())
                                .fill(Color32::from_rgb(30, 110, 210)),
                        )
                        .clicked()
                    {
                        trigger_export = true;
                    }
                });
            });

        if close_dialog {
            is_open = false;
        }
        self.export.show_export_dialog = is_open;

        if trigger_export {
            self.export.is_exporting = true;
            self.export.export_progress = 0.05;
            self.export.export_progress_label = if lang == Language::Fr {
                "Préparation de l'export...".to_string()
            } else {
                "Preparing export...".to_string()
            };
            self.export.export_status = None;
            // Ferme immédiatement la fenêtre d'export pour laisser place au travail
            // L'avancement et le statut sont suivis en direct dans la barre du bas
            self.export.show_export_dialog = false;

            if let Err(err) = self.spawn_background_export() {
                self.export.is_exporting = false;
                self.export.export_status =
                    Some((format!("{}{err}", t(Key::ExportFailedPrefix)), false));
                self.status_message = format!("❌ Erreur d'export: {err}");
            }
        }
    }
}
