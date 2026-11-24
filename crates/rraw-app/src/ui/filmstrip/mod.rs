#![forbid(unsafe_code)]

//! Bottom horizontal filmstrip thumbnail browser and left sidebar panel.

pub mod badges;
pub mod context_menu;
pub mod thumbnail;
mod toolbar;

use std::path::{Path, PathBuf};

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::Language;
use crate::state::LeftPanelTab;

use self::context_menu::{CardContextActions, ContextMenuParams};

/// Fixed logical width of a single filmstrip thumbnail card in points.
pub const FILMSTRIP_CARD_WIDTH: f32 = 120.0;
/// Fixed logical height of a single filmstrip thumbnail card in points.
pub const FILMSTRIP_CARD_HEIGHT: f32 = 104.0;
/// Horizontal gap between consecutive filmstrip cards in points.
pub const FILMSTRIP_CARD_SPACING: f32 = 8.0;
/// Horizontal pitch (`width + spacing`) between consecutive card origins in points.
pub const FILMSTRIP_CARD_STRIDE: f32 = FILMSTRIP_CARD_WIDTH + FILMSTRIP_CARD_SPACING;
/// Number of extra cards rendered on each side of the viewport for smooth scrolling.
pub const FILMSTRIP_OVERSCAN_CARDS: usize = 2;

/// Returns the total horizontal content width occupied by `total_cards` filmstrip cards.
#[must_use]
pub fn filmstrip_total_width(total_cards: usize, card_width: f32, card_spacing: f32) -> f32 {
    if total_cards == 0 || !card_width.is_finite() || !card_spacing.is_finite() {
        return 0.0;
    }
    let w = card_width.max(0.0);
    let s = card_spacing.max(0.0);
    (total_cards as f32) * w + (total_cards.saturating_sub(1) as f32) * s
}

/// Returns the horizontal origin offset of card `index` inside the filmstrip content space.
#[must_use]
pub fn filmstrip_card_x_offset(index: usize, card_stride: f32) -> f32 {
    if !card_stride.is_finite() || card_stride <= 0.0 {
        return 0.0;
    }
    (index as f32) * card_stride
}

/// Computes the half-open `[start..end)` range of card indices visible inside
/// `[viewport_min_x, viewport_max_x]`, expanded by `overscan` cards on each side.
#[must_use]
pub fn visible_filmstrip_range(
    total_cards: usize,
    viewport_min_x: f32,
    viewport_max_x: f32,
    card_stride: f32,
    overscan: usize,
) -> std::ops::Range<usize> {
    if total_cards == 0
        || !card_stride.is_finite()
        || card_stride <= 0.0
        || !viewport_min_x.is_finite()
        || !viewport_max_x.is_finite()
        || viewport_max_x <= viewport_min_x
        || viewport_max_x <= 0.0
    {
        return 0..0;
    }

    let clamped_min = viewport_min_x.max(0.0);
    let clamped_max = viewport_max_x.max(0.0);
    let first_visible = (clamped_min / card_stride).floor() as usize;
    if first_visible >= total_cards {
        return 0..0;
    }

    let last_exclusive = ((clamped_max / card_stride).ceil() as usize).min(total_cards);
    if last_exclusive <= first_visible {
        return 0..0;
    }

    let start = first_visible.saturating_sub(overscan);
    let end = last_exclusive.saturating_add(overscan).min(total_cards);
    start..end
}

/// Shared per-frame strip context passed to each visible card renderer.
struct FilmstripStripContext<'a> {
    lang: Language,
    filtered_indices: &'a [usize],
    active_path: Option<&'a Path>,
    has_clipboard: bool,
    clipboard_summary: Option<&'a str>,
}

/// Deferred user actions collected across the filmstrip card loop.
#[derive(Default)]
struct FilmstripFrameActions {
    file_to_open: Option<PathBuf>,
    star_change: Option<(PathBuf, u8)>,
    thumbs_to_request: Vec<PathBuf>,
    context: CardContextActions,
}

