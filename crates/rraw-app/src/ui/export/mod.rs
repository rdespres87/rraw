#![forbid(unsafe_code)]

//! Image export dialog, batch processor, and RAW Timelapse & Temporal Deflicker sequence exporter.

mod destination_settings;
mod format_settings;
mod timelapse;

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::state::{ExportBitDepth, ExportFormatChoice, ExportScaleMode};

/// Configuration parameters for single image encoding and disk serialization.
#[derive(Debug, Clone, Copy)]
pub struct ExportWriteConfig {
    pub format: ExportFormatChoice,
    pub jpeg_quality: u8,
    pub bit_depth: ExportBitDepth,
    pub scale_mode: ExportScaleMode,
    pub hdr_gain_map: bool,
}

impl RrawApp {
    /// Renders the export configuration dialog window.
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.export.show_export_dialog {
            return;
        }

        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let mut is_open = self.export.show_export_dialog;
        let mut trigger_export =
            ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.egui_wants_keyboard_input();
        let mut close_dialog = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        egui::Window::new(t(Key::ExportWindowTitle))
            .id(egui::Id::NULL.with("export_timelapse_modal_window"))
            .open(&mut is_open)
            .movable(true)
            .collapsible(true)
            .resizable(false)
            .default_width(470.0)
            .default_pos(egui::pos2(780.0, 52.0))
            .show(ctx, |ui| {
                self.render_export_target_section(ui, lang);
                self.render_export_format_section(ui, lang);
                self.render_export_destination_section(ui, lang);
                let footer = self.render_export_footer(ui, lang);
                close_dialog |= footer.close_dialog;
                trigger_export |= footer.trigger_export;
            });

        if close_dialog {
            is_open = false;
        }
        self.export.set_dialog_open(
            crate::controllers::export::ActiveModalDialog::Export,
            is_open,
        );

        if trigger_export {
            self.export.is_exporting = true;
            self.export.export_progress = 0.05;
            self.export.export_progress_label =
                crate::i18n::tc(lang, "Export.PreparingStatus").to_owned();
            self.export.export_status = None;
            // Close the export modal immediately while progress is tracked in the status bar
            self.export
                .close_dialog(crate::controllers::export::ActiveModalDialog::Export);

            if let Err(err) = self.spawn_background_export() {
                self.export.is_exporting = false;
                self.export.export_status =
                    Some((format!("{}{err}", t(Key::ExportFailedPrefix)), false));
                self.status_message = format!("{}{err}", t(Key::ExportFailedPrefix));
            }
        }
    }
}
