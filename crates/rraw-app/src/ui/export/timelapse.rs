#![forbid(unsafe_code)]

//! RAW Timelapse sequence settings, temporal deflicker, and Holy Grail keyframe ramping controls.

use std::path::{Path, PathBuf};

use egui::Color32;

use crate::app::RrawApp;
use crate::i18n::{Key, Language};
use crate::state::{load_photo_sidecars, save_photo_sidecars};

/// Presentation descriptor for a timelapse Holy Grail keyframe endpoint (Start or End).
struct KeyframeEndpoint<'a> {
    path: &'a Path,
    title: String,
    title_color: Color32,
    view_tooltip: &'static str,
}

impl RrawApp {
    /// Renders the timelapse FPS, deflicker, and keyframe ramping controls.
    pub fn render_timelapse_controls_ui(&mut self, ui: &mut egui::Ui, id_salt_prefix: &str) {
        let lang = self.lang();
        let t = |key: Key| key.translate(lang);
        let tl_paths = self.timelapse_candidate_paths();
        let frame_count = tl_paths.len();
        let dur_s = self
            .export
            .config
            .timelapse
            .fps
            .duration_seconds(frame_count);

        let seq_text = crate::i18n::tc(lang, "Export.TimelapseSequence")
            .replace("{count}", &frame_count.to_string());
        let dur_text = crate::i18n::tc(lang, "Export.TimelapseDuration")
            .replace("{dur}", &format!("{dur_s:.2}"))
            .replace("{fps}", &self.export.config.timelapse.fps.fps().to_string());
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(seq_text)
                    .strong()
                    .color(Color32::from_rgb(120, 220, 255)),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(dur_text)
                    .strong()
                    .color(Color32::from_rgb(130, 235, 165)),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(t(Key::TimelapseFps));
            egui::ComboBox::from_id_salt(format!("{id_salt_prefix}_timelapse_fps_combo"))
                .selected_text(self.export.config.timelapse.fps.label())
                .show_ui(ui, |ui| {
                    for fps in rraw_export::TimelapseFrameRate::ALL {
                        ui.selectable_value(
                            &mut self.export.config.timelapse.fps,
                            fps,
                            fps.label(),
                        );
                    }
                });
        });

        ui.separator();
        ui.checkbox(
            &mut self.export.config.timelapse.deflicker.enabled,
            t(Key::TimelapseDeflickerTitle),
        )
        .on_hover_text(t(Key::TimelapseDeflickerTooltip));

        if self.export.config.timelapse.deflicker.enabled {
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseWindowSize));
                let mut rad = self.export.config.timelapse.deflicker.window_radius as u32;
                if ui
                    .add(
                        egui::Slider::new(&mut rad, 1..=15)
                            .suffix(crate::i18n::tc(lang, "Export.TimelapseFramesSuffix")),
                    )
                    .changed()
                {
                    self.export.config.timelapse.deflicker.window_radius = rad as usize;
                }
            });
            ui.horizontal(|ui| {
                ui.label(t(Key::TimelapseIntensity));
                let mut pct = (self.export.config.timelapse.deflicker.strength * 100.0).round();
                if ui
                    .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%"))
                    .changed()
                {
                    self.export.config.timelapse.deflicker.strength = (pct / 100.0).clamp(0.0, 1.0);
                }
            });
        }

        ui.separator();
        ui.checkbox(
            &mut self.export.config.timelapse.keyframe_ramping,
            t(Key::TimelapseHolyGrailTitle),
        )
        .on_hover_text(t(Key::TimelapseHolyGrailTooltip));

        if self.export.config.timelapse.keyframe_ramping && frame_count >= 2 {
            self.render_timelapse_keyframes(
                ui,
                lang,
                &tl_paths[0],
                &tl_paths[frame_count - 1],
                frame_count,
            );
        }
    }

    fn render_timelapse_keyframes(
        &mut self,
        ui: &mut egui::Ui,
        lang: Language,
        first_path: &Path,
        last_path: &Path,
        frame_count: usize,
    ) {
        let t = |key: Key| key.translate(lang);

        if let Some(loaded) = &self.session.loaded_photo {
            self.library
                .photo_gradings
                .insert(loaded.path.clone(), self.session.grading.clone());
        }
        for path in [first_path, last_path] {
            if !self.library.photo_gradings.contains_key(path) {
                let g = load_photo_sidecars(path).0.unwrap_or_default();
                self.library.photo_gradings.insert(path.to_path_buf(), g);
            }
        }

        let first_name = first_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Image 1");
        let last_name = last_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Image N");

        let start_ep = KeyframeEndpoint {
            path: first_path,
            title: format!("{} (#1 : {first_name})", t(Key::TimelapseStartLabel)),
            title_color: Color32::from_rgb(255, 215, 110),
            view_tooltip: t(Key::TimelapseViewStartTooltip),
        };
        let end_ep = KeyframeEndpoint {
            path: last_path,
            title: format!(
                "{} (#{frame_count} : {last_name})",
                t(Key::TimelapseEndLabel)
            ),
            title_color: Color32::from_rgb(155, 195, 255),
            view_tooltip: t(Key::TimelapseViewEndTooltip),
        };

        let mut open_preview_target: Option<PathBuf> = None;

        ui.add_space(4.0);
        if self.render_keyframe_endpoint(ui, lang, &start_ep) {
            open_preview_target = Some(first_path.to_path_buf());
        }
        ui.add_space(2.0);
        if self.render_keyframe_endpoint(ui, lang, &end_ep) {
            open_preview_target = Some(last_path.to_path_buf());
        }

        if let Some(target) = open_preview_target {
            self.open_raw_file(&target);
        }
    }

    fn render_keyframe_endpoint(
        &mut self,
        ui: &mut egui::Ui,
        lang: Language,
        ep: &KeyframeEndpoint<'_>,
    ) -> bool {
        let t = |key: Key| key.translate(lang);
        let mut g = self
            .library
            .photo_gradings
            .get(ep.path)
            .cloned()
            .unwrap_or_default();
        let mut changed = false;
        let mut commit_disk = false;
        let mut open_preview = false;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&ep.title)
                    .small()
                    .strong()
                    .color(ep.title_color),
            );
            if ui
                .small_button(t(Key::TimelapseViewButton))
                .on_hover_text(ep.view_tooltip)
                .clicked()
            {
                open_preview = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t(Key::TimelapseExpShort)).small());
            let r_exp = ui.add(
                egui::Slider::new(&mut g.tone.exposure_ev, -4.0..=4.0)
                    .step_by(0.05)
                    .suffix(" EV"),
            );
            if r_exp.changed() {
                changed = true;
            }
            if r_exp.drag_stopped() || r_exp.lost_focus() {
                commit_disk = true;
            }

            ui.label(egui::RichText::new(t(Key::TimelapseTempShort)).small());
            let r_tmp = ui.add(
                egui::Slider::new(&mut g.color.kelvin, 2000.0..=12000.0)
                    .step_by(50.0)
                    .suffix(" K"),
            );
            if r_tmp.changed() {
                changed = true;
            }
            if r_tmp.drag_stopped() || r_tmp.lost_focus() {
                commit_disk = true;
            }
        });

        if changed {
            self.library
                .photo_gradings
                .insert(ep.path.to_path_buf(), g.clone());
            if self
                .session
                .loaded_photo
                .as_ref()
                .is_some_and(|p| p.path == ep.path)
            {
                self.session.grading = g.clone();
                self.session.needs_preview_update = true;
                ui.ctx().request_repaint();
            }
        }
        if commit_disk {
            let r = self
                .library
                .photo_ratings
                .get(ep.path)
                .copied()
                .unwrap_or(0);
            save_photo_sidecars(ep.path, &g, r);
        }

        open_preview
    }
}
