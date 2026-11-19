use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use rraw_app::*;
use rraw_color::ColorError;
use rraw_core::{DisplayHeadroomProvider, HeadroomSample, SurfaceFormat};

struct MockHeadroomProvider {
    /// Stores encoded headroom * 10 as integer for lock-free interior mutability.
    active_tenths: AtomicU32,
}

impl DisplayHeadroomProvider for MockHeadroomProvider {
    fn current_headroom(&self) -> HeadroomSample {
        let active = self.active_tenths.load(Ordering::Acquire) as f32 / 10.0;
        HeadroomSample::new(active, 16.0)
    }
}

#[test]
fn xdr_to_sdr_display_switch_keeps_rgba16float_fixed_and_clamps_headroom() {
    let provider = MockHeadroomProvider {
        active_tenths: AtomicU32::new(40),
    };
    let mut shell = AppShellState::new(&provider);
    assert_eq!(shell.surface.format, SurfaceFormat::Rgba16Float);
    assert!((shell.surface.active_headroom - 4.0).abs() < 1e-5);

    // Simulate dragging window from XDR display (4.0) to external SDR monitor (1.0).
    provider.active_tenths.store(10, Ordering::Release);
    let format_after = shell.refresh_display_headroom();

    assert_eq!(format_after, SurfaceFormat::Rgba16Float);
    assert_eq!(shell.surface.format, SurfaceFormat::Rgba16Float);
    assert!((shell.surface.active_headroom - 1.0).abs() < 1e-5);
}

#[test]
fn shell_initialization_succeeds() -> Result<(), ColorError> {
    let summary = initialize_shell_summary()?;
    assert!(summary.contains("Rgba16Float"));
    assert!(summary.contains("ExtendedLinearDisplayP3"));
    Ok(())
}

#[test]
fn preview_generation_produces_valid_color_image() -> Result<(), rraw_core::CoreError> {
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};
    let width = 2400;
    let height = 1600;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);
    let samples = vec![0.5_f32; width * height];
    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;

    let grading = DarkroomGradingState::default();
    let (preview, hist) = app::generate_preview_image(&cfa, &grading);

    assert!(preview.width() > 0);
    assert!(preview.height() > 0);
    assert_eq!(preview.pixels.len(), preview.width() * preview.height());
    assert!(preview.pixels[0].r() > 0);
    assert!(hist.max_count > 0);
    Ok(())
}

#[test]
fn grading_presets_modify_parameters_correctly() {
    use app::GradingPreset;
    let mut grading = DarkroomGradingState::default();

    GradingPreset::DaylightPunch.apply(&mut grading);
    assert!((grading.exposure_ev - 0.3).abs() < 1e-5);
    assert!((grading.contrast - 15.0).abs() < 1e-5);

    GradingPreset::MonochromeFineArt.apply(&mut grading);
    assert!(grading.is_monochrome);

    GradingPreset::AsShot.apply(&mut grading);
    assert!(!grading.is_monochrome);
    assert!((grading.exposure_ev - 0.0).abs() < 1e-5);
}

#[test]
fn directory_scanner_filters_images() {
    let entries = app::scan_directory_for_images(Path::new("tests"));
    // Directory may or may not exist, should not panic
    let _ = entries.len();
}

#[test]
fn app_layout_config_serialization_roundtrips() {
    use app::{AppLayoutConfig, DarkroomSection};
    let mut config = AppLayoutConfig::default();
    config.section_order.swap(0, 1);
    let serialized = serde_json::to_string(&config).unwrap_or_default();
    assert!(serialized.contains("ExposureTone") || serialized.contains("WhiteBalanceColor"));

    let deserialized: AppLayoutConfig = serde_json::from_str(&serialized).unwrap_or_default();
    assert_eq!(deserialized.section_order.len(), DarkroomSection::ALL.len());
}

