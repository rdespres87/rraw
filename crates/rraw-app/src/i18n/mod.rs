#![forbid(unsafe_code)]

//! Strongly-typed, scalable internationalization (i18n) engine for `rraw-app`.
//!
//! # Architecture & Guarantees
//! - **Zero Runtime Allocation**: All translations return `&'static str` literals with zero heap allocations during the render loop.
//! - **Scalable External Resource-Based Catalogs**: Translations are stored in external JSON catalogs under `assets/i18n/`.
//!   Default catalogs (FR, EN) are bundled at compile time for fully standalone operation.
//! - **Dynamic Zero-Code Language Addition**: New languages (e.g. Spanish, German, Japanese) can be added simply by dropping
//!   a JSON translation file into `assets/i18n/` or user configuration directory without modifying core Rust code.
//! - **Fallback Guarantee**: Incomplete translation files automatically fall back to English with zero runtime panics.
//! - **Reactive O(1) Switching**: UI switches language instantly by mutating `app.layout_config.language`.
//! - **Persistence**: Language selection is saved to `~/.config/rraw/layout.json` via Serde.

pub mod catalog;
pub mod engine;
pub mod exif_map;
pub mod key;
pub mod language;

pub use catalog::Catalog;
pub use engine::I18nEngine;
pub use exif_map::{exif_tag_to_key, exif_val_to_key, translate_exif_tag, translate_exif_val};
pub use key::{Key, ALL_KEYS};
pub use language::Language;

/// Global helper to translate a key for a given language.
#[inline]
#[must_use]
pub fn t(lang: Language, key: Key) -> &'static str {
    key.translate(lang)
}

/// Returns the localized human-readable label for a `SoftProofRenderingIntent`,
/// consulting custom external catalog overrides first before falling back to built-in translations.
#[must_use]
pub fn soft_proof_intent_label_lang(
    intent: rraw_color::SoftProofRenderingIntent,
    lang: Language,
) -> &'static str {
    let custom_key = match intent {
        rraw_color::SoftProofRenderingIntent::Perceptual => "SoftProofIntent.Perceptual",
        rraw_color::SoftProofRenderingIntent::RelativeColorimetric => {
            "SoftProofIntent.RelativeColorimetric"
        }
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (intent, lang) {
        (rraw_color::SoftProofRenderingIntent::Perceptual, Language::Fr) => {
            "Perceptuel (Compression douce)"
        }
        (rraw_color::SoftProofRenderingIntent::Perceptual, _) if is_es => {
            "Perceptual (Compresión suave)"
        }
        (rraw_color::SoftProofRenderingIntent::Perceptual, _) => "Perceptual (Smooth Compression)",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, Language::Fr) => {
            "Colorimétrique Relatif (Exact / Écrêtage)"
        }
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) if is_es => {
            "Colorimétrico Relativo (Exacto / Recorte)"
        }
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) => {
            "Relative Colorimetric (Exact / Clipping)"
        }
    }
}

/// Returns the short compact label for a `SoftProofRenderingIntent` (used in top bar & viewport HUD).
#[must_use]
pub fn soft_proof_intent_short_label_lang(
    intent: rraw_color::SoftProofRenderingIntent,
    lang: Language,
) -> &'static str {
    let custom_key = match intent {
        rraw_color::SoftProofRenderingIntent::Perceptual => "SoftProofIntent.Perceptual.Short",
        rraw_color::SoftProofRenderingIntent::RelativeColorimetric => {
            "SoftProofIntent.RelativeColorimetric.Short"
        }
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (intent, lang) {
        (rraw_color::SoftProofRenderingIntent::Perceptual, Language::Fr) => "Perceptuel",
        (rraw_color::SoftProofRenderingIntent::Perceptual, _) => "Perceptual",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, Language::Fr) => "Relatif",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) if is_es => "Relativo",
        (rraw_color::SoftProofRenderingIntent::RelativeColorimetric, _) => "Relative",
    }
}

