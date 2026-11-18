#![forbid(unsafe_code)]

//! Directory explorer navigation tab in the left panel.

use egui::Color32;
use std::path::PathBuf;

use crate::app::RrawApp;

impl RrawApp {
    /// Renders the folder explorer navigation in the left panel with breadcrumbs and `..` parent navigation.
    pub fn render_explorer_tab(&mut self, ui: &mut egui::Ui) {
        let dir_opt = self.current_directory.clone();
        let mut dir_to_open = None;

        ui.vertical(|ui| {
            if let Some(dir) = dir_opt {
                // Action row: Browse button & photo count
                ui.horizontal(|ui| {
                    if ui
                        .button("📂 Parcourir...")
                        .on_hover_text("Choisir un dossier avec le sélecteur natif macOS")
                        .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            dir_to_open = Some(folder);
                        }
                    }
                    let count = self.directory_entries.len();
                    ui.label(
                        egui::RichText::new(format!(
                            "{count} photo{}",
                            if count > 1 { "s" } else { "" }
                        ))
                        .small()
                        .color(Color32::from_rgb(100, 180, 255)),
                    );
                });

                ui.add_space(4.0);

                // Interactive Breadcrumb path navigation
                ui.horizontal_wrapped(|ui| {
                    let mut accum = PathBuf::new();
                    let components: Vec<_> = dir.components().collect();
                    for (i, comp) in components.iter().enumerate() {
                        accum.push(comp);
                        let name = comp.as_os_str().to_str().unwrap_or("/");
                        let display_name = if i == 0 && name == "/" { "🏠" } else { name };
                        let target = accum.clone();
                        if ui
                            .small_button(display_name)
                            .on_hover_text(target.display().to_string())
                            .clicked()
                        {
                            dir_to_open = Some(target);
                        }
                        if i + 1 < components.len() {
                            ui.label(egui::RichText::new("/").color(Color32::DARK_GRAY));
                        }
                    }
                });

                ui.add_space(6.0);
                ui.separator();

                // "📁 .." parent directory navigation entry at the top of the list
                if let Some(parent) = dir.parent() {
                    if !parent.as_os_str().is_empty() {
                        let btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new("📁 .. (Dossier parent)")
                                    .strong()
                                    .color(Color32::from_rgb(140, 190, 255)),
                            )
                            .frame(false),
                        );
                        if btn
                            .on_hover_text(format!("Remonter vers {}", parent.display()))
                            .clicked()
                        {
                            dir_to_open = Some(parent.to_path_buf());
                        }
                    }
                }

                // Subdirectories list
                let subdirs: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                    .map(|e| e.path())
                    .collect();

                if !subdirs.is_empty() {
                    ui.add_space(2.0);
                    for subdir in subdirs {
                        let name = subdir
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("folder");
                        let btn = ui.add(egui::Button::new(format!("📁 {name}")).frame(false));
                        if btn.clicked() {
                            dir_to_open = Some(subdir);
                        }
                    }
                }
            } else {
                ui.label("Aucun dossier ouvert.");
                if ui.button("📂 Parcourir un dossier...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        dir_to_open = Some(folder);
                    }
                }
            }
        });

        if let Some(d) = dir_to_open {
            self.open_directory(&d);
            if let Some(first) = self.directory_entries.first().cloned() {
                self.open_raw_file(&first);
            }
        }
    }
}
