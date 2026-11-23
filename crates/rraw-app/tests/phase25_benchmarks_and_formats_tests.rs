#![forbid(unsafe_code)]

//! Phase 25 — Pre-Release Performance Benchmarks, Memory Footprint & RAW Format Evaluation.
//!
//! Verifies:
//! 1. End-to-end RAW decoding latency and synthetic stream parsing throughput.
//! 2. Richardson-Lucy PSF capture sharpening multi-core scaling via Rayon.
//! 3. ISO 21496-1 Gain Map calculation and binary `.tmap` / `.hdrgm.xmp` serialization throughput.
//! 4. Memory footprint limits and LRU photo cache saturation bounds (800 MB cap).
//! 5. Multi-format RAW matrix detection (ARW, CR2, CR3, DNG, NEF, RAF, RW2, ORF, PEF) and EXIF orientation parsing.
//! 6. UI preview rendering latency under full grading load against the 120 FPS / 60 FPS interactive budget.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rraw_app::preview::orientation::{is_raw_extension, read_exif_orientation};
use rraw_app::preview::render_preview_from_base_profiled;
use rraw_app::preview::types::{BaseRgbPreview, CachedPhotoEntry, LoadedPhotoView};
use rraw_app::state::DarkroomGradingState;
use rraw_app::RrawApp;
use rraw_core::{BayerOrigin, CfaPattern, PipelineStageId, SignedCfaBuffer};
use rraw_decode::opcodes::{parse_opcode_list, DngOpcode};
use rraw_decode::synthetic::SyntheticRawBuilder;
use rraw_decode::{PureRustRawDecoder, RawDecoder};
use rraw_export::{GainMapCalculator, Iso21496Params, Iso21496TmapBox, Iso21496XmpPacket};
use rraw_optics::{execute_richardson_lucy, RichardsonLucyParams};
use rraw_storage::{DiskThumbnailCache, ThumbnailSize};

#[test]
fn test_bench_raw_decoding_and_synthetic_stream() -> Result<(), Box<dyn Error>> {
    let start_synth = Instant::now();
    let opcode_stream = SyntheticRawBuilder::build_synthetic_opcode_list_stream()?;
    let opcodes = parse_opcode_list(&opcode_stream)?;
    let elapsed_synth = start_synth.elapsed();

    assert_eq!(opcodes.len(), 2);
    assert!(matches!(opcodes[0], DngOpcode::GainMap { .. }));
    assert!(matches!(opcodes[1], DngOpcode::FixVignetteRadial { .. }));
    println!(
        "[BENCH] Synthetic DNG opcode stream build & parse: {:?} (2 opcodes)",
        elapsed_synth
    );

    // If real DNG files exist in test location, benchmark real hardware decode
    let candidates = [
        PathBuf::from("/Users/rdespres/Desktop/test/IMGP4058.DNG"),
        PathBuf::from("/Users/rdespres/Desktop/test/IMGP3879.DNG"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            let decoder = PureRustRawDecoder::new();
            let start_real = Instant::now();
            let decoded = decoder.decode_file(candidate)?;
            let elapsed_real = start_real.elapsed();

            let num_pixels = decoded.cfa_signed.width() * decoded.cfa_signed.height();
            let mpx = num_pixels as f64 / 1_000_000.0;
            let mpx_per_sec = mpx / elapsed_real.as_secs_f64().max(1e-6);

            println!(
                "[BENCH] Real RAW decode ({}): {:.2} MPx in {:?} ({:.1} MPx/s)",
                candidate
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("DNG"),
                mpx,
                elapsed_real,
                mpx_per_sec
            );

            assert!(decoded.cfa_signed.width() > 0);
            assert!(decoded.cfa_signed.height() > 0);
            assert_eq!(decoded.cfa_signed.samples_arc().len(), num_pixels);
            assert!(
                mpx_per_sec > 1.0,
                "RAW decode throughput must exceed 1 MPx/s"
            );
            break;
        }
    }

    Ok(())
}