/// Returns the localized human-readable label for a `SoftProofProfile`,
/// consulting custom external catalog overrides first before falling back to built-in translations.
#[must_use]
pub fn soft_proof_profile_label_lang(
    profile: rraw_color::SoftProofProfile,
    lang: Language,
) -> &'static str {
    let custom_key = match profile {
        rraw_color::SoftProofProfile::FineArtMattePaper => "SoftProofProfile.FineArtMattePaper",
        rraw_color::SoftProofProfile::GlossyBarytaPaper => "SoftProofProfile.GlossyBarytaPaper",
        rraw_color::SoftProofProfile::SrgbIec61966 => "SoftProofProfile.SrgbIec61966",
        rraw_color::SoftProofProfile::DisplayP3 => "SoftProofProfile.DisplayP3",
        rraw_color::SoftProofProfile::AdobeRgb1998 => "SoftProofProfile.AdobeRgb1998",
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (profile, lang) {
        (rraw_color::SoftProofProfile::SrgbIec61966, _) => "sRGB IEC61966-2.1 (Web / SDR)",
        (rraw_color::SoftProofProfile::DisplayP3, Language::Fr) => {
            "Apple Display P3 (Écran large gamut)"
        }
        (rraw_color::SoftProofProfile::DisplayP3, _) if is_es => {
            "Apple Display P3 (Pantalla amplia gama)"
        }
        (rraw_color::SoftProofProfile::DisplayP3, _) => "Apple Display P3 (Wide-Gamut Display)",
        (rraw_color::SoftProofProfile::AdobeRgb1998, Language::Fr) => {
            "Adobe RGB (1998) (Pré-presse)"
        }
        (rraw_color::SoftProofProfile::AdobeRgb1998, _) if is_es => {
            "Adobe RGB (1998) (Preimpresión)"
        }
        (rraw_color::SoftProofProfile::AdobeRgb1998, _) => "Adobe RGB (1998) (Prepress)",
        (rraw_color::SoftProofProfile::FineArtMattePaper, Language::Fr) => {
            "Papier Fine Art Mat (Coton / Hahnemühle)"
        }
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) if is_es => {
            "Papel Fine Art Mate (Algodón / Hahnemühle)"
        }
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) => {
            "Fine Art Matte Paper (Cotton / Hahnemühle)"
        }
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, Language::Fr) => {
            "Papier Photo Baryté / Brillant (Glossy)"
        }
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) if is_es => {
            "Papel Fotográfico Baritado / Brillante (Glossy)"
        }
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) => {
            "Glossy / Baryta Photo Paper (Glossy)"
        }
    }
}

/// Returns the short compact badge label for a `SoftProofProfile`.
#[must_use]
pub fn soft_proof_profile_short_label_lang(
    profile: rraw_color::SoftProofProfile,
    lang: Language,
) -> &'static str {
    let custom_key = match profile {
        rraw_color::SoftProofProfile::FineArtMattePaper => {
            "SoftProofProfile.FineArtMattePaper.Short"
        }
        rraw_color::SoftProofProfile::GlossyBarytaPaper => {
            "SoftProofProfile.GlossyBarytaPaper.Short"
        }
        rraw_color::SoftProofProfile::SrgbIec61966 => "SoftProofProfile.SrgbIec61966.Short",
        rraw_color::SoftProofProfile::DisplayP3 => "SoftProofProfile.DisplayP3.Short",
        rraw_color::SoftProofProfile::AdobeRgb1998 => "SoftProofProfile.AdobeRgb1998.Short",
    };
    if let Some(custom_val) = I18nEngine::global().translate_custom(lang, custom_key) {
        return custom_val;
    }

    let is_es = lang.is_spanish();
    match (profile, lang) {
        (rraw_color::SoftProofProfile::SrgbIec61966, _) => "sRGB",
        (rraw_color::SoftProofProfile::DisplayP3, _) => "Display P3",
        (rraw_color::SoftProofProfile::AdobeRgb1998, _) => "Adobe RGB",
        (rraw_color::SoftProofProfile::FineArtMattePaper, Language::Fr) => "Fine Art Mat",
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) if is_es => "Fine Art Mate",
        (rraw_color::SoftProofProfile::FineArtMattePaper, _) => "Fine Art Matte",
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, Language::Fr) => "Photo Baryté",
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) if is_es => "Foto Baritado",
        (rraw_color::SoftProofProfile::GlossyBarytaPaper, _) => "Glossy Baryta",
    }
}

