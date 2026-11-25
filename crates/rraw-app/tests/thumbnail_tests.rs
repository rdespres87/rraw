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
        geometry: rraw_app::state::GeometryGradingParams {
            crop_enabled: true,
            crop_left: 0.5,
            crop_right: 1.0,
            crop_top: 0.0,
            crop_bottom: 1.0,
            ..Default::default()
        },
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

#[test]
fn test_is_raw_extension_classification() {
    use rraw_app::preview::{is_raw_extension, RAW_EXTENSIONS, SUPPORTED_IMAGE_EXTENSIONS};

    assert!(is_raw_extension("dng"));
    assert!(is_raw_extension("DNG"));
    assert!(is_raw_extension("arw"));
    assert!(is_raw_extension("cr2"));
    assert!(is_raw_extension("cr3"));
    assert!(is_raw_extension("nef"));
    assert!(is_raw_extension("raf"));
    assert!(is_raw_extension("crw"));
    assert!(is_raw_extension("iiq"));
    assert!(is_raw_extension("srw"));
    assert!(is_raw_extension("rwl"));
    assert!(is_raw_extension("ori"));
    assert!(is_raw_extension("nrw"));
    assert!(is_raw_extension("mrw"));
    assert!(is_raw_extension("3fr"));
    assert!(is_raw_extension("fff"));
    assert!(is_raw_extension("pef"));

    for ext in RAW_EXTENSIONS {
        assert!(
            SUPPORTED_IMAGE_EXTENSIONS.contains(ext),
            "SUPPORTED_IMAGE_EXTENSIONS must include RAW extension {ext}"
        );
    }

    assert!(!is_raw_extension("jpg"));
    assert!(!is_raw_extension("jpeg"));
    assert!(!is_raw_extension("png"));
    assert!(!is_raw_extension("txt"));
}

#[test]
fn test_create_thumbnail_portrait_aspect_ratio() {
    let src_w = 200;
    let src_h = 300; // Portrait 2:3
    let pixels = vec![Color32::from_rgb(100, 150, 200); src_w * src_h];
    let preview = egui::ColorImage::new([src_w, src_h], pixels);
    let grading = DarkroomGradingState::default();

    let thumb = create_thumbnail_from_preview(&preview, &grading, 140, 100);
    // In portrait, aspect = 200/300 = 0.667. Max box is 140x100.
    // thumb height should be 100 and thumb width should be round(100 * 0.667) = 67.
    assert_eq!(thumb.size[1], 100);
    assert!((thumb.size[0] as i32 - 67).abs() <= 1);
}

#[test]
fn test_read_exif_orientation_tiff_subifd() -> Result<(), Box<dyn std::error::Error>> {
    // Construct a synthetic Little-Endian TIFF with orientation in Exif SubIFD (tag 0x8769)
    let mut tiff = vec![0_u8; 512];
    // Header: II (LE), magic 42, IFD0 offset = 8
    tiff[0..4].copy_from_slice(&[b'I', b'I', 42, 0]);
    tiff[4..8].copy_from_slice(&8_u32.to_le_bytes());

    // IFD0 at offset 8: 1 entry pointing to SubIFD (tag 0x8769)
    let ifd0_off = 8;
    tiff[ifd0_off..ifd0_off + 2].copy_from_slice(&1_u16.to_le_bytes());
    let entry_off = ifd0_off + 2;
    // Tag 0x8769 (LONG, count 1, value = 64)
    tiff[entry_off..entry_off + 2].copy_from_slice(&0x8769_u16.to_le_bytes());
    tiff[entry_off + 2..entry_off + 4].copy_from_slice(&4_u16.to_le_bytes()); // LONG
    tiff[entry_off + 4..entry_off + 8].copy_from_slice(&1_u32.to_le_bytes());
    tiff[entry_off + 8..entry_off + 12].copy_from_slice(&64_u32.to_le_bytes()); // SubIFD offset = 64

    // Next IFD = 0
    tiff[entry_off + 12..entry_off + 16].copy_from_slice(&0_u32.to_le_bytes());

    // SubIFD at offset 64: 1 entry for Orientation (tag 0x0112 = 6, Rotate 90 CW)
    let sub_off = 64;
    tiff[sub_off..sub_off + 2].copy_from_slice(&1_u16.to_le_bytes());
    let sub_entry = sub_off + 2;
    tiff[sub_entry..sub_entry + 2].copy_from_slice(&0x0112_u16.to_le_bytes());
    tiff[sub_entry + 2..sub_entry + 4].copy_from_slice(&3_u16.to_le_bytes()); // SHORT
    tiff[sub_entry + 4..sub_entry + 8].copy_from_slice(&1_u32.to_le_bytes());
    tiff[sub_entry + 8..sub_entry + 10].copy_from_slice(&6_u16.to_le_bytes()); // Orientation 6

    // Write to a temporary file and read back
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("test_orientation_subifd.tif");
    std::fs::write(&temp_file, &tiff)?;

    let orient = rraw_app::preview::read_exif_orientation(&temp_file);
    let _ = std::fs::remove_file(&temp_file);

    assert_eq!(orient, 6);
    Ok(())
}
