#![forbid(unsafe_code)]

//! Bottom horizontal filmstrip thumbnail browser and photo selection.

use egui::Color32;
use std::path::PathBuf;

use crate::app::RrawApp;
use crate::i18n::Key;
use crate::state::*;

impl RrawApp {
    /// Renders the bottom filmstrip panel with rating filters, multi-selection, and thumbnail cards.
    pub fn render_filmstrip_panel(&mut self, ui: &mut egui::Ui) {
        // 3. Bottom Filmstrip Panel (Horizontal thumbnail strip)
        egui::Panel::bottom("filmstrip_panel")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(egui::Color32::from_rgb(22, 24, 28))
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                // Filmstrip Toolbar: Rating Filter Chips & Quick Navigation
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Filtre :").strong().size(12.0));

                    let mut filter_changed = false;

                    let all_label = format!("Toutes ({})", self.library.directory_entries.len());
                    if ui
                        .selectable_label(self.library.rating_filter == RatingFilter::All, all_label)
                        .clicked()
                    {
                        self.library.rating_filter = RatingFilter::All;
                        filter_changed = true;
                    }

                    for stars in 1..=5 {
                        let count = self.count_photos_with_min_rating(stars);
                        let label = format!("★ {}+ ({})", stars, count);
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
                    let unrated_label = format!("0★ ({})", unrated_count);
                    if ui
                        .selectable_label(
                            self.library.rating_filter == RatingFilter::Unrated,
                            unrated_label,
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
                    let has_non_raw = raw_count < self.library.directory_entries.len() && raw_count > 0;
                    if has_non_raw {
                        let raw_chip_text = format!("📷 RAW ({raw_count})");
                        if ui
                            .selectable_label(self.library.raw_only_filter, raw_chip_text)
                            .on_hover_text(
                                "Masquer les fichiers JPG/PNG pour n'afficher que les fichiers RAW d'origine",
                            )
                            .clicked()
                        {
                            self.library.raw_only_filter = !self.library.raw_only_filter;
                            self.layout_config.raw_only_filter = Some(self.library.raw_only_filter);
                            self.layout_config.save();
                            filter_changed = true;
                        }
                    }

                    if filter_changed {
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

                    ui.separator();
                    let sel_count = self
                        .library
                        .selected_photos
                        .iter()
                        .filter(|p| self.matches_active_filter(p))
                        .count();
                    let sel_text = if sel_count > 0 {
                        format!(
                            "Lot d'export : ✓ {} cochée{}",
                            sel_count,
                            if sel_count > 1 { "s" } else { "" }
                        )
                    } else {
                        "Lot d'export : 0 cochée".to_string()
                    };
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
                                .small_button("+ Cocher active")
                                .on_hover_text("Ajouter la photo courante à la sélection d'export")
                                .clicked()
                        {
                            self.library.selected_photos.insert(photo.path.clone());
                        }
                    }

                    if sel_count > 0 {
                        let export_btn = egui::Button::new(
                            egui::RichText::new(format!("📤 Exporter sélection ({sel_count})"))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(30, 120, 225));
                        if ui
                            .add(export_btn)
                            .on_hover_text(
                                "Ouvrir la fenêtre d'export pour les photos sélectionnées",
                            )
                            .clicked()
                        {
                            self.export.set_target_mode(ExportTargetMode::BatchSelection);
                            self.export
                                .open_dialog(crate::controllers::export::ActiveModalDialog::Export);
                            self.export.export_status = None;
                        }
                    }

                    // Clipboard quick actions in filmstrip toolbar
                    ui.separator();
                    let has_clip = self.library.recipe_clipboard.is_some();
                    let copy_btn = ui
                        .small_button(self.t(Key::FilmstripToolbarCopy))
                        .on_hover_text(self.t(Key::FilmstripToolbarCopyTooltip));
                    if copy_btn.clicked() {
                        self.copy_active_recipe();
                    }

                    if has_clip {
                        let clip_summary = self
                            .library
                            .recipe_clipboard
                            .as_ref()
                            .map(|c| c.summary())
                            .unwrap_or_default();
                        let paste_label = if sel_count > 0 {
                            format!("{} ({sel_count})", self.t(Key::FilmstripToolbarPasteSelection))
                        } else {
                            self.t(Key::FilmstripToolbarPaste).to_string()
                        };
                        let paste_btn = egui::Button::new(
                            egui::RichText::new(paste_label)
                                .small()
                                .color(Color32::from_rgb(180, 240, 180)),
                        )
                        .fill(Color32::from_rgb(32, 54, 38));
                        if ui
                            .add(paste_btn)
                            .on_hover_text(format!(
                                "Coller le rendu ({clip_summary}) sur la sélection ou la photo active (Cmd+Shift+V)"
                            ))
                            .clicked()
                        {
                            let ctx = ui.ctx().clone();
                            self.paste_recipe_to_selection(&ctx);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("▶")
                            .on_hover_text("Photo suivante (Flèche droite)")
                            .clicked()
                        {
                            self.navigate_relative(1);
                        }
                        if ui
                            .button("◀")
                            .on_hover_text("Photo précédente (Flèche gauche)")
                            .clicked()
                        {
                            self.navigate_relative(-1);
                        }
                        ui.label(
                            egui::RichText::new("Raccourcis : ◀/▶ (Naviguer) | 0-5 (Noter)")
                                .small()
                                .color(Color32::GRAY),
                        );
                    });
                });

                ui.add_space(4.0);

                // Map vertical mouse wheel scroll to horizontal filmstrip scroll when hovering bottom panel
                let panel_rect = ui.max_rect();
                if ui.rect_contains_pointer(panel_rect) {
                    ui.input_mut(|i| {
                        if i.smooth_scroll_delta.x.abs() < 1e-4
                            && i.smooth_scroll_delta.y.abs() > 0.0
                        {
                            i.smooth_scroll_delta.x = i.smooth_scroll_delta.y;
                            i.smooth_scroll_delta.y = 0.0;
                        }
                    });
                }

                // Horizontal Filmstrip scroll area with visible scrollbar positioned safely below cards
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .max_height(134.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let active_path = self
                                    .session
                                    .loaded_photo
                                    .as_ref()
                                    .map(|p| p.path.clone())
                                    .or_else(|| self.layout_config.last_open_photo.clone());
                                let mut file_to_open = None;
                                let mut star_change: Option<(PathBuf, u8)> = None;
                                let mut thumbs_to_request = Vec::new();
                                let entries: Vec<PathBuf> = self
                                    .library
                                    .directory_entries
                                    .iter()
                                    .filter(|p| self.matches_active_filter(p))
                                    .cloned()
                                    .collect();
                                let has_clipboard = self.library.recipe_clipboard.is_some();
                                let clipboard_summary =
                                    self.library.recipe_clipboard.as_ref().map(|c| c.summary());
                                let mut copy_recipe_path = None;
                                let mut paste_recipe_path = None;
                                let mut toggle_export_path = None;

                                for (entry_idx, entry) in entries.iter().enumerate() {
                                    let rating =
                                        self.library.photo_ratings.get(entry).copied().unwrap_or(0);

                                    let is_selected = active_path.as_ref() == Some(entry);
                                    let is_multi_selected = self.library.selected_photos.contains(entry);
                                    let is_cached =
                                        self.photo_cache.iter().any(|e| &e.path == entry);

                                    let file_name = entry
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("photo");

                                    let ext = entry
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("RAW")
                                        .to_uppercase();

                                    let border_color = if is_selected {
                                        Color32::from_rgb(80, 160, 255)
                                    } else if is_multi_selected {
                                        Color32::from_rgb(50, 140, 230)
                                    } else {
                                        Color32::from_rgb(45, 48, 55)
                                    };
                                    let border_width = if is_selected || is_multi_selected {
                                        2.0
                                    } else {
                                        1.0
                                    };
                                    let bg_color = if is_selected {
                                        Color32::from_rgb(28, 36, 48)
                                    } else if is_multi_selected {
                                        Color32::from_rgb(22, 32, 44)
                                    } else {
                                        Color32::from_rgb(20, 22, 26)
                                    };

                                    let card_size = egui::vec2(120.0, 104.0);
                                    let (rect, response) =
                                        ui.allocate_exact_size(card_size, egui::Sense::click());

                                    // Follow active thumbnail card into view when navigating via keyboard or controls
                                    if is_selected && self.library.scroll_filmstrip_to_active {
                                        response.scroll_to_me(None);
                                        self.library.scroll_filmstrip_to_active = false;
                                    }

                                    // Contextual right-click menu for batch development & clipboard
                                    response.context_menu(|ui| {
                                        ui.set_min_width(230.0);
                                        if ui.button(self.t(Key::FilmstripCopyRecipe)).clicked() {
                                            copy_recipe_path = Some(entry.clone());
                                            ui.close();
                                        }

                                        let is_in_selection = self.library.selected_photos.contains(entry);
                                        let paste_label = if is_in_selection
                                            && self.library.selected_photos.len() > 1
                                        {
                                            format!(
                                                "{} ({})",
                                                self.t(Key::FilmstripPasteRecipeSelection),
                                                self.library.selected_photos.len()
                                            )
                                        } else {
                                            self.t(Key::FilmstripPasteRecipeThis).to_string()
                                        };
                                        let paste_btn = egui::Button::new(paste_label);
                                        if ui.add_enabled(has_clipboard, paste_btn).clicked() {
                                            paste_recipe_path = Some(entry.clone());
                                            ui.close();
                                        }

                                        if let Some(summary) = &clipboard_summary {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "Presse-papiers : {summary}"
                                                ))
                                                .small()
                                                .color(Color32::from_rgb(140, 150, 165)),
                                            );
                                        }

                                        ui.separator();
                                        let chk_label = if is_multi_selected {
                                            "✕ Décocher du lot d'export"
                                        } else {
                                            "✓ Cocher pour le lot d'export"
                                        };
                                        if ui.button(chk_label).clicked() {
                                            toggle_export_path = Some(entry.clone());
                                            ui.close();
                                        }
                                    });
                                    let painter = ui.painter_at(rect);