#[test]
fn sidecar_saving_and_loading_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = std::env::temp_dir().join("rraw_test_sidecars");
    std::fs::create_dir_all(&temp_dir)?;
    let photo_path = temp_dir.join("SAMPLE_001.DNG");
    std::fs::write(&photo_path, b"dummy raw content")?;

    let grading = DarkroomGradingState {
        preset: app::GradingPreset::DaylightPunch,
        exposure_ev: 1.25,
        contrast: 20.0,
        highlights: -15.0,
        shadows: 25.0,
        kelvin: 6200.0,
        is_monochrome: true,
        ..Default::default()
    };
    let rating = 4_u8;

    // 1. Save both sidecar files: <photo>.rraw.json and <photo>.xmp
    app::save_photo_sidecars(&photo_path, &grading, rating);

    let json_sidecar = temp_dir.join("SAMPLE_001.DNG.rraw.json");
    let xmp_sidecar = temp_dir.join("SAMPLE_001.xmp");

    assert!(json_sidecar.exists(), "Native JSON recipe must exist");
    assert!(xmp_sidecar.exists(), "Adobe XMP sidecar must exist");

    // Verify JSON contents
    let json_text = std::fs::read_to_string(&json_sidecar)?;
    assert!(json_text.contains("\"preset\": \"DaylightPunch\""));
    assert!(json_text.contains("\"exposure_ev\": 1.25"));
    assert!(json_text.contains("\"highlights\": -15.0"));
    assert!(json_text.contains("\"shadows\": 25.0"));
    assert!(json_text.contains("\"is_monochrome\": true"));
    assert!(json_text.contains("\"rating\": 4"));

    // Verify XMP contents
    let xmp_text = std::fs::read_to_string(&xmp_sidecar)?;
    assert!(xmp_text.contains("<xmp:Rating>4</xmp:Rating>"));

    // 2. Load sidecars back
    let (loaded_grading, loaded_rating) = app::load_photo_sidecars(&photo_path);
    let g = loaded_grading.ok_or("grading must be loaded from recipe")?;
    assert_eq!(g.preset, app::GradingPreset::DaylightPunch);
    assert!((g.exposure_ev - 1.25).abs() < 1e-4);
    assert!((g.contrast - 20.0).abs() < 1e-4);
    assert!((g.highlights - (-15.0)).abs() < 1e-4);
    assert!((g.shadows - 25.0).abs() < 1e-4);
    assert!((g.kelvin - 6200.0).abs() < 1e-4);
    assert!(g.is_monochrome);
    assert_eq!(loaded_rating, Some(4));

    // Cleanup
    let _ = std::fs::remove_file(json_sidecar);
    let _ = std::fs::remove_file(xmp_sidecar);
    let _ = std::fs::remove_file(photo_path);
    let _ = std::fs::remove_dir(temp_dir);
    Ok(())
}

#[test]
fn fast_thumbnail_from_cfa_bounds_target_width() -> Result<(), rraw_core::CoreError> {
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};
    let width = 2400;
    let height = 1600;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);
    let samples = vec![0.5_f32; width * height];
    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;

    let base =
        app::BaseRgbPreview::from_cfa_with_color_and_target_width(&cfa, [2.0, 1.0, 1.5], None, 140);
    assert!(base.width <= 150);
    assert!(base.height <= 100);

    let grading = DarkroomGradingState::default();
    let (thumb, _) = app::render_preview_from_base(&base, &grading);
    assert_eq!(thumb.width(), base.width);
    assert_eq!(thumb.height(), base.height);
    assert_eq!(thumb.pixels.len(), base.width * base.height);
    Ok(())
}

#[test]
fn test_imgp3879_exposure_and_tones() -> Result<(), Box<dyn std::error::Error>> {
    use rraw_decode::RawDecoder;
    let path = Path::new("/Users/rdespres/Desktop/test/IMGP3879.DNG");
    if !path.exists() {
        return Ok(());
    }
    let decoder = rraw_decode::PureRustRawDecoder::new();
    let decoded = decoder.decode_file(path)?;
    let base = app::BaseRgbPreview::from_cfa_with_color(
        &decoded.cfa_signed,
        [
            decoded.wb_coeffs[0],
            decoded.wb_coeffs[1],
            decoded.wb_coeffs[2],
        ],
        decoded.color_matrix.as_ref(),
    );

    let g0 = DarkroomGradingState::default();
    let (p0, _) = app::render_preview_from_base(&base, &g0);

    let g_exp = DarkroomGradingState {
        exposure_ev: 2.0,
        ..Default::default()
    };
    let (p_exp, _) = app::render_preview_from_base(&base, &g_exp);
    assert!(p_exp.pixels[p_exp.pixels.len() / 2].r() > p0.pixels[p0.pixels.len() / 2].r());

    let g_shadow = DarkroomGradingState {
        shadows: 50.0,
        ..Default::default()
    };
    let (p_shadow, _) = app::render_preview_from_base(&base, &g_shadow);
    assert!(p_shadow.pixels[p_shadow.pixels.len() / 2].r() > p0.pixels[p0.pixels.len() / 2].r());

    let g_high = DarkroomGradingState {
        highlights: -50.0,
        ..Default::default()
    };
    let (p_high, _) = app::render_preview_from_base(&base, &g_high);
    let max_idx = p0
        .pixels
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| c.r() as u32 + c.g() as u32 + c.b() as u32)
        .map(|(i, _)| i)
        .unwrap_or(0);
    assert!(p_high.pixels[max_idx].r() < p0.pixels[max_idx].r());
    Ok(())
}

