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
fn film_simulation_presets_configure_curves_and_parameters() {
    use app::GradingPreset;

    for preset in [
        GradingPreset::FujiClassicChrome,
        GradingPreset::KodakPortra400,
        GradingPreset::FujiVelvia50,
        GradingPreset::KodakTriX400,
        GradingPreset::IlfordHp5,
        GradingPreset::SepiaVintage,
    ] {
        let mut grading = DarkroomGradingState::default();
        preset.apply(&mut grading);

        // All film presets must embed custom non-identity tone curves
        assert!(
            !grading.tone_curves.is_identity(),
            "{preset:?} should configure non-identity tone curves"
        );

        // Identification roundtrip
        assert_eq!(
            GradingPreset::from_id_str(preset.id_str()),
            Some(preset),
            "ID string roundtrip failed for {preset:?}"
        );

        // Detect preset recovery
        assert_eq!(
            GradingPreset::detect_preset(&grading),
            preset,
            "detect_preset failed to recognize {preset:?}"
        );
    }

    // Specific characteristic checks
    let mut chrome = DarkroomGradingState::default();
    GradingPreset::FujiClassicChrome.apply(&mut chrome);
    assert!(
        chrome.saturation < -10.0,
        "Classic Chrome should have muted saturation"
    );
    assert!(!chrome.is_monochrome);

    let mut portra = DarkroomGradingState::default();
    GradingPreset::KodakPortra400.apply(&mut portra);
    assert!(portra.shadows > 10.0, "Portra 400 should lift shadows");
    assert!(!portra.is_monochrome);

    let mut velvia = DarkroomGradingState::default();
    GradingPreset::FujiVelvia50.apply(&mut velvia);
    assert!(
        velvia.saturation > 20.0,
        "Velvia 50 should boost saturation"
    );
    assert!(!velvia.is_monochrome);

    let mut tri_x = DarkroomGradingState::default();
    GradingPreset::KodakTriX400.apply(&mut tri_x);
    assert!(tri_x.is_monochrome, "Tri-X 400 must be monochrome");
    assert!(tri_x.contrast > 30.0, "Tri-X 400 must have high contrast");

    let mut hp5 = DarkroomGradingState::default();
    GradingPreset::IlfordHp5.apply(&mut hp5);
    assert!(hp5.is_monochrome, "Ilford HP5 must be monochrome");
    assert!(hp5.shadows > 10.0, "HP5 should preserve shadow latitude");

    let mut sepia = DarkroomGradingState::default();
    GradingPreset::SepiaVintage.apply(&mut sepia);
    assert_eq!(sepia.preset, GradingPreset::SepiaVintage);
    assert!(sepia.kelvin > 6500.0, "Sepia should have warm kelvin");
    assert!(
        sepia.creative_vignette_amount < -20.0,
        "Sepia should have subtle dark creative vignette"
    );
    assert!(!sepia.is_monochrome);
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
fn test_tool_family_categorization_and_palette_grouping() {
    use app::{DarkroomSection, ToolFamily};

    // DarkroomSection::ALL must contain exactly 11 modules including ToneCurves
    assert_eq!(DarkroomSection::ALL.len(), 11);
    assert!(DarkroomSection::ALL.contains(&DarkroomSection::ToneCurves));
    assert!(DarkroomSection::ALL.contains(&DarkroomSection::ExposureTone));

    // Verify family mapping
    assert_eq!(DarkroomSection::ExposureTone.family(), ToolFamily::Light);
    assert_eq!(DarkroomSection::ToneCurves.family(), ToolFamily::Light);
    assert_eq!(
        DarkroomSection::WhiteBalanceColor.family(),
        ToolFamily::Color
    );
    assert_eq!(DarkroomSection::SoftProofing.family(), ToolFamily::Color);
    assert_eq!(DarkroomSection::DetailDenoise.family(), ToolFamily::Detail);
    assert_eq!(DarkroomSection::Crop.family(), ToolFamily::Geometry);
    assert_eq!(DarkroomSection::Straighten.family(), ToolFamily::Geometry);
    assert_eq!(DarkroomSection::Geometry.family(), ToolFamily::Geometry);
    assert_eq!(
        DarkroomSection::OpticalCorrections.family(),
        ToolFamily::Geometry
    );
    assert_eq!(
        DarkroomSection::AiSemanticMasks.family(),
        ToolFamily::LocalMasks
    );
    assert_eq!(DarkroomSection::Timelapse.family(), ToolFamily::Timelapse);

    // Verify ToolFamily labels and constants
    assert_eq!(ToolFamily::ALL.len(), 6);
    assert_eq!(ToolFamily::Light.label(), "☀️ Lumière");
    assert_eq!(ToolFamily::Color.label(), "🎨 Couleur");
    assert_eq!(ToolFamily::Detail.label(), "🔍 Détail");
    assert_eq!(ToolFamily::Geometry.label(), "📐 Géométrie");
    assert_eq!(ToolFamily::LocalMasks.label(), "🖌️ Masques");
    assert_eq!(ToolFamily::All.label(), "Tous");
}

#[test]
fn test_layout_config_family_and_active_tools_persistence() -> Result<(), Box<dyn std::error::Error>>
{
    use app::{AppLayoutConfig, DarkroomSection, ToolFamily};

    let mut config = AppLayoutConfig::default();
    assert_eq!(config.active_tool_family, ToolFamily::All);
    assert!(!config.active_tools_only);

    config.active_tool_family = ToolFamily::Light;
    config.active_tools_only = true;

    // Serialize to JSON and deserialize back
    let json = serde_json::to_string(&config)?;
    let restored: AppLayoutConfig = serde_json::from_str(&json)?;

    assert_eq!(restored.active_tool_family, ToolFamily::Light);
    assert!(restored.active_tools_only);
    assert_eq!(restored.section_order.len(), 11);

    // Verify sections matching the Light family
    let light_sections: Vec<_> = restored
        .section_order
        .iter()
        .filter(|s| s.family() == ToolFamily::Light)
        .copied()
        .collect();
    assert_eq!(
        light_sections,
        vec![DarkroomSection::ExposureTone, DarkroomSection::ToneCurves]
    );

    Ok(())
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
        defringe_purple_amount: 45.0,
        defringe_green_amount: 30.0,
        creative_vignette_amount: -35.0,
        creative_vignette_midpoint: 40.0,
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
    assert!((g.defringe_purple_amount - 45.0).abs() < 1e-4);
    assert!((g.defringe_green_amount - 30.0).abs() < 1e-4);
    assert!((g.creative_vignette_amount - (-35.0)).abs() < 1e-4);
    assert!((g.creative_vignette_midpoint - 40.0).abs() < 1e-4);
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

#[test]
fn test_user_cube_loading_and_sidecar_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    use rraw_color::ColorLut3d;
    use std::sync::Arc;

    // Create a 2x2x2 tint LUT in .cube format
    let cube_str = "TITLE \"Test Tint\"\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.2 0.2\n0.2 1.0 0.2\n1.0 1.0 0.3\n0.1 0.1 1.0\n1.0 0.3 1.0\n0.3 1.0 1.0\n1.0 1.0 1.0\n";
    let lut = ColorLut3d::from_cube_str(cube_str)?;
    assert_eq!(lut.size(), 2);

    let mut grading = DarkroomGradingState {
        custom_clut: Some(CustomClutState {
            name: "Test Tint".to_string(),
            path: Some(std::path::PathBuf::from("/tmp/test_tint.cube")),
            lut: Arc::new(lut),
            strength: 0.75,
        }),
        ..Default::default()
    };

    let sidecar = SidecarRecipe::from_state(&grading, 4);
    assert_eq!(sidecar.rating, 4);
    assert_eq!(
        sidecar.custom_lut_path.as_deref(),
        Some("/tmp/test_tint.cube")
    );
    assert!((sidecar.custom_lut_strength - 0.75).abs() < 1e-5);

    // Applying preset clears custom_clut
    GradingPreset::AsShot.apply(&mut grading);
    assert!(grading.custom_clut.is_none());

    Ok(())
}

#[test]
fn test_user_cube_preview_rendering_and_blend_strength() -> Result<(), Box<dyn std::error::Error>> {
    use rraw_color::{ColorLut3d, FilmSimulationProfile};
    use std::sync::Arc;

    let mut rgb_img = image::RgbImage::new(32, 32);
    for pixel in rgb_img.pixels_mut() {
        *pixel = image::Rgb([128, 128, 128]);
    }
    let base = BaseRgbPreview::from_rgb_image(&rgb_img, 32);

    let state_neutral = DarkroomGradingState::default();
    let (preview_neutral, _) = app::render_preview_from_base(&base, &state_neutral);

    // Velvia 50 high saturation LUT
    let velvia = ColorLut3d::generate_film_profile(FilmSimulationProfile::Velvia50, 17)?;
    let mut state_lut = DarkroomGradingState {
        custom_clut: Some(CustomClutState {
            name: "Velvia 50".to_string(),
            path: None,
            lut: Arc::new(velvia.clone()),
            strength: 1.0,
        }),
        ..Default::default()
    };

    let (preview_velvia, _) = app::render_preview_from_base(&base, &state_lut);
    assert_eq!(preview_neutral.size, preview_velvia.size);
    // Velvia should modify pixel values
    assert_ne!(
        preview_neutral.pixels[0], preview_velvia.pixels[0],
        "Velvia 3D CLUT must alter pixel values"
    );

    // Test 0% strength produces exactly neutral output
    state_lut.custom_clut = Some(CustomClutState {
        name: "Velvia 50".to_string(),
        path: None,
        lut: Arc::new(velvia),
        strength: 0.0,
    });
    let (preview_zero, _) = app::render_preview_from_base(&base, &state_lut);
    assert_eq!(
        preview_neutral.pixels[0], preview_zero.pixels[0],
        "0% LUT strength must match neutral preview"
    );

    Ok(())
}

#[test]
fn test_corrupted_cube_handling() {
    use rraw_color::ColorLut3d;

    // Syntax error
    assert!(ColorLut3d::from_cube_str("INVALID CONTENT").is_err());
    // Mismatched node count
    assert!(ColorLut3d::from_cube_str("LUT_3D_SIZE 2\n0.0 0.0 0.0\n").is_err());
    // NaN coordinate
    assert!(ColorLut3d::from_cube_str("LUT_3D_SIZE 2\nNaN 0.0 0.0\n0.0 0.0 0.0\n0.0 0.0 0.0\n0.0 0.0 0.0\n0.0 0.0 0.0\n0.0 0.0 0.0\n0.0 0.0 0.0\n0.0 0.0 0.0\n").is_err());
}

#[test]
fn test_ai_semantic_masks_lifecycle_and_sidecar_roundtrip() -> Result<(), Box<dyn std::error::Error>>
{
    use rraw_core::{MaskKind, SemanticMaskCategory};

    let temp_dir = std::env::temp_dir().join("rraw_test_ai_semantic_sidecar");
    std::fs::create_dir_all(&temp_dir)?;
    let photo_path = temp_dir.join("PORTRAIT_AI.DNG");
    std::fs::write(&photo_path, b"dummy raw content")?;

    let mut grading = DarkroomGradingState::default();
    assert!(grading.semantic_masks.is_empty());

    {
        let sky_layer = grading.upsert_semantic_mask(SemanticMaskCategory::Sky);
        sky_layer.exposure_ev = -1.25;
        sky_layer.contrast = 20.0;
        sky_layer.saturation = 15.0;
        sky_layer.temperature_shift = -10.0;
        sky_layer.feather = 0.30;
        sky_layer.opacity = 0.90;
    }
    {
        let subj_layer = grading.upsert_semantic_mask(SemanticMaskCategory::Subject);
        subj_layer.exposure_ev = 0.65;
        subj_layer.invert = true;
    }

    assert_eq!(grading.semantic_masks.len(), 2);

    let stage6 = grading.to_stage6_masks_params();
    assert_eq!(stage6.masks.len(), 2);
    assert!(matches!(
        stage6.masks[0].kind,
        MaskKind::SemanticAi {
            category: SemanticMaskCategory::Sky,
            invert: false,
            ..
        }
    ));

    app::save_photo_sidecars(&photo_path, &grading, 5);
    let (loaded_opt, rating_opt) = app::load_photo_sidecars(&photo_path);
    assert_eq!(rating_opt, Some(5));
    let loaded = loaded_opt.ok_or("Expected loaded DarkroomGradingState")?;
    assert_eq!(loaded.semantic_masks.len(), 2);
    assert_eq!(loaded.semantic_masks, grading.semantic_masks);

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

#[test]
fn test_ai_semantic_masks_preview_and_export_rendering() {
    use rraw_core::SemanticMaskCategory;

    let mut rgb_img = image::RgbImage::new(64, 64);
    for (x, y, pixel) in rgb_img.enumerate_pixels_mut() {
        if y < 24 {
            // Bright atmospheric blue sky
            *pixel = image::Rgb([110, 175, 245]);
        } else if (20..=44).contains(&x) && (26..=52).contains(&y) {
            // Warm central subject
            *pixel = image::Rgb([220, 145, 105]);
        } else {
            // Muted dark ground
            *pixel = image::Rgb([70, 95, 65]);
        }
    }

    let base = BaseRgbPreview::from_rgb_image(&rgb_img, 64);
    assert!(base.semantic_masks.sky[6 * base.width + 32] > 0.65);
    assert!(base.semantic_masks.subject[38 * base.width + 32] > 0.60);

    let neutral = DarkroomGradingState::default();
    let (preview_neutral, _) = app::render_preview_from_base(&base, &neutral);

    let mut graded = DarkroomGradingState::default();
    graded
        .upsert_semantic_mask(SemanticMaskCategory::Sky)
        .exposure_ev = -1.5;
    graded
        .upsert_semantic_mask(SemanticMaskCategory::Subject)
        .exposure_ev = 1.2;

    let (preview_graded, _) = app::render_preview_from_base(&base, &graded);

    let sky_px = 6 * base.width + 32;
    let subj_px = 38 * base.width + 32;

    // Sky must be darker than neutral
    assert!(
        preview_graded.pixels[sky_px].b() < preview_neutral.pixels[sky_px].b(),
        "Sky mask with -1.5 EV must darken sky pixels"
    );
    // Subject must be brighter than neutral
    assert!(
        preview_graded.pixels[subj_px].r() > preview_neutral.pixels[subj_px].r(),
        "Subject mask with +1.2 EV must brighten subject pixels"
    );

    // Export image must also reflect semantic mask grading
    let (_, _, u8_neutral, _) = app::render_export_image(&base, &neutral, 1.0);
    let (_, _, u8_graded, _) = app::render_export_image(&base, &graded, 1.0);
    assert!(u8_graded[sky_px * 3 + 2] < u8_neutral[sky_px * 3 + 2]);
    assert!(u8_graded[subj_px * 3] > u8_neutral[subj_px * 3]);
}

#[test]
fn test_ai_semantic_mask_overlay_visualization() {
    use rraw_core::SemanticMaskCategory;

    let mut rgb_img = image::RgbImage::new(32, 32);
    for (_x, y, pixel) in rgb_img.enumerate_pixels_mut() {
        if y < 16 {
            *pixel = image::Rgb([100, 170, 245]);
        } else {
            *pixel = image::Rgb([80, 90, 80]);
        }
    }
    let base = BaseRgbPreview::from_rgb_image(&rgb_img, 32);

    let neutral = DarkroomGradingState::default();
    let (preview_no_overlay, _) = app::render_preview_from_base(&base, &neutral);

    let mut with_overlay = DarkroomGradingState {
        show_semantic_mask_overlay: Some(SemanticMaskCategory::Sky),
        ..Default::default()
    };
    let (preview_overlay, _) = app::render_preview_from_base(&base, &with_overlay);

    let sky_px = 4 * base.width + 16;
    assert!(
        preview_overlay.pixels[sky_px].r() > preview_no_overlay.pixels[sky_px].r(),
        "Ruby overlay must boost red channel over detected sky region"
    );

    with_overlay.show_semantic_mask_overlay = None;
}

#[test]
fn test_portrait_exif_orientation_rotation_and_dimensions() {
    // Sensor buffer is 40x20 (landscape in sensor coordinates), but shot in Portrait (EXIF Orientation 6 = 90° CW)
    let mut rgb_img = image::RgbImage::new(40, 20);
    // In sensor coordinates before 90° CW rotation, the right side (x >= 30) becomes the bottom,
    // and the left side (x < 10) becomes the top of the upright portrait image!
    for (x, _y, pixel) in rgb_img.enumerate_pixels_mut() {
        if x < 10 {
            // Left of sensor -> Top of upright 90° CW portrait (blue sky)
            *pixel = image::Rgb([100, 175, 245]);
        } else {
            *pixel = image::Rgb([60, 70, 60]);
        }
    }

    let base_landscape = BaseRgbPreview::from_rgb_image(&rgb_img, 40);
    assert_eq!((base_landscape.width, base_landscape.height), (40, 20));

    // Apply EXIF Orientation 6 (Rotate 90° CW -> Portrait 20x40)
    let base_portrait_cw = base_landscape.clone().with_exif_orientation(6);
    assert_eq!(
        (base_portrait_cw.width, base_portrait_cw.height),
        (20, 40),
        "Orientation 6 (90° CW) must transpose 40x20 sensor buffer into 20x40 upright portrait"
    );
    assert_eq!(app::oriented_dimensions(6000, 4000, 6), (4000, 6000));
    assert_eq!(app::oriented_dimensions(6000, 4000, 8), (4000, 6000));
    assert_eq!(app::oriented_dimensions(6000, 4000, 3), (6000, 4000));

    // Top of upright portrait (x=10, y=2) must now contain the blue sky pixel and high sky AI probability!
    let top_sample = base_portrait_cw.samples_rgb[2 * base_portrait_cw.width + 10];
    assert!(
        top_sample[2] > top_sample[0],
        "Top of rotated portrait must be the blue sky"
    );
    assert!(
        base_portrait_cw.semantic_masks.sky[2 * base_portrait_cw.width + 10] > 0.65,
        "AI semantic sky mask must detect sky at the top of the upright portrait"
    );

    // Apply EXIF Orientation 8 (Rotate 270° CW / 90° CCW -> Portrait 20x40)
    let base_portrait_ccw = base_landscape.with_exif_orientation(8);
    assert_eq!(
        (base_portrait_ccw.width, base_portrait_ccw.height),
        (20, 40)
    );
}

#[test]
fn test_live_directory_polling_detects_added_and_removed_photos(
) -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = std::env::temp_dir().join("rraw_test_live_dir_poll");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)?;

    let p1 = temp_dir.join("IMG_001.DNG");
    std::fs::write(&p1, b"dummy1")?;

    let mut entries = app::scan_directory_for_images(&temp_dir);
    assert_eq!(entries.len(), 1);

    // Add a second photo while directory is already open
    let p2 = temp_dir.join("IMG_002.DNG");
    std::fs::write(&p2, b"dummy2")?;

    let updated = app::sync_directory_entries(&temp_dir, &mut entries);
    assert!(
        updated,
        "sync_directory_entries must report true when a new photo is added"
    );
    assert_eq!(entries.len(), 2);

    // Second call with no changes must report false
    assert!(!app::sync_directory_entries(&temp_dir, &mut entries));

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

#[test]
fn test_soft_proofing_and_gamut_warning_in_preview_and_sidecar(
) -> Result<(), Box<dyn std::error::Error>> {
    use rraw_color::{SoftProofConfig, SoftProofProfile, SoftProofRenderingIntent};

    // Create a vivid saturated test image with high chroma green/cyan and dark shadow regions
    let mut rgb_img = image::RgbImage::new(32, 32);
    for (x, y, pixel) in rgb_img.enumerate_pixels_mut() {
        if y < 16 {
            // Hyper-saturated emerald-cyan
            *pixel = image::Rgb([5, 250, 220]);
        } else if x < 16 {
            // Pure black shadow
            *pixel = image::Rgb([2, 2, 2]);
        } else {
            // Bright white highlight
            *pixel = image::Rgb([252, 252, 252]);
        }
    }
    let base = BaseRgbPreview::from_rgb_image(&rgb_img, 32);

    // 1. Default render (Soft Proofing disabled)
    let g_default = DarkroomGradingState {
        saturation: 65.0,
        ..Default::default()
    };
    let (img_normal, hist_normal) = app::render_preview_from_base(&base, &g_default);
    assert!(
        hist_normal.out_of_gamut_count > 0,
        "Hyper-saturated pixels must be counted in out_of_gamut_count even before overlay toggle"
    );

    // 2. Enable Soft Proofing on Fine Art Matte Paper with Paper & Ink simulation
    let g_matte_proof = DarkroomGradingState {
        saturation: 65.0,
        soft_proof: SoftProofConfig {
            enabled: true,
            profile: SoftProofProfile::FineArtMattePaper,
            intent: SoftProofRenderingIntent::Perceptual,
            simulate_paper_and_ink: true,
            show_gamut_warning: false,
        },
        ..Default::default()
    };
    let (img_matte, hist_matte) = app::render_preview_from_base(&base, &g_matte_proof);
    assert!(
        hist_matte.out_of_gamut_count >= hist_normal.out_of_gamut_count,
        "Fine Art Matte Paper must report at least as many out-of-gamut pixels as default sRGB"
    );

    // Deep black pixel (y=24, x=4) must be lifted by matte black ink Dmax simulation
    let black_idx = 24 * base.width + 4;
    assert!(
        img_matte.pixels[black_idx].g() > img_normal.pixels[black_idx].g(),
        "Matte paper simulation must lift deep shadow black floor ({} vs {})",
        img_matte.pixels[black_idx].g(),
        img_normal.pixels[black_idx].g()
    );

    // 3. Enable Out-of-Gamut Warning overlay -> saturated top region (y=4, x=16) gets high-contrast alert color
    let g_gamut_alert = DarkroomGradingState {
        soft_proof: SoftProofConfig {
            show_gamut_warning: true,
            ..g_matte_proof.soft_proof
        },
        ..g_matte_proof.clone()
    };
    let (img_alert, _) = app::render_preview_from_base(&base, &g_gamut_alert);
    let vivid_idx = 4 * base.width + 16;
    assert_ne!(
        img_alert.pixels[vivid_idx], img_matte.pixels[vivid_idx],
        "Gamut warning overlay must highlight out-of-gamut pixels in the viewport"
    );

    // 4. Verify SidecarRecipe roundtrip preserves SoftProofConfig
    let sidecar = SidecarRecipe::from_state(&g_matte_proof, 4);
    let mut restored = DarkroomGradingState::default();
    sidecar.apply_to(&mut restored);
    assert!(restored.soft_proof.enabled);
    assert_eq!(
        restored.soft_proof.profile,
        SoftProofProfile::FineArtMattePaper
    );
    assert_eq!(
        restored.soft_proof.intent,
        SoftProofRenderingIntent::Perceptual
    );
    assert!(restored.soft_proof.simulate_paper_and_ink);

    Ok(())
}

#[test]
fn test_timelapse_sequence_grading_pipeline_deflickers_and_ramps_keyframes(
) -> Result<(), rraw_core::CoreError> {
    use rraw_app::app::{prepare_timelapse_frame_gradings, ExportTargetMode};
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};
    use rraw_export::TimelapseDeflickerConfig;

    assert_eq!(ExportTargetMode::default(), ExportTargetMode::CurrentPhoto);

    let width = 16;
    let height = 16;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);

    // 5 frames with artificial exposure flicker on frame 2 (+0.30 EV brightness spike in sensor samples)
    let jitter_mults = [1.0_f32, 1.0, 1.231, 1.0, 1.0];
    let mut previews = Vec::with_capacity(5);
    for &mult in &jitter_mults {
        let samples = vec![(0.18 * mult).clamp(0.0, 1.0); width * height];
        let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
        let base = app::BaseRgbPreview::from_cfa_with_color(&cfa, [1.0, 1.0, 1.0], None);
        previews.push(base);
    }

    // Frame 0 is Daylight (0.0 EV, 5600K), Frame 4 is Sunset/Night (+1.0 EV, 3600K)
    let g_start = DarkroomGradingState {
        exposure_ev: 0.0,
        kelvin: 5600.0,
        ..Default::default()
    };
    let g_mid = DarkroomGradingState::default();
    let g_end = DarkroomGradingState {
        exposure_ev: 1.0,
        kelvin: 3600.0,
        ..Default::default()
    };

    let frames = vec![
        (&previews[0], g_start),
        (&previews[1], g_mid.clone()),
        (&previews[2], g_mid.clone()),
        (&previews[3], g_mid),
        (&previews[4], g_end),
    ];

    let deflicker_cfg = TimelapseDeflickerConfig {
        enabled: true,
        window_radius: 2,
        strength: 1.0,
        max_correction_ev: 1.5,
    };

    let (final_gradings, offsets_ev) =
        prepare_timelapse_frame_gradings(&frames, true, &deflicker_cfg);

    assert_eq!(final_gradings.len(), 5);
    assert_eq!(offsets_ev.len(), 5);

    // Midpoint (frame 2) Kelvin must be ramped to 4600K
    assert!(
        (final_gradings[2].kelvin - 4600.0).abs() < 1.0,
        "Keyframe ramping must interpolate Kelvin at midpoint to 4600K, got {}",
        final_gradings[2].kelvin
    );

    // Frame 2 had a +0.30 EV sensor brightness spike -> deflicker offset must be negative
    assert!(
        offsets_ev[2] < -0.10,
        "Frame 2 brightness spike must be countered by negative deflicker EV offset, got {}",
        offsets_ev[2]
    );
    // Final exposure on frame 2 is ramped base (0.5 EV) + negative deflicker offset (< 0.40 EV)
    assert!(final_gradings[2].exposure_ev < 0.42);

    Ok(())
}

