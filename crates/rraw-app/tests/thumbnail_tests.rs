#![forbid(unsafe_code)]

use egui::Color32;
use rraw_app::preview::create_thumbnail_from_preview;
use rraw_app::state::DarkroomGradingState;

#[test]
fn test_create_thumbnail_from_preview_full() {
    let src_w = 300;
    let src_h = 200;
    let mut pixels = Vec::with_capacity(src_w * src_h);
    for y in 0..src_h {
        for x in 0..src_w {
            pixels.push(Color32::from_rgb((x % 256) as u8, (y % 256) as u8, 128));
        }
    }
    let preview = egui::ColorImage::new([src_w, src_h], pixels);
    let grading = DarkroomGradingState::default();

    let thumb = create_thumbnail_from_preview(&preview, &grading, 140, 100);
    assert!(thumb.size[0] <= 140);
    assert!(thumb.size[1] <= 100);
    assert_eq!(thumb.pixels.len(), thumb.size[0] * thumb.size[1]);
    assert!(!thumb.pixels.is_empty());
}

#[test]
fn test_create_thumbnail_from_preview_cropped() {
    let src_w = 400;
    let src_h = 400;
    let mut pixels = Vec::with_capacity(src_w * src_h);
    for _y in 0..src_h {
        for x in 0..src_w {
            // Left half is black, right half is white
            let col = if x < 200 { 0 } else { 255 };
            pixels.push(Color32::from_rgb(col, col, col));
        }
    }
    let preview = egui::ColorImage::new([src_w, src_h], pixels);

    // Crop to right half only: crop_left = 0.5, crop_right = 1.0
    let grading = DarkroomGradingState {
        crop_enabled: true,
        crop_left: 0.5,
        crop_right: 1.0,
        crop_top: 0.0,
        crop_bottom: 1.0,
        ..Default::default()
    };

    let thumb = create_thumbnail_from_preview(&preview, &grading, 140, 100);
    assert!(thumb.size[0] <= 140);
    assert!(thumb.size[1] <= 100);
    // All sampled pixels in cropped right half must be white (255)
    for px in &thumb.pixels {
        assert_eq!(px.r(), 255);
        assert_eq!(px.g(), 255);
        assert_eq!(px.b(), 255);
    }
}
