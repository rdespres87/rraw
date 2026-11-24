#![forbid(unsafe_code)]

//! Strongly-typed, scalable internationalization (i18n) engine for `rraw-app`.
//!
//! # Architecture & Guarantees
//! - **Zero Runtime Allocation**: All static translations return `&'static str` literals with zero heap allocations during the render loop.
//! - **Scalable External Resource-Based Catalogs**: Translations are stored in external JSON catalogs under `assets/i18n/`.
//!   Default catalogs (FR, EN, ES) are bundled at compile time for fully standalone operation.
//! - **Dynamic Zero-Code Language Addition**: New languages (e.g. German, Japanese) can be added simply by dropping
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
pub use key::{Key, ParseKeyError, ALL_KEYS};
pub use language::Language;

/// Global helper to translate a typed `Key` for a given language.
#[inline]
#[must_use]
pub fn t(lang: Language, key: Key) -> &'static str {
    key.translate(lang)
}

/// Global helper to translate a dynamic dot-namespaced JSON catalog key
/// (e.g. `"Preset.AsShot.Name"`, `"Hud.Brush.Paint"`) with zero heap allocation,
/// falling back to English and finally to the key identifier if missing.
#[inline]
#[must_use]
pub fn tc(lang: Language, custom_key: &'static str) -> &'static str {
    I18nEngine::global()
        .translate_custom(lang, custom_key)
        .unwrap_or(custom_key)
}

/// Returns the localized human-readable label for a `SoftProofRenderingIntent`.
#[must_use]
pub fn soft_proof_intent_label_lang(
    intent: rraw_color::SoftProofRenderingIntent,
    lang: Language,
) -> &'static str {
    let key = match intent {
        rraw_color::SoftProofRenderingIntent::Perceptual => "SoftProofIntent.Perceptual",
        rraw_color::SoftProofRenderingIntent::RelativeColorimetric => {
            "SoftProofIntent.RelativeColorimetric"
        }
    };
    tc(lang, key)
}

/// Returns the short compact label for a `SoftProofRenderingIntent` (used in top bar & viewport HUD).
#[must_use]
pub fn soft_proof_intent_short_label_lang(
    intent: rraw_color::SoftProofRenderingIntent,
    lang: Language,
) -> &'static str {
    let key = match intent {
        rraw_color::SoftProofRenderingIntent::Perceptual => "SoftProofIntent.Perceptual.Short",
        rraw_color::SoftProofRenderingIntent::RelativeColorimetric => {
            "SoftProofIntent.RelativeColorimetric.Short"
        }
    };
    tc(lang, key)
}

/// Returns the localized human-readable label for a `SoftProofProfile`.
#[must_use]
pub fn soft_proof_profile_label_lang(
    profile: rraw_color::SoftProofProfile,
    lang: Language,
) -> &'static str {
    let key = match profile {
        rraw_color::SoftProofProfile::FineArtMattePaper => "SoftProofProfile.FineArtMattePaper",
        rraw_color::SoftProofProfile::GlossyBarytaPaper => "SoftProofProfile.GlossyBarytaPaper",
        rraw_color::SoftProofProfile::SrgbIec61966 => "SoftProofProfile.SrgbIec61966",
        rraw_color::SoftProofProfile::DisplayP3 => "SoftProofProfile.DisplayP3",
        rraw_color::SoftProofProfile::AdobeRgb1998 => "SoftProofProfile.AdobeRgb1998",
    };
    tc(lang, key)
}

/// Returns the short compact badge label for a `SoftProofProfile`.
#[must_use]
pub fn soft_proof_profile_short_label_lang(
    profile: rraw_color::SoftProofProfile,
    lang: Language,
) -> &'static str {
    let key = match profile {
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
    tc(lang, key)
}

/// Formats the localized viewport Soft Proofing HUD banner string.
#[must_use]
pub fn format_soft_proofing_hud_text(
    soft_proof: &rraw_color::SoftProofConfig,
    out_of_gamut_count: u32,
    lang: Language,
) -> String {
    let prefix = tc(lang, "SoftProofHud.Prefix");
    let profile_str = soft_proof_profile_label_lang(soft_proof.profile, lang);
    let intent_str = soft_proof_intent_short_label_lang(soft_proof.intent, lang);

    let paper_str = if soft_proof.simulate_paper_and_ink {
        let paper_label = tc(lang, "SoftProofHud.PaperInk");
        format!(" | {paper_label}")
    } else {
        String::new()
    };

    let oog_label = tc(lang, "SoftProofHud.OutOfGamut");

    format!(
        "{prefix} : {profile_str} ({intent_str}{paper_str}) | {oog_label}: {out_of_gamut_count} px"
    )
}

