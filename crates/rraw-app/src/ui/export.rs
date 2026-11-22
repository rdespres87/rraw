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

impl RrawApp {
    fn write_exported_image(
        &self,
        target_path: &Path,
        out_w: usize,
        out_h: usize,
        u8_bytes: &[u8],
        u16_samples: &[u16],
    ) -> Result<(), String> {
        match self.export_format {
            ExportFormatChoice::Jpeg => {
                let file = std::fs::File::create(target_path)
                    .map_err(|e| format!("Impossible de créer le fichier: {e}"))?;
                let mut writer = std::io::BufWriter::new(file);
                let quality = self.export_jpeg_quality.clamp(1, 100);
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
                match self.export_bit_depth {
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
                match self.export_bit_depth {
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
        }
        Ok(())
    }

    fn compute_export_target_width(&self, cfa_width: usize, cfa_height: usize) -> usize {
        match self.export_scale_mode {
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

    fn build_export_base_from_cfa(
        &self,
        cfa: &rraw_core::SignedCfaBuffer,
        ref_preview: &BaseRgbPreview,
    ) -> BaseRgbPreview {
        let target_width = self.compute_export_target_width(cfa.width(), cfa.height());
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

    fn resolve_photo_preview_and_grading(
        &self,
        photo_path: &Path,
    ) -> Option<(BaseRgbPreview, DarkroomGradingState)> {
        let is_loaded = self.loaded_photo.as_ref().map(|p| p.path.as_path()) == Some(photo_path);
        if is_loaded {
            let base = match (&self.cached_cfa, &self.base_preview) {
                (Some(cfa), Some(prev)) => self.build_export_base_from_cfa(cfa, prev),
                (_, Some(prev)) => prev.clone(),
                _ => return None,
            };
            Some((base, self.grading.clone()))
        } else if let Some(cached) = self.photo_cache.iter().find(|e| e.path == photo_path) {
            let grading = self
                .photo_gradings
                .get(photo_path)
                .cloned()
                .unwrap_or_else(|| load_photo_sidecars(photo_path).0.unwrap_or_default());
            let base = self.build_export_base_from_cfa(&cached.cfa, &cached.base_preview);
            Some((base, grading))
        } else if let Some(payload) = decode_photo_file_payload(photo_path, false) {
            let grading = self
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
        if self.selected_photos.len() >= 2 {
            let mut sorted: Vec<PathBuf> = self.selected_photos.iter().cloned().collect();
            sorted.sort();
            sorted
        } else {
            let mut filtered: Vec<PathBuf> = self
                .directory_entries
                .iter()
                .filter(|p| {
                    let rating = self.photo_ratings.get(*p).copied().unwrap_or(0);
                    self.rating_filter.matches(rating)
                })
                .cloned()
                .collect();
            filtered.sort();
            filtered
        }
    }

    /// Executes photographic export (Single photo, Batch selection, or Timelapse sequence with
    /// temporal anti-flicker and keyframe ramping).
    pub fn execute_export(&mut self) -> Result<PathBuf, String> {
        // Keep backward compatibility if export_batch_selection was toggled programmatically
        let mode = if self.export_target_mode == ExportTargetMode::CurrentPhoto
            && self.export_batch_selection
        {
            ExportTargetMode::BatchSelection
        } else {
            self.export_target_mode
        };

        if mode == ExportTargetMode::TimelapseSequence {
            return self.execute_timelapse_export();
        }

        let photos_to_export: Vec<PathBuf> = match mode {
            ExportTargetMode::BatchSelection => {
                if self.selected_photos.is_empty() {
                    if let Some(loaded) = &self.loaded_photo {
                        vec![loaded.path.clone()]
                    } else {
                        return Err("Aucune photo sélectionnée pour l'export par lot.".to_string());
                    }
                } else {
                    let mut sorted: Vec<PathBuf> = self.selected_photos.iter().cloned().collect();
                    sorted.sort();
                    sorted
                }
            }
            ExportTargetMode::CurrentPhoto | ExportTargetMode::TimelapseSequence => {
                if let Some(loaded) = &self.loaded_photo {
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

            let base_dest = self.export_custom_dir.clone().unwrap_or_else(|| {
                photo_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
            });

            let final_dest =
                if self.export_use_subfolder && !self.export_subfolder.trim().is_empty() {
                    base_dest.join(self.export_subfolder.trim())
                } else {
                    base_dest
                };

            let _ = std::fs::create_dir_all(&final_dest);

            let stem = photo_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let ext = self.export_format.extension();
            let suffix = &self.export_suffix;
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
        if let Some(loaded) = &self.loaded_photo {
            self.photo_gradings
                .insert(loaded.path.clone(), self.grading.clone());
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
            self.timelapse_keyframe_ramping,
            &self.timelapse_deflicker,
        );

        let first_path = &decoded_frames[0].0;
        let base_dest = self.export_custom_dir.clone().unwrap_or_else(|| {
            first_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });

        let final_dest = if self.export_use_subfolder && !self.export_subfolder.trim().is_empty() {
            base_dest.join(self.export_subfolder.trim())
        } else {
            base_dest
        };

        std::fs::create_dir_all(&final_dest)
            .map_err(|e| format!("Impossible de créer le dossier Timelapse: {e}"))?;

        let ext = self.export_format.extension();
        let mut last_path = None;

        for (idx, ((_, base, _), grading)) in decoded_frames
            .iter()
            .zip(smoothed_gradings.iter())
            .enumerate()
        {
            let (out_w, out_h, u8_bytes, u16_samples) = render_export_image(base, grading, 1.0);
            let seq_filename = rraw_export::format_sequence_filename(
                &self.timelapse_sequence_prefix,
                idx + 1,
                ext,
            );
            let target_path = final_dest.join(seq_filename);
            self.write_exported_image(&target_path, out_w, out_h, &u8_bytes, &u16_samples)?;
            last_path = Some(target_path);
        }

        let manifest_path = rraw_export::write_timelapse_manifest(
            &final_dest,
            &self.timelapse_sequence_prefix,
            ext,
            self.timelapse_fps,
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
        let dur_s = self.timelapse_fps.duration_seconds(frame_count);

        let seq_text = if lang == Language::Fr {
            format!("🎞️ Séquence : {} images", frame_count)
        } else {
            format!("🎞️ Sequence: {} frames", frame_count)
        };
        let dur_text = if lang == Language::Fr {
            format!("Durée : {:.2} s @ {} fps", dur_s, self.timelapse_fps.fps())
        } else {
            format!(
                "Duration: {:.2} s @ {} fps",
                dur_s,
                self.timelapse_fps.fps()
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
                .selected_text(self.timelapse_fps.label())
                .show_ui(ui, |ui| {
                    for fps in rraw_export::TimelapseFrameRate::ALL {
                        ui.selectable_value(&mut self.timelapse_fps, fps, fps.label());
                    }
                });
        });

        ui.separator();
        ui.checkbox(
            &mut self.timelapse_deflicker.enabled,
            t(Key::TimelapseDeflickerTitle),
        )
        .on_hover_text(t(Key::TimelapseDeflickerTooltip));

        if self.timelapse_deflicker.enabled {
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseWindowSize));
                let mut rad = self.timelapse_deflicker.window_radius as u32;
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
                    self.timelapse_deflicker.window_radius = rad as usize;
                }
            });
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseIntensity));
                let mut pct = (self.timelapse_deflicker.strength * 100.0).round();
                if ui
                    .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%"))
                    .changed()
                {
                    self.timelapse_deflicker.strength = (pct / 100.0).clamp(0.0, 1.0);
                }
            });
        }

        ui.separator();
        ui.checkbox(
            &mut self.timelapse_keyframe_ramping,
            t(Key::TimelapseHolyGrailTitle),
        )
        .on_hover_text(t(Key::TimelapseHolyGrailTooltip));

        if self.timelapse_keyframe_ramping && frame_count >= 2 {
            let first_path = tl_paths[0].clone();
            let last_path = tl_paths[frame_count - 1].clone();

            if let Some(loaded) = &self.loaded_photo {
                self.photo_gradings
                    .insert(loaded.path.clone(), self.grading.clone());
            }
            if !self.photo_gradings.contains_key(&first_path) {
                let g = load_photo_sidecars(&first_path).0.unwrap_or_default();
                self.photo_gradings.insert(first_path.clone(), g);
            }
            if !self.photo_gradings.contains_key(&last_path) {
                let g = load_photo_sidecars(&last_path).0.unwrap_or_default();
                self.photo_gradings.insert(last_path.clone(), g);
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
                self.photo_gradings
                    .insert(first_path.clone(), g_first.clone());
                if self
                    .loaded_photo
                    .as_ref()
                    .is_some_and(|p| p.path == first_path)
                {
                    self.grading = g_first.clone();
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            if first_commit_disk {
                let r = self.photo_ratings.get(&first_path).copied().unwrap_or(0);
                save_photo_sidecars(&first_path, &g_first, r);
            }

            ui.add_space(2.0);
            // Keyframe N (End)
            let mut g_last = self
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
                self.photo_gradings
                    .insert(last_path.clone(), g_last.clone());
                if self
                    .loaded_photo
                    .as_ref()
                    .is_some_and(|p| p.path == last_path)
                {
                    self.grading = g_last.clone();
                    self.needs_preview_update = true;
                    ui.ctx().request_repaint();
                }
            }
            if last_commit_disk {
                let r = self.photo_ratings.get(&last_path).copied().unwrap_or(0);
                save_photo_sidecars(&last_path, &g_last, r);
            }

            if let Some(target) = open_preview_target {
                self.open_raw_file(&target);
            }
        }
    }

    /// Renders the export configuration dialog window.
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_export_dialog {
            return;
        }

        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let mut is_open = self.show_export_dialog;
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
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .radio_value(
                            &mut self.export_target_mode,
                            ExportTargetMode::CurrentPhoto,
                            t(Key::ExportTargetCurrent),
                        )
                        .clicked()
                    {
                        self.export_batch_selection = false;
                    }
                    let sel_count = self.selected_photos.len();
                    let batch_word = t(Key::ExportTargetBatch);
                    let s_suffix = if sel_count > 1 { "s" } else { "" };
                    let batch_label = format!("{batch_word} ({sel_count} photo{s_suffix})");
                    if ui
                        .radio_value(
                            &mut self.export_target_mode,
                            ExportTargetMode::BatchSelection,
                            batch_label,
                        )
                        .clicked()
                    {
                        self.export_batch_selection = true;
                    }
                    if ui
                        .radio_value(
                            &mut self.export_target_mode,
                            ExportTargetMode::TimelapseSequence,
                            t(Key::ExportTargetTimelapse),
                        )
                        .clicked()
                    {
                        self.export_batch_selection = true;
                        if self.export_subfolder == "Export" {
                            self.export_subfolder = "Timelapse".to_string();
                        }
                    }
                });

                match self.export_target_mode {
                    ExportTargetMode::CurrentPhoto => {
                        if let Some(photo) = &self.loaded_photo {
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
                            let crop_text = if self.grading.geometry.crop_enabled {
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
                        if self.selected_photos.is_empty() {
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
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Jpeg, "JPEG");
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Png, "PNG");
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Tiff, "TIFF");
                });

                ui.add_space(4.0);
                match self.export_format {
                    ExportFormatChoice::Jpeg => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportJpegQuality));
                            ui.add(
                                egui::Slider::new(&mut self.export_jpeg_quality, 1..=100)
                                    .suffix("%"),
                            );
                        });
                        let quality_desc = if self.export_jpeg_quality >= 92 {
                            t(Key::ExportJpegQualityMax)
                        } else if self.export_jpeg_quality >= 80 {
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
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits8,
                                t(Key::ExportBitDepth8Standard),
                            );
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits16,
                                t(Key::ExportBitDepth16Lossless),
                            );
                        });
                    }
                    ExportFormatChoice::Tiff => {
                        ui.horizontal(|ui| {
                            ui.label(t(Key::ExportBitDepth));
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits8,
                                t(Key::ExportBitDepth8),
                            );
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits16,
                                t(Key::ExportBitDepth16Studio),
                            );
                        });
                    }
                }

                ui.separator();
                ui.heading(t(Key::ExportResizeHeading));
                egui::ComboBox::from_id_salt("export_scale_combo")
                    .selected_text(self.export_scale_mode.label_lang(lang))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Original,
                            ExportScaleMode::Original.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Percent75,
                            ExportScaleMode::Percent75.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Percent50,
                            ExportScaleMode::Percent50.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::FitLongEdge2048,
                            ExportScaleMode::FitLongEdge2048.label_lang(lang),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::FitLongEdge3840,
                            ExportScaleMode::FitLongEdge3840.label_lang(lang),
                        );
                    });

                ui.separator();
                ui.heading(t(Key::ExportDestinationHeading));
                ui.horizontal(|ui| {
                    let dest_label = if let Some(dir) = &self.export_custom_dir {
                        dir.to_string_lossy().to_string()
                    } else if let Some(photo) = &self.loaded_photo {
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
                            self.export_custom_dir = Some(folder);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.export_use_subfolder, t(Key::ExportUseSubfolder));
                    if self.export_use_subfolder {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export_subfolder)
                                .desired_width(120.0),
                        );
                    }
                });

                if self.export_target_mode == ExportTargetMode::TimelapseSequence {
                    ui.horizontal(|ui| {
                        ui.label(t(Key::ExportSequencePrefix));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.timelapse_sequence_prefix)
                                .desired_width(120.0),
                        );
                        let ext = self.export_format.extension();
                        let sample = rraw_export::format_sequence_filename(
                            &self.timelapse_sequence_prefix,
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
                            egui::TextEdit::singleline(&mut self.export_suffix)
                                .desired_width(120.0),
                        );
                    });
                }

                if let Some((msg, success)) = &self.export_status {
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
                    if ui.button(t(Key::ExportCancel)).clicked() {
                        close_dialog = true;
                    }
                    let export_btn_text = match self.export_target_mode {
                        ExportTargetMode::CurrentPhoto => t(Key::ExportBtnSingle),
                        ExportTargetMode::BatchSelection => t(Key::ExportBtnBatch),
                        ExportTargetMode::TimelapseSequence => t(Key::ExportBtnTimelapse),
                    };
                    if ui
                        .add(
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
        self.show_export_dialog = is_open;

        if trigger_export {
            match self.execute_export() {
                Ok(path) => {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
                    let status_txt = match self.export_target_mode {
                        ExportTargetMode::CurrentPhoto => {
                            format!("{}{file_name}", t(Key::ExportSuccessSinglePrefix))
                        }
                        ExportTargetMode::BatchSelection => {
                            let count = self.selected_photos.len().max(1);
                            if lang == Language::Fr {
                                format!("✅ Export par lot réussi ({count} fichier(s))")
                            } else {
                                format!("✅ Batch export successful ({count} file(s))")
                            }
                        }
                        ExportTargetMode::TimelapseSequence => {
                            let frames = self.timelapse_candidate_paths().len();
                            let fps = self.timelapse_fps.fps();
                            if lang == Language::Fr {
                                format!("✅ Séquence Timelapse exportée ({frames} images @ {fps} fps + manifeste JSON)")
                            } else {
                                format!("✅ Timelapse sequence exported ({frames} frames @ {fps} fps + JSON manifest)")
                            }
                        }
                    };
                    self.status_message = status_txt.clone();
                    self.export_status = Some((status_txt, true));
                    self.show_export_dialog = false;
                }
                Err(err) => {
                    self.export_status =
                        Some((format!("{}{err}", t(Key::ExportFailedPrefix)), false));
                }
            }
        }
    }
}