                                    painter.rect_filled(rect, 6.0, bg_color);
                                    painter.rect_stroke(
                                        rect,
                                        6.0,
                                        egui::Stroke::new(border_width, border_color),
                                        egui::StrokeKind::Outside,
                                    );

                                    // Top miniature thumbnail frame
                                    let thumb_rect = egui::Rect::from_min_size(
                                        rect.min + egui::vec2(5.0, 4.0),
                                        egui::vec2(110.0, 62.0),
                                    );

                                    // Fill thumbnail background to letterbox portrait & non-16:9 images cleanly
                                    painter.rect_filled(
                                        thumb_rect,
                                        2.0,
                                        Color32::from_rgb(14, 16, 20),
                                    );

                                    if let Some(tex) = self.workers.thumbnail_textures.get(entry) {
                                        let tex_size = tex.size_vec2();
                                        let tex_aspect = tex_size.x / tex_size.y.max(1.0);
                                        let frame_aspect = thumb_rect.width() / thumb_rect.height().max(1.0);
                                        let fitted_rect = if tex_aspect > frame_aspect {
                                            // Limited by width (letterbox top/bottom)
                                            let h = (thumb_rect.width() / tex_aspect).clamp(10.0, thumb_rect.height());
                                            let y = thumb_rect.center().y - h * 0.5;
                                            egui::Rect::from_min_size(
                                                egui::pos2(thumb_rect.left(), y),
                                                egui::vec2(thumb_rect.width(), h),
                                            )
                                        } else {
                                            // Limited by height (pillarbox left/right for portrait / square crops)
                                            let w = (thumb_rect.height() * tex_aspect).clamp(10.0, thumb_rect.width());
                                            let x = thumb_rect.center().x - w * 0.5;
                                            egui::Rect::from_min_size(
                                                egui::pos2(x, thumb_rect.top()),
                                                egui::vec2(w, thumb_rect.height()),
                                            )
                                        };

                                        painter.image(
                                            tex.id(),
                                            fitted_rect,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            Color32::WHITE,
                                        );
                                        painter.rect_stroke(
                                            thumb_rect,
                                            2.0,
                                            egui::Stroke::new(1.0, Color32::from_rgb(45, 50, 60)),
                                            egui::StrokeKind::Outside,
                                        );
                                    } else {
                                        thumbs_to_request.push(entry.clone());
                                        painter.rect_stroke(
                                            thumb_rect,
                                            2.0,
                                            egui::Stroke::new(1.0, Color32::from_rgb(35, 38, 45)),
                                            egui::StrokeKind::Outside,
                                        );
                                        painter.text(
                                            thumb_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            format!("📷 [{}]", ext),
                                            egui::FontId::monospace(10.0),
                                            Color32::from_rgb(110, 120, 135),
                                        );
                                    }