/// Returns the localized top-bar Out-of-Gamut badge string (`✓ ⚠️ Hors-Gamut (N)`).
#[must_use]
pub fn soft_proof_out_of_gamut_badge(lang: Language, oog_count: u32) -> String {
    let oog_word = tc(lang, "SoftProofHud.OutOfGamut");
    format!("✓ ⚠️ {oog_word} ({oog_count})")
}

/// Returns the localized parent folder button label in the explorer sidebar.
#[must_use]
pub fn explorer_parent_folder_label(lang: Language) -> &'static str {
    tc(lang, "Explorer.ParentFolder")
}

/// Returns the localized batch export selection status message.
#[must_use]
pub fn batch_selection_count_message(lang: Language, sel_count: usize) -> String {
    let tpl = if sel_count > 1 {
        tc(lang, "Batch.SelectionCount.Plural")
    } else {
        tc(lang, "Batch.SelectionCount.Singular")
    };
    tpl.replace("{count}", &sel_count.to_string())
}

/// Returns the localized batch export deselection status message.
#[must_use]
pub fn batch_deselection_message(lang: Language, prev_count: usize) -> String {
    if prev_count == 0 {
        return tc(lang, "Batch.Deselection.Empty").to_string();
    }
    let tpl = if prev_count > 1 {
        tc(lang, "Batch.Deselection.Plural")
    } else {
        tc(lang, "Batch.Deselection.Singular")
    };
    tpl.replace("{count}", &prev_count.to_string())
}

/// Returns the localized message when adding/removing active photo to export batch.
#[must_use]
pub fn batch_toggle_photo_message(lang: Language, added: bool, total_count: usize) -> String {
    let tpl = if added {
        tc(lang, "Batch.Toggle.Added")
    } else {
        tc(lang, "Batch.Toggle.Removed")
    };
    tpl.replace("{count}", &total_count.to_string())
}

/// Returns the localized parent folder hover tooltip in the explorer sidebar.
#[must_use]
pub fn explorer_parent_folder_tooltip(lang: Language, parent_display: &str) -> String {
    tc(lang, "Explorer.ParentFolderTooltip").replace("{path}", parent_display)
}

/// Returns the localized empty-folder state label in the explorer sidebar.
#[must_use]
pub fn explorer_no_folder_open(lang: Language) -> &'static str {
    tc(lang, "Explorer.NoFolderOpen")
}

/// Returns the localized browse-folder button label in the explorer sidebar.
#[must_use]
pub fn explorer_browse_folder_button(lang: Language) -> &'static str {
    tc(lang, "Explorer.BrowseFolder")
}

/// Returns the localized EXIF exposure bias label.
#[must_use]
pub fn exif_exposure_bias_label(lang: Language) -> &'static str {
    tc(lang, "Exif.ExposureBiasLabel")
}

/// Returns the localized status message when EXIF metadata is copied to the clipboard.
#[must_use]
pub fn exif_copied_clipboard_status(lang: Language) -> &'static str {
    tc(lang, "Exif.CopiedClipboardStatus")
}

/// Returns the localized status message when EXIF metadata is exported to a `.exif.json` file.
#[must_use]
pub fn exif_exported_to_status(lang: Language, path_display: &str) -> String {
    tc(lang, "Exif.ExportedToStatus").replace("{path}", path_display)
}

/// Returns the localized label when no EXIF tags match the active filter query.
#[must_use]
pub fn exif_no_matching_tags(lang: Language) -> &'static str {
    tc(lang, "Exif.NoMatchingTags")
}

/// Returns the localized filmstrip filter prefix label.
#[must_use]
pub fn filmstrip_filter_label(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.FilterLabel")
}

