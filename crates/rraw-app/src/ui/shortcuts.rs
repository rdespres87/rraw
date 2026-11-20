#![forbid(unsafe_code)]

//! Keyboard shortcuts cheat sheet dialog.

use egui::{Color32, RichText};

use crate::app::RrawApp;

impl RrawApp {
    /// Renders the modal shortcuts reference window.
    pub fn render_shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts_dialog {
            return;
        }

        let mut is_open = self.show_shortcuts_dialog;
        let mut close_requested = false;

        egui::Window::new("⌨ Guide des Raccourcis Clavier — RRAW")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .default_width(580.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Raccourcis essentiels pour une navigation et une retouche ultra-fluides.")
                        .italics()
                        .color(Color32::from_rgb(160, 165, 175)),
                );
                ui.add_space(8.0);

                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    render_section(ui, "👁️ Affichage & Chambre Noire", &[
                        ("Y", "Activer / désactiver la comparaison Avant | Après (séparateur déplaçable)"),
                        ("D, P, \\", "Maintenir pour afficher l'image d'origine sans retouche (Before)"),
                        ("J", "Basculer l'alerte d'écrêtage (Bleu = ombres bouchées, Rouge = hautes lumières)"),
                        ("I", "Afficher / masquer le HUD sur l'image (paramètres de prise de vue & note)"),
                        ("F9", "Afficher / masquer le bandeau inférieur de pellicule (Filmstrip)"),
                        ("O", "Afficher / masquer l'incrustation colorée des masques locaux"),
                        ("Double-clic", "Basculer entre vue ajustée (Fit) et vue 1:1 (100%)"),
                        ("Molette", "Zoomer / dézoomer centré sur la position du curseur"),
                        ("Espace + Glisser", "Déplacer l'image (panoramique) en mode zoom"),
                    ]);

                    ui.add_space(10.0);

                    render_section(ui, "🖼️ Navigation & Sélection (Pellicule)", &[
                        ("◀ / ▶", "Photo précédente / suivante dans le dossier"),
                        ("PageUp / PageDown", "Saut rapide de photos"),
                        ("0 à 5", "Attribuer une note de 0 à 5 étoiles (0 pour effacer)"),
                        ("Cmd + A", "Sélectionner / cocher toutes les photos pour l'export par lot"),
                        ("Cmd + D", "Tout désélectionner / décocher le lot d'export"),
                        ("Espace", "Ajouter ou retirer la photo active du lot d'export"),
                        ("Shift + ◀/▶", "Naviguer en étendant la sélection multiple"),
                    ]);

                    ui.add_space(10.0);

                    render_section(ui, "📋 Presse-papiers & Développement par Lot", &[
                        ("Cmd + Shift + C", "Copier le rendu colorimétrique & tonal de la photo"),
                        ("Cmd + Shift + V", "Coller le rendu sur la photo active ou la sélection multiple"),
                        ("Clic droit", "Menu contextuel sur vignette (Copier / Coller / Cocher)"),
                    ]);

                    ui.add_space(10.0);

                    render_section(ui, "📁 Fichiers, Export & Outils", &[
                        ("Cmd + O", "Ouvrir une photo RAW / TIFF"),
                        ("Cmd + E", "Ouvrir la boîte de dialogue d'exportation (JPEG / TIFF / HDR)"),
                        ("Échap", "Désactiver l'outil actif (pinceau, recadrage) ou fermer"),
                        ("F1", "Ouvrir / fermer ce guide des raccourcis clavier"),
                    ]);

                    ui.add_space(10.0);

                    render_section(ui, "🖌️ Retouche Locale & Pinceau", &[
                        ("[ / ]", "Diminuer / augmenter le rayon du pinceau"),
                        ("Alt + Molette", "Ajuster rapidement le rayon du pinceau"),
                        ("Clic gauche", "Peindre un trait de masque sur l'image"),
                        ("Clic droit", "Gommer / effacer des portions du masque"),
                    ]);
                });

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Fermer (Échap)").clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        if close_requested {
            is_open = false;
        }
        self.show_shortcuts_dialog = is_open;
    }
}

fn render_section(ui: &mut egui::Ui, title: &str, items: &[(&str, &str)]) {
    ui.label(
        RichText::new(title)
            .strong()
            .color(Color32::from_rgb(100, 180, 255)),
    );
    ui.add_space(3.0);

    egui::Grid::new(title)
        .num_columns(2)
        .spacing([18.0, 5.0])
        .striped(true)
        .show(ui, |ui| {
            for (key, desc) in items {
                ui.label(
                    RichText::new(*key)
                        .monospace()
                        .strong()
                        .color(Color32::from_rgb(255, 215, 110)),
                );
                ui.label(*desc);
                ui.end_row();
            }
        });
}