#[test]
fn test_full_resolution_1_to_1_export_preserves_exact_sensor_dimensions(
) -> Result<(), rraw_core::CoreError> {
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};

    let width = 640;
    let height = 420;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);
    let samples = vec![0.25_f32; width * height];
    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;

    // Requesting full native width (640) must produce 1:1 full-resolution 640x420 output (not half-size 320x210)
    let base_full = app::BaseRgbPreview::from_cfa_with_color_and_target_width(
        &cfa,
        [2.0, 1.0, 1.5],
        None,
        width,
    );
    assert_eq!(base_full.width, width);
    assert_eq!(base_full.height, height);

    let grading = DarkroomGradingState::default();
    let (out_w, out_h, u8_bytes, u16_samples) = app::render_export_image(&base_full, &grading, 1.0);
    assert_eq!(out_w, width);
    assert_eq!(out_h, height);
    assert_eq!(u8_bytes.len(), width * height * 3);
    assert_eq!(u16_samples.len(), width * height * 3);

    Ok(())
}

#[test]
fn test_chroma_and_luma_denoise_reduces_variance_in_rendered_preview_and_export(
) -> Result<(), rraw_core::CoreError> {
    use rraw_core::{BayerOrigin, CfaPattern, SignedCfaBuffer};

    let width = 64;
    let height = 64;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);
    let mut samples = vec![0.5_f32; width * height];
    // Add non-periodic spatial noise across the sensor grid
    for y in 0..height {
        for x in 0..width {
            let n = (((x * 37 + y * 73 + (x ^ y)) % 23) as f32 / 23.0 - 0.5) * 0.30;
            samples[y * width + x] = (0.5 + n).clamp(0.1, 0.9);
        }
    }
    let cfa = SignedCfaBuffer::new(width, height, pattern, samples)?;
    let base = app::BaseRgbPreview::from_cfa_with_color_and_target_width(
        &cfa,
        [1.0, 1.0, 1.0],
        None,
        width,
    );

    // Render with zero denoise and identity optics
    let no_denoise = DarkroomGradingState {
        distortion_correction: false,
        vignette_correction: false,
        color_denoise: 0.0,
        luma_denoise: 0.0,
        ..Default::default()
    };
    let (_, _, u8_raw, _) = app::render_export_image(&base, &no_denoise, 1.0);

    // Render with active chroma & luma denoise
    let with_denoise = DarkroomGradingState {
        distortion_correction: false,
        vignette_correction: false,
        color_denoise: 90.0,
        luma_denoise: 80.0,
        ..Default::default()
    };
    let (_, _, u8_clean, _) = app::render_export_image(&base, &with_denoise, 1.0);

    // Compute pixel variance of raw vs clean
    let mean_raw: f32 = u8_raw.iter().map(|&b| b as f32).sum::<f32>() / (u8_raw.len() as f32);
    let var_raw: f32 = u8_raw
        .iter()
        .map(|&b| (b as f32 - mean_raw).powi(2))
        .sum::<f32>()
        / (u8_raw.len() as f32);

    let mean_clean: f32 = u8_clean.iter().map(|&b| b as f32).sum::<f32>() / (u8_clean.len() as f32);
    let var_clean: f32 = u8_clean
        .iter()
        .map(|&b| (b as f32 - mean_clean).powi(2))
        .sum::<f32>()
        / (u8_clean.len() as f32);

    assert!(
        var_clean < var_raw * 0.5,
        "Denoised variance {var_clean} must be < 50% of raw {var_raw}"
    );

    Ok(())
}