/// Formats the localized viewport Soft Proofing HUD banner string.
#[must_use]
pub fn format_soft_proofing_hud_text(
    soft_proof: &rraw_color::SoftProofConfig,
    out_of_gamut_count: u32,
    lang: Language,
) -> String {
    let engine = I18nEngine::global();
    let is_es = lang.is_spanish();

    let prefix = engine
        .translate_custom(lang, "SoftProofHud.Prefix")
        .unwrap_or(if lang == Language::Fr {
            "🖨️ ÉPREUVAGE"
        } else if is_es {
            "🖨️ PRUEBA DE COLOR"
        } else {
            "🖨️ SOFT PROOF"
        });

    let profile_str = soft_proof_profile_label_lang(soft_proof.profile, lang);
    let intent_str = soft_proof_intent_short_label_lang(soft_proof.intent, lang);

    let paper_str = if soft_proof.simulate_paper_and_ink {
        let paper_label = engine
            .translate_custom(lang, "SoftProofHud.PaperInk")
            .unwrap_or(if lang == Language::Fr {
                "Papier & Encre (Dmin/Dmax)"
            } else if is_es {
                "Papel y Tinta (Dmin/Dmax)"
            } else {
                "Paper & Ink (Dmin/Dmax)"
            });
        format!(" | {paper_label}")
    } else {
        String::new()
    };

    let oog_label = engine
        .translate_custom(lang, "SoftProofHud.OutOfGamut")
        .unwrap_or(if lang == Language::Fr {
            "Hors-Gamut"
        } else if is_es {
            "Fuera de gama"
        } else {
            "Out-of-Gamut"
        });

    format!(
        "{prefix} : {profile_str} ({intent_str}{paper_str}) | {oog_label}: {out_of_gamut_count} px"
    )
}

/// Returns the localized top-bar Out-of-Gamut badge string (`✓ ⚠️ Hors-Gamut (N)`).
#[must_use]
pub fn soft_proof_out_of_gamut_badge(lang: Language, oog_count: u32) -> String {
    let oog_word = I18nEngine::global()
        .translate_custom(lang, "SoftProofHud.OutOfGamut")
        .unwrap_or(if lang == Language::Fr {
            "Hors-Gamut"
        } else if lang.is_spanish() {
            "Fuera de gama"
        } else {
            "Out-of-Gamut"
        });
    format!("✓ ⚠️ {oog_word} ({oog_count})")
}

/// Returns the localized parent folder button label in the explorer sidebar.
#[must_use]
pub fn explorer_parent_folder_label(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Explorer.ParentFolder") {
        return v;
    }
    if lang == Language::Fr {
        "📁 .. (Dossier parent)"
    } else if lang.is_spanish() {
        "📁 .. (Carpeta superior)"
    } else {
        "📁 .. (Parent folder)"
    }
}

/// Returns the localized batch export selection status message.
#[must_use]
pub fn batch_selection_count_message(lang: Language, sel_count: usize) -> String {
    if lang == Language::Fr {
        let plural = if sel_count > 1 { "s" } else { "" };
        format!("Sélection : {sel_count} photo{plural} cochée{plural} pour l'export")
    } else if lang.is_spanish() {
        let plural = if sel_count > 1 { "s" } else { "" };
        format!("Selección: {sel_count} foto{plural} marcada{plural} para exportar")
    } else {
        let plural = if sel_count > 1 { "s" } else { "" };
        format!("Selection: {sel_count} photo{plural} checked for export")
    }
}

/// Returns the localized batch export deselection status message.
#[must_use]
pub fn batch_deselection_message(lang: Language, prev_count: usize) -> String {
    if prev_count > 0 {
        if lang == Language::Fr {
            let plural = if prev_count > 1 { "s" } else { "" };
            format!("Désélection : {prev_count} photo{plural} retirée{plural} du lot d'export")
        } else if lang.is_spanish() {
            let plural = if prev_count > 1 { "s" } else { "" };
            format!(
                "Deselección: {prev_count} foto{plural} eliminada{plural} del lote de exportación"
            )
        } else {
            let plural = if prev_count > 1 { "s" } else { "" };
            format!("Deselection: {prev_count} photo{plural} removed from export batch")
        }
    } else if lang == Language::Fr {
        "Sélection d'exportation : aucune photo cochée".to_string()
    } else if lang.is_spanish() {
        "Selección de exportación: ninguna foto marcada".to_string()
    } else {
        "Export selection: no photos checked".to_string()
    }
}