impl RrawApp {
    /// Renders the bottom filmstrip panel with rating filters, multi-selection, and thumbnail cards.
    pub fn render_filmstrip_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("filmstrip_panel")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(Color32::from_rgb(22, 24, 28))
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                let lang = self.lang();
                self.render_filmstrip_toolbar(ui, lang);
                ui.add_space(4.0);
                redirect_vertical_wheel_to_horizontal(ui);

                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .max_height(134.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show_viewport(ui, |ui, viewport| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = FILMSTRIP_CARD_SPACING;
                                self.render_filmstrip_cards(ui, viewport, lang);
                            });
                            // Dedicated vertical padding so the scrollbar track sits below cards
                            ui.add_space(14.0);
                        });
                    });
            });
    }

    fn render_filmstrip_cards(&mut self, ui: &mut egui::Ui, viewport: egui::Rect, lang: Language) {
        let active_path = self
            .session
            .loaded_photo
            .as_ref()
            .map(|p| p.path.clone())
            .or_else(|| self.layout_config.last_open_photo.clone());
        let filtered_indices: Vec<usize> = self
            .library
            .directory_entries
            .iter()
            .enumerate()
            .filter_map(|(idx, p)| self.matches_active_filter(p).then_some(idx))
            .collect();
        let total_cards = filtered_indices.len();
        let strip_origin = ui.cursor().min;

        if self.library.scroll_filmstrip_to_active {
            if let Some(active) = active_path.as_deref() {
                if let Some(active_pos) = filtered_indices.iter().position(|&dir_idx| {
                    self.library.directory_entries[dir_idx].as_path() == active
                }) {
                    let target_x =
                        strip_origin.x + filmstrip_card_x_offset(active_pos, FILMSTRIP_CARD_STRIDE);
                    let target_rect = egui::Rect::from_min_size(
                        egui::pos2(target_x, strip_origin.y),
                        egui::vec2(FILMSTRIP_CARD_WIDTH, FILMSTRIP_CARD_HEIGHT),
                    );
                    ui.scroll_to_rect(target_rect, None);
                }
            }
            self.library.scroll_filmstrip_to_active = false;
        }

        let range = visible_filmstrip_range(
            total_cards,
            viewport.min.x,
            viewport.max.x,
            FILMSTRIP_CARD_STRIDE,
            FILMSTRIP_OVERSCAN_CARDS,
        );

        if total_cards == 0 {
            return;
        }
        if range.is_empty() {
            let total_w =
                filmstrip_total_width(total_cards, FILMSTRIP_CARD_WIDTH, FILMSTRIP_CARD_SPACING);
            ui.allocate_space(egui::vec2(total_w, FILMSTRIP_CARD_HEIGHT));
            return;
        }

        if range.start > 0 {
            let leading_w =
                ((range.start as f32) * FILMSTRIP_CARD_STRIDE - FILMSTRIP_CARD_SPACING).max(0.0);
            ui.allocate_space(egui::vec2(leading_w, FILMSTRIP_CARD_HEIGHT));
        }

        let has_clipboard = self.library.recipe_clipboard.is_some();
        let clipboard_summary = self.library.recipe_clipboard.as_ref().map(|c| c.summary());
        let mut actions = FilmstripFrameActions::default();

        let strip_ctx = FilmstripStripContext {
            lang,
            filtered_indices: &filtered_indices,
            active_path: active_path.as_deref(),
            has_clipboard,
            clipboard_summary: clipboard_summary.as_deref(),
        };

        for visible_pos in range.clone() {
            let entry = self.library.directory_entries[filtered_indices[visible_pos]].clone();
            self.render_single_filmstrip_card(ui, &strip_ctx, visible_pos, &entry, &mut actions);
        }

        if range.end < total_cards {
            let remaining = total_cards - range.end;
            let trailing_w =
                ((remaining as f32) * FILMSTRIP_CARD_STRIDE - FILMSTRIP_CARD_SPACING).max(0.0);
            ui.allocate_space(egui::vec2(trailing_w, FILMSTRIP_CARD_HEIGHT));
        }

        self.apply_filmstrip_actions(ui, actions);
    }

    fn render_single_filmstrip_card(
        &mut self,
        ui: &mut egui::Ui,
        strip: &FilmstripStripContext<'_>,
        entry_idx: usize,
        entry: &Path,
        actions: &mut FilmstripFrameActions,
    ) {
        let rating = self.library.photo_ratings.get(entry).copied().unwrap_or(0);
        let is_selected = strip.active_path == Some(entry);
        let is_multi_selected = self.library.selected_photos.contains(entry);
        let is_cached = self.photo_cache.iter().any(|e| e.path == entry);
        let file_name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("photo");
        let ext = entry
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("RAW")
            .to_uppercase();

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(FILMSTRIP_CARD_WIDTH, FILMSTRIP_CARD_HEIGHT),
            egui::Sense::click(),
        );

        let menu_params = ContextMenuParams {
            entry,
            lang: strip.lang,
            is_in_selection: is_multi_selected,
            selection_len: self.library.selected_photos.len(),
            has_clipboard: strip.has_clipboard,
            clipboard_summary: strip.clipboard_summary,
        };
        context_menu::attach_card_context_menu(&response, &menu_params, &mut actions.context);

        let painter = ui.painter_at(rect);
        paint_card_background(&painter, rect, is_selected, is_multi_selected);

        let thumb_rect =
            egui::Rect::from_min_size(rect.min + egui::vec2(5.0, 4.0), egui::vec2(110.0, 62.0));
        let tex = self.workers.thumbnail_textures.get_and_touch(entry);
        if thumbnail::paint_thumbnail_frame(&painter, thumb_rect, tex, &ext) {
            actions.thumbs_to_request.push(entry.to_path_buf());
        }

        badges::paint_format_badge(&painter, thumb_rect, &ext);
        if is_cached {
            badges::paint_cache_indicator(&painter, thumb_rect);
        }
        let clicked_chk = badges::paint_selection_checkbox(
            ui,
            &painter,
            &response,
            thumb_rect,
            is_multi_selected,
        );
        badges::paint_file_name(&painter, rect, file_name, is_selected);

        let clicked_star = badges::paint_star_rating_row(ui, &painter, &response, rect, rating);
        if let Some(new_r) = clicked_star {
            actions.star_change = Some((entry.to_path_buf(), new_r));
        }

        let mut toggle_select = clicked_chk;
        if response.clicked() && clicked_star.is_none() && !clicked_chk {
            if ui.input(|i| i.modifiers.shift) {
                let anchor = self.library.last_selected_index.unwrap_or(0);
                let (min_i, max_i) = (anchor.min(entry_idx), anchor.max(entry_idx));
                for &dir_idx in strip
                    .filtered_indices
                    .iter()
                    .skip(min_i)
                    .take(max_i - min_i + 1)
                {
                    if let Some(p) = self.library.directory_entries.get(dir_idx) {
                        self.library.selected_photos.insert(p.clone());
                    }
                }
                actions.file_to_open = Some(entry.to_path_buf());
            } else if ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
                toggle_select = true;
                self.library.last_selected_index = Some(entry_idx);
            } else {
                self.library.last_selected_index = Some(entry_idx);
                actions.file_to_open = Some(entry.to_path_buf());
            }
        }

        if toggle_select {
            if self.library.selected_photos.contains(entry) {
                self.library.selected_photos.remove(entry);
            } else {
                self.library.selected_photos.insert(entry.to_path_buf());
            }
            self.library.last_selected_index = Some(entry_idx);
        }
    }

    fn apply_filmstrip_actions(&mut self, ui: &egui::Ui, actions: FilmstripFrameActions) {
        if let Some(path) = actions.context.copy_recipe_path {
            self.copy_recipe_for_path(&path);
        }
        if let Some(path) = actions.context.paste_recipe_path {
            let ctx = ui.ctx().clone();
            if self.library.selected_photos.contains(&path)
                && self.library.selected_photos.len() > 1
            {
                self.paste_recipe_to_selection(&ctx);
            } else {
                self.paste_recipe_to_path(&path, &ctx);
            }
        }
        if let Some(path) = actions.context.toggle_export_path {
            if self.library.selected_photos.contains(&path) {
                self.library.selected_photos.remove(&path);
            } else {
                self.library.selected_photos.insert(path);
            }
        }
        if let Some((path, new_r)) = actions.star_change {
            self.set_photo_rating_for_path(&path, new_r);
        }
        for path in actions.thumbs_to_request {
            self.request_thumbnail(path);
        }
        if let Some(path) = actions.file_to_open {
            self.open_raw_file(&path);
        }
    }

    /// Renders the left sidebar panel containing the reference histogram, folder explorer, styles, and EXIF.
    pub fn render_left_panel(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        egui::Panel::left("left_panel_fixed")
            .resizable(false)
            .exact_size(320.0)
            .frame(egui::Frame::side_top_panel(ui.style()).fill(Color32::from_rgb(22, 24, 28)))
            .show(ui, |ui| {
                ui.add_space(4.0);
                self.render_histogram_widget(ui);
                ui.separator();

                let tab_w = ((ui.available_width() - 8.0) / 3.0).floor();
                let (tab1_label, tab2_label, tab3_label) = crate::i18n::left_panel_tab_labels(lang);

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

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.left_panel_tab {
                        LeftPanelTab::Explorer => self.render_explorer_tab(ui),
                        LeftPanelTab::Presets => self.render_presets_section(ui),
                        LeftPanelTab::Exif => self.render_exif_panel(ui),
                    });
            });
    }
}

fn redirect_vertical_wheel_to_horizontal(ui: &egui::Ui) {
    let panel_rect = ui.max_rect();
    if ui.rect_contains_pointer(panel_rect) {
        ui.input_mut(|i| {
            if i.smooth_scroll_delta.x.abs() < 1e-4 && i.smooth_scroll_delta.y.abs() > 0.0 {
                i.smooth_scroll_delta.x = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta.y = 0.0;
            }
        });
    }
}

fn paint_card_background(
    painter: &egui::Painter,
    rect: egui::Rect,
    is_selected: bool,
    is_multi_selected: bool,
) {
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

    painter.rect_filled(rect, 6.0, bg_color);
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(border_width, border_color),
        egui::StrokeKind::Outside,
    );
}
