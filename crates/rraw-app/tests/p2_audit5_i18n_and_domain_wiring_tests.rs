#![forbid(unsafe_code)]

//! TDD integration tests for i18n completeness and domain crate wiring:
//! - 100% residual French string localization across explorer, EXIF, filmstrip, left sidebar, and top bar.
//! - Domain crate feature wiring into `rraw-app`:
//!   - `rraw-optics::execute_richardson_lucy` capture sharpening deconvolution in `PreparedGradingFrame`.
//!   - `rraw-export` ISO 21496-1 Gain Map HDR metadata calculation and `.hdrgm.xmp` / `.tmap` emission in export.

use std::error::Error;
use std::sync::Arc;

use rraw_app::i18n::{
    exif_copied_clipboard_status, exif_exported_to_status, exif_exposure_bias_label,
    exif_no_matching_tags, explorer_browse_folder_button, explorer_no_folder_open,
    explorer_parent_folder_label, explorer_parent_folder_tooltip, filmstrip_check_active_button,
    filmstrip_check_active_tooltip, filmstrip_clipboard_prefix, filmstrip_export_batch_count,
    filmstrip_export_selection_button, filmstrip_export_selection_tooltip, filmstrip_filter_all,
    filmstrip_filter_label, filmstrip_next_photo_tooltip, filmstrip_paste_tooltip,
    filmstrip_prev_photo_tooltip, filmstrip_raw_only_tooltip, filmstrip_shortcuts_hint,
    filmstrip_toggle_batch_label, left_panel_tab_labels, soft_proof_out_of_gamut_badge, Language,
};
use rraw_app::preview::{
    render_export_image_with_depth, render_preview_from_base_profiled, BaseRgbPreview,
};
use rraw_app::state::{DarkroomGradingState, ExportBitDepth, ExportFormatChoice, ExportScaleMode};
use rraw_app::ui::export::ExportWriteConfig;
use rraw_app::RrawApp;

/// Verifies that all explorer, EXIF, filmstrip, left sidebar, and top-bar Soft Proofing
/// strings are 100% localized in English and Spanish with zero residual French words.
#[test]
fn test_p2_1_explorer_exif_filmstrip_and_topbar_complete_i18n_localization(
) -> Result<(), Box<dyn Error>> {
    let forbidden_french_fragments = [
        "Dossier",
        "Remonter",
        "Aucun",
        "Parcourir",
        "Biais d'exposition",
        "Métadonnées",
        "presse-papier",
        "Filtre :",
        "Toutes (",
        "Masquer les fichiers",
        "Lot d'export",
        "cochée",
        "Cocher",
        "Décocher",
        "Exporter sélection",
        "Flèche",
        "Raccourcis :",
        "Presse-papiers",
        "Hors-Gamut",
        "Fichiers",
    ];

    for lang in [Language::En, Language::custom("es", "Español")] {
        let (tab1, tab2, tab3) = left_panel_tab_labels(lang);
        let samples = vec![
            explorer_parent_folder_label(lang).to_string(),
            explorer_parent_folder_tooltip(lang, "/Users/photos"),
            explorer_no_folder_open(lang).to_string(),
            explorer_browse_folder_button(lang).to_string(),
            exif_exposure_bias_label(lang).to_string(),
            exif_copied_clipboard_status(lang).to_string(),
            exif_exported_to_status(lang, "/tmp/sample.exif.json"),
            exif_no_matching_tags(lang).to_string(),
            filmstrip_filter_label(lang).to_string(),
            filmstrip_filter_all(lang, 12),
            filmstrip_raw_only_tooltip(lang).to_string(),
            filmstrip_export_batch_count(lang, 0),
            filmstrip_export_batch_count(lang, 1),
            filmstrip_export_batch_count(lang, 4),
            filmstrip_check_active_button(lang).to_string(),
            filmstrip_check_active_tooltip(lang).to_string(),
            filmstrip_export_selection_button(lang, 4),
            filmstrip_export_selection_tooltip(lang).to_string(),
            filmstrip_paste_tooltip(lang, "AsShot · +0.00 EV · 5500K"),
            filmstrip_next_photo_tooltip(lang).to_string(),
            filmstrip_prev_photo_tooltip(lang).to_string(),
            filmstrip_shortcuts_hint(lang).to_string(),
            filmstrip_clipboard_prefix(lang, "AsShot"),
            filmstrip_toggle_batch_label(lang, true).to_string(),
            filmstrip_toggle_batch_label(lang, false).to_string(),
            tab1.to_string(),
            tab2.to_string(),
            tab3.to_string(),
            soft_proof_out_of_gamut_badge(lang, 42),
        ];

        for text in &samples {
            assert!(!text.is_empty());
            for fr in forbidden_french_fragments {
                assert!(
                    !text.contains(fr),
                    "Language {lang:?} produced string {text:?} containing residual French fragment {fr:?}"
                );
            }
        }
    }

    // Verify French baseline returns expected French labels
    assert!(explorer_parent_folder_label(Language::Fr).contains("Dossier parent"));
    assert!(filmstrip_filter_label(Language::Fr).contains("Filtre"));
    assert!(soft_proof_out_of_gamut_badge(Language::Fr, 7).contains("Hors-Gamut"));

    Ok(())
}

