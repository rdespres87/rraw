#![forbid(unsafe_code)]

//! Unit tests for the pure-Rust DNG/TIFF/raster fallback decoder.

use std::path::PathBuf;

struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_temp(name: &str, data: &[u8]) -> std::io::Result<TempFile> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rraw_{}_{seq}_{name}", std::process::id()));
    std::fs::write(&path, data)?;
    Ok(TempFile(path))
}

fn build_tiff(le: bool, tags: &[(u16, u16, u32, Vec<u8>)], payload: &[u8]) -> Vec<u8> {
    let u16_b = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let u32_b = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };

    let header_and_ifd = 8 + 2 + tags.len() * 12 + 4;
    let extra_total: usize = tags
        .iter()
        .filter(|(tag, _, _, val)| {
            !((*tag == 273 || *tag == 324) && val.is_empty()) && val.len() > 4
        })
        .map(|(_, _, _, val)| val.len())
        .sum();
    let payload_off = (header_and_ifd + extra_total) as u32;

    let mut extra: Vec<u8> = Vec::new();
    let mut entries: Vec<u8> = Vec::new();

    for (tag, typ, count, val) in tags {
        entries.extend_from_slice(&u16_b(*tag));
        entries.extend_from_slice(&u16_b(*typ));
        entries.extend_from_slice(&u32_b(*count));

        if (*tag == 273 || *tag == 324) && val.is_empty() {
            entries.extend_from_slice(&u32_b(payload_off));
        } else if val.len() <= 4 {
            let mut v = val.clone();
            v.resize(4, 0);
            entries.extend_from_slice(&v);
        } else {
            let off = (header_and_ifd + extra.len()) as u32;
            entries.extend_from_slice(&u32_b(off));
            extra.extend_from_slice(val);
        }
    }

    let mut buf = Vec::new();
    buf.extend_from_slice(if le { b"II" } else { b"MM" });
    buf.extend_from_slice(&u16_b(42));
    buf.extend_from_slice(&u32_b(8));
    buf.extend_from_slice(&u16_b(tags.len() as u16));
    buf.extend_from_slice(&entries);
    buf.extend_from_slice(&u32_b(0));
    buf.extend_from_slice(&extra);
    buf.extend_from_slice(payload);
    buf
}

#[test]
fn fallback_embedded_png() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::RgbImage::from_pixel(4, 4, image::Rgb([255, 0, 0]));
    let mut png_data = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_data),
        image::ImageFormat::Png,
    )?;
    let tf = write_temp("test_fallback.png", &png_data)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!(
        r.linear_rgb.as_ref().map(|b| (b.width(), b.height())),
        Some((4, 4))
    );
    Ok(())
}

#[test]
fn fallback_error_paths() -> Result<(), Box<dyn std::error::Error>> {
    let tf1 = write_temp("short.bin", &[1, 2])?;
    assert!(super::try_decode_fallback(&tf1.0).is_err());

    let tf2 = write_temp("badmagic.bin", b"NOPE0000")?;
    assert!(super::try_decode_fallback(&tf2.0).is_err());

    let tf3 = write_temp("badtiff.bin", b"II\x2a\x01\x08\0\0\0")?;
    assert!(super::try_decode_fallback(&tf3.0).is_err());
    Ok(())
}

#[test]
fn fallback_uncompressed_cfa_le_and_lut() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Vec<u8> = (0..256u16).flat_map(|i| (i % 4).to_le_bytes()).collect();
    let lut: Vec<u8> = [0u16, 100, 200, 300]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let tags = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 16u16.to_le_bytes().to_vec()),
        (259, 3, 1, 1u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_le_bytes().to_vec()),
        (279, 4, 1, 512u32.to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
        (50712, 3, 4, lut),
        (50714, 11, 1, 10.0f32.to_le_bytes().to_vec()),
        (50717, 11, 1, 1000.0f32.to_le_bytes().to_vec()),
    ];
    let data = build_tiff(true, &tags, &payload);
    let tf = write_temp("le_cfa.dng", &data)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!(r.cfa_signed.width(), 16);
    assert_eq!(r.cfa_signed.height(), 16);
    assert!(r.linear_rgb.is_none());
    Ok(())
}