                                    // Format badge tag inside thumbnail (top-left)
                                    let badge_rect = egui::Rect::from_min_size(
                                        thumb_rect.min + egui::vec2(3.0, 3.0),
                                        egui::vec2(30.0, 13.0),
                                    );
                                    painter.rect_filled(
                                        badge_rect,
                                        2.0,
                                        Color32::from_rgba_premultiplied(0, 0, 0, 180),
                                    );
                                    painter.text(
                                        badge_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        &ext,
                                        egui::FontId::monospace(9.0),
                                        Color32::from_rgb(200, 210, 230),
                                    );

                                    // Cache indicator (top-right of thumbnail, offset from checkbox)
                                    if is_cached {
                                        painter.text(
                                            egui::pos2(
                                                thumb_rect.max.x - 26.0,
                                                thumb_rect.min.y + 3.0,
                                            ),
                                            egui::Align2::RIGHT_TOP,
                                            "●",
                                            egui::FontId::proportional(11.0),
                                            Color32::from_rgb(80, 220, 120),
                                        );
                                    }

                                    // Selection checkbox (top-right of thumbnail) for batch export
                                    let chk_rect = egui::Rect::from_min_size(
                                        egui::pos2(thumb_rect.max.x - 23.0, thumb_rect.min.y + 2.0),
                                        egui::vec2(20.0, 20.0),
                                    );
                                    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
                                    let hovered_chk = ui
                                        .input(|i| i.pointer.hover_pos())
                                        .is_some_and(|p| chk_rect.expand(2.0).contains(p));
                                    if hovered_chk {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    let clicked_chk = response.clicked()
                                        && pointer_pos
                                            .is_some_and(|p| chk_rect.expand(3.0).contains(p));
                                    let mut toggle_select = clicked_chk;

                                    if is_multi_selected {
                                        painter.circle_filled(
                                            chk_rect.center(),
                                            9.0,
                                            Color32::from_rgb(30, 130, 240),
                                        );
                                        painter.text(
                                            chk_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "✓",
                                            egui::FontId::proportional(12.0),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        let circle_col = if hovered_chk {
                                            Color32::from_rgba_premultiplied(50, 130, 230, 220)
                                        } else {
                                            Color32::from_rgba_premultiplied(0, 0, 0, 140)
                                        };
                                        painter.circle_filled(chk_rect.center(), 8.0, circle_col);
                                        painter.circle_stroke(
                                            chk_rect.center(),
                                            8.0,
                                            egui::Stroke::new(
                                                1.2,
                                                Color32::from_rgb(160, 175, 200),
                                            ),
                                        );
                                    }

                                    // Middle row: File name
                                    let truncated_name = if file_name.len() > 14 {
                                        format!("{}…", &file_name[..13])
                                    } else {
                                        file_name.to_string()
                                    };
                                    let name_color = if is_selected {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(200, 205, 215)
                                    };
                                    painter.text(
                                        egui::pos2(rect.center().x, rect.min.y + 70.0),
                                        egui::Align2::CENTER_TOP,
                                        truncated_name,
                                        egui::FontId::proportional(10.5),
                                        name_color,
                                    );

                                    // Bottom row: 5 clickable star glyphs completely above the bottom edge
                                    let star_y = rect.min.y + 90.0;
                                    let star_spacing = 15.0;
                                    let star_start_x = rect.center().x - (2.0 * star_spacing);

                                    let mut hovered_star = None;
                                    let hover_pos = ui.input(|i| i.pointer.hover_pos());
                                    for s in 1..=5 {
                                        let star_pos = egui::pos2(
                                            star_start_x + (s as f32 - 1.0) * star_spacing,
                                            star_y,
                                        );
                                        let star_rect = egui::Rect::from_center_size(
                                            star_pos,
                                            egui::vec2(14.0, 14.0),
                                        );
                                        if hover_pos.is_some_and(|p| star_rect.contains(p)) {
                                            hovered_star = Some(s);
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if response.clicked()
                                            && pointer_pos.is_some_and(|p| star_rect.contains(p))
                                        {
                                            let new_r = if rating == s { 0 } else { s };
                                            star_change = Some((entry.clone(), new_r));
                                        }
                                    }

                                    for s in 1..=5 {
                                        let star_pos = egui::pos2(
                                            star_start_x + (s as f32 - 1.0) * star_spacing,
                                            star_y,
                                        );
                                        let is_lit = if let Some(h) = hovered_star {
                                            s <= h
                                        } else {
                                            s <= rating
                                        };
                                        let (star_char, star_col) = if is_lit {
                                            ("★", Color32::from_rgb(255, 205, 50))
                                        } else {
                                            ("☆", Color32::from_rgb(75, 80, 95))
                                        };

                                        painter.text(
                                            star_pos,
                                            egui::Align2::CENTER_CENTER,
                                            star_char,
                                            egui::FontId::proportional(13.0),
                                            star_col,
                                        );
                                    }

                                    // Classical mouse selection: Click opens photo, Cmd/Ctrl+Click or Checkbox toggles batch selection, Shift+Click selects range
                                    if response.clicked() && star_change.is_none() && !clicked_chk {
                                        if ui.input(|i| i.modifiers.shift) {
                                            let anchor = self.library.last_selected_index.unwrap_or(0);
                                            let min_i = anchor.min(entry_idx);
                                            let max_i = anchor.max(entry_idx);
                                            for i in min_i..=max_i {
                                                if let Some(p) = entries.get(i) {
                                                    self.library.selected_photos.insert(p.clone());
                                                }
                                            }
                                            file_to_open = Some(entry.clone());
                                        } else if ui
                                            .input(|i| i.modifiers.command || i.modifiers.ctrl)
                                        {
                                            toggle_select = true;
                                            self.library.last_selected_index = Some(entry_idx);
                                        } else {
                                            self.library.last_selected_index = Some(entry_idx);
                                            file_to_open = Some(entry.clone());
                                        }
                                    }

                                    if toggle_select {
                                        if self.library.selected_photos.contains(entry) {
                                            self.library.selected_photos.remove(entry);
                                        } else {
                                            self.library.selected_photos.insert(entry.clone());
                                        }
                                        self.library.last_selected_index = Some(entry_idx);
                                    }
                                }

                                if let Some(path) = copy_recipe_path {
                                    self.copy_recipe_for_path(&path);
                                }
                                if let Some(path) = paste_recipe_path {
                                    let ctx = ui.ctx().clone();
                                    if self.library.selected_photos.contains(&path)
                                        && self.library.selected_photos.len() > 1
                                    {
                                        self.paste_recipe_to_selection(&ctx);
                                    } else {
                                        self.paste_recipe_to_path(&path, &ctx);
                                    }
                                }
                                if let Some(path) = toggle_export_path {
                                    if self.library.selected_photos.contains(&path) {
                                        self.library.selected_photos.remove(&path);
                                    } else {
                                        self.library.selected_photos.insert(path);
                                    }
                                }

                                if let Some((path, new_r)) = star_change {
                                    self.set_photo_rating_for_path(&path, new_r);
                                }

                                for path in thumbs_to_request {
                                    self.request_thumbnail(path);
                                }

                                if let Some(path) = file_to_open {
                                    self.open_raw_file(&path);
                                }
                            });