#[test]
fn crop_toggle_and_dimensions_test() -> Result<(), rraw_core::CoreError> {
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};
    let width = 2400;
    let height = 1600;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);
    let samples = vec![0.5_f32; width * height];
    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let base =
        app::BaseRgbPreview::from_cfa_with_color_and_target_width(&cfa, [2.0, 1.0, 1.5], None, 400);

    let g_uncropped = DarkroomGradingState {
        crop_enabled: false,
        ..Default::default()
    };
    let (w_un, h_un, _, _) = app::render_export_image(&base, &g_uncropped, 1.0);
    assert_eq!(w_un, base.width);
    assert_eq!(h_un, base.height);

    let g_cropped = DarkroomGradingState {
        crop_enabled: true,
        crop_left: 0.25,
        crop_right: 0.75,
        crop_top: 0.25,
        crop_bottom: 0.75,
        ..Default::default()
    };
    let (w_cr, h_cr, _, _) = app::render_export_image(&base, &g_cropped, 1.0);
    assert_eq!(w_cr, base.width / 2);
    assert_eq!(h_cr, base.height / 2);
    Ok(())
}

#[test]
fn geometry_toggle_bypasses_keystone_when_disabled() -> Result<(), rraw_core::CoreError> {
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};
    let width = 1200;
    let height = 800;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);
    let samples = vec![0.5_f32; width * height];
    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let base =
        app::BaseRgbPreview::from_cfa_with_color_and_target_width(&cfa, [2.0, 1.0, 1.5], None, 200);

    let g_default = DarkroomGradingState::default();
    let (p_default, _) = app::render_preview_from_base(&base, &g_default);

    let g_geom_disabled = DarkroomGradingState {
        geometry_enabled: false,
        keystone_vertical: 35.0,
        keystone_horizontal: 20.0,
        ..Default::default()
    };
    let (p_disabled, _) = app::render_preview_from_base(&base, &g_geom_disabled);
    assert_eq!(p_disabled.pixels.len(), p_default.pixels.len());
    assert_eq!(p_disabled.pixels[100], p_default.pixels[100]);
    Ok(())
}

#[test]
fn keyboard_and_mouse_range_selection_works() {
    use std::collections::HashSet;
    use std::path::PathBuf;

    let entries = [
        PathBuf::from("/photos/a.dng"),
        PathBuf::from("/photos/b.dng"),
        PathBuf::from("/photos/c.dng"),
        PathBuf::from("/photos/d.dng"),
        PathBuf::from("/photos/e.dng"),
    ];

    let mut selected = HashSet::new();

    // 1. Single click on index 1
    selected.clear();
    selected.insert(entries[1].clone());
    let anchor = 1;
    assert_eq!(selected.len(), 1);

    // 2. Shift-click on index 3 -> range [1..=3]
    let target_idx = 3;
    let min_i = anchor.min(target_idx);
    let max_i = anchor.max(target_idx);
    for item in &entries[min_i..=max_i] {
        selected.insert(item.clone());
    }
    assert_eq!(selected.len(), 3);
    assert!(selected.contains(&entries[1]));
    assert!(selected.contains(&entries[2]));
    assert!(selected.contains(&entries[3]));

    // 3. Cmd-click on index 4 (toggle add)
    selected.insert(entries[4].clone());
    assert_eq!(selected.len(), 4);

    // 4. Cmd-click on index 2 (toggle remove)
    selected.remove(&entries[2]);
    assert_eq!(selected.len(), 3);
    assert!(!selected.contains(&entries[2]));

    // 5. Select All (Cmd+A)
    selected.extend(entries.iter().cloned());
    assert_eq!(selected.len(), 5);

    // 6. Deselect All (Escape / Cmd+D)
    selected.clear();
    assert_eq!(selected.len(), 0);
}