/// Returns the localized message when adding/removing active photo to export batch.
#[must_use]
pub fn batch_toggle_photo_message(lang: Language, added: bool, total_count: usize) -> String {
    if added {
        if lang == Language::Fr {
            format!("Photo ajoutée au lot d'export ({total_count} au total)")
        } else if lang.is_spanish() {
            format!("Foto añadida al lote de exportación ({total_count} en total)")
        } else {
            format!("Photo added to export batch ({total_count} total)")
        }
    } else if lang == Language::Fr {
        format!("Photo retirée du lot d'export (reste {total_count})")
    } else if lang.is_spanish() {
        format!("Foto eliminada del lote de exportación (quedan {total_count})")
    } else {
        format!("Photo removed from export batch ({total_count} remaining)")
    }
}

/// Returns the localized parent folder hover tooltip in the explorer sidebar.
#[must_use]
pub fn explorer_parent_folder_tooltip(lang: Language, parent_display: &str) -> String {
    if lang == Language::Fr {
        format!("Remonter vers {parent_display}")
    } else if lang.is_spanish() {
        format!("Subir a {parent_display}")
    } else {
        format!("Go up to {parent_display}")
    }
}

/// Returns the localized empty-folder state label in the explorer sidebar.
#[must_use]
pub fn explorer_no_folder_open(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Explorer.NoFolderOpen") {
        return v;
    }
    if lang == Language::Fr {
        "Aucun dossier ouvert."
    } else if lang.is_spanish() {
        "Ninguna carpeta abierta."
    } else {
        "No folder open."
    }
}

/// Returns the localized browse-folder button label in the explorer sidebar.
#[must_use]
pub fn explorer_browse_folder_button(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Explorer.BrowseFolder") {
        return v;
    }
    if lang == Language::Fr {
        "📂 Parcourir un dossier..."
    } else if lang.is_spanish() {
        "📂 Examinar carpeta..."
    } else {
        "📂 Browse folder..."
    }
}

/// Returns the localized EXIF exposure bias label.
#[must_use]
pub fn exif_exposure_bias_label(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Exif.ExposureBiasLabel") {
        return v;
    }
    if lang == Language::Fr {
        "Biais d'exposition :"
    } else if lang.is_spanish() {
        "Compensación de exposición:"
    } else {
        "Exposure Bias:"
    }
}

/// Returns the localized status message when EXIF metadata is copied to the clipboard.
#[must_use]
pub fn exif_copied_clipboard_status(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Exif.CopiedClipboardStatus") {
        return v;
    }
    if lang == Language::Fr {
        "Métadonnées EXIF copiées au presse-papier !"
    } else if lang.is_spanish() {
        "¡Metadatos EXIF copiados al portapapeles!"
    } else {
        "EXIF metadata copied to clipboard!"
    }
}

/// Returns the localized status message when EXIF metadata is exported to a `.exif.json` file.
#[must_use]
pub fn exif_exported_to_status(lang: Language, path_display: &str) -> String {
    if lang == Language::Fr {
        format!("EXIF exporté vers {path_display}")
    } else if lang.is_spanish() {
        format!("EXIF exportado a {path_display}")
    } else {
        format!("EXIF exported to {path_display}")
    }
}

/// Returns the localized label when no EXIF tags match the active filter query.
#[must_use]
pub fn exif_no_matching_tags(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Exif.NoMatchingTags") {
        return v;
    }
    if lang == Language::Fr {
        "Aucun tag correspondant."
    } else if lang.is_spanish() {
        "Ninguna etiqueta coincidente."
    } else {
        "No matching tags."
    }
}

/// Returns the localized filmstrip filter prefix label.
#[must_use]
pub fn filmstrip_filter_label(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.FilterLabel") {
        return v;
    }
    if lang == Language::Fr {
        "Filtre :"
    } else if lang.is_spanish() {
        "Filtro:"
    } else {
        "Filter:"
    }
}

/// Returns the localized filmstrip "All (N)" filter chip text.
#[must_use]
pub fn filmstrip_filter_all(lang: Language, count: usize) -> String {
    if lang == Language::Fr {
        format!("Toutes ({count})")
    } else if lang.is_spanish() {
        format!("Todas ({count})")
    } else {
        format!("All ({count})")
    }
}