#[test]
fn fallback_uncompressed_cfa_be_and_width_quirk() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Vec<u8> = (0..256u16)
        .flat_map(|i| (100 + (i % 16)).to_be_bytes())
        .collect();
    let tags = [
        (256, 3, 1, 20u16.to_be_bytes().to_vec()),
        (257, 3, 1, 16u16.to_be_bytes().to_vec()),
        (258, 3, 1, 16u16.to_be_bytes().to_vec()),
        (259, 3, 1, 1u16.to_be_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_be_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_be_bytes().to_vec()),
        (279, 4, 1, 512u32.to_be_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
    ];
    let data = build_tiff(false, &tags, &payload);
    let tf = write_temp("be_cfa.dng", &data)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!(r.cfa_signed.width(), 16);
    assert_eq!(r.cfa_signed.height(), 16);
    Ok(())
}

#[test]
fn fallback_uncompressed_rgb_tiff_8_and_16_be() -> Result<(), Box<dyn std::error::Error>> {
    let p8 = vec![10u8; 12];
    let bps8: Vec<u8> = [8u16, 8, 8]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect();
    let t8 = [
        (256, 3, 1, 2u16.to_be_bytes().to_vec()),
        (257, 3, 1, 2u16.to_be_bytes().to_vec()),
        (258, 3, 3, bps8),
        (259, 3, 1, 1u16.to_be_bytes().to_vec()),
        (262, 3, 1, 2u16.to_be_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 3u16.to_be_bytes().to_vec()),
        (279, 4, 1, 12u32.to_be_bytes().to_vec()),
    ];
    let d8 = build_tiff(false, &t8, &p8);
    let tf8 = write_temp("rgb8.tif", &d8)?;
    let r8 = super::try_decode_fallback(&tf8.0)?;
    assert_eq!(
        r8.linear_rgb.as_ref().map(|b| (b.width(), b.height())),
        Some((2, 2))
    );

    let p16: Vec<u8> = (0..12u16).flat_map(|v| (v * 1000).to_be_bytes()).collect();
    let bps16: Vec<u8> = [16u16, 16, 16]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect();
    let t16 = [
        (256, 3, 1, 2u16.to_be_bytes().to_vec()),
        (257, 3, 1, 2u16.to_be_bytes().to_vec()),
        (258, 3, 3, bps16),
        (259, 3, 1, 1u16.to_be_bytes().to_vec()),
        (262, 3, 1, 2u16.to_be_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 3u16.to_be_bytes().to_vec()),
        (279, 4, 1, 24u32.to_be_bytes().to_vec()),
    ];
    let d16 = build_tiff(false, &t16, &p16);
    let tf16 = write_temp("rgb16.tif", &d16)?;
    let r16 = super::try_decode_fallback(&tf16.0)?;
    assert_eq!(
        r16.linear_rgb.as_ref().map(|b| (b.width(), b.height())),
        Some((2, 2))
    );
    Ok(())
}