#[test]
fn test_bench_richardson_lucy_rayon_scaling() -> Result<(), Box<dyn Error>> {
    let width = 512_usize;
    let height = 512_usize;
    let num_pixels = width * height;

    let mut input = Vec::with_capacity(num_pixels * 3);
    for y in 0..height {
        for x in 0..width {
            let v = ((x ^ y) % 256) as f32 / 255.0;
            input.push(v);
            input.push(v * 0.9);
            input.push(v * 0.8);
        }
    }

    let params = RichardsonLucyParams {
        iterations: 5,
        epsilon: 1e-4,
        base_psf_sigma: 1.2,
        deringing_threshold: 0.05,
        noise_profile: rraw_core::NoiseProfile::default(),
    };

    // Warm-up run
    let _ = execute_richardson_lucy(&input, width as u32, height as u32, &params);

    // Profiled run
    let start = Instant::now();
    let sharpened = execute_richardson_lucy(&input, width as u32, height as u32, &params);
    let elapsed = start.elapsed();

    assert_eq!(sharpened.len(), num_pixels * 3);
    let all_finite = sharpened.iter().all(|&v| v.is_finite());
    assert!(
        all_finite,
        "Richardson-Lucy output must be finite (no NaN/Inf)"
    );

    let mpx = num_pixels as f64 / 1_000_000.0;
    let mpx_per_sec = mpx / elapsed.as_secs_f64().max(1e-6);
    println!(
        "[BENCH] Richardson-Lucy (512x512, 5 iter): {:.2} MPx in {:?} ({:.1} MPx/s)",
        mpx, elapsed, mpx_per_sec
    );

    let min_mpx_per_sec = if cfg!(debug_assertions) { 1.0 } else { 10.0 };
    assert!(
        mpx_per_sec > min_mpx_per_sec,
        "Richardson-Lucy Rayon parallel throughput must exceed {min_mpx_per_sec} MPx/s (got {mpx_per_sec:.1})"
    );

    Ok(())
}

#[test]
fn test_bench_iso21496_gain_map_generation_and_sidecars() -> Result<(), Box<dyn Error>> {
    let width = 1000_usize;
    let height = 1000_usize;
    let num_pixels = width * height;

    let mut sdr = Vec::with_capacity(num_pixels * 3);
    let mut hdr = Vec::with_capacity(num_pixels * 3);
    for i in 0..num_pixels {
        let v = (i % 1000) as f32 / 1000.0;
        sdr.push(v);
        sdr.push(v * 0.9);
        sdr.push(v * 0.8);

        hdr.push(v * 2.0);
        hdr.push(v * 1.8);
        hdr.push(v * 1.6);
    }

    let params = Iso21496Params {
        gain_map_min: 1.0,
        gain_map_max: 4.0,
        gamma: 1.0,
        offset_sdr: 0.015625,
        offset_hdr: 0.015625,
        hdr_capacity_min: 1.0,
        hdr_capacity_max: 4.0,
    };

    // 1. Benchmark Gain Map Compute
    let start_calc = Instant::now();
    let gain_map = GainMapCalculator::compute(&sdr, &hdr, width, height, &params)?;
    let elapsed_calc = start_calc.elapsed();

    assert_eq!(gain_map.len(), num_pixels);
    let all_in_range = gain_map.iter().all(|&v| (0.0..=1.0).contains(&v));
    assert!(
        all_in_range,
        "Gain map values must be normalized to [0.0, 1.0]"
    );

    let mpx = num_pixels as f64 / 1_000_000.0;
    let mpx_per_sec = mpx / elapsed_calc.as_secs_f64().max(1e-6);
    println!(
        "[BENCH] ISO 21496-1 Gain Map compute (1 MPx): {:?} ({:.1} MPx/s)",
        elapsed_calc, mpx_per_sec
    );

    // 2. Benchmark Binary .tmap Box & XMP Packet Serialization
    let start_sidecars = Instant::now();
    let tmap_bytes = Iso21496TmapBox::encode(&params)?;
    let xmp_xml = Iso21496XmpPacket::encode(&params);
    let elapsed_sidecars = start_sidecars.elapsed();

    assert_eq!(
        tmap_bytes.len(),
        Iso21496TmapBox::BOX_SIZE as usize,
        "ISOBMFF .tmap box must be exactly 40 bytes"
    );
    assert!(
        xmp_xml.contains("<hdrgm:Version>1.0</hdrgm:Version>"),
        "ISO 21496-1 XMP packet must declare Version 1.0"
    );

    // Round-trip verification
    let decoded_tmap = Iso21496TmapBox::decode(&tmap_bytes)?;
    assert!((decoded_tmap.gain_map_max - params.gain_map_max).abs() < 1e-4);

    let decoded_xmp = Iso21496XmpPacket::decode(&xmp_xml)?;
    assert!((decoded_xmp.gain_map_max - params.gain_map_max).abs() < 1e-4);

    println!(
        "[BENCH] ISO 21496-1 Sidecars serialize & round-trip: {:?}",
        elapsed_sidecars
    );

    Ok(())
}

