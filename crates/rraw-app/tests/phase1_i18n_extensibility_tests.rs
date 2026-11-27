#![forbid(unsafe_code)]

//! TDD integration tests: 100% dynamic catalog-driven internationalization.
//!
//! Verifies that adding a brand-new external JSON language catalog (e.g. German `de_ext`)
//! translates every UI message, preset, shortcut, HUD badge, and status string without
//! requiring any Rust code changes or falling back to hardcoded `if lang == Language::Fr` branches.

use rraw_app::i18n::{
    batch_deselection_message, batch_selection_count_message, batch_toggle_photo_message,
    exif_exported_to_status, explorer_parent_folder_tooltip, filmstrip_clipboard_prefix,
    filmstrip_export_batch_count, filmstrip_export_selection_button, filmstrip_filter_all,
    filmstrip_paste_tooltip, filmstrip_toggle_batch_label, left_panel_tab_labels, tc, Catalog,
    I18nEngine, Language,
};
use rraw_app::presets::GradingPreset;

#[test]
fn test_dynamic_new_language_overrides_all_formerly_hardcoded_strings(
) -> Result<(), Box<dyn std::error::Error>> {
    let engine = I18nEngine::global();
    let de = Language::custom("de_ext", "Deutsch");

    let de_json = r#"{
        "_language_name": "Deutsch",
        "Batch.SelectionCount.Singular": "Auswahl: {count} Foto für den Export markiert",
        "Batch.SelectionCount.Plural": "Auswahl: {count} Fotos für den Export markiert",
        "Batch.Deselection.Singular": "Abwahl: {count} Foto aus dem Export-Stapel entfernt",
        "Batch.Deselection.Plural": "Abwahl: {count} Fotos aus dem Export-Stapel entfernt",
        "Batch.Deselection.Empty": "Export-Auswahl: keine Fotos markiert",
        "Batch.Toggle.Added": "Foto zum Export-Stapel hinzugefügt ({count} gesamt)",
        "Batch.Toggle.Removed": "Foto aus dem Export-Stapel entfernt ({count} verbleibend)",
        "Explorer.ParentFolderTooltip": "Nach oben zu {path}",
        "Exif.ExportedToStatus": "EXIF exportiert nach {path}",
        "Filmstrip.FilterAll": "Alle ({count})",
        "Filmstrip.ExportBatchCount.Zero": "Export-Stapel: 0 markiert",
        "Filmstrip.ExportBatchCount.Singular": "Export-Stapel: ✓ {count} markiert",
        "Filmstrip.ExportBatchCount.Plural": "Export-Stapel: ✓ {count} markiert",
        "Filmstrip.ExportSelectionButton": "📤 Auswahl exportieren ({count})",
        "Filmstrip.PasteTooltip": "Bearbeitung ({summary}) auf Auswahl oder aktives Foto einfügen (Cmd+V)",
        "Filmstrip.ClipboardPrefix": "Zwischenablage: {summary}",
        "Filmstrip.ToggleBatch.Uncheck": "✕ Vom Export-Stapel abwählen",
        "Filmstrip.ToggleBatch.Check": "✓ Für Export-Stapel markieren",
        "LeftPanel.TabFiles": "📁 Dateien",
        "LeftPanel.TabStyles": "🎨 Stile",
        "LeftPanel.TabExif": "ℹ️ EXIF",
        "Undo.Status": "↶ Änderung rückgängig gemacht",
        "Redo.Status": "↷ Änderung wiederhergestellt",
        "TopBar.UndoTooltip": "Letzte Änderung rückgängig machen (Cmd+Z)",
        "TopBar.RedoTooltip": "Rückgängig gemachte Änderung wiederherstellen (Cmd+Y)",
        "Export.PreparingStatus": "Export wird vorbereitet...",
        "Histogram.Blacks": "Schwarz",
        "Inspector.ActiveModuleTooltip": "Dieses Modul enthält aktive benutzerdefinierte Anpassungen",
        "Viewport.DecodingStatus": "RAW-Datei wird im Hintergrund dekodiert...",
        "Hud.Brush.Paint": "Malen 🖌️",
        "Shortcuts.View.SplitCompare": "Vorher | Nachher-Vergleich umschalten",
        "Preset.KodakTriX400.Name": "Kodak Tri-X 400 (S/W)",
        "Preset.KodakTriX400.Desc": "Klassisches Schwarzweiß für Fotojournalismus"
    }"#;

    let catalog = Catalog::from_json_str(de, de_json, Some(&engine.catalog(Language::En)))?;
    engine.register_catalog(catalog);

    // 1. Batch messages
    assert_eq!(
        batch_selection_count_message(de, 1),
        "Auswahl: 1 Foto für den Export markiert"
    );
    assert_eq!(
        batch_selection_count_message(de, 3),
        "Auswahl: 3 Fotos für den Export markiert"
    );
    assert_eq!(
        batch_deselection_message(de, 0),
        "Export-Auswahl: keine Fotos markiert"
    );
    assert_eq!(
        batch_deselection_message(de, 2),
        "Abwahl: 2 Fotos aus dem Export-Stapel entfernt"
    );
    assert_eq!(
        batch_toggle_photo_message(de, true, 5),
        "Foto zum Export-Stapel hinzugefügt (5 gesamt)"
    );
    assert_eq!(
        batch_toggle_photo_message(de, false, 4),
        "Foto aus dem Export-Stapel entfernt (4 verbleibend)"
    );

    // 2. Explorer, EXIF, Filmstrip, Left Panel
    assert_eq!(
        explorer_parent_folder_tooltip(de, "/Photos"),
        "Nach oben zu /Photos"
    );
    assert_eq!(
        exif_exported_to_status(de, "/tmp/a.exif.json"),
        "EXIF exportiert nach /tmp/a.exif.json"
    );
    assert_eq!(filmstrip_filter_all(de, 12), "Alle (12)");
    assert_eq!(
        filmstrip_export_batch_count(de, 0),
        "Export-Stapel: 0 markiert"
    );
    assert_eq!(
        filmstrip_export_batch_count(de, 2),
        "Export-Stapel: ✓ 2 markiert"
    );
    assert_eq!(
        filmstrip_export_selection_button(de, 4),
        "📤 Auswahl exportieren (4)"
    );
    assert_eq!(
        filmstrip_paste_tooltip(de, "AsShot"),
        "Bearbeitung (AsShot) auf Auswahl oder aktives Foto einfügen (Cmd+V)"
    );
    assert_eq!(
        filmstrip_clipboard_prefix(de, "AsShot"),
        "Zwischenablage: AsShot"
    );
    assert_eq!(
        filmstrip_toggle_batch_label(de, true),
        "✕ Vom Export-Stapel abwählen"
    );
    assert_eq!(
        filmstrip_toggle_batch_label(de, false),
        "✓ Für Export-Stapel markieren"
    );
    assert_eq!(
        left_panel_tab_labels(de),
        ("📁 Dateien", "🎨 Stile", "ℹ️ EXIF")
    );

    // 3. Direct catalog keys & presets
    assert_eq!(tc(de, "Undo.Status"), "↶ Änderung rückgängig gemacht");
    assert_eq!(tc(de, "Redo.Status"), "↷ Änderung wiederhergestellt");
    assert_eq!(
        tc(de, "TopBar.UndoTooltip"),
        "Letzte Änderung rückgängig machen (Cmd+Z)"
    );
    assert_eq!(
        tc(de, "TopBar.RedoTooltip"),
        "Rückgängig gemachte Änderung wiederherstellen (Cmd+Y)"
    );
    assert_eq!(
        tc(de, "Export.PreparingStatus"),
        "Export wird vorbereitet..."
    );
    assert_eq!(tc(de, "Histogram.Blacks"), "Schwarz");
    assert_eq!(
        tc(de, "Inspector.ActiveModuleTooltip"),
        "Dieses Modul enthält aktive benutzerdefinierte Anpassungen"
    );
    assert_eq!(
        tc(de, "Viewport.DecodingStatus"),
        "RAW-Datei wird im Hintergrund dekodiert..."
    );
    assert_eq!(tc(de, "Hud.Brush.Paint"), "Malen 🖌️");
    assert_eq!(
        tc(de, "Shortcuts.View.SplitCompare"),
        "Vorher | Nachher-Vergleich umschalten"
    );
    assert_eq!(
        GradingPreset::KodakTriX400.name_lang(de),
        "Kodak Tri-X 400 (S/W)"
    );
    assert_eq!(
        GradingPreset::KodakTriX400.description_lang(de),
        "Klassisches Schwarzweiß für Fotojournalismus"
    );

    Ok(())
}

