#![forbid(unsafe_code)]

use std::sync::atomic::Ordering;

#[test]
fn test_background_workers_export_cancel_flag_lifecycle() -> Result<(), Box<dyn std::error::Error>>
{
    let workers = rraw_app::controllers::workers::BackgroundWorkers::new_headless();

    assert!(!workers.is_export_cancelled());

    let token = workers.reset_export_cancel();
    assert!(!token.load(Ordering::Acquire));

    workers.cancel_export();
    assert!(workers.is_export_cancelled());
    assert!(token.load(Ordering::Acquire));

    let token2 = workers.reset_export_cancel();
    assert!(!workers.is_export_cancelled());
    assert!(!token2.load(Ordering::Acquire));

    Ok(())
}

#[test]
fn test_load_cube_lut_from_path_valid_invalid_and_size_limit(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join("rraw_phase27_lut_test");
    std::fs::create_dir_all(&dir)?;

    let valid_path = dir.join("valid.cube");
    let valid_content =
        "TITLE \"Identity\"\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
    std::fs::write(&valid_path, valid_content)?;

    let res = rraw_app::ui::presets::load_cube_lut_from_path(&valid_path);
    assert!(res.is_ok());

    let res_limited = rraw_app::ui::presets::load_cube_lut_from_path_with_limit(&valid_path, 16);
    assert!(res_limited.is_err());

    let invalid_path = dir.join("invalid.cube");
    std::fs::write(&invalid_path, "NOT A LUT")?;

    let res_invalid = rraw_app::ui::presets::load_cube_lut_from_path(&invalid_path);
    assert!(res_invalid.is_err());

    let _ = std::fs::remove_dir_all(&dir);

    Ok(())
}

#[test]
fn test_export_runner_has_zero_residual_french_strings() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_path = manifest_dir.join("src/ui/export_runner.rs");
    let source = std::fs::read_to_string(&source_path)?;

    let forbidden = ["Aucune photo", "Décodage ", "Rendu ", "Analyse deflicker"];

    for s in &forbidden {
        assert!(
            !source.contains(s),
            "export_runner.rs still contains residual French string: {s:?}"
        );
    }

    Ok(())
}

#[test]
fn test_i18n_catalogs_include_export_cancel_keys() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let langs = ["en", "fr", "es"];

    for lang in &langs {
        let path = manifest_dir
            .join("../../assets/i18n")
            .join(format!("{lang}.json"));
        let raw = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&raw)?;
        let table = value.get("strings").unwrap_or(&value);

        let cancel_btn = table
            .get("Export.CancelButton")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            !cancel_btn.is_empty(),
            "lang {lang}: Export.CancelButton is missing or empty"
        );

        let cancelling = table
            .get("Export.CancellingStatus")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            !cancelling.is_empty(),
            "lang {lang}: Export.CancellingStatus is missing or empty"
        );
    }

    Ok(())
}