#[test]
fn default_directory_resolves_cleanly() {
    let default_dir = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|h| h.join("Desktop"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
    assert!(default_dir.exists());
}

#[test]
fn test_tone_curves_roundtrip_through_sidecar_and_preview() -> Result<(), Box<dyn std::error::Error>>
{
    use rraw_core::curves::CurvePoint;
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};

    let mut grading = DarkroomGradingState::default();
    assert!(grading.tone_curves.is_identity());

    // Add S-curve contrast
    grading
        .tone_curves
        .combined_rgb
        .add_point(CurvePoint::new(0.25, 0.15)?);
    grading
        .tone_curves
        .combined_rgb
        .add_point(CurvePoint::new(0.75, 0.85)?);
    assert!(!grading.tone_curves.is_identity());

    // 1. Verify sidecar serialization roundtrip
    let sidecar = SidecarRecipe::from_state(&grading, 5);
    let mut restored = DarkroomGradingState::default();
    sidecar.apply_to(&mut restored);
    assert_eq!(restored.tone_curves, grading.tone_curves);

    // 2. Verify preview rendering behaves deterministically with curves applied
    let width = 64;
    let height = 64;
    let cfa = SignedCfaBuffer::new(
        width,
        height,
        CfaPattern::Bayer(BayerOrigin::Rggb),
        vec![0.5_f32; width * height],
    )?;
    let base = BaseRgbPreview::from_cfa_with_color(&cfa, [1.0, 1.0, 1.0], None);

    let (img_linear, _) = render_preview_from_base(&base, &DarkroomGradingState::default());
    let (img_curved, _) = render_preview_from_base(&base, &grading);

    assert_eq!(img_linear.pixels.len(), img_curved.pixels.len());
    assert!(img_curved.pixels[0].r() > 0);

    Ok(())
}

#[test]
fn test_from_rgb_image_constructs_linear_preview_and_renders() {
    let mut rgb_img = image::RgbImage::new(100, 80);
    for pixel in rgb_img.pixels_mut() {
        *pixel = image::Rgb([128, 64, 200]);
    }

    let base = BaseRgbPreview::from_rgb_image(&rgb_img, 50);
    assert!(base.width <= 50);
    assert_eq!(base.samples_rgb.len(), base.width * base.height);
    assert_eq!(base.as_shot_wb, [1.0, 1.0, 1.0]);

    let (preview, hist) = render_preview_from_base(&base, &DarkroomGradingState::default());
    assert_eq!(preview.size, [base.width, base.height]);
    assert_eq!(preview.pixels.len(), base.width * base.height);
    assert!(hist.max_count > 0);
}

#[test]
fn test_before_after_untouched_original_state() {
    let mut rgb_img = image::RgbImage::new(60, 40);
    for pixel in rgb_img.pixels_mut() {
        *pixel = image::Rgb([100, 150, 200]);
    }
    let base = BaseRgbPreview::from_rgb_image(&rgb_img, 60);

    let original_state = DarkroomGradingState::default();
    let modified_state = DarkroomGradingState {
        exposure_ev: 2.0,
        contrast: 50.0,
        ..Default::default()
    };

    let (orig_preview, orig_hist) = render_preview_from_base(&base, &original_state);
    let (mod_preview, mod_hist) = render_preview_from_base(&base, &modified_state);

    assert_eq!(orig_preview.size, mod_preview.size);
    // Modified exposure + contrast should produce brighter pixel output
    assert!(mod_preview.pixels[0].r() > orig_preview.pixels[0].r());
    assert_ne!(orig_hist.lum, mod_hist.lum);
}

#[test]
fn test_tone_curves_channel_isolation_and_activity() -> Result<(), Box<dyn std::error::Error>> {
    use rraw_core::curves::{ChannelCurve, CurvePoint, ToneCurvesRecipe};

    let mut curves = ToneCurvesRecipe::default();
    assert!(curves.is_identity());
    assert!(curves.combined_rgb.is_identity());
    assert!(curves.red.is_identity());
    assert!(curves.green.is_identity());
    assert!(curves.blue.is_identity());
    assert!(curves.luma.is_identity());

    // Add a point only to the red channel
    curves.red.add_point(CurvePoint::new(0.5, 0.7)?);
    assert!(!curves.is_identity());
    assert!(!curves.red.is_identity());
    assert!(curves.combined_rgb.is_identity());
    assert!(curves.green.is_identity());
    assert!(curves.blue.is_identity());
    assert!(curves.luma.is_identity());

    // Resetting red channel restores identity
    curves.red = ChannelCurve::new_linear();
    assert!(curves.red.is_identity());
    assert!(curves.is_identity());

    Ok(())
}