#[test]
fn fallback_unsupported_compression_and_missing_cfa() -> Result<(), Box<dyn std::error::Error>> {
    let payload = vec![0u8; 8];
    let tags_comp = [
        (256, 3, 1, 2u16.to_be_bytes().to_vec()),
        (257, 3, 1, 2u16.to_be_bytes().to_vec()),
        (258, 3, 1, 16u16.to_be_bytes().to_vec()),
        (259, 3, 1, 99u16.to_be_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_be_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_be_bytes().to_vec()),
        (279, 4, 1, 8u32.to_be_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
    ];
    let d = build_tiff(false, &tags_comp, &payload);
    let tf = write_temp("badcomp.dng", &d)?;
    assert!(super::try_decode_fallback(&tf.0).is_err());

    let tags_nocfa = [
        (256, 3, 1, 2u16.to_be_bytes().to_vec()),
        (257, 3, 1, 2u16.to_be_bytes().to_vec()),
        (258, 3, 1, 16u16.to_be_bytes().to_vec()),
        (259, 3, 1, 1u16.to_be_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_be_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_be_bytes().to_vec()),
        (279, 4, 1, 8u32.to_be_bytes().to_vec()),
    ];
    let d2 = build_tiff(false, &tags_nocfa, &payload);
    let tf2 = write_temp("nocfa.dng", &d2)?;
    assert!(super::try_decode_fallback(&tf2.0).is_err());
    Ok(())
}

#[test]
fn fallback_dng_jpeg_cfa_strip_and_matrices() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::GrayImage::from_pixel(16, 16, image::Luma([128u8]));
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageLuma8(img).write_to(
        &mut std::io::Cursor::new(&mut jpeg),
        image::ImageFormat::Jpeg,
    )?;
    let cm1: Vec<u8> = [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let asn: Vec<u8> = [1.0f32, 1.0, 1.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let aa: Vec<u8> = [0u32, 0, 16, 16]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let tags = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 8u16.to_le_bytes().to_vec()),
        (259, 3, 1, 34892u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_le_bytes().to_vec()),
        (279, 4, 1, (jpeg.len() as u32).to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
        (50721, 11, 9, cm1),
        (50728, 11, 3, asn),
        (50829, 4, 4, aa),
    ];
    let d = build_tiff(true, &tags, &jpeg);
    let tf = write_temp("jpeg_cfa.dng", &d)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!(r.cfa_signed.width(), 16);
    assert_eq!(r.cfa_signed.height(), 16);
    Ok(())
}

#[test]
fn fallback_dng_vc5_cfa_strip() -> Result<(), Box<dyn std::error::Error>> {
    let mut vc5 = Vec::new();
    vc5.extend_from_slice(b"VC-5");
    let push_tag = |buf: &mut Vec<u8>, tag: u16, val: u16| {
        buf.extend_from_slice(&tag.to_be_bytes());
        buf.extend_from_slice(&val.to_be_bytes());
    };
    push_tag(&mut vc5, 0x000c, 4);
    push_tag(&mut vc5, 0x0014, 16);
    push_tag(&mut vc5, 0x0015, 16);
    push_tag(&mut vc5, 0x006d, 0);
    for ch in 0u16..4 {
        push_tag(&mut vc5, 0x003e, ch);
        push_tag(&mut vc5, 0x0030, 0);
        push_tag(&mut vc5, 0x2000, 1);
        vc5.extend_from_slice(&[0x03, 0xE8, 0x00, 0x00]);
        for sb in 1u16..10 {
            push_tag(&mut vc5, 0x0030, sb);
            push_tag(&mut vc5, 0x2000, 2);
            vc5.extend_from_slice(&[0u8; 8]);
        }
    }
    let tags = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 16u16.to_le_bytes().to_vec()),
        (259, 3, 1, 9u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_le_bytes().to_vec()),
        (279, 4, 1, (vc5.len() as u32).to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
    ];
    let d = build_tiff(true, &tags, &vc5);
    let tf = write_temp("vc5_cfa.dng", &d)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!(r.cfa_signed.width(), 16);
    assert_eq!(r.cfa_signed.height(), 16);
    Ok(())
}