                            // Dedicated vertical padding so the scrollbar track sits entirely below cards
                            ui.add_space(14.0);
                        });
                    });
            });
    }

    /// Renders the left sidebar panel containing the reference histogram, folder explorer, styles, and EXIF.
    pub fn render_left_panel(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        egui::Panel::left("left_panel_fixed")
            .resizable(false)
            .exact_size(320.0)
            .frame(
                egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(22, 24, 28)),
            )
            .show(ui, |ui| {
                ui.add_space(4.0);

                // 1. Reference Histogram pinned prominently at the very top of the sidebar
                self.render_histogram_widget(ui);

                ui.separator();

                // 2. High-Level Contextual Navigation Tabs (Cmd+1 / Cmd+2 / Cmd+3)
                let tab_w = ((ui.available_width() - 8.0) / 3.0).floor();
                let is_fr = lang == crate::i18n::Language::Fr;
                let (tab1_label, tab2_label, tab3_label) = if is_fr {
                    ("📁 Fichiers", "🎨 Styles", "ℹ️ EXIF")
                } else {
                    ("📁 Files", "🎨 Styles", "ℹ️ EXIF")
                };

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    let tabs = [
                        (LeftPanelTab::Explorer, tab1_label, "Cmd + 1"),
                        (LeftPanelTab::Presets, tab2_label, "Cmd + 2"),
                        (LeftPanelTab::Exif, tab3_label, "Cmd + 3"),
                    ];
                    for (tab, label, tip) in tabs {
                        let is_active = self.left_panel_tab == tab;
                        if ui
                            .add_sized(
                                [tab_w, 24.0],
                                egui::Button::new(
                                    egui::RichText::new(label).size(11.5).strong().color(
                                        if is_active {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_rgb(150, 155, 165)
                                        },
                                    ),
                                )
                                .selected(is_active),
                            )
                            .on_hover_text(format!("{label} ({tip})"))
                            .clicked()
                        {
                            self.set_left_panel_tab(tab);
                            self.layout_config.save();
                        }
                    }
                });

                ui.add_space(4.0);
                ui.separator();

                // 3. Scrollable active module: Explorer, Presets, or EXIF in full height
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.left_panel_tab {
                        LeftPanelTab::Explorer => {
                            self.render_explorer_tab(ui);
                        }
                        LeftPanelTab::Presets => {
                            self.render_presets_section(ui);
                        }
                        LeftPanelTab::Exif => {
                            self.render_exif_panel(ui);
                        }
                    });
            });
    }
}
