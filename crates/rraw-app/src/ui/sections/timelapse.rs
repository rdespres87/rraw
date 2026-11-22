#![forbid(unsafe_code)]

//! Timelapse export trigger button.

use egui::{Color32, Ui};

use crate::i18n::{Key, Language};

/// Renders the Timelapse export action button.
pub fn render_export_button(ui: &mut Ui, cur_lang: Language) -> bool {
    let t = |key: Key| key.translate(cur_lang);
    let mut trigger_export = false;

    ui.add_space(4.0);
    if ui
        .add(
            egui::Button::new(egui::RichText::new(t(Key::InspectorTimelapseExport)).strong())
                .fill(Color32::from_rgb(30, 110, 210)),
        )
        .clicked()
    {
        trigger_export = true;
    }

    trigger_export
}