#[test]
fn fallback_dng_jpeg_cfa_tiled_and_rational() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::GrayImage::from_pixel(16, 16, image::Luma([128u8]));
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageLuma8(img).write_to(
        &mut std::io::Cursor::new(&mut jpeg),
        image::ImageFormat::Jpeg,
    )?;
    let srational_id: Vec<u8> = [1i32, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 1, 1]
        .into_iter()
        .flat_map(i32::to_le_bytes)
        .collect();
    let rational_wb: Vec<u8> = [1u32, 1, 1, 1, 1, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let tags = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 8u16.to_le_bytes().to_vec()),
        (259, 3, 1, 7u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (271, 2, 6, b"Maker\0".to_vec()),
        (272, 2, 6, b"Model\0".to_vec()),
        (277, 3, 1, 1u16.to_le_bytes().to_vec()),
        (322, 3, 1, 16u16.to_le_bytes().to_vec()),
        (323, 3, 1, 16u16.to_le_bytes().to_vec()),
        (324, 4, 1, vec![]),
        (325, 4, 1, (jpeg.len() as u32).to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
        (50721, 10, 9, srational_id.clone()),
        (50722, 10, 9, srational_id),
        (50728, 5, 3, rational_wb),
        (50779, 3, 1, 21u16.to_le_bytes().to_vec()),
    ];
    let d = build_tiff(true, &tags, &jpeg);
    let tf = write_temp("tiled_cfa.dng", &d)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!(r.metadata.camera_make, "Maker");
    assert_eq!(r.cfa_signed.width(), 16);
    assert_eq!(r.cfa_signed.height(), 16);
    Ok(())
}

#[test]
fn fallback_dng_jpeg_strip_and_error_variants() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::GrayImage::from_pixel(16, 16, image::Luma([100u8]));
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageLuma8(img).write_to(
        &mut std::io::Cursor::new(&mut jpeg),
        image::ImageFormat::Jpeg,
    )?;

    let tags = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 8u16.to_le_bytes().to_vec()),
        (259, 3, 1, 7u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_le_bytes().to_vec()),
        (278, 3, 1, 16u16.to_le_bytes().to_vec()),
        (279, 4, 1, (jpeg.len() as u32).to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
    ];
    let d = build_tiff(true, &tags, &jpeg);
    let tf = write_temp("jpeg_strip.dng", &d)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!(r.cfa_signed.width(), 16);
    assert_eq!(r.cfa_signed.height(), 16);

    let tags_bad_bps = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 12u16.to_le_bytes().to_vec()),
        (259, 3, 1, 1u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 1u16.to_le_bytes().to_vec()),
        (279, 4, 1, 0u32.to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
    ];
    let tf_bad = write_temp("bad_bps.dng", &build_tiff(true, &tags_bad_bps, &[]))?;
    assert!(super::try_decode_fallback(&tf_bad.0).is_err());

    let tags_spp2 = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 16u16.to_le_bytes().to_vec()),
        (259, 3, 1, 1u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (273, 4, 1, vec![]),
        (277, 3, 1, 2u16.to_le_bytes().to_vec()),
        (279, 4, 1, 0u32.to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
    ];
    let tf_spp2 = write_temp("spp2.dng", &build_tiff(true, &tags_spp2, &[]))?;
    assert!(super::try_decode_fallback(&tf_spp2.0).is_err());
    Ok(())
}

#[test]
fn fallback_dng_cinemadng_tile_and_no_valid_ifd() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::GrayImage::from_pixel(32, 8, image::Luma([120u8]));
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageLuma8(img).write_to(
        &mut std::io::Cursor::new(&mut jpeg),
        image::ImageFormat::Jpeg,
    )?;

    let tags = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (258, 3, 1, 8u16.to_le_bytes().to_vec()),
        (259, 3, 1, 34892u16.to_le_bytes().to_vec()),
        (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
        (277, 3, 1, 1u16.to_le_bytes().to_vec()),
        (322, 3, 1, 16u16.to_le_bytes().to_vec()),
        (323, 3, 1, 16u16.to_le_bytes().to_vec()),
        (324, 4, 1, vec![]),
        (325, 4, 1, (jpeg.len() as u32).to_le_bytes().to_vec()),
        (33422, 1, 4, vec![0, 1, 1, 2]),
    ];
    let tf = write_temp("cinemadng.dng", &build_tiff(true, &tags, &jpeg))?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!((r.cfa_signed.width(), r.cfa_signed.height()), (16, 16));

    let tags_bad = [
        (256, 3, 1, 16u16.to_le_bytes().to_vec()),
        (257, 3, 1, 16u16.to_le_bytes().to_vec()),
        (262, 3, 1, 1u16.to_le_bytes().to_vec()),
    ];
    let tf2 = write_temp("novalid_ifd.dng", &build_tiff(true, &tags_bad, &[]))?;
    assert!(super::try_decode_fallback(&tf2.0).is_err());
    Ok(())
}