#[test]
fn test_all_builtin_catalogs_define_custom_ui_keys_without_fallback_to_key_name() {
    let es = Language::custom("es", "Español");
    let required_keys = [
        "Undo.Status",
        "Redo.Status",
        "TopBar.UndoTooltip",
        "TopBar.RedoTooltip",
        "Export.BatchSuccess",
        "Export.TimelapseSuccess",
        "Export.TimelapseSequence",
        "Export.TimelapseDuration",
        "Export.TimelapseFramesSuffix",
        "Export.PreparingStatus",
        "Histogram.Blacks",
        "Histogram.Shadows",
        "Histogram.Exposure",
        "Histogram.Highlights",
        "Histogram.HoverTooltip",
        "Inspector.ActiveModuleTooltip",
        "SoftProof.OutOfGamutPixels",
        "SoftProof.InGamutStatus",
        "Viewport.DefaultMaskName",
        "Viewport.DecodingStatus",
        "Viewport.EmptyStatus",
        "Hud.LocalMaskOverlay",
        "Hud.Brush.Erase",
        "Hud.Brush.Paint",
        "Hud.Brush.ZoomSuffix",
        "Hud.Brush.Banner",
        "Batch.SelectionCount.Singular",
        "Batch.SelectionCount.Plural",
        "Batch.Deselection.Singular",
        "Batch.Deselection.Plural",
        "Batch.Deselection.Empty",
        "Batch.Toggle.Added",
        "Batch.Toggle.Removed",
        "Explorer.ParentFolder",
        "Explorer.ParentFolderTooltip",
        "Explorer.NoFolderOpen",
        "Explorer.BrowseFolder",
        "Exif.ExposureBiasLabel",
        "Exif.CopiedClipboardStatus",
        "Exif.ExportedToStatus",
        "Exif.NoMatchingTags",
        "Filmstrip.FilterLabel",
        "Filmstrip.FilterAll",
        "Filmstrip.RawOnlyTooltip",
        "Filmstrip.ExportBatchCount.Zero",
        "Filmstrip.ExportBatchCount.Singular",
        "Filmstrip.ExportBatchCount.Plural",
        "Filmstrip.CheckActiveButton",
        "Filmstrip.CheckActiveTooltip",
        "Filmstrip.ExportSelectionButton",
        "Filmstrip.ExportSelectionTooltip",
        "Filmstrip.PasteTooltip",
        "Filmstrip.NextPhotoTooltip",
        "Filmstrip.PrevPhotoTooltip",
        "Filmstrip.ShortcutsHint",
        "Filmstrip.ClipboardPrefix",
        "Filmstrip.ToggleBatch.Uncheck",
        "Filmstrip.ToggleBatch.Check",
        "LeftPanel.TabFiles",
        "LeftPanel.TabStyles",
        "LeftPanel.TabExif",
        "Export.HdrGainMapCheckbox",
        "Export.HdrGainMapTooltip",
        "SoftProofIntent.Perceptual",
        "SoftProofIntent.RelativeColorimetric",
        "SoftProofIntent.Perceptual.Short",
        "SoftProofIntent.RelativeColorimetric.Short",
        "SoftProofProfile.SrgbIec61966",
        "SoftProofProfile.DisplayP3",
        "SoftProofProfile.AdobeRgb1998",
        "SoftProofProfile.FineArtMattePaper",
        "SoftProofProfile.GlossyBarytaPaper",
        "SoftProofProfile.SrgbIec61966.Short",
        "SoftProofProfile.DisplayP3.Short",
        "SoftProofProfile.AdobeRgb1998.Short",
        "SoftProofProfile.FineArtMattePaper.Short",
        "SoftProofProfile.GlossyBarytaPaper.Short",
        "SoftProofHud.Prefix",
        "SoftProofHud.PaperInk",
        "SoftProofHud.OutOfGamut",
    ];

    for lang in [Language::En, Language::Fr, es] {
        for key in required_keys {
            let val = tc(lang, key);
            assert_ne!(
                val, key,
                "Key {key:?} must be explicitly defined in catalog for {lang:?}"
            );
            assert!(
                !val.trim().is_empty(),
                "Key {key:?} must not be empty in catalog for {lang:?}"
            );
        }
    }
}
