#![forbid(unsafe_code)]

//! Right-click context menu for filmstrip thumbnail cards.

use std::path::{Path, PathBuf};

use egui::Color32;

use crate::i18n::{Key, Language};

/// Deferred actions emitted by a filmstrip card context menu.
#[derive(Default)]
pub struct CardContextActions {
    pub copy_recipe_path: Option<PathBuf>,
    pub paste_recipe_path: Option<PathBuf>,
    pub toggle_export_path: Option<PathBuf>,
}

/// Parameters required to display a card's right-click context menu.
pub struct ContextMenuParams<'a> {
    pub entry: &'a Path,
    pub lang: Language,
    pub is_in_selection: bool,
    pub selection_len: usize,
    pub has_clipboard: bool,
    pub clipboard_summary: Option<&'a str>,
}

/// Attaches the right-click context menu for recipe copy/paste and batch selection toggling.
pub fn attach_card_context_menu(
    response: &egui::Response,
    params: &ContextMenuParams<'_>,
    actions: &mut CardContextActions,
) {
    let lang = params.lang;
    let t = |k: Key| k.translate(lang);

    response.context_menu(|ui| {
        ui.set_min_width(230.0);
        if ui.button(t(Key::FilmstripCopyRecipe)).clicked() {
            actions.copy_recipe_path = Some(params.entry.to_path_buf());
            ui.close();
        }

        let paste_label = if params.is_in_selection && params.selection_len > 1 {
            format!(
                "{} ({})",
                t(Key::FilmstripPasteRecipeSelection),
                params.selection_len
            )
        } else {
            t(Key::FilmstripPasteRecipeThis).to_owned()
        };
        let paste_btn = egui::Button::new(paste_label);
        if ui.add_enabled(params.has_clipboard, paste_btn).clicked() {
            actions.paste_recipe_path = Some(params.entry.to_path_buf());
            ui.close();
        }

        if let Some(summary) = params.clipboard_summary {
            ui.label(
                egui::RichText::new(crate::i18n::filmstrip_clipboard_prefix(lang, summary))
                    .small()
                    .color(Color32::from_rgb(140, 150, 165)),
            );
        }

        ui.separator();
        let chk_label = crate::i18n::filmstrip_toggle_batch_label(lang, params.is_in_selection);
        if ui.button(chk_label).clicked() {
            actions.toggle_export_path = Some(params.entry.to_path_buf());
            ui.close();
        }
    });
}