#[test]
fn fallback_dng_subifd_cfa() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Vec<u8> = (0..256u16).flat_map(|i| (i % 4).to_le_bytes()).collect();
    let sub_ifd = build_tiff(
        true,
        &[
            (254, 4, 1, 0u32.to_le_bytes().to_vec()),
            (256, 3, 1, 16u16.to_le_bytes().to_vec()),
            (257, 3, 1, 16u16.to_le_bytes().to_vec()),
            (258, 3, 1, 16u16.to_le_bytes().to_vec()),
            (259, 3, 1, 1u16.to_le_bytes().to_vec()),
            (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
            (273, 4, 1, vec![]),
            (277, 3, 1, 1u16.to_le_bytes().to_vec()),
            (279, 4, 1, 512u32.to_le_bytes().to_vec()),
            (33422, 1, 4, vec![0, 1, 1, 2]),
        ],
        &payload,
    );
    // Strip the 8-byte TIFF header from sub_ifd and rebase its StripOffsets by +ifd0_len
    let sub_body = &sub_ifd[8..];
    let mut ifd0 = build_tiff(
        true,
        &[
            (254, 4, 1, 1u32.to_le_bytes().to_vec()),
            (330, 4, 1, 0u32.to_le_bytes().to_vec()),
        ],
        sub_body,
    );
    let sub_off = (ifd0.len() - sub_body.len()) as u32;
    // Patch tag 330 value in IFD0 (entry 1 at offset 8+2+12+8 = 30)
    ifd0[30..34].copy_from_slice(&sub_off.to_le_bytes());
    // Patch StripOffsets in sub_ifd (entry 6 at sub_off + 2 + 6*12 + 8 = sub_off + 82)
    let orig_strip_off = u32::from_le_bytes([
        ifd0[sub_off as usize + 82],
        ifd0[sub_off as usize + 83],
        ifd0[sub_off as usize + 84],
        ifd0[sub_off as usize + 85],
    ]);
    let new_strip_off = orig_strip_off - 8 + sub_off;
    ifd0[sub_off as usize + 82..sub_off as usize + 86]
        .copy_from_slice(&new_strip_off.to_le_bytes());

    let tf = write_temp("subifd.dng", &ifd0)?;
    let r = super::try_decode_fallback(&tf.0)?;
    assert_eq!((r.cfa_signed.width(), r.cfa_signed.height()), (16, 16));
    Ok(())
}

#[test]
fn fallback_dng_missing_offsets_errors() -> Result<(), Box<dyn std::error::Error>> {
    for comp in [1u16, 7, 9] {
        let tags = [
            (256, 3, 1, 16u16.to_le_bytes().to_vec()),
            (257, 3, 1, 16u16.to_le_bytes().to_vec()),
            (258, 3, 1, 16u16.to_le_bytes().to_vec()),
            (259, 3, 1, comp.to_le_bytes().to_vec()),
            (262, 3, 1, 32803u16.to_le_bytes().to_vec()),
            (277, 3, 1, 1u16.to_le_bytes().to_vec()),
            (33422, 1, 4, vec![0, 1, 1, 2]),
        ];
        let tf = write_temp(&format!("nooff_{comp}.dng"), &build_tiff(true, &tags, &[]))?;
        assert!(super::try_decode_fallback(&tf.0).is_err());
    }
    Ok(())
}