#[test]
fn test_bench_memory_footprint_and_lru_saturation() -> Result<(), Box<dyn Error>> {
    let mut app = RrawApp::new_headless();

    let width = 800_usize;
    let height = 600_usize;
    let num_pixels = width * height;

    // Create 12 distinct photo entries (exceeding MAX_PHOTO_CACHE_ENTRIES = 10)
    for i in 0..12 {
        let path = PathBuf::from(format!("/tmp/mock_photo_{i}.dng"));
        let samples = vec![0.5_f32; num_pixels];
        let cfa =
            SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)?;

        let base_preview = BaseRgbPreview {
            width,
            height,
            samples_rgb: Arc::from(vec![[0.5_f32; 3]; num_pixels]),
            as_shot_wb: [2.0, 1.0, 1.5],
            cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
                width,
                height,
                subject: vec![0.0; num_pixels],
                sky: vec![0.0; num_pixels],
                background: vec![1.0; num_pixels],
                portrait: vec![0.0; num_pixels],
            }),
            exif_orientation: 1,
            matched_lens: None,
            focal_length_mm: Some(50.0),
            aperture_f_number: Some(2.8),
        };

        let entry = CachedPhotoEntry {
            path,
            photo_view: LoadedPhotoView {
                path: PathBuf::from(format!("/tmp/mock_photo_{i}.dng")),
                camera_make: "Mock".to_string(),
                camera_model: "Camera".to_string(),
                lens_model: "50mm".to_string(),
                dimensions: (width as u32, height as u32),
                iso: 100,
                shutter_str: "1/250s".to_string(),
                aperture_str: "f/2.8".to_string(),
                focal_length_str: "50mm".to_string(),
                date_time: "2026:11:23 12:00:00".to_string(),
                exposure_bias: "0 EV".to_string(),
                rating: 0,
                exif_tags: Vec::new(),
            },
            base_preview,
            cfa,
        };

        app.insert_cached_photo(entry);
    }

    // Cache count must be clamped to 10
    assert_eq!(
        app.photo_cache.len(),
        10,
        "Photo cache must clamp to MAX_PHOTO_CACHE_ENTRIES = 10"
    );

    // Total memory footprint must be within the 800 MB limit
    let total_bytes: usize = app
        .photo_cache
        .iter()
        .map(|e| e.memory_footprint_bytes())
        .sum();
    println!(
        "[BENCH] LRU cache footprint: {} bytes ({:.2} MB across 10 photos)",
        total_bytes,
        total_bytes as f64 / 1_000_000.0
    );
    assert!(
        total_bytes <= 800_000_000,
        "Total photo cache memory must remain <= 800 MB"
    );

    // Verify DiskThumbnailCache latency
    let temp_cache_dir =
        std::env::temp_dir().join(format!("rraw_test_thumb_{}", std::process::id()));
    let cache = DiskThumbnailCache::new(&temp_cache_dir)?;
    let thumb_payload = vec![0x42_u8; 16384]; // 16 KB thumbnail

    let start_write = Instant::now();
    cache.store_thumbnail("content_hash_12345", ThumbnailSize::Size160, &thumb_payload)?;
    let elapsed_write = start_write.elapsed();

    let start_read = Instant::now();
    let retrieved = cache.get_thumbnail("content_hash_12345", ThumbnailSize::Size160);
    let elapsed_read = start_read.elapsed();

    assert_eq!(retrieved.as_deref(), Some(thumb_payload.as_slice()));
    println!(
        "[BENCH] DiskThumbnailCache 16KB write: {:?}, read: {:?}",
        elapsed_write, elapsed_read
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_cache_dir);

    Ok(())
}

