#![forbid(unsafe_code)]

//! Image export dialog and file encoding execution.

use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ByteOrder};
use egui::Color32;
use image::ImageEncoder;

use crate::app::RrawApp;
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

    /// Executes photographic export of the active or selected images with all darkroom corrections,
    /// cropping, scaling, and the user-selected format & compression level.
    pub fn execute_export(&mut self) -> Result<PathBuf, String> {
        let photos_to_export: Vec<PathBuf> = if self.export_batch_selection {
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
        } else if let Some(loaded) = &self.loaded_photo {
            vec![loaded.path.clone()]
        } else {
            return Err("Aucune photo chargée à exporter.".to_string());
        };

        let mut last_exported_path = None;
        let mut count = 0;

        for photo_path in &photos_to_export {
            let is_loaded = self.loaded_photo.as_ref().map(|p| &p.path) == Some(photo_path);
            let (base_preview, grading) = if is_loaded {
                let base = match &self.cached_cfa {
                    Some(cfa) => {
                        let target_width = match self.export_scale_mode {
                            ExportScaleMode::Original => cfa.width().saturating_div(2).max(1200),
                            ExportScaleMode::Percent75 => (cfa.width() * 3 / 8).max(900),
                            ExportScaleMode::Percent50 => (cfa.width() / 4).max(600),
                            ExportScaleMode::FitLongEdge2048 => 2048,
                            ExportScaleMode::FitLongEdge3840 => 3840,
                        };
                        let as_shot_wb = self
                            .base_preview
                            .as_ref()
                            .map(|b| b.as_shot_wb)
                            .unwrap_or([2.0, 1.0, 1.5]);
                        let exif_orient =
                            self.base_preview.as_ref().map_or(1, |b| b.exif_orientation);
                        let mut b = BaseRgbPreview::from_cfa_with_color_and_target_width(
                            cfa,
                            as_shot_wb,
                            None,
                            target_width,
                        );
                        if let Some(prev) = &self.base_preview {
                            b.cam_to_srgb = prev.cam_to_srgb;
                        }
                        b.with_exif_orientation(exif_orient)
                    }
                    None => match &self.base_preview {
                        Some(b) => b.clone(),
                        None => continue,
                    },
                };
                (base, self.grading.clone())
            } else if let Some(cached) = self.photo_cache.iter().find(|e| &e.path == photo_path) {
                let grading = self
                    .photo_gradings
                    .get(photo_path)
                    .cloned()
                    .unwrap_or_else(|| load_photo_sidecars(photo_path).0.unwrap_or_default());
                (cached.base_preview.clone(), grading)
            } else if let Some(payload) = decode_photo_file_payload(photo_path, false) {
                let grading = self
                    .photo_gradings
                    .get(photo_path)
                    .cloned()
                    .unwrap_or_else(|| load_photo_sidecars(photo_path).0.unwrap_or_default());
                (payload.base_preview, grading)
            } else {
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

    /// Renders the export configuration dialog window.
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_export_dialog {
            return;
        }

        let mut is_open = self.show_export_dialog;
        let mut trigger_export = false;
        let mut close_dialog = false;

        egui::Window::new("📤 Exporter")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading("Cible d'export");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.export_batch_selection, false, "Photo courante");
                    let sel_count = self.selected_photos.len();
                    let batch_label = format!(
                        "Sélection par lot ({} photo{})",
                        sel_count,
                        if sel_count > 1 { "s" } else { "" }
                    );
                    ui.radio_value(&mut self.export_batch_selection, true, batch_label);
                });

                if !self.export_batch_selection {
                    if let Some(photo) = &self.loaded_photo {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Photo active :").strong());
                            if let Some(name) = photo.path.file_name().and_then(|n| n.to_str()) {
                                ui.label(
                                    egui::RichText::new(name).color(Color32::from_rgb(100, 180, 255)),
                                );
                            }
                        });
                        let crop_text = if self.grading.crop_enabled {
                            " (Recadrage actif appliqué)"
                        } else {
                            ""
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "Définition capteur : {} × {} px{crop_text}",
                                photo.dimensions.0, photo.dimensions.1
                            ))
                            .small()
                            .color(Color32::LIGHT_GRAY),
                        );
                    }
                } else if self.selected_photos.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "⚠️ Aucune photo cochée dans les vignettes. Seule la photo active sera exportée.",
                        )
                        .small()
                        .color(Color32::from_rgb(255, 180, 50)),
                    );
                }

                ui.separator();
                ui.heading("Format de sortie & Compression");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Jpeg, "JPEG");
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Png, "PNG");
                    ui.selectable_value(&mut self.export_format, ExportFormatChoice::Tiff, "TIFF");
                });

                ui.add_space(4.0);
                match self.export_format {
                    ExportFormatChoice::Jpeg => {
                        ui.horizontal(|ui| {
                            ui.label("Qualité JPEG :");
                            ui.add(
                                egui::Slider::new(&mut self.export_jpeg_quality, 1..=100)
                                    .suffix("%"),
                            );
                        });
                        let quality_desc = if self.export_jpeg_quality >= 92 {
                            "Qualité maximale (Recommandé pour tirage / archivage)"
                        } else if self.export_jpeg_quality >= 80 {
                            "Qualité équilibrée (Idéal pour le web et les réseaux)"
                        } else {
                            "Forte compression (Taille de fichier minimale)"
                        };
                        ui.label(egui::RichText::new(quality_desc).small().color(Color32::GRAY));
                    }
                    ExportFormatChoice::Png => {
                        ui.horizontal(|ui| {
                            ui.label("Profondeur :");
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits8,
                                "8-bit sRGB (Standard)",
                            );
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits16,
                                "16-bit (Master sans perte)",
                            );
                        });
                    }
                    ExportFormatChoice::Tiff => {
                        ui.horizontal(|ui| {
                            ui.label("Profondeur :");
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits8,
                                "8-bit",
                            );
                            ui.radio_value(
                                &mut self.export_bit_depth,
                                ExportBitDepth::Bits16,
                                "16-bit (Studio)",
                            );
                        });
                    }
                }

                ui.separator();
                ui.heading("Redimensionnement");
                egui::ComboBox::from_id_salt("export_scale_combo")
                    .selected_text(self.export_scale_mode.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Original,
                            ExportScaleMode::Original.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Percent75,
                            ExportScaleMode::Percent75.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::Percent50,
                            ExportScaleMode::Percent50.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::FitLongEdge2048,
                            ExportScaleMode::FitLongEdge2048.label(),
                        );
                        ui.selectable_value(
                            &mut self.export_scale_mode,
                            ExportScaleMode::FitLongEdge3840,
                            ExportScaleMode::FitLongEdge3840.label(),
                        );
                    });

                ui.separator();
                ui.heading("Dossier de destination & Nom");
                ui.horizontal(|ui| {
                    let dest_label = if let Some(dir) = &self.export_custom_dir {
                        dir.to_string_lossy().to_string()
                    } else if let Some(photo) = &self.loaded_photo {
                        photo
                            .path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Dossier source".to_string())
                    } else {
                        "Dossier source".to_string()
                    };
                    ui.label(egui::RichText::new(dest_label).small().monospace());
                    if ui.button("📁 Parcourir...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.export_custom_dir = Some(folder);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.export_use_subfolder, "Créer un sous-dossier :");
                    if self.export_use_subfolder {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export_subfolder)
                                .desired_width(120.0),
                        );
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Suffixe de fichier :");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.export_suffix).desired_width(120.0),
                    );
                });

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
                    if ui.button("Annuler").clicked() {
                        close_dialog = true;
                    }
                    let export_btn_text = if self.export_batch_selection {
                        "🚀 Exporter le lot"
                    } else {
                        "🚀 Exporter l'image"
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(export_btn_text).strong(),
                            )
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
                    let status_txt = if self.export_batch_selection {
                        format!(
                            "✅ Export par lot réussi ({} fichier(s))",
                            self.selected_photos.len().max(1)
                        )
                    } else {
                        format!("✅ Export réussi : {file_name}")
                    };
                    self.status_message = status_txt.clone();
                    self.export_status = Some((status_txt, true));
                    // Automatically close dialog on success as requested by user
                    self.show_export_dialog = false;
                }
                Err(err) => {
                    self.export_status = Some((format!("❌ Échec : {err}"), false));
                }
            }
        }
    }
}
