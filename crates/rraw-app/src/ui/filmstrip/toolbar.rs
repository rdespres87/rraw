#![forbid(unsafe_code)]

//! Top toolbar of the bottom filmstrip panel (rating filters, batch selection, clipboard, navigation).

use std::path::PathBuf;

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::state::{ExportTargetMode, RatingFilter};

impl RrawApp {
    /// Renders the filmstrip toolbar with rating filter chips, selection controls, clipboard, and nav buttons.
    pub(super) fn render_filmstrip_toolbar(&mut self, ui: &mut egui::Ui, lang: Language) {
        ui.horizontal(|ui| {
            self.render_rating_filter_chips(ui, lang);
            ui.separator();
            let sel_count = self.render_selection_controls(ui, lang);
            ui.separator();
            self.render_clipboard_controls(ui, lang, sel_count);
            self.render_navigation_controls(ui, lang);
        });
    }

    fn render_rating_filter_chips(&mut self, ui: &mut egui::Ui, lang: Language) {
        ui.label(
            egui::RichText::new(crate::i18n::filmstrip_filter_label(lang))
                .strong()
                .size(12.0),
        );

        let mut filter_changed = false;

        let all_label =
            crate::i18n::filmstrip_filter_all(lang, self.library.directory_entries.len());
        if ui
            .selectable_label(self.library.rating_filter == RatingFilter::All, all_label)
            .clicked()
        {
            self.library.rating_filter = RatingFilter::All;
            filter_changed = true;
        }

        for stars in 1..=5 {
            let count = self.count_photos_with_min_rating(stars);
            let label = format!("★ {stars}+ ({count})");
            if ui
                .selectable_label(
                    self.library.rating_filter == RatingFilter::MinStars(stars),
                    label,
                )
                .clicked()
            {
                self.library.rating_filter = RatingFilter::MinStars(stars);
                filter_changed = true;
            }
        }

        let unrated_count = self.count_photos_unrated();
        if ui
            .selectable_label(
                self.library.rating_filter == RatingFilter::Unrated,
                format!("0★ ({unrated_count})"),
            )
            .clicked()
        {
            self.library.rating_filter = RatingFilter::Unrated;
            filter_changed = true;
        }

        let raw_count = self
            .library
            .directory_entries
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(crate::preview::is_raw_extension)
            })
            .count();
        if raw_count < self.library.directory_entries.len()
            && raw_count > 0
            && ui
                .selectable_label(
                    self.library.raw_only_filter,
                    format!("📷 RAW ({raw_count})"),
                )
                .on_hover_text(crate::i18n::filmstrip_raw_only_tooltip(lang))
                .clicked()
        {
            self.library.raw_only_filter = !self.library.raw_only_filter;
            self.layout_config.raw_only_filter = Some(self.library.raw_only_filter);
            self.layout_config.save();
            filter_changed = true;
        }

        if filter_changed {
            self.prune_selection_to_active_filter();
        }
    }

    fn prune_selection_to_active_filter(&mut self) {
        let to_remove: Vec<PathBuf> = self
            .library
            .selected_photos
            .iter()
            .filter(|p| !self.matches_active_filter(p))
            .cloned()
            .collect();
        for p in to_remove {
            self.library.selected_photos.remove(&p);
        }
        if let Some(loaded) = &self.session.loaded_photo {
            if !self.matches_active_filter(&loaded.path) {
                if let Some(target) = self.first_filtered_entry() {
                    self.open_raw_file(&target);
                }
            }
        }
    }

    fn render_selection_controls(&mut self, ui: &mut egui::Ui, lang: Language) -> usize {
        let sel_count = self
            .library
            .selected_photos
            .iter()
            .filter(|p| self.matches_active_filter(p))
            .count();
        let sel_text = crate::i18n::filmstrip_export_batch_count(lang, sel_count);
        ui.label(
            egui::RichText::new(sel_text)
                .small()
                .color(if sel_count > 0 {
                    Color32::from_rgb(100, 190, 255)
                } else {
                    Color32::GRAY
                }),
        );

        if ui
            .small_button(self.t(Key::FilmstripSelectAll))
            .on_hover_text(self.t(Key::FilmstripSelectAllTooltip))
            .clicked()
        {
            for entry in &self.library.directory_entries {
                if self.matches_active_filter(entry) {
                    self.library.selected_photos.insert(entry.clone());
                }
            }
        }
        if sel_count > 0 && ui.small_button(self.t(Key::FilmstripDeselectAll)).clicked() {
            self.library.selected_photos.clear();
        }
        if let Some(photo) = &self.session.loaded_photo {
            if !self.library.selected_photos.contains(&photo.path)
                && ui
                    .small_button(crate::i18n::filmstrip_check_active_button(lang))
                    .on_hover_text(crate::i18n::filmstrip_check_active_tooltip(lang))
                    .clicked()
            {
                self.library.selected_photos.insert(photo.path.clone());
            }
        }

        if sel_count > 0 {
            let export_btn = egui::Button::new(
                egui::RichText::new(crate::i18n::filmstrip_export_selection_button(
                    lang, sel_count,
                ))
                .strong()
                .color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(30, 120, 225));
            if ui
                .add(export_btn)
                .on_hover_text(crate::i18n::filmstrip_export_selection_tooltip(lang))
                .clicked()
            {
                self.export
                    .set_target_mode(ExportTargetMode::BatchSelection);
                self.export
                    .open_dialog(crate::controllers::export::ActiveModalDialog::Export);
                self.export.stats.status = None;
            }
        }

        sel_count
    }

    fn render_clipboard_controls(&mut self, ui: &mut egui::Ui, lang: Language, sel_count: usize) {
        let copy_btn = ui
            .small_button(self.t(Key::FilmstripToolbarCopy))
            .on_hover_text(self.t(Key::FilmstripToolbarCopyTooltip));
        if copy_btn.clicked() {
            self.copy_active_recipe();
        }

        if let Some(clip) = &self.library.recipe_clipboard {
            let clip_summary = clip.summary(self.lang());
            let paste_label = if sel_count > 0 {
                format!(
                    "{} ({sel_count})",
                    self.t(Key::FilmstripToolbarPasteSelection)
                )
            } else {
                self.t(Key::FilmstripToolbarPaste).to_owned()
            };
            let paste_btn = egui::Button::new(
                egui::RichText::new(paste_label)
                    .small()
                    .color(Color32::from_rgb(180, 240, 180)),
            )
            .fill(Color32::from_rgb(32, 54, 38));
            if ui
                .add(paste_btn)
                .on_hover_text(crate::i18n::filmstrip_paste_tooltip(lang, &clip_summary))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.paste_recipe_to_selection(&ctx);
            }
        }
    }

    fn render_navigation_controls(&mut self, ui: &mut egui::Ui, lang: Language) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("▶")
                .on_hover_text(crate::i18n::filmstrip_next_photo_tooltip(lang))
                .clicked()
            {
                self.navigate_relative(1);
            }
            if ui
                .button("◀")
                .on_hover_text(crate::i18n::filmstrip_prev_photo_tooltip(lang))
                .clicked()
            {
                self.navigate_relative(-1);
            }
            ui.label(
                egui::RichText::new(crate::i18n::filmstrip_shortcuts_hint(lang))
                    .small()
                    .color(Color32::GRAY),
            );
        });
    }
}