/// Verifies that `rraw_optics::execute_richardson_lucy` capture sharpening is wired into
/// `PreparedGradingFrame` when `capture_sharpening_amount > 0.5`, sharpening blurred edges and
/// maintaining 100% mathematical parity between interactive preview and export.
#[test]
fn test_p2_2_richardson_lucy_capture_sharpening_wired_in_preview_and_export(
) -> Result<(), Box<dyn Error>> {
    let width = 24_usize;
    let height = 24_usize;
    let num_pixels = width * height;
    let mut samples_rgb = Vec::with_capacity(num_pixels);
    for _y in 0..height {
        for x in 0..width {
            // Soft ramp across columns 10..14 simulating optical PSF blur on a step edge
            let v = if x < 10 {
                0.20_f32
            } else if x >= 14 {
                0.80_f32
            } else {
                0.20 + ((x - 10) as f32 / 4.0) * 0.60
            };
            samples_rgb.push([v, v, v]);
        }
    }

    let base = BaseRgbPreview {
        width,
        height,
        samples_rgb: Arc::from(samples_rgb),
        as_shot_wb: [1.0, 1.0, 1.0],
        cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
            width,
            height,
            subject: vec![0.0; num_pixels],
            sky: vec![0.0; num_pixels],
            background: vec![0.0; num_pixels],
            portrait: vec![0.0; num_pixels],
        }),
        exif_orientation: 1,
        matched_lens: None,
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(2.8),
    };

    let mut neutral = DarkroomGradingState::untouched_raw();
    neutral.denoise_sharpen.capture_sharpening_amount = 0.5;

    let mut sharpened = DarkroomGradingState::untouched_raw();
    sharpened.denoise_sharpen.capture_sharpening_amount = 1.75;

    let (prev_neutral, _, _) = render_preview_from_base_profiled(&base, &neutral);
    let (prev_sharpened, _, _) = render_preview_from_base_profiled(&base, &sharpened);

    assert_ne!(
        prev_neutral.pixels, prev_sharpened.pixels,
        "Active capture_sharpening_amount (1.75) must apply Richardson-Lucy deconvolution"
    );

    // Measure contrast span across the transition zone (x = 10 vs x = 13 at row y = 12)
    let idx_low = 12 * width + 10;
    let idx_high = 12 * width + 13;
    let span_neutral =
        (prev_neutral.pixels[idx_high].r() as i32) - (prev_neutral.pixels[idx_low].r() as i32);
    let span_sharpened =
        (prev_sharpened.pixels[idx_high].r() as i32) - (prev_sharpened.pixels[idx_low].r() as i32);
    assert!(
        span_sharpened > span_neutral,
        "Richardson-Lucy capture sharpening must increase edge acutance (neutral={span_neutral}, sharpened={span_sharpened})"
    );

    // Verify 100% bit-exact parity between preview and 8-bit export with RL sharpening active
    let (out_w, out_h, export_buf) =
        render_export_image_with_depth(&base, &sharpened, 1.0, ExportBitDepth::Bits8);
    assert_eq!((out_w, out_h), (width, height));
    let export_u8 = export_buf
        .as_u8_slice()
        .ok_or("Expected U8 export raster buffer")?;
    for (i, px) in prev_sharpened.pixels.iter().enumerate() {
        assert_eq!(
            [px.r(), px.g(), px.b()],
            [export_u8[i * 3], export_u8[i * 3 + 1], export_u8[i * 3 + 2]],
            "Preview and export pixel {i} must be bit-identical with Richardson-Lucy active"
        );
    }

    Ok(())
}

