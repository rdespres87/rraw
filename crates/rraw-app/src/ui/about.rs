#![forbid(unsafe_code)]

//! "About" dialog: displays the compile-time semantic version, git commit
//! hash and build date embedded by `rraw-core::build_info`.

use egui::{Color32, RichText};

use rraw_core::build_info::BuildInfo;

use crate::app::RrawApp;
use crate::i18n::Key;

impl RrawApp {
    /// Renders the modal "About" version window.
    pub fn render_about_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_about_dialog {
            return;
        }

        let mut is_open = self.show_about_dialog;
        let mut close_requested = false;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close_requested = true;
        }

        let info = BuildInfo::current();
        egui::Window::new(self.t(Key::AboutTitle))
            .id(egui::Id::NULL.with("about_modal_window"))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("RRAW")
                        .strong()
                        .size(22.0)
                        .color(Color32::from_rgb(100, 180, 255)),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(self.t(Key::AboutDescription))
                        .italics()
                        .color(Color32::from_rgb(160, 165, 175)),
                );
                ui.add_space(10.0);

                egui::Grid::new("about_metadata_grid")
                    .num_columns(2)
                    .spacing([18.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new(self.t(Key::AboutVersion)).strong());
                        ui.label(RichText::new(format!("v{}", info.version)).monospace());
                        ui.end_row();
                        ui.label(RichText::new(self.t(Key::AboutCommit)).strong());
                        ui.label(RichText::new(info.commit_hash).monospace());
                        ui.end_row();
                        ui.label(RichText::new(self.t(Key::AboutBuildDate)).strong());
                        ui.label(RichText::new(info.build_date).monospace());
                        ui.end_row();
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.t(Key::AboutClose)).clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        if close_requested {
            is_open = false;
        }
        self.show_about_dialog = is_open;
    }
}
