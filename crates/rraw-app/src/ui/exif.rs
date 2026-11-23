#![forbid(unsafe_code)]

//! EXIF metadata panel and summary inspection cards.

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::state::*;

impl RrawApp {
    /// Renders the complete, clear EXIF table in the left panel.
    pub fn render_exif_panel(&mut self, ui: &mut egui::Ui) {
        let Some(photo) = self.session.loaded_photo.clone() else {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(self.t(Key::SidebarNoPhotoLoaded))
                    .small()
                    .italics()
                    .color(Color32::GRAY),
            );
            return;
        };

        let avail_w = ui.available_width();
        let max_val_w = (avail_w - 85.0).max(120.0);

        ui.vertical(|ui| {
            // Summary Table in clean striped grid
            egui::Grid::new("exif_left_panel_grid")
                .striped(true)
                .min_col_width(80.0)
                .max_col_width(max_val_w)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(self.t(Key::ExifCamera))
                            .strong()
                            .size(12.0),
                    );
                    ui.add(
                        egui::Label::new(egui::RichText::new(&photo.camera_model).size(12.0))
                            .wrap(),
                    )
                    .on_hover_text(&photo.camera_model);
                    ui.end_row();

                    ui.label(
                        egui::RichText::new(self.t(Key::ExifMake))
                            .strong()
                            .size(12.0),
                    );
                    ui.label(egui::RichText::new(&photo.camera_make).size(12.0));
                    ui.end_row();

                    if !photo.lens_model.is_empty() {
                        ui.label(
                            egui::RichText::new(self.t(Key::ExifLens))
                                .strong()
                                .size(12.0),
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(&photo.lens_model).size(12.0))
                                .wrap(),
                        )
                        .on_hover_text(&photo.lens_model);
                        ui.end_row();
                    }