/// Verifies that enabling ISO 21496-1 Gain Map HDR in `ExportWriteConfig` computes the
/// gain map via `rraw_export::GainMapCalculator` and emits both the `.hdrgm.xmp` XMP packet and
/// the 40-byte `.tmap` binary ISOBMFF box alongside the exported image.
#[test]
fn test_p2_2_iso21496_hdr_gain_map_export_wiring() -> Result<(), Box<dyn Error>> {
    let unique_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("rraw_audit5_p2_gainmap_{unique_nanos}"));
    std::fs::create_dir_all(&temp_dir)?;

    let target_path = temp_dir.join("hdr_test_frame.png");
    let out_w = 8_usize;
    let out_h = 8_usize;
    let num_samples = out_w * out_h * 3;
    let mut u8_bytes = Vec::with_capacity(num_samples);
    let mut u16_samples = Vec::with_capacity(num_samples);
    for i in 0..(out_w * out_h) {
        let is_highlight = i >= (out_w * out_h) / 2;
        let v8 = if is_highlight { 250_u8 } else { 90_u8 };
        let v16 = if is_highlight { 64000_u16 } else { 18000_u16 };
        u8_bytes.extend_from_slice(&[v8, v8, v8]);
        u16_samples.extend_from_slice(&[v16, v16, v16]);
    }

    let config = ExportWriteConfig {
        format: ExportFormatChoice::Png,
        jpeg_quality: 92,
        bit_depth: ExportBitDepth::Bits16,
        scale_mode: ExportScaleMode::Original,
        hdr_gain_map: true,
    };

    RrawApp::write_exported_image_static(
        config,
        &target_path,
        out_w,
        out_h,
        &u8_bytes,
        &u16_samples,
    )?;

    assert!(target_path.exists(), "Primary exported image must exist");

    let xmp_path = target_path.with_extension("hdrgm.xmp");
    let tmap_path = target_path.with_extension("tmap");
    assert!(
        xmp_path.exists(),
        "ISO 21496-1 XMP packet sidecar (.hdrgm.xmp) must be emitted when hdr_gain_map is enabled"
    );
    assert!(
        tmap_path.exists(),
        "ISO 21496-1 binary tmap box (.tmap) must be emitted when hdr_gain_map is enabled"
    );

    let xmp_content = std::fs::read_to_string(&xmp_path)?;
    let decoded_xmp = rraw_export::Iso21496XmpPacket::decode(&xmp_content)?;
    let tmap_bytes = std::fs::read(&tmap_path)?;
    assert_eq!(
        tmap_bytes.len(),
        rraw_export::Iso21496TmapBox::BOX_SIZE as usize
    );
    let decoded_tmap = rraw_export::Iso21496TmapBox::decode(&tmap_bytes)?;

    assert!(
        (decoded_xmp.gain_map_max - decoded_tmap.gain_map_max).abs() < 1e-4,
        "Decoded XMP and tmap GainMapMax must match"
    );
    assert!(decoded_tmap.gain_map_max >= 1.0);

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

#[test]
fn test_rraw_app_export_runner_writes_jxl_8bit_and_16bit() -> Result<(), Box<dyn std::error::Error>>
{
    let temp_dir = std::env::temp_dir().join(format!("rraw_app_jxl_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)?;

    assert_eq!(ExportFormatChoice::Jxl.extension(), "jxl");
    assert_eq!(ExportFormatChoice::Jxl.label(), "JPEG XL (.jxl)");

    let out_w = 16;
    let out_h = 16;
    let u8_bytes = vec![128_u8; out_w * out_h * 3];
    let u16_samples = vec![32768_u16; out_w * out_h * 3];

    for (name, bit_depth) in [
        ("out8.jxl", ExportBitDepth::Bits8),
        ("out16.jxl", ExportBitDepth::Bits16),
    ] {
        let target_path = temp_dir.join(name);
        let config = ExportWriteConfig {
            format: ExportFormatChoice::Jxl,
            jpeg_quality: 92,
            bit_depth,
            scale_mode: ExportScaleMode::Original,
            hdr_gain_map: false,
        };
        RrawApp::write_exported_image_static(
            config,
            &target_path,
            out_w,
            out_h,
            &u8_bytes,
            &u16_samples,
        )?;
        let bytes = std::fs::read(&target_path)?;
        assert!(bytes.len() > 16);
        assert_eq!(&bytes[0..2], &[0xFF, 0x0A]);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}
