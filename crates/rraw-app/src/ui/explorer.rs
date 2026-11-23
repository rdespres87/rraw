#![forbid(unsafe_code)]

//! Directory explorer navigation tab in the left panel.

use egui::Color32;
use std::path::PathBuf;

use crate::app::RrawApp;
use crate::i18n::Key;

impl RrawApp {
    /// Renders the folder explorer navigation in the left panel with breadcrumbs and `..` parent navigation.
    pub fn render_explorer_tab(&mut self, ui: &mut egui::Ui) {
        let dir_opt = self.library.current_directory.clone();
        let mut dir_to_open = None;

        ui.vertical(|ui| {
            if let Some(dir) = dir_opt {
                // Action row: Browse button & photo count
                ui.horizontal(|ui| {
                    if ui
                        .button(self.t(Key::SidebarBrowseButton))
                        .on_hover_text(self.t(Key::SidebarBrowseTooltip))
                        .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            dir_to_open = Some(folder);
                        }
                    }
                    let count = self.library.directory_entries.len();
                    let unit = if count > 1 {
                        self.t(Key::SidebarPhotosCountPlural)
                    } else {
                        self.t(Key::SidebarPhotosCount)
                    };
                    ui.label(
                        egui::RichText::new(format!("{count} {unit}"))
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

                // Subdirectories list (served from sorted cache without per-frame read_dir syscalls)
                if !self.library.cached_subdirectories.is_empty() {
                    ui.add_space(2.0);
                    for subdir in &self.library.cached_subdirectories {
                        let name = subdir
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("folder");
                        let btn = ui.add(egui::Button::new(format!("📁 {name}")).frame(false));
                        if btn.clicked() {
                            dir_to_open = Some(subdir.clone());
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
            if let Some(first) = self
                .first_filtered_entry()
                .or_else(|| self.library.directory_entries.first().cloned())
            {
                self.open_raw_file(&first);
            }
        }
    }
}