                    ui.label(
                        egui::RichText::new(self.t(Key::ExifDimensions))
                            .strong()
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} × {} ({:.1} MP)",
                            photo.dimensions.0,
                            photo.dimensions.1,
                            (photo.dimensions.0 as f32 * photo.dimensions.1 as f32) / 1_000_000.0
                        ))
                        .size(12.0),
                    );
                    ui.end_row();

                    ui.label(
                        egui::RichText::new(self.t(Key::ExifShutter))
                            .strong()
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(&photo.shutter_str)
                            .size(12.0)
                            .color(Color32::from_rgb(100, 200, 255)),
                    );
                    ui.end_row();

                    ui.label(
                        egui::RichText::new(self.t(Key::ExifAperture))
                            .strong()
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(&photo.aperture_str)
                            .size(12.0)
                            .color(Color32::from_rgb(255, 180, 100)),
                    );
                    ui.end_row();

                    ui.label(
                        egui::RichText::new(self.t(Key::ExifIso))
                            .strong()
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(format!("ISO {}", photo.iso))
                            .size(12.0)
                            .color(Color32::from_rgb(120, 225, 140)),
                    );
                    ui.end_row();

                    if !photo.focal_length_str.is_empty() {
                        ui.label(
                            egui::RichText::new(self.t(Key::ExifFocalLength))
                                .strong()
                                .size(12.0),
                        );
                        ui.label(egui::RichText::new(&photo.focal_length_str).size(12.0));
                        ui.end_row();
                    }

                    if !photo.exposure_bias.is_empty() {
                        ui.label(
                            egui::RichText::new("Biais d'exposition :")
                                .strong()
                                .size(12.0),
                        );
                        ui.label(egui::RichText::new(&photo.exposure_bias).size(12.0));
                        ui.end_row();
                    }

                    if !photo.date_time.is_empty() {
                        ui.label(
                            egui::RichText::new(self.t(Key::ExifDate))
                                .strong()
                                .size(12.0),
                        );
                        ui.label(egui::RichText::new(&photo.date_time).size(12.0));
                        ui.end_row();
                    }
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.small_button(self.t(Key::ExifCopyButton)).clicked() {
                    ui.ctx().copy_text(photo.export_exif_json());
                    self.status_message = "Métadonnées EXIF copiées au presse-papier !".to_string();
                }
                if ui.small_button(self.t(Key::ExifExportButton)).clicked() {
                    let out_path = photo.path.with_extension("exif.json");
                    let json = photo.export_exif_json();
                    if std::fs::write(&out_path, json).is_ok() {
                        self.status_message = format!("EXIF exporté vers {}", out_path.display());
                    }
                }
            });

            // Detailed all EXIF tags list
            if !photo.exif_tags.is_empty() {
                ui.add_space(6.0);
                egui::CollapsingHeader::new(
                    egui::RichText::new(format!(
                        "{} ({})",
                        self.t(Key::ExifAllTagsDetailed),
                        photo.exif_tags.len()
                    ))
                    .strong()
                    .size(12.0)
                    .color(Color32::from_rgb(200, 215, 235)),
                )
                .id_salt("exif_detailed_tags_collapsing_header")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🔎").size(11.0));
                        let filter_hint = self.t(Key::ExifFilterTags);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.library.exif_search_query)
                                .hint_text(filter_hint)
                                .desired_width(140.0),
                        );
                        if !self.library.exif_search_query.is_empty()
                            && ui.small_button("✖").clicked()
                        {
                            self.library.exif_search_query.clear();
                        }
                    });

                    ui.add_space(4.0);
                    let query = self.library.exif_search_query.to_lowercase();
                    let lang = self.layout_config.language;
                    let filtered_tags: Vec<&(String, String)> = photo
                        .exif_tags
                        .iter()
                        .filter(|(tag, val)| {
                            let trans_tag = crate::i18n::translate_exif_tag(tag, lang);
                            let trans_val = crate::i18n::translate_exif_val(val, lang);

                            if query.is_empty() {
                                const SUMMARY_TAGS: &[&str] = &[
                                    "Camera Make",
                                    "Camera Model",
                                    "Lens Model",
                                    "Lens Make",
                                    "ISO Sensitivity",
                                    "Aperture",
                                    "Exposure Time",
                                    "Shutter Speed",
                                    "Focal Length",
                                    "Exposure Bias",
                                    "Date/Time Original",
                                ];
                                !SUMMARY_TAGS.contains(&tag.as_str())
                            } else {
                                tag.to_lowercase().contains(&query)
                                    || trans_tag.to_lowercase().contains(&query)
                                    || val.to_lowercase().contains(&query)
                                    || trans_val.to_lowercase().contains(&query)
                            }
                        })
                        .collect();

                    if filtered_tags.is_empty() {
                        ui.label(
                            egui::RichText::new("Aucun tag correspondant.")
                                .small()
                                .italics()
                                .color(Color32::GRAY),
                        );
                    } else {
                        egui::Grid::new("exif_all_tags_grid")
                            .striped(true)
                            .min_col_width(80.0)
                            .max_col_width(max_val_w)
                            .show(ui, |ui| {
                                for (tag, val) in filtered_tags {
                                    let trans_tag = crate::i18n::translate_exif_tag(tag, lang);
                                    let trans_val = crate::i18n::translate_exif_val(val, lang);
                                    ui.label(egui::RichText::new(trans_tag).strong().size(11.0));
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(trans_val).size(11.0))
                                            .truncate(),
                                    )
                                    .on_hover_text(trans_val);
                                    ui.end_row();
                                }
                            });
                    }
                });
            }
        });
    }

    /// Renders an at-a-glance EXIF summary card for backwards compatibility.
    pub fn render_exif_compact_card(&mut self, ui: &mut egui::Ui) {
        self.render_exif_panel(ui);
    }

    /// Renders a concise EXIF card in the reorderable inspector sections.
    pub fn render_exif_card(&mut self, ui: &mut egui::Ui) {
        if let Some(photo) = &self.session.loaded_photo {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&photo.camera_model).strong());
                if !photo.lens_model.is_empty() {
                    ui.label(
                        egui::RichText::new(&photo.lens_model)
                            .small()
                            .color(Color32::LIGHT_GRAY),
                    );
                }
                let fl_str = if !photo.focal_length_str.is_empty() {
                    format!(" | {}", photo.focal_length_str)
                } else {
                    String::new()
                };
                ui.label(format!(
                    "{} | {} | ISO {}{fl_str}",
                    photo.shutter_str, photo.aperture_str, photo.iso
                ));
                if !photo.exposure_bias.is_empty() {
                    ui.label(format!("Biais : {}", photo.exposure_bias));
                }
                if !photo.date_time.is_empty() {
                    ui.label(
                        egui::RichText::new(&photo.date_time)
                            .small()
                            .color(Color32::GRAY),
                    );
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Onglet EXIF complet 📋").clicked() {
                        self.left_panel_tab = LeftPanelTab::Exif;
                        self.layout_config.left_panel_tab = LeftPanelTab::Exif;
                        self.layout_config.save();
                    }
                    if ui.button("Copier JSON").clicked() {
                        ui.ctx().copy_text(photo.export_exif_json());
                        self.status_message =
                            "Métadonnées EXIF copiées dans le presse-papier !".to_string();
                    }
                });
            });
        } else {
            ui.label(
                egui::RichText::new("Aucune photo chargée.")
                    .italics()
                    .color(Color32::GRAY),
            );
        }
    }

    /// Renders the complete, detailed EXIF and metadata inspector with export controls.
    pub fn render_exif_full_panel(&mut self, ui: &mut egui::Ui) {
        let Some(photo) = self.session.loaded_photo.clone() else {
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new("Aucune photo RAW chargée.")
                    .italics()
                    .color(Color32::GRAY),
            );
            return;
        };

        ui.vertical(|ui| {
            ui.heading("Métadonnées EXIF & Prise de vue");
            ui.label(
                egui::RichText::new(photo.path.display().to_string())
                    .small()
                    .color(Color32::GRAY),
            );
            ui.add_space(6.0);

            // Export Actions
            ui.horizontal(|ui| {
                if ui
                    .button("📥 Exporter EXIF (.json)")
                    .on_hover_text(
                        "Enregistrer l'intégralité des métadonnées EXIF dans un fichier .exif.json",
                    )
                    .clicked()
                {
                    let out_path = photo.path.with_extension("exif.json");
                    let json = photo.export_exif_json();
                    if std::fs::write(&out_path, json).is_ok() {
                        self.status_message = format!("EXIF exporté vers {}", out_path.display());
                    } else {
                        self.status_message = "Échec d'écriture du fichier EXIF.".to_string();
                    }
                }

                if ui
                    .button("📋 Copier au presse-papier")
                    .on_hover_text(
                        "Copier les métadonnées EXIF au format JSON dans le presse-papier système",
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(photo.export_exif_json());
                    self.status_message = "JSON EXIF copié dans le presse-papier !".to_string();
                }
            });

            ui.separator();

            // Summary Table
            let win_max_val_w = (ui.available_width() - 110.0).max(180.0);
            egui::Grid::new("exif_summary_grid")
                .striped(true)
                .min_col_width(100.0)
                .max_col_width(win_max_val_w)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Marque :").strong().size(13.0));
                    ui.label(egui::RichText::new(&photo.camera_make).size(13.0));
                    ui.end_row();

                    ui.label(egui::RichText::new("Modèle :").strong().size(13.0));
                    ui.add(
                        egui::Label::new(egui::RichText::new(&photo.camera_model).size(13.0))
                            .wrap(),
                    )
                    .on_hover_text(&photo.camera_model);
                    ui.end_row();

                    if !photo.lens_model.is_empty() {
                        ui.label(egui::RichText::new("Objectif :").strong().size(13.0));
                        ui.add(
                            egui::Label::new(egui::RichText::new(&photo.lens_model).size(13.0))
                                .wrap(),
                        )
                        .on_hover_text(&photo.lens_model);
                        ui.end_row();
                    }

                    ui.label(egui::RichText::new("Dimensions :").strong().size(13.0));
                    ui.label(
                        egui::RichText::new(format!(
                            "{} × {} ({:.1} MP)",
                            photo.dimensions.0,
                            photo.dimensions.1,
                            (photo.dimensions.0 as f32 * photo.dimensions.1 as f32) / 1_000_000.0
                        ))
                        .size(13.0),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Vitesse :").strong().size(13.0));
                    ui.label(egui::RichText::new(&photo.shutter_str).size(13.0));
                    ui.end_row();

                    ui.label(egui::RichText::new("Ouverture :").strong().size(13.0));
                    ui.label(egui::RichText::new(&photo.aperture_str).size(13.0));
                    ui.end_row();

                    ui.label(egui::RichText::new("Sensibilité ISO :").strong().size(13.0));
                    ui.label(egui::RichText::new(photo.iso.to_string()).size(13.0));
                    ui.end_row();

                    if !photo.focal_length_str.is_empty() {
                        ui.label(egui::RichText::new("Focale :").strong().size(13.0));
                        ui.label(egui::RichText::new(&photo.focal_length_str).size(13.0));
                        ui.end_row();
                    }

                    if !photo.exposure_bias.is_empty() {
                        ui.label(
                            egui::RichText::new("Biais d'exposition :")
                                .strong()
                                .size(13.0),
                        );
                        ui.label(egui::RichText::new(&photo.exposure_bias).size(13.0));
                        ui.end_row();
                    }

                    if !photo.date_time.is_empty() {
                        ui.label(egui::RichText::new("Date de capture :").strong().size(13.0));
                        ui.label(egui::RichText::new(&photo.date_time).size(13.0));
                        ui.end_row();
                    }
                });

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Tous les tags EXIF décodés :")
                    .strong()
                    .size(14.0),
            );
            ui.separator();

            let tag_val_w = (ui.available_width() - 140.0).max(200.0);
            let lang = self.layout_config.language;
            egui::Grid::new("exif_all_tags_window_grid")
                .striped(true)
                .min_col_width(120.0)
                .max_col_width(tag_val_w)
                .show(ui, |ui| {
                    for (tag, val) in &photo.exif_tags {
                        let trans_tag = crate::i18n::translate_exif_tag(tag, lang);
                        let trans_val = crate::i18n::translate_exif_val(val, lang);
                        ui.label(egui::RichText::new(trans_tag).strong().size(13.0));
                        ui.add(
                            egui::Label::new(egui::RichText::new(trans_val).size(13.0)).truncate(),
                        )
                        .on_hover_text(trans_val);
                        ui.end_row();
                    }
                });
        });
    }
}
