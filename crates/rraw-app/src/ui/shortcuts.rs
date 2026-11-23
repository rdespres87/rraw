#![forbid(unsafe_code)]

//! Keyboard shortcuts cheat sheet dialog.

use egui::{Color32, RichText};

use crate::app::RrawApp;
use crate::i18n::{Key, Language};

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

                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    let is_es = matches!(lang, Language::Custom(code) if &code[..2] == b"es");
                    let (sec_view, sec_nav, sec_batch, sec_files, sec_brush) = if lang == Language::Fr {
                        (
                            &[
                                ("Y", "Activer / désactiver la comparaison Avant | Après (séparateur déplaçable)"),
                                ("D, P, \\", "Maintenir pour afficher l'image d'origine sans retouche (Before)"),
                                ("J", "Basculer l'alerte d'écrêtage (Bleu = ombres bouchées, Rouge = hautes lumières)"),
                                ("I", "Afficher / masquer le HUD sur l'image (paramètres de prise de vue & note)"),
                                ("F9", "Afficher / masquer le bandeau inférieur de pellicule (Filmstrip)"),
                                ("O", "Afficher / masquer l'incrustation colorée des masques locaux"),
                                ("Cmd + 1 / 2 / 3", "Panneau gauche : basculer entre Fichiers / Styles / EXIF"),
                                ("Double-clic", "Basculer entre vue ajustée (Fit) et vue 1:1 (100%)"),
                                ("Molette", "Zoomer / dézoomer centré sur la position du curseur"),
                                ("Espace + Glisser", "Déplacer l'image (panoramique) en mode zoom"),
                            ][..],
                            &[
                                ("◀ / ▶", "Photo précédente / suivante dans le dossier"),
                                ("PageUp / PageDown", "Saut rapide de photos"),
                                ("Home / End", "Première / dernière photo du dossier"),
                                ("0 à 5", "Attribuer une note de 0 à 5 étoiles (0 pour effacer)"),
                                ("Cmd + A", "Sélectionner / cocher toutes les photos pour l'export par lot"),
                                ("Cmd + D", "Tout désélectionner / décocher le lot d'export"),
                                ("Espace", "Ajouter ou retirer la photo active du lot d'export"),
                                ("Shift + ◀/▶", "Naviguer en étendant la sélection multiple"),
                            ][..],
                            &[
                                ("Cmd + Shift + C", "Copier le rendu colorimétrique & tonal de la photo"),
                                ("Cmd + Shift + V", "Coller le rendu sur la photo active ou la sélection multiple"),
                                ("Clic droit", "Menu contextuel sur vignette (Copier / Coller / Cocher)"),
                            ][..],
                            &[
                                ("Cmd + O", "Ouvrir une photo RAW / TIFF"),
                                ("Cmd + Shift + O", "Ouvrir un dossier de photos"),
                                ("Cmd + E", "Ouvrir la boîte de dialogue d'exportation (JPEG / TIFF / HDR)"),
                                ("Entrée", "Lancer l'exportation (dans la boîte d'export)"),
                                ("Échap", "Désactiver l'outil actif (pinceau, recadrage) ou fermer"),
                                ("F1", "Ouvrir / fermer ce guide des raccourcis clavier"),
                            ][..],
                            &[
                                ("[ / ]", "Diminuer / augmenter le rayon du pinceau"),
                                ("Alt + Molette", "Ajuster rapidement le rayon du pinceau"),
                                ("Clic gauche", "Peindre un trait de masque sur l'image"),
                                ("Clic droit", "Gommer / effacer des portions du masque"),
                            ][..],
                        )
                    } else if is_es {
                        (
                            &[
                                ("Y", "Activar / desactivar la vista dividida Antes | Después"),
                                ("D, P, \\", "Mantener para ver la foto original sin retoques (Before)"),
                                ("J", "Alternar advertencia de recorte (Azul = sombras, Rojo = altas luces)"),
                                ("I", "Mostrar / ocultar HUD en imagen (parámetros de disparo y nota)"),
                                ("F9", "Mostrar / ocultar tira de película inferior (Filmstrip)"),
                                ("O", "Mostrar / ocultar superposición de color de máscaras locales"),
                                ("Cmd + 1 / 2 / 3", "Panel izquierdo: alternar entre Archivos / Estilos / EXIF"),
                                ("Doble clic", "Alternar entre vista ajustada (Fit) y vista 1:1 (100%)"),
                                ("Rueda", "Zoom centrado en la posición del cursor"),
                                ("Espacio + Arrastre", "Mover la imagen (panorámica) en modo zoom"),
                            ][..],
                            &[
                                ("◀ / ▶", "Foto anterior / siguiente en la carpeta"),
                                ("PageUp / PageDown", "Salto rápido de fotos"),
                                ("Inicio / Fin", "Primera / última foto de la carpeta"),
                                ("0 a 5", "Asignar calificación de 0 a 5 estrellas (0 para borrar)"),
                                ("Cmd + A", "Seleccionar / marcar todas las fotos para exportación por lote"),
                                ("Cmd + D", "Deseleccionar todas las fotos del lote de exportación"),
                                ("Espacio", "Añadir o quitar la foto activa del lote de exportación"),
                                ("Shift + ◀/▶", "Navegar ampliando la selección múltiple"),
                            ][..],
                            &[
                                ("Cmd + Shift + C", "Copiar ajustes de color y tono de la foto"),
                                ("Cmd + Shift + V", "Pegar ajustes en la foto activa o selección"),
                                ("Clic derecho", "Menú contextual en miniatura (Copiar / Pegar / Marcar)"),
                            ][..],
                            &[
                                ("Cmd + O", "Abrir una foto RAW / TIFF"),
                                ("Cmd + Shift + O", "Abrir carpeta de fotos"),
                                ("Cmd + E", "Abrir el cuadro de diálogo de exportación (JPEG / TIFF / HDR)"),
                                ("Intro", "Iniciar la exportación (en el cuadro de diálogo)"),
                                ("Escape", "Desactivar herramienta activa (pincel, recorte) o cerrar"),
                                ("F1", "Abrir / cerrar esta guía de atajos de teclado"),
                            ][..],
                            &[
                                ("[ / ]", "Reducir / aumentar el radio del pincel"),
                                ("Alt + Rueda", "Ajustar rápidamente el radio del pincel"),
                                ("Clic izquierdo", "Pintar un trazo de máscara en la imagen"),
                                ("Clic derecho", "Borrar porciones de la máscara"),
                            ][..],
                        )
                    } else {
                        (
                            &[
                                ("Y", "Toggle Before | After split comparison (movable divider)"),
                                ("D, P, \\", "Hold to preview untouched camera original (Before)"),
                                ("J", "Toggle clipping warning (Blue = crushed shadows, Red = blown highlights)"),
                                ("I", "Toggle on-canvas HUD (shooting parameters & rating)"),
                                ("F9", "Show / hide bottom thumbnail filmstrip"),
                                ("O", "Toggle colored local mask overlay"),
                                ("Cmd + 1 / 2 / 3", "Left panel: switch between Files / Styles / EXIF"),
                                ("Double-click", "Toggle between Fit view and 1:1 (100%) zoom"),
                                ("Scroll wheel", "Zoom in / out centered on cursor position"),
                                ("Space + Drag", "Pan zoomed image across viewport"),
                            ][..],
                            &[
                                ("◀ / ▶", "Previous / next photo in active folder"),
                                ("PageUp / PageDown", "Fast photo paging jump"),
                                ("0 to 5", "Assign 0 to 5 star rating (0 to clear)"),
                                ("Cmd + A", "Select / check all photos for batch export"),
                                ("Cmd + D", "Deselect / uncheck all photos from export"),
                                ("Space", "Toggle current photo in batch export selection"),
                                ("Shift + ◀/▶", "Extend multi-photo range selection"),
                            ][..],
                            &[
                                ("Cmd + Shift + C", "Copy color & tone grading recipe from active photo"),
                                ("Cmd + Shift + V", "Paste recipe onto active photo or multi-selection"),
                                ("Right click", "Thumbnail context menu (Copy / Paste / Export)"),
                            ][..],
                            &[
                                ("Cmd + O", "Open a RAW or TIFF photo"),
                                ("Cmd + Shift + O", "Open a photo folder"),
                                ("Cmd + E", "Open photo export dialog (JPEG / TIFF / HDR)"),
                                ("Enter", "Start export (in export dialog)"),
                                ("Esc", "Cancel active tool (brush, crop) or close dialog"),
                                ("F1", "Open / close this keyboard shortcuts guide"),
                            ][..],
                            &[
                                ("[ / ]", "Decrease / increase brush radius"),
                                ("Alt + Scroll", "Quickly adjust brush radius"),
                                ("Left click", "Paint mask stroke onto image"),
                                ("Right click", "Erase mask strokes from image"),
                            ][..],
                        )
                    };

                    render_section(ui, "sec_view", self.t(Key::ShortcutsSectionView), sec_view);
                    ui.add_space(10.0);
                    render_section(ui, "sec_nav", self.t(Key::ShortcutsSectionNav), sec_nav);
                    ui.add_space(10.0);
                    render_section(ui, "sec_batch", self.t(Key::ShortcutsSectionBatch), sec_batch);
                    ui.add_space(10.0);
                    render_section(ui, "sec_files", self.t(Key::ShortcutsSectionFiles), sec_files);
                    ui.add_space(10.0);
                    render_section(ui, "sec_brush", self.t(Key::ShortcutsSectionBrush), sec_brush);
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
        self.export.show_shortcuts_dialog = is_open;
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