#[test]
fn test_raw_format_matrix_and_orientation_workflow() -> Result<(), Box<dyn Error>> {
    // 1. Check all 9 raw formats in lowercase and uppercase
    let formats = [
        ("dng", true),
        ("DNG", true),
        ("arw", true),
        ("ARW", true),
        ("cr2", true),
        ("CR2", true),
        ("cr3", true),
        ("CR3", true),
        ("nef", true),
        ("NEF", true),
        ("raf", true),
        ("RAF", true),
        ("rw2", true),
        ("RW2", true),
        ("orf", true),
        ("ORF", true),
        ("pef", true),
        ("PEF", true),
        ("jpg", false),
        ("png", false),
        ("txt", false),
    ];

    for (ext, expected) in formats {
        assert_eq!(
            is_raw_extension(ext),
            expected,
            "Format extension '{ext}' recognition mismatch"
        );
    }

    // 2. EXIF orientation parser on synthetic Little-Endian TIFF headers (orientations 1..=8)
    let temp_dir = std::env::temp_dir().join(format!("rraw_test_exif_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir)?;

    for orient in 1..=8_u16 {
        let tiff_path = temp_dir.join(format!("synthetic_orient_{orient}.tiff"));
        let mut tiff_data = Vec::new();
        // Header: 'II' (0x49, 0x49), version 42, offset to IFD = 8
        tiff_data.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]);
        // IFD0: 1 tag entry
        tiff_data.extend_from_slice(&[0x01, 0x00]);
        // Tag 0x0112 (Orientation): type SHORT (3), count 1, value = orient
        tiff_data.extend_from_slice(&[0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00]);
        tiff_data.extend_from_slice(&orient.to_le_bytes());
        tiff_data.extend_from_slice(&[0x00, 0x00]); // padding to 4 bytes
                                                    // Next IFD offset: 0
        tiff_data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

        std::fs::write(&tiff_path, &tiff_data)?;
        let parsed_orient = read_exif_orientation(&tiff_path);
        assert_eq!(
            parsed_orient, orient,
            "Orientation tag 0x0112 mismatch for orientation {orient}"
        );
    }

    // 3. Graceful error rejection on malformed or truncated raw files without panics
    let corrupt_path = temp_dir.join("corrupt.cr3");
    std::fs::write(&corrupt_path, b"ftypcrx bad payload truncated")?;

    let decoder = PureRustRawDecoder::new();
    let res = decoder.decode_file(&corrupt_path);
    assert!(
        res.is_err(),
        "Corrupted RAW file must fail gracefully returning Err"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}

#[test]
fn test_ui_preview_responsiveness_120fps_budget() -> Result<(), Box<dyn Error>> {
    let width = 1600_usize;
    let height = 1066_usize;
    let num_pixels = width * height;

    let base = BaseRgbPreview {
        width,
        height,
        samples_rgb: Arc::from(vec![[0.45_f32, 0.50_f32, 0.55_f32]; num_pixels]),
        as_shot_wb: [2.1, 1.0, 1.6],
        cam_to_srgb: [1.4, -0.3, -0.1, -0.1, 1.3, -0.2, 0.0, -0.2, 1.2],
        semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
            width,
            height,
            subject: vec![0.2; num_pixels],
            sky: vec![0.3; num_pixels],
            background: vec![0.5; num_pixels],
            portrait: vec![0.0; num_pixels],
        }),
        exif_orientation: 1,
        matched_lens: None,
        focal_length_mm: Some(35.0),
        aperture_f_number: Some(2.0),
    };

    let mut grading = DarkroomGradingState::default();
    grading.tone.exposure_ev = 0.75;
    grading.tone.contrast = 20.0;
    grading.tone.saturation = 15.0;
    grading.color.kelvin = 5400.0;
    grading.color.tint = 8.0;
    grading.denoise_sharpen.capture_sharpening_amount = 0.5; // Fast unsharp mask mode

    // Warm-up run
    let _ = render_preview_from_base_profiled(&base, &grading);

    // Profiled measurement run
    let (img, hist, timing) = render_preview_from_base_profiled(&base, &grading);

    assert_eq!(img.size, [width, height]);
    assert_eq!(img.pixels.len(), num_pixels);
    assert!(hist.max_count > 0);

    assert!(timing.stage_timing(PipelineStageId::Stage5Optics).is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage6TonalGrading)
        .is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage7EdrDisplay)
        .is_some());

    let frame_ms = timing.viewport_interactive_millis();
    let mpx = num_pixels as f64 / 1_000_000.0;
    let mpx_per_sec = mpx / (frame_ms / 1000.0).max(1e-6);

    println!(
        "[BENCH] UI Preview Grading (1600x1066): {:.2} ms ({:.1} MPx/s)",
        frame_ms, mpx_per_sec
    );

    // Under debug builds, CPU contention from parallel test runner is allowed;
    // verify interactive budget under reasonable bounds.
    let max_budget_ms = if cfg!(debug_assertions) { 250.0 } else { 8.333 };
    assert!(
        frame_ms < max_budget_ms,
        "UI preview rendering exceeded interactive frame budget"
    );

    Ok(())
}