/// Returns the localized filmstrip "All (N)" filter chip text.
#[must_use]
pub fn filmstrip_filter_all(lang: Language, count: usize) -> String {
    tc(lang, "Filmstrip.FilterAll").replace("{count}", &count.to_string())
}

/// Returns the localized tooltip for the filmstrip RAW-only filter chip.
#[must_use]
pub fn filmstrip_raw_only_tooltip(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.RawOnlyTooltip")
}

/// Returns the localized filmstrip batch selection counter label.
#[must_use]
pub fn filmstrip_export_batch_count(lang: Language, sel_count: usize) -> String {
    if sel_count == 0 {
        return tc(lang, "Filmstrip.ExportBatchCount.Zero").to_string();
    }
    let tpl = if sel_count > 1 {
        tc(lang, "Filmstrip.ExportBatchCount.Plural")
    } else {
        tc(lang, "Filmstrip.ExportBatchCount.Singular")
    };
    tpl.replace("{count}", &sel_count.to_string())
}

/// Returns the localized filmstrip "+ Check active" button label.
#[must_use]
pub fn filmstrip_check_active_button(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.CheckActiveButton")
}

/// Returns the localized tooltip for the filmstrip "+ Check active" button.
#[must_use]
pub fn filmstrip_check_active_tooltip(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.CheckActiveTooltip")
}

/// Returns the localized filmstrip "Export selection (N)" button label.
#[must_use]
pub fn filmstrip_export_selection_button(lang: Language, sel_count: usize) -> String {
    tc(lang, "Filmstrip.ExportSelectionButton").replace("{count}", &sel_count.to_string())
}

/// Returns the localized tooltip for the filmstrip "Export selection" button.
#[must_use]
pub fn filmstrip_export_selection_tooltip(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.ExportSelectionTooltip")
}

/// Returns the localized tooltip for pasting the copied recipe in the filmstrip.
#[must_use]
pub fn filmstrip_paste_tooltip(lang: Language, clip_summary: &str) -> String {
    tc(lang, "Filmstrip.PasteTooltip").replace("{summary}", clip_summary)
}

/// Returns the localized tooltip for the filmstrip next-photo button (`▶`).
#[must_use]
pub fn filmstrip_next_photo_tooltip(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.NextPhotoTooltip")
}

/// Returns the localized tooltip for the filmstrip previous-photo button (`◀`).
#[must_use]
pub fn filmstrip_prev_photo_tooltip(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.PrevPhotoTooltip")
}

/// Returns the localized filmstrip keyboard navigation hint label.
#[must_use]
pub fn filmstrip_shortcuts_hint(lang: Language) -> &'static str {
    tc(lang, "Filmstrip.ShortcutsHint")
}

/// Returns the localized clipboard summary line in the filmstrip context menu.
#[must_use]
pub fn filmstrip_clipboard_prefix(lang: Language, summary: &str) -> String {
    tc(lang, "Filmstrip.ClipboardPrefix").replace("{summary}", summary)
}

/// Returns the localized context-menu toggle label for adding/removing a thumbnail from the export batch.
#[must_use]
pub fn filmstrip_toggle_batch_label(lang: Language, is_multi_selected: bool) -> &'static str {
    if is_multi_selected {
        tc(lang, "Filmstrip.ToggleBatch.Uncheck")
    } else {
        tc(lang, "Filmstrip.ToggleBatch.Check")
    }
}

/// Returns the localized left sidebar tab labels `(Files, Styles, EXIF)`.
#[must_use]
pub fn left_panel_tab_labels(lang: Language) -> (&'static str, &'static str, &'static str) {
    (
        tc(lang, "LeftPanel.TabFiles"),
        tc(lang, "LeftPanel.TabStyles"),
        tc(lang, "LeftPanel.TabExif"),
    )
}

/// Returns the localized checkbox label for ISO 21496-1 Gain Map HDR export.
#[must_use]
pub fn export_hdr_gain_map_checkbox_label(lang: Language) -> &'static str {
    tc(lang, "Export.HdrGainMapCheckbox")
}

/// Returns the localized tooltip for ISO 21496-1 Gain Map HDR export.
#[must_use]
pub fn export_hdr_gain_map_checkbox_tooltip(lang: Language) -> &'static str {
    tc(lang, "Export.HdrGainMapTooltip")
}