/// Returns the localized tooltip for the filmstrip RAW-only filter chip.
#[must_use]
pub fn filmstrip_raw_only_tooltip(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.RawOnlyTooltip") {
        return v;
    }
    if lang == Language::Fr {
        "Masquer les fichiers JPG/PNG pour n'afficher que les fichiers RAW d'origine"
    } else if lang.is_spanish() {
        "Ocultar archivos JPG/PNG para mostrar solo archivos RAW originales"
    } else {
        "Hide JPG/PNG files to show only original RAW files"
    }
}

/// Returns the localized filmstrip batch selection counter label.
#[must_use]
pub fn filmstrip_export_batch_count(lang: Language, sel_count: usize) -> String {
    if lang == Language::Fr {
        if sel_count > 0 {
            format!(
                "Lot d'export : ✓ {sel_count} cochée{}",
                if sel_count > 1 { "s" } else { "" }
            )
        } else {
            "Lot d'export : 0 cochée".to_string()
        }
    } else if lang.is_spanish() {
        if sel_count > 0 {
            format!(
                "Lote de exportación: ✓ {sel_count} marcada{}",
                if sel_count > 1 { "s" } else { "" }
            )
        } else {
            "Lote de exportación: 0 marcadas".to_string()
        }
    } else if sel_count > 0 {
        format!("Export batch: ✓ {sel_count} checked")
    } else {
        "Export batch: 0 checked".to_string()
    }
}

/// Returns the localized filmstrip "+ Check active" button label.
#[must_use]
pub fn filmstrip_check_active_button(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.CheckActiveButton") {
        return v;
    }
    if lang == Language::Fr {
        "+ Cocher active"
    } else if lang.is_spanish() {
        "+ Marcar activa"
    } else {
        "+ Check active"
    }
}

/// Returns the localized tooltip for the filmstrip "+ Check active" button.
#[must_use]
pub fn filmstrip_check_active_tooltip(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.CheckActiveTooltip") {
        return v;
    }
    if lang == Language::Fr {
        "Ajouter la photo courante à la sélection d'export"
    } else if lang.is_spanish() {
        "Añadir la foto actual a la selección de exportación"
    } else {
        "Add current photo to export selection"
    }
}

/// Returns the localized filmstrip "Export selection (N)" button label.
#[must_use]
pub fn filmstrip_export_selection_button(lang: Language, sel_count: usize) -> String {
    if lang == Language::Fr {
        format!("📤 Exporter sélection ({sel_count})")
    } else if lang.is_spanish() {
        format!("📤 Exportar selección ({sel_count})")
    } else {
        format!("📤 Export selection ({sel_count})")
    }
}

/// Returns the localized tooltip for the filmstrip "Export selection" button.
#[must_use]
pub fn filmstrip_export_selection_tooltip(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.ExportSelectionTooltip")
    {
        return v;
    }
    if lang == Language::Fr {
        "Ouvrir la fenêtre d'export pour les photos sélectionnées"
    } else if lang.is_spanish() {
        "Abrir ventana de exportación para las fotos seleccionadas"
    } else {
        "Open export dialog for selected photos"
    }
}

/// Returns the localized tooltip for pasting the copied recipe in the filmstrip.
#[must_use]
pub fn filmstrip_paste_tooltip(lang: Language, clip_summary: &str) -> String {
    if lang == Language::Fr {
        format!("Coller le rendu ({clip_summary}) sur la sélection ou la photo active (Cmd+V)")
    } else if lang.is_spanish() {
        format!("Pegar ajustes ({clip_summary}) en la selección o foto activa (Cmd+V)")
    } else {
        format!("Paste grading ({clip_summary}) onto selection or active photo (Cmd+V)")
    }
}

/// Returns the localized tooltip for the filmstrip next-photo button (`▶`).
#[must_use]
pub fn filmstrip_next_photo_tooltip(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.NextPhotoTooltip") {
        return v;
    }
    if lang == Language::Fr {
        "Photo suivante (Flèche droite)"
    } else if lang.is_spanish() {
        "Foto siguiente (Flecha derecha)"
    } else {
        "Next photo (Right arrow)"
    }
}

