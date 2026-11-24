#![forbid(unsafe_code)]

//! Export format, quality, bit-depth, and HDR Gain Map settings UI.

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::state::{ExportBitDepth, ExportFormatChoice};

impl RrawApp {
    /// Renders the output format selector, codec quality/bit-depth controls, and HDR Gain Map toggle.
    pub(super) fn render_export_format_section(&mut self, ui: &mut egui::Ui, lang: Language) {
        let t = |key: Key| key.translate(lang);

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
            ExportFormatChoice::Jpeg | ExportFormatChoice::Avif => {
                self.render_lossy_quality_slider(ui, lang);
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

        ui.add_space(4.0);
        ui.checkbox(
            &mut self.export.export_hdr_gain_map,
            crate::i18n::export_hdr_gain_map_checkbox_label(lang),
        )
        .on_hover_text(crate::i18n::export_hdr_gain_map_checkbox_tooltip(lang));
    }

    fn render_lossy_quality_slider(&mut self, ui: &mut egui::Ui, lang: Language) {
        let t = |key: Key| key.translate(lang);
        ui.horizontal(|ui| {
            ui.label(t(Key::ExportJpegQuality));
            ui.add(egui::Slider::new(&mut self.export.export_jpeg_quality, 1..=100).suffix("%"));
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
}
