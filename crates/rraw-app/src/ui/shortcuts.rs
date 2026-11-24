#![forbid(unsafe_code)]

//! Keyboard shortcuts cheat sheet dialog.

use egui::{Color32, RichText};

use crate::app::RrawApp;
use crate::i18n::{tc, Key};

impl RrawApp {
    /// Renders the modal shortcuts reference window.
    pub fn render_shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.export.show_shortcuts_dialog {
            return;
        }

        let mut is_open = self.export.show_shortcuts_dialog;
        let mut close_requested = false;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close_requested = true;
        }

        let lang = self.lang();
        egui::Window::new(self.t(Key::ShortcutsWindowTitle))
            .id(egui::Id::NULL.with("shortcuts_modal_window"))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .default_width(580.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(self.t(Key::ShortcutsSubtitle))
                        .italics()
                        .color(Color32::from_rgb(160, 165, 175)),
                );
                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .max_height(480.0)
                    .show(ui, |ui| {
                        let sec_view = [
                            ("Y", tc(lang, "Shortcuts.View.SplitCompare")),
                            ("D, P, \\", tc(lang, "Shortcuts.View.HoldOriginal")),
                            ("J", tc(lang, "Shortcuts.View.Clipping")),
                            ("I", tc(lang, "Shortcuts.View.Hud")),
                            ("F9", tc(lang, "Shortcuts.View.Filmstrip")),
                            ("O", tc(lang, "Shortcuts.View.MaskOverlay")),
                            ("Cmd + 1 / 2 / 3", tc(lang, "Shortcuts.View.LeftPanelTabs")),
                            (
                                tc(lang, "Shortcuts.Key.DoubleClick"),
                                tc(lang, "Shortcuts.View.FitZoom"),
                            ),
                            (
                                tc(lang, "Shortcuts.Key.ScrollWheel"),
                                tc(lang, "Shortcuts.View.WheelZoom"),
                            ),
                            (
                                tc(lang, "Shortcuts.Key.SpaceDrag"),
                                tc(lang, "Shortcuts.View.Pan"),
                            ),
                        ];
                        let sec_nav = [
                            ("◀ / ▶", tc(lang, "Shortcuts.Nav.PrevNext")),
                            ("PageUp / PageDown", tc(lang, "Shortcuts.Nav.Page")),
                            (
                                tc(lang, "Shortcuts.Key.HomeEnd"),
                                tc(lang, "Shortcuts.Nav.HomeEnd"),
                            ),
                            (
                                tc(lang, "Shortcuts.Key.RatingRange"),
                                tc(lang, "Shortcuts.Nav.Rate"),
                            ),
                            ("Cmd + A", tc(lang, "Shortcuts.Nav.SelectAll")),
                            ("Cmd + D", tc(lang, "Shortcuts.Nav.DeselectAll")),
                            (
                                tc(lang, "Shortcuts.Key.Space"),
                                tc(lang, "Shortcuts.Nav.ToggleBatch"),
                            ),
                        ];
                        let sec_batch = [
                            ("Cmd + Z", tc(lang, "Shortcuts.Batch.Undo")),
                            ("Cmd + Y", tc(lang, "Shortcuts.Batch.Redo")),
                            ("Cmd + C", tc(lang, "Shortcuts.Batch.Copy")),
                            ("Cmd + V", tc(lang, "Shortcuts.Batch.Paste")),
                            (
                                tc(lang, "Shortcuts.Key.RightClick"),
                                tc(lang, "Shortcuts.Batch.ContextMenu"),
                            ),
                        ];
                        let sec_files = [
                            ("Cmd + O", tc(lang, "Shortcuts.Files.OpenImage")),
                            ("Cmd + Shift + O", tc(lang, "Shortcuts.Files.OpenFolder")),
                            ("Cmd + E", tc(lang, "Shortcuts.Files.Export")),
                            (
                                tc(lang, "Shortcuts.Key.Enter"),
                                tc(lang, "Shortcuts.Files.StartExport"),
                            ),
                            (
                                tc(lang, "Shortcuts.Key.Esc"),
                                tc(lang, "Shortcuts.Files.Escape"),
                            ),
                            ("F1", tc(lang, "Shortcuts.Files.Help")),
                        ];
                        let sec_brush = [
                            ("[ / ]", tc(lang, "Shortcuts.Brush.Radius")),
                            (
                                tc(lang, "Shortcuts.Key.AltScroll"),
                                tc(lang, "Shortcuts.Brush.AltScroll"),
                            ),
                            (
                                tc(lang, "Shortcuts.Key.LeftClick"),
                                tc(lang, "Shortcuts.Brush.Paint"),
                            ),
                            (
                                tc(lang, "Shortcuts.Key.RightClick"),
                                tc(lang, "Shortcuts.Brush.Erase"),
                            ),
                        ];

                        render_section(
                            ui,
                            "sec_view",
                            self.t(Key::ShortcutsSectionView),
                            &sec_view,
                        );
                        ui.add_space(10.0);
                        render_section(ui, "sec_nav", self.t(Key::ShortcutsSectionNav), &sec_nav);
                        ui.add_space(10.0);
                        render_section(
                            ui,
                            "sec_batch",
                            self.t(Key::ShortcutsSectionBatch),
                            &sec_batch,
                        );
                        ui.add_space(10.0);
                        render_section(
                            ui,
                            "sec_files",
                            self.t(Key::ShortcutsSectionFiles),
                            &sec_files,
                        );
                        ui.add_space(10.0);
                        render_section(
                            ui,
                            "sec_brush",
                            self.t(Key::ShortcutsSectionBrush),
                            &sec_brush,
                        );
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.t(Key::ShortcutsClose)).clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        if close_requested {
            is_open = false;
        }
        self.export.set_dialog_open(
            crate::controllers::export::ActiveModalDialog::Shortcuts,
            is_open,
        );
    }
}

fn render_section(ui: &mut egui::Ui, id: &str, title: &str, items: &[(&str, &str)]) {
    ui.label(
        RichText::new(title)
            .strong()
            .color(Color32::from_rgb(100, 180, 255)),
    );
    ui.add_space(3.0);

    egui::Grid::new(id)
        .num_columns(2)
        .spacing([18.0, 5.0])
        .striped(true)
        .show(ui, |ui| {
            for (key, desc) in items {
                ui.label(RichText::new(*key).strong().monospace().size(11.5));
                ui.label(RichText::new(*desc).size(11.5));
                ui.end_row();
            }
        });
}