#[test]
fn test_package_bundle_layout_and_release_readiness() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .ok_or("workspace root must exist")?;

    // 1. Verify macOS packaging assets exist
    let info_plist = workspace_root.join("assets/macos/Info.plist");
    assert!(info_plist.exists(), "Info.plist template must exist");
    let plist_str = std::fs::read_to_string(&info_plist)?;
    assert!(plist_str.contains("com.rraw.darkroom"));
    assert!(plist_str.contains("<key>NSHighResolutionCapable</key>"));
    assert!(plist_str.contains("public.camera-raw-image"));

    let entitlements = workspace_root.join("assets/macos/Entitlements.plist");
    assert!(entitlements.exists(), "Entitlements.plist must exist");
    let entitlements_str = std::fs::read_to_string(&entitlements)?;
    assert!(entitlements_str.contains("com.apple.security.files.user-selected.read-write"));

    let package_script = workspace_root.join("scripts/package_macos.sh");
    assert!(
        package_script.exists(),
        "scripts/package_macos.sh must exist"
    );

    // 2. Verify candidate i18n directory discovery in mock macOS bundle layout
    let mock_exe = PathBuf::from("/Applications/Rraw.app/Contents/MacOS/rraw-app");
    let candidates =
        rraw_app::i18n::I18nEngine::candidate_i18n_directories(Some(&mock_exe), None, None);
    assert!(
        candidates
            .iter()
            .any(|p| p == Path::new("/Applications/Rraw.app/Contents/Resources/assets/i18n")),
        "macOS .app bundle candidate i18n directory must be discovered"
    );

    // 3. Verify translation catalogs in repository assets
    let assets_i18n = workspace_root.join("assets/i18n");
    assert!(assets_i18n.exists(), "assets/i18n must exist");
    let engine = rraw_app::i18n::I18nEngine::global();
    let loaded_count = engine
        .load_from_directory(&assets_i18n)
        .map_err(|e| format!("failed to load i18n catalogs: {e}"))?;
    assert!(
        loaded_count >= 1,
        "External catalogs (e.g. es) must load successfully"
    );

    let registered_languages = engine.available_languages();
    assert!(
        registered_languages.len() >= 3,
        "Engine must have FR, EN, and ES registered"
    );

    Ok(())
}
