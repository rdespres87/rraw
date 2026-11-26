#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

use rraw_app::app::RrawApp;
use rraw_app::preview::pipeline::clip_saturated_sensor_highlights;
use rraw_app::preview::LoadedPhotoView;
use rraw_app::state::DarkroomGradingState;
use rraw_core::SmartLightingMode;

#[test]
fn test_open_empty_directory_clears_active_photo() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = RrawApp::new_headless();
    app.session.loaded_photo = Some(LoadedPhotoView {
        path: PathBuf::from("/tmp/previous_photo.dng"),
        camera_make: "Test".to_owned(),
        camera_model: "Cam".to_owned(),
        lens_model: String::new(),
        dimensions: (100, 100),
        iso: 100,
        shutter_str: String::new(),
        aperture_str: String::new(),
        focal_length_str: String::new(),
        date_time: String::new(),
        exposure_bias: String::new(),
        rating: 0,
        exif_tags: Vec::new(),
    });

    let empty_dir =
        std::env::temp_dir().join(format!("rraw_empty_dir_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&empty_dir);
    fs::create_dir_all(&empty_dir)?;

    app.open_directory(&empty_dir);

    let _ = fs::remove_dir_all(&empty_dir);
    assert!(app.library.directory_entries.is_empty());
    assert!(
        app.session.loaded_photo.is_none(),
        "opening an empty directory must clear previously displayed photo"
    );
    assert!(app.session.base_preview.is_none());
    assert!(app.session.preview_texture.is_none());
    Ok(())
}

#[test]
fn test_subdirectories_sorted_case_insensitively() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("rraw_subdir_sort_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("Nikon"))?;
    fs::create_dir_all(root.join("Minolta"))?;
    fs::create_dir_all(root.join("moto g8plus"))?;

    let mut app = RrawApp::new_headless();
    app.open_directory(&root);
    let names: Vec<String> = app
        .library
        .cached_subdirectories
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    let _ = fs::remove_dir_all(&root);
    assert_eq!(names, vec!["Minolta", "moto g8plus", "Nikon"]);
    Ok(())
}

#[test]
fn test_clip_saturated_sensor_highlights_neutralizes_magenta_and_preserves_midtones_and_hdr() {
    let wb = [2.2_f32, 1.0_f32, 1.8_f32];

    // Clipped white sensor pixel (1.0, 1.0, 1.0) must become neutral white after WB
    let clipped = clip_saturated_sensor_highlights([1.0, 1.0, 1.0], wb);
    let wb_r = clipped[0] * wb[0];
    let wb_g = clipped[1] * wb[1];
    let wb_b = clipped[2] * wb[2];
    assert!((wb_r - 1.0).abs() < 1e-5);
    assert!((wb_g - 1.0).abs() < 1e-5);
    assert!((wb_b - 1.0).abs() < 1e-5);

    // Sensor clipped above nominal white level (e.g. Nikon Coolpix P7100: 4095/3800 = 1.0776)
    let p7100_wb = [1.088_f32, 1.0_f32, 3.397_f32];
    let p7100_clipped = clip_saturated_sensor_highlights([1.0776, 1.0776, 1.0776], p7100_wb);
    assert!((p7100_clipped[0] * p7100_wb[0] - 1.0).abs() < 1e-4);
    assert!((p7100_clipped[2] * p7100_wb[2] - 1.0).abs() < 1e-4);

    // Midtone pixel untouched
    let mid = [0.3_f32, 0.4_f32, 0.2_f32];
    assert_eq!(clip_saturated_sensor_highlights(mid, wb), mid);

    // Unclipped HDR float pixel (> 1.30) untouched
    let hdr = [2.5_f32, 2.0_f32, 1.8_f32];
    assert_eq!(clip_saturated_sensor_highlights(hdr, wb), hdr);
}

#[test]
fn test_default_smart_lighting_is_slight() {
    let grading = DarkroomGradingState::default();
    assert_eq!(grading.smart_lighting.mode, SmartLightingMode::Slight);
}

#[test]
fn test_base_preview_from_linear_rgb_and_export_fallback() {
    use rraw_app::preview::BaseRgbPreview;
    use rraw_app::state::ExportScaleMode;
    use rraw_core::SignedCfaBuffer;

    let samples = vec![[0.25_f32, 0.5_f32, 0.75_f32]; 16 * 8];
    let full = BaseRgbPreview::from_linear_rgb_with_color_and_target_width(
        &samples,
        16,
        8,
        [1.0, 1.0, 1.0],
        None,
        32,
    );
    assert_eq!(full.width, 16);
    assert_eq!(full.height, 8);
    assert_eq!(full.samples_rgb.len(), 128);
    assert_eq!(full.samples_rgb[0], [0.25, 0.5, 0.75]);

    let scaled = BaseRgbPreview::from_linear_rgb_with_color_and_target_width(
        &samples,
        16,
        8,
        [1.0, 1.0, 1.0],
        None,
        8,
    );
    assert_eq!(scaled.width, 8);
    assert_eq!(scaled.height, 4);
    assert_eq!(scaled.samples_rgb.len(), 32);

    // Export builder with empty CFA (Linear RGB or raster image) must preserve ref_preview
    let empty_cfa = SignedCfaBuffer::default();
    let exported = RrawApp::build_export_base_static(ExportScaleMode::Original, &empty_cfa, &full);
    assert_eq!(exported.width, 16);
    assert_eq!(exported.height, 8);

    // Export builder with full-resolution LinearRgbBuffer must honor full sensor dimensions
    let full_res_rgb = rraw_decode::LinearRgbBuffer::new(32, 16, vec![[0.25, 0.5, 0.75]; 32 * 16]);
    assert!(full_res_rgb.is_ok());
    if let Ok(buf) = full_res_rgb {
        let exported_orig = RrawApp::build_export_base_with_linear_rgb_static(
            ExportScaleMode::Original,
            &empty_cfa,
            Some(&buf),
            &scaled,
        );
        assert_eq!(exported_orig.width, 32);
        assert_eq!(exported_orig.height, 16);

        let exported_half = RrawApp::build_export_base_with_linear_rgb_static(
            ExportScaleMode::Percent50,
            &empty_cfa,
            Some(&buf),
            &scaled,
        );
        assert_eq!(exported_half.width, 16);
        assert_eq!(exported_half.height, 8);
    }
}

#[test]
fn test_decode_fuji_superccd_45_deg_upright_corpus() {
    use rraw_decode::{PureRustRawDecoder, RawDecoder};
    use std::path::Path;

    let samples = [
        "/Users/rdespres/Projets/raw-pixls-samples/Fujifilm/Fujifilm - FinePix S5000 - 4:3.RAF",
        "/Users/rdespres/Projets/raw-pixls-samples/Fujifilm/Fujifilm - FinePix S5600 - 4:3.RAF",
    ];
    let decoder = PureRustRawDecoder::new();

    for sample in samples {
        let path = Path::new(sample);
        if !path.exists() {
            continue;
        }
        let res = decoder.decode_file(path);
        assert!(
            res.is_ok(),
            "SuperCCD sample {} should decode cleanly: {:?}",
            sample,
            res.err()
        );
        if let Ok(decoded) = res {
            assert!(
                decoded.is_linear_rgb(),
                "SuperCCD should output LinearRgbBuffer"
            );
            let w = decoded.metadata.width;
            let h = decoded.metadata.height;
            assert!(
                w > h,
                "SuperCCD 4:3 image should be landscape upright ({w}x{h})"
            );
            let aspect = w as f32 / h as f32;
            assert!(
                (1.2..=1.5).contains(&aspect),
                "SuperCCD aspect ratio should be near 4:3, got {aspect:.3} ({w}x{h})"
            );
            if let Some(rgb) = &decoded.linear_rgb {
                let non_zero = rgb
                    .samples()
                    .iter()
                    .filter(|p| p[0] > 0.01 || p[1] > 0.01 || p[2] > 0.01)
                    .count();
                assert!(non_zero > (w * h) / 4, "SuperCCD image should not be black");
            }
        }
    }
}

#[test]
fn test_decode_canon_powershot_g1_four_color_cygm_corpus() {
    use rraw_decode::{PureRustRawDecoder, RawDecoder};
    use std::path::Path;

    let path = Path::new(
        "/Users/rdespres/Projets/raw-pixls-samples/Canon/Canon - PowerShot G1 - RAW (4:3).crw",
    );
    if !path.exists() {
        return;
    }

    let decoder = PureRustRawDecoder::new();
    let res = decoder.decode_file(path);
    assert!(
        res.is_ok(),
        "Canon PowerShot G1 4-color CYGM should decode cleanly: {:?}",
        res.err()
    );
    if let Ok(decoded) = res {
        assert!(decoded.is_linear_rgb());
        assert_eq!(
            decoded.color_matrix,
            Some(rraw_decode::special_sensors::XYZ_TO_SRGB_D65)
        );
        let cam_to_srgb = rraw_app::preview::compute_cam_to_srgb_matrix(
            decoded.color_matrix.as_ref(),
            &[1.0, 1.0, 1.0],
        );
        let identity = [1.0_f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        for (i, (&got, &exp)) in cam_to_srgb.iter().zip(identity.iter()).enumerate() {
            assert!(
                (got - exp).abs() < 1e-3,
                "cam_to_srgb[{i}] should collapse to identity for pre-calibrated linear sRGB, got {got}, expected {exp}"
            );
        }
        if let Some(rgb) = &decoded.linear_rgb {
            let n = rgb.samples().len() as f64;
            let (mut sr, mut sg, mut sb) = (0.0_f64, 0.0_f64, 0.0_f64);
            for p in rgb.samples() {
                assert!(p[0].is_finite() && p[1].is_finite() && p[2].is_finite());
                sr += f64::from(p[0]);
                sg += f64::from(p[1]);
                sb += f64::from(p[2]);
            }
            let (mr, mg, mb) = (sr / n, sg / n, sb / n);
            assert!(
                mr > 0.02 && mg > 0.02 && mb > 0.02 && mr < 0.85 && mg < 0.85 && mb < 0.85,
                "4-color CYGM output should have balanced RGB means, got ({mr:.4}, {mg:.4}, {mb:.4})"
            );
            assert!(
                mr / mg < 1.5 && mb / mg < 1.5,
                "4-color CYGM foliage scene must not have magenta/blue cast, got ({mr:.4}, {mg:.4}, {mb:.4})"
            );
        }
    }
}

#[test]
fn test_decode_dng_fallback_blackmagic_moto_dji_corpus() {
    use rraw_decode::{PureRustRawDecoder, RawDecoder};
    use std::path::Path;

    let decoder = PureRustRawDecoder::new();

    // 1. Blackmagic 12-bit Lossy CinemaDNG (SOF1 tiles + LinearizationTable)
    let bm_samples = [
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Blackmagic/Blackmagic - Micro Cinema Camera - 12bit (16:9).dng",
            1952,
            1104,
        ),
        (
            "/Users/rdespres/Projets/photo_data/Blackmagic/Blackmagic Pocket Cinema Camera 4k/BMPCC_4k_DCI_3-1.dng",
            4128,
            2176,
        ),
    ];
    for (sample, exp_w, exp_h) in bm_samples {
        let path = Path::new(sample);
        if !path.exists() {
            continue;
        }
        let res = decoder.decode_file(path);
        assert!(
            res.is_ok(),
            "Blackmagic 12-bit Lossy DNG {} should decode via fallback: {:?}",
            sample,
            res.err()
        );
        if let Ok(decoded) = res {
            assert_eq!(decoded.metadata.width, exp_w);
            assert_eq!(decoded.metadata.height, exp_h);
            assert!(!decoded.is_linear_rgb());
            let non_zero = decoded
                .cfa_signed
                .samples()
                .iter()
                .filter(|&&v| v.is_finite() && v > 0.01)
                .count();
            assert!(
                non_zero > (exp_w * exp_h) / 10,
                "Blackmagic CFA should contain non-black signal"
            );
        }
    }

    // 2. Embedded JPEG saved with .dng extension (moto g8plus)
    let moto = Path::new(
        "/Users/rdespres/Projets/raw-pixls-samples/moto g8plus/moto g8plus - moto g8plus.dng",
    );
    if moto.exists() {
        let res = decoder.decode_file(moto);
        assert!(
            res.is_ok(),
            "moto g8plus JPEG-in-DNG should decode via raster fallback: {:?}",
            res.err()
        );
        if let Ok(decoded) = res {
            assert!(decoded.is_linear_rgb());
            assert_eq!(decoded.metadata.width, 1198);
            assert_eq!(decoded.metadata.height, 1600);
        }
    }

    // 3. Uncompressed 16-bit DNG with strip width quirk (DJI / Hasselblad L2D-20c)
    let dji = Path::new("/Users/rdespres/Projets/photo_data/HASSELBLAD/L2D-20c/DJI_0023.DNG");
    if dji.exists() {
        let res = decoder.decode_file(dji);
        assert!(
            res.is_ok(),
            "Hasselblad L2D-20c DNG should decode via uncompressed fallback: {:?}",
            res.err()
        );
        if let Ok(decoded) = res {
            assert!(decoded.metadata.width >= 5200);
            assert!(decoded.metadata.height >= 3900);
        }
    }
}

#[test]
fn test_decode_camera_aliases_monochrome_and_coolscan_corpus() {
    use rraw_decode::{PureRustRawDecoder, RawDecoder};
    use std::path::Path;

    let decoder = PureRustRawDecoder::new();

    // 1. Camera aliases / rebadges (authentic make & model preserved)
    let alias_samples = [
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Leica/Leica - D-LUX 6 - 16:9.RWL",
            "LEICA",
            "D-LUX 6",
        ),
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Minolta/Minolta - DiMAGE 5 - 4:3.MRW",
            "Minolta Co., Ltd.",
            "DiMAGE 5",
        ),
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Hasselblad/Hasselblad - CF132.3FR",
            "Hasselblad",
            " Flash Sync",
        ),
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Phase One/Phase One - P40+ - IIQ S (4:3).IIQ",
            "Phase One A/S",
            "P40+",
        ),
        (
            "/Users/rdespres/Projets/photo_data/HASSELBLAD/Lunar/DSC00343.ARW",
            "HASSELBLAD",
            "Lunar",
        ),
        (
            "/Users/rdespres/Projets/photo_data/SAMSUNG TECHWIN/GX-1L/SG1L5458.PEF",
            "SAMSUNG TECHWIN",
            "GX-1L",
        ),
    ];

    for (sample, exp_make, exp_model) in alias_samples {
        let path = Path::new(sample);
        if !path.exists() {
            continue;
        }
        let res = decoder.decode_file(path);
        assert!(
            res.is_ok(),
            "Alias sample {} should decode cleanly: {:?}",
            sample,
            res.err()
        );
        if let Ok(decoded) = res {
            assert_eq!(decoded.metadata.camera_make, exp_make);
            assert_eq!(decoded.metadata.camera_model, exp_model);
            assert!(decoded.metadata.width > 0 && decoded.metadata.height > 0);
        }
    }

    // 2. True monochrome sensors (CFA-less -> neutral LinearRgbBuffer)
    let mono_samples = [
        "/Users/rdespres/Projets/photo_data/Leica/M Monochrom (Typ 246)/M2462362.DNG",
        "/Users/rdespres/Projets/photo_data/PENTAX/K-3 Mark III Monochrome/K3III.PEF",
    ];
    for sample in mono_samples {
        let path = Path::new(sample);
        if !path.exists() {
            continue;
        }
        let res = decoder.decode_file(path);
        assert!(
            res.is_ok(),
            "Monochrome sample {} should decode cleanly: {:?}",
            sample,
            res.err()
        );
        if let Ok(decoded) = res {
            assert!(decoded.is_linear_rgb());
            if let Some(rgb) = &decoded.linear_rgb {
                for px in rgb.samples().iter().take(64) {
                    assert!(
                        (px[0] - px[1]).abs() < 1e-6 && (px[1] - px[2]).abs() < 1e-6,
                        "monochrome pixels must be strictly neutral gray"
                    );
                }
            }
        }
    }

    // 3. Uncompressed RGB TIFF scanners (Nikon COOLSCAN IV ED 8-bit & V ED 16-bit)
    let coolscan_samples = [
        "/Users/rdespres/Projets/raw-pixls-samples/Nikon/Nikon - Nikon COOLSCAN IV ED - uncompressed (4:3).nef",
        "/Users/rdespres/Projets/raw-pixls-samples/Nikon/Nikon - Nikon COOLSCAN V ED - uncompressed (3:2).nef",
    ];
    for sample in coolscan_samples {
        let path = Path::new(sample);
        if !path.exists() {
            continue;
        }
        let res = decoder.decode_file(path);
        assert!(
            res.is_ok(),
            "Coolscan sample {} should decode cleanly: {:?}",
            sample,
            res.err()
        );
        if let Ok(decoded) = res {
            assert!(decoded.is_linear_rgb());
            assert!(decoded.metadata.width > 0 && decoded.metadata.height > 0);
        }
    }
}