/// Returns the localized tooltip for the filmstrip previous-photo button (`◀`).
#[must_use]
pub fn filmstrip_prev_photo_tooltip(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.PrevPhotoTooltip") {
        return v;
    }
    if lang == Language::Fr {
        "Photo précédente (Flèche gauche)"
    } else if lang.is_spanish() {
        "Foto anterior (Flecha izquierda)"
    } else {
        "Previous photo (Left arrow)"
    }
}

/// Returns the localized filmstrip keyboard navigation hint label.
#[must_use]
pub fn filmstrip_shortcuts_hint(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Filmstrip.ShortcutsHint") {
        return v;
    }
    if lang == Language::Fr {
        "Raccourcis : ◀/▶ (Naviguer) | 0-5 (Noter)"
    } else if lang.is_spanish() {
        "Atajos: ◀/▶ (Navegar) | 0-5 (Calificar)"
    } else {
        "Shortcuts: ◀/▶ (Navigate) | 0-5 (Rate)"
    }
}

/// Returns the localized clipboard summary line in the filmstrip context menu.
#[must_use]
pub fn filmstrip_clipboard_prefix(lang: Language, summary: &str) -> String {
    if lang == Language::Fr {
        format!("Presse-papiers : {summary}")
    } else if lang.is_spanish() {
        format!("Portapapeles: {summary}")
    } else {
        format!("Clipboard: {summary}")
    }
}

/// Returns the localized context-menu toggle label for adding/removing a thumbnail from the export batch.
#[must_use]
pub fn filmstrip_toggle_batch_label(lang: Language, is_multi_selected: bool) -> &'static str {
    if is_multi_selected {
        if lang == Language::Fr {
            "✕ Décocher du lot d'export"
        } else if lang.is_spanish() {
            "✕ Desmarcar del lote de exportación"
        } else {
            "✕ Uncheck from export batch"
        }
    } else if lang == Language::Fr {
        "✓ Cocher pour le lot d'export"
    } else if lang.is_spanish() {
        "✓ Marcar para el lote de exportación"
    } else {
        "✓ Check for export batch"
    }
}

/// Returns the localized left sidebar tab labels `(Files, Styles, EXIF)`.
#[must_use]
pub fn left_panel_tab_labels(lang: Language) -> (&'static str, &'static str, &'static str) {
    if lang == Language::Fr {
        ("📁 Fichiers", "🎨 Styles", "ℹ️ EXIF")
    } else if lang.is_spanish() {
        ("📁 Archivos", "🎨 Estilos", "ℹ️ EXIF")
    } else {
        ("📁 Files", "🎨 Styles", "ℹ️ EXIF")
    }
}

/// Returns the localized checkbox label for ISO 21496-1 Gain Map HDR export.
#[must_use]
pub fn export_hdr_gain_map_checkbox_label(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Export.HdrGainMapCheckbox") {
        return v;
    }
    if lang == Language::Fr {
        "Inclure Gain Map HDR (ISO 21496-1 : .hdrgm.xmp + .tmap)"
    } else if lang.is_spanish() {
        "Incluir Gain Map HDR (ISO 21496-1: .hdrgm.xmp + .tmap)"
    } else {
        "Include HDR Gain Map (ISO 21496-1: .hdrgm.xmp + .tmap)"
    }
}

/// Returns the localized tooltip for ISO 21496-1 Gain Map HDR export.
#[must_use]
pub fn export_hdr_gain_map_checkbox_tooltip(lang: Language) -> &'static str {
    if let Some(v) = I18nEngine::global().translate_custom(lang, "Export.HdrGainMapTooltip") {
        return v;
    }
    if lang == Language::Fr {
        "Calcule la carte de gain HDR ISO 21496-1 et génère les métadonnées XMP (hdrgm) et le boîtier binaire ISOBMFF (tmap)"
    } else if lang.is_spanish() {
        "Calcula el mapa de ganancia HDR ISO 21496-1 y genera los metadatos XMP (hdrgm) y la caja binaria ISOBMFF (tmap)"
    } else {
        "Computes the ISO 21496-1 HDR Gain Map and emits XMP (hdrgm) metadata and binary ISOBMFF (tmap) box"
    }
}
