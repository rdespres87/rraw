#![forbid(unsafe_code)]

use rraw_app::i18n::{
    batch_deselection_message, batch_selection_count_message, batch_toggle_photo_message, Key,
    Language,
};

#[test]
fn test_new_escape_keys_translated_in_all_catalogs() {
    let keys = [
        Key::EscapeCropDeactivated,
        Key::EscapeHorizonDeactivated,
        Key::EscapePerspectiveDeactivated,
        Key::EscapeBrushDeactivated,
        Key::EscapeSelectionReset,
    ];

    for &key in &keys {
        let fr = key.translate(Language::Fr);
        let en = key.translate(Language::En);
        let es = key.translate(Language::custom("es", "Español"));

        assert!(!fr.is_empty());
        assert!(!en.is_empty());
        assert!(!es.is_empty());

        assert_ne!(fr, en);
    }
}

#[test]
fn test_batch_selection_messages_localization() {
    let fr_sel = batch_selection_count_message(Language::Fr, 2);
    let en_sel = batch_selection_count_message(Language::En, 2);
    assert!(fr_sel.contains("Sélection : 2 photos"));
    assert!(en_sel.contains("Selection: 2 photos"));

    let fr_desel = batch_deselection_message(Language::Fr, 3);
    let en_desel = batch_deselection_message(Language::En, 3);
    assert!(fr_desel.contains("Désélection : 3 photos"));
    assert!(en_desel.contains("Deselection: 3 photos"));

    let fr_empty_desel = batch_deselection_message(Language::Fr, 0);
    let en_empty_desel = batch_deselection_message(Language::En, 0);
    assert!(fr_empty_desel.contains("aucune photo cochée"));
    assert!(en_empty_desel.contains("no photos checked"));

    let fr_toggle_add = batch_toggle_photo_message(Language::Fr, true, 5);
    let en_toggle_add = batch_toggle_photo_message(Language::En, true, 5);
    assert!(fr_toggle_add.contains("Photo ajoutée au lot d'export (5 au total)"));
    assert!(en_toggle_add.contains("Photo added to export batch (5 total)"));

    let fr_toggle_rem = batch_toggle_photo_message(Language::Fr, false, 4);
    let en_toggle_rem = batch_toggle_photo_message(Language::En, false, 4);
    assert!(fr_toggle_rem.contains("Photo retirée du lot d'export (reste 4)"));
    assert!(en_toggle_rem.contains("Photo removed from export batch (4 remaining)"));
}