#[test]
fn test_decode_naked_headerless_corpus() {
    use rraw_decode::{PureRustRawDecoder, RawDecoder};
    use std::path::Path;

    let decoder = PureRustRawDecoder::new();
    let samples = [
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Pentax/Pentax - Pentax Optio S4.RAW",
            "Optio S4",
            false,
        ),
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Minolta/Minolta - Dimage Z2.MRW",
            "DiMAGE Z2",
            false,
        ),
        (
            "/Users/rdespres/Projets/raw-pixls-samples/Kodak/Kodak - Kodak C330 - Format 4:2:2 YRGB (4:2).RAW",
            "C330",
            true,
        ),
        (
            "/Users/rdespres/Projets/photo_data/Canon/PowerShot SX710 HS/CANON_SX710HS_CHDK_CRW_0717.CRW",
            "SX710",
            false,
        ),
        (
            "/Users/rdespres/Projets/photo_data/Canon/PowerShot SX20 IS/CRW_6488.CRW",
            "SX20",
            false,
        ),
        (
            "/Users/rdespres/Projets/photo_data/Paralenz/+/IMG_0005.RAW",
            "Dive Camera",
            false,
        ),
        (
            "/Users/rdespres/Projets/photo_data/Xiro/Xplorer V/2017_0908_092308_001.RAW",
            "Xplorer V",
            false,
        ),
        (
            "/Users/rdespres/Projets/photo_data/Kodak/Kodak C603  Kodak C643/KODAK_C603_C643_FORMAT_422_CFAI0001.RAW",
            "C603",
            false,
        ),
    ];

    for (sample, expected_model, expect_linear_rgb) in samples {
        let path = Path::new(sample);
        if !path.exists() {
            continue;
        }
        let res = decoder.decode_file(path);
        assert!(
            res.is_ok(),
            "Headerless RAW {sample} should decode cleanly: {:?}",
            res.err()
        );
        if let Ok(decoded) = res {
            assert!(
                decoded.metadata.camera_model.contains(expected_model),
                "expected model containing '{expected_model}', got '{}'",
                decoded.metadata.camera_model
            );
            assert_eq!(decoded.is_linear_rgb(), expect_linear_rgb);
            assert!(decoded.metadata.width > 0 && decoded.metadata.height > 0);
        }
    }
}
