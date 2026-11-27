#![forbid(unsafe_code)]

use rraw_app::app::RrawApp;
use rraw_app::controllers::viewport::ActiveCanvasTool;
use rraw_app::preview::decoder::{
    compute_thumbnail_content_hash, deserialize_color_image, generate_fast_thumbnail,
    serialize_color_image,
};
use rraw_app::state::{ParallelToolPhase, ToolFamily};
use rraw_storage::{DiskThumbnailCache, ThumbnailSize};

#[test]
fn test_darkroom_reset_layout_resynchronizes_active_tool_family_and_tools_only() {
    let mut app = RrawApp::new_headless();

    // Set non-default layout state
    app.set_active_tool_family(ToolFamily::Color);
    app.set_active_tools_only(true);
    assert_eq!(app.active_tool_family, ToolFamily::Color);
    assert!(app.active_tools_only);
    assert_eq!(app.layout_config.active_tool_family, ToolFamily::Color);
    assert!(app.layout_config.active_tools_only);

    // Call reset_layout
    app.reset_layout();

    // Verify layout_config and app fields are reset to default
    assert_eq!(app.layout_config.active_tool_family, ToolFamily::All);
    assert!(!app.layout_config.active_tools_only);
    assert_eq!(app.active_tool_family, ToolFamily::All);
    assert!(!app.active_tools_only);
}

#[test]
fn test_canvas_tool_completion_calls_activate_none() {
    let mut app = RrawApp::new_headless();

    // 1. Activate Crop
    app.activate_canvas_tool(ActiveCanvasTool::Crop);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::Crop);
    assert!(app.viewport.crop.is_editing);

    // Complete Crop
    app.activate_canvas_tool(ActiveCanvasTool::None);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::None);
    assert!(!app.viewport.crop.is_editing);

    // 2. Activate Horizon
    app.activate_canvas_tool(ActiveCanvasTool::Horizon);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::Horizon);
    assert!(app.viewport.geometry_tools.horizon_active);

    // Complete Horizon
    app.activate_canvas_tool(ActiveCanvasTool::None);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::None);
    assert!(!app.viewport.geometry_tools.horizon_active);

    // 3. Activate Parallel Lines
    app.activate_canvas_tool(ActiveCanvasTool::ParallelLines(
        ParallelToolPhase::DrawingLine1,
    ));
    assert_eq!(
        app.active_canvas_tool(),
        ActiveCanvasTool::ParallelLines(ParallelToolPhase::DrawingLine1)
    );

    // Complete Parallel Lines
    app.activate_canvas_tool(ActiveCanvasTool::None);
    assert_eq!(app.active_canvas_tool(), ActiveCanvasTool::None);
    assert_eq!(
        app.viewport.geometry_tools.parallel_phase,
        ParallelToolPhase::Inactive
    );
}

#[test]
fn test_thumbnail_disk_cache_roundtrip_and_serialization() -> Result<(), Box<dyn std::error::Error>>
{
    let temp_cache_dir = std::env::temp_dir().join("rraw_test_thumb_cache");
    let cache = DiskThumbnailCache::new(&temp_cache_dir)?;

    let test_img = egui::ColorImage::from_rgba_unmultiplied(
        [4, 4],
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255, 128, 0, 0, 255, 0,
            128, 0, 255, 0, 0, 128, 255, 128, 128, 128, 255, 64, 0, 0, 255, 0, 64, 0, 255, 0, 0,
            64, 255, 64, 64, 64, 255, 32, 0, 0, 255, 0, 32, 0, 255, 0, 0, 32, 255, 32, 32, 32, 255,
        ],
    );

    let bytes = serialize_color_image(&test_img);
    assert_eq!(bytes.len(), 8 + 4 * 4 * 4);

    let recovered = deserialize_color_image(&bytes).ok_or("Deserialization failed")?;
    assert_eq!(recovered.size, [4, 4]);
    assert_eq!(recovered.pixels.len(), 16);
    assert_eq!(
        recovered.pixels[0],
        egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255)
    );
    assert_eq!(
        recovered.pixels[1],
        egui::Color32::from_rgba_unmultiplied(0, 255, 0, 255)
    );

    let dummy_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    cache.store_thumbnail(dummy_hash, ThumbnailSize::Size160, &bytes)?;

    let retrieved_bytes = cache
        .get_thumbnail(dummy_hash, ThumbnailSize::Size160)
        .ok_or("Failed to get thumbnail from disk cache")?;
    let retrieved_img =
        deserialize_color_image(&retrieved_bytes).ok_or("Failed to deserialize disk bytes")?;
    assert_eq!(retrieved_img.size, [4, 4]);
    assert_eq!(
        retrieved_img.pixels[0],
        egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255)
    );

    // Verify hash generation and cached entry retrieval for a temporary test image
    let test_img_path = std::env::temp_dir().join("rraw_test_dummy_photo.png");
    image::RgbImage::new(16, 16).save(&test_img_path)?;
    let hash = compute_thumbnail_content_hash(&test_img_path).ok_or("Failed to compute hash")?;
    assert_eq!(hash.len(), 64);
    let gen_img =
        generate_fast_thumbnail(&test_img_path).ok_or("Failed to generate fast thumbnail")?;
    assert!(gen_img.size[0] > 0);

    // Second call should succeed from cache
    let cached_img =
        generate_fast_thumbnail(&test_img_path).ok_or("Failed to retrieve cached thumbnail")?;
    assert_eq!(cached_img.size, gen_img.size);

    let _ = std::fs::remove_file(test_img_path);
    Ok(())
}
