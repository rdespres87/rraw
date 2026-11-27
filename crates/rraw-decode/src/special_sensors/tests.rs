#![forbid(unsafe_code)]

use super::four_color::{demosaic_four_color_bilinear, pseudo_inverse_4x3};
use super::superccd::{fuji_rotated_dimensions, rotate_rgb_45cw};

#[test]
fn pseudo_inverse_4x3_recovers_identity() {
    let m = [
        [0.8, 0.2, 0.0],
        [0.1, 0.7, 0.2],
        [0.0, 0.3, 0.7],
        [0.5, 0.4, 0.1],
    ];
    let pinv = pseudo_inverse_4x3(&m);
    assert!(pinv.is_some());
    if let Some(pinv) = pinv {
        for (r, pinv_row) in pinv.iter().enumerate() {
            for c in 0..3 {
                let prod = pinv_row[0] * m[0][c]
                    + pinv_row[1] * m[1][c]
                    + pinv_row[2] * m[2][c]
                    + pinv_row[3] * m[3][c];
                let expected = if r == c { 1.0 } else { 0.0 };
                assert!(
                    (prod - expected).abs() < 1e-4,
                    "pinv*m[{r}][{c}] = {prod}, expected {expected}"
                );
            }
        }
    }
}

#[test]
fn rotate_rgb_45cw_preserves_uniform_color_and_inscribed_dims() {
    let src_w = 100;
    let src_h = 100;
    let fuji_w = 60;
    let (exp_w, exp_h) = fuji_rotated_dimensions(src_w, fuji_w);
    assert!(exp_w > exp_h);
    assert_eq!(exp_w, (60.0 * std::f64::consts::SQRT_2).floor() as usize);
    assert_eq!(exp_h, (40.0 * std::f64::consts::SQRT_2).floor() as usize);

    let src = vec![[0.25_f32, 0.5, 0.75]; src_w * src_h];
    let res = rotate_rgb_45cw(&src, src_w, src_h, fuji_w);
    assert!(res.is_ok());
    if let Ok((w, h, pixels)) = res {
        assert_eq!((w, h), (exp_w, exp_h));
        let center = pixels[(h / 2) * w + (w / 2)];
        assert!((center[0] - 0.25).abs() < 1e-4);
        assert!((center[1] - 0.50).abs() < 1e-4);
        assert!((center[2] - 0.75).abs() < 1e-4);
    }
}

#[test]
fn four_color_bilinear_recovers_constant_channels_everywhere() {
    let w = 6;
    let h = 6;
    let lut = [[0, 1], [2, 3]];
    let vals = [0.2_f32, 0.4, 0.6, 0.8];
    let mut plane = vec![0.0_f32; w * h];
    for y in 0..h {
        for x in 0..w {
            plane[y * w + x] = vals[lut[y & 1][x & 1]];
        }
    }

    let out = demosaic_four_color_bilinear(&plane, w, h, lut);
    for (idx, pix) in out.iter().enumerate() {
        for ch in 0..4 {
            assert!(
                (pix[ch] - vals[ch]).abs() < 1e-5,
                "pixel {idx} ch {ch}: got {}, expected {}",
                pix[ch],
                vals[ch]
            );
        }
    }
}

#[test]
fn detect_isolated_bayer_hot_pixels_heals_spike_and_preserves_highlights() {
    let w = 12;
    let h = 12;
    let mut plane = vec![0.08_f32; w * h];
    // Isolated saturated hot pixel at (5, 5)
    plane[5 * w + 5] = 1.04;
    // Isolated shadow warm pixel at (9, 5)
    plane[5 * w + 9] = 0.25;
    // Isolated dead pixel at (5, 9)
    plane[9 * w + 5] = 0.0;
    // Real diagonal 2-pixel same-color highlight at (2, 2) and (4, 4) must NOT be touched
    plane[2 * w + 2] = 1.00;
    plane[4 * w + 4] = 0.98;

    let fixes = super::detect_isolated_bayer_hot_pixels(&plane, w, h);
    assert_eq!(fixes.len(), 3);
    let by_idx: std::collections::HashMap<usize, f32> = fixes.into_iter().collect();
    assert!((by_idx[&(5 * w + 5)] - 0.08).abs() < 1e-6);
    assert!((by_idx[&(5 * w + 9)] - 0.08).abs() < 1e-6);
    assert!((by_idx[&(9 * w + 5)] - 0.08).abs() < 1e-6);
}

#[test]
fn canon_pro70_ciff_synthetic_and_errors() -> Result<(), Box<dyn std::error::Error>> {
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let dir = std::env::temp_dir();
    let tf_bad = TempFile(dir.join(format!("rraw_test_bad_{}.ciff", std::process::id())));
    std::fs::write(&tf_bad.0, b"NOT_CIFF")?;
    assert!(super::try_decode_canon_pro70_ciff(&tf_bad.0).is_err());

    const SIZE: usize = 26 + 1024 * 1940 + 80;
    let mut buf = vec![0x55_u8; SIZE];
    buf[0..2].copy_from_slice(b"II");
    buf[6..14].copy_from_slice(b"HEAPCCDR");
    let tail_start = 26 + 1024 * 1940;
    buf[tail_start..tail_start + 15].copy_from_slice(b"PowerShot Pro70");
    let tf = TempFile(dir.join(format!("rraw_test_good_{}.ciff", std::process::id())));
    std::fs::write(&tf.0, &buf)?;
    let raw = super::try_decode_canon_pro70_ciff(&tf.0)?;
    assert_eq!(raw.metadata.width, 1536);
    assert_eq!(raw.metadata.height, 1024);
    assert!(raw.linear_rgb.is_some());
    Ok(())
}

#[test]
fn is_four_color_2x2_cfa_checks() {
    assert!(super::is_four_color_2x2_cfa(&rawler::cfa::CFA::new("CYGM")));
    assert!(!super::is_four_color_2x2_cfa(&rawler::cfa::CFA::new(
        "RGGB"
    )));
}

fn make_test_raw_image(w: usize, h: usize, data: Vec<u16>, cfa_str: &str) -> rawler::RawImage {
    let cam = rawler::decoders::Camera {
        cfa: rawler::cfa::CFA::new(cfa_str),
        ..Default::default()
    };
    let pix = rawler::pixarray::PixU16::new_with(data, w, h);
    rawler::RawImage::new(
        cam,
        pix,
        1,
        [1.0; 4],
        rawler::rawimage::RawPhotometricInterpretation::BlackIsZero,
        Some(rawler::rawimage::BlackLevel::new(&[100u16; 4], 2, 2, 1)),
        Some(rawler::rawimage::WhiteLevel::new(vec![4095; 4])),
        false,
    )
}

#[test]
fn test_demosaic_and_rotate_fuji_superccd_synthetic() -> Result<(), Box<dyn std::error::Error>> {
    let raw = make_test_raw_image(32, 32, vec![1000u16; 1024], "RGGB");
    let buf = super::demosaic_and_rotate_fuji_superccd(
        &raw,
        &rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
        20,
    )?;
    assert!(buf.width() > 0 && buf.height() > 0);
    Ok(())
}

#[test]
fn test_decode_monochrome_sensor_to_linear_rgb_synthetic() -> Result<(), Box<dyn std::error::Error>>
{
    let raw = make_test_raw_image(16, 16, vec![2000u16; 256], "");
    let buf = super::decode_monochrome_sensor_to_linear_rgb(&raw)?;
    assert!(buf.width() == 16 && buf.height() == 16);

    let mut raw2 = make_test_raw_image(16, 16, vec![2000u16; 256], "");
    raw2.data = rawler::RawImageData::Float(vec![2000.0; 256]);
    let buf2 = super::decode_monochrome_sensor_to_linear_rgb(&raw2)?;
    assert!(buf2.width() == 16 && buf2.height() == 16);
    Ok(())
}

#[test]
fn test_demosaic_four_color_cfa_error_on_bayer() {
    let raw = make_test_raw_image(16, 16, vec![1000u16; 256], "RGGB");
    assert!(
        super::demosaic_four_color_cfa_to_linear_srgb(&raw, &rawler::cfa::CFA::new("RGGB"))
            .is_err()
    );
}

#[test]
fn test_demosaic_four_color_cfa_to_linear_srgb_synthetic() -> Result<(), Box<dyn std::error::Error>>
{
    let mut raw = make_test_raw_image(16, 16, vec![1000u16; 256], "RGBE");
    raw.color_matrix.insert(
        rawler::imgop::xyz::Illuminant::D65,
        vec![0.8, 0.2, 0.0, 0.1, 0.7, 0.2, 0.0, 0.3, 0.7, 0.5, 0.4, 0.1],
    );
    let buf = super::demosaic_four_color_cfa_to_linear_srgb(&raw, &rawler::cfa::CFA::new("RGBE"))?;
    assert_eq!((buf.width(), buf.height()), (16, 16));
    Ok(())
}

#[test]
fn test_demosaic_four_color_cfa_float_synthetic() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = make_test_raw_image(16, 16, vec![1000u16; 256], "RGBE");
    raw.data = rawler::RawImageData::Float(vec![1000.0; 256]);
    raw.color_matrix.insert(
        rawler::imgop::xyz::Illuminant::D65,
        vec![0.8, 0.2, 0.0, 0.1, 0.7, 0.2, 0.0, 0.3, 0.7, 0.5, 0.4, 0.1],
    );
    let buf = super::demosaic_four_color_cfa_to_linear_srgb(&raw, &rawler::cfa::CFA::new("RGBE"))?;
    assert_eq!((buf.width(), buf.height()), (16, 16));
    Ok(())
}

#[test]
fn test_fuji_superccd_float_and_xtrans_err() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = make_test_raw_image(32, 32, vec![1000u16; 1024], "RGGB");
    raw.data = rawler::RawImageData::Float(vec![1000.0; 1024]);
    let buf = super::demosaic_and_rotate_fuji_superccd(
        &raw,
        &rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
        20,
    )?;
    assert!(buf.width() > 0 && buf.height() > 0);
    assert!(super::demosaic_and_rotate_fuji_superccd(
        &raw,
        &rraw_core::CfaPattern::fujifilm_xtrans(),
        20,
    )
    .is_err());
    Ok(())
}

#[test]
fn test_monochrome_and_4color_crop_area() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw_mono = make_test_raw_image(16, 16, vec![2000u16; 256], "");
    raw_mono.crop_area = Some(rawler::imgop::Rect::new(
        rawler::imgop::Point::new(2, 2),
        rawler::imgop::Dim2::new(10, 10),
    ));
    let buf = super::decode_monochrome_sensor_to_linear_rgb(&raw_mono)?;
    assert_eq!((buf.width(), buf.height()), (10, 10));

    let mut raw4 = make_test_raw_image(16, 16, vec![1000u16; 256], "RGBE");
    raw4.crop_area = Some(rawler::imgop::Rect::new(
        rawler::imgop::Point::new(2, 2),
        rawler::imgop::Dim2::new(12, 12),
    ));
    raw4.color_matrix.insert(
        rawler::imgop::xyz::Illuminant::D65,
        vec![0.8, 0.2, 0.0, 0.1, 0.7, 0.2, 0.0, 0.3, 0.7, 0.5, 0.4, 0.1],
    );
    let buf4 =
        super::demosaic_four_color_cfa_to_linear_srgb(&raw4, &rawler::cfa::CFA::new("RGBE"))?;
    assert_eq!((buf4.width(), buf4.height()), (12, 12));

    let mut raw_fuji = make_test_raw_image(32, 32, vec![1000u16; 1024], "RGGB");
    raw_fuji.crop_area = Some(rawler::imgop::Rect::new(
        rawler::imgop::Point::new(2, 2),
        rawler::imgop::Dim2::new(28, 28),
    ));
    let buf_fuji = super::demosaic_and_rotate_fuji_superccd(
        &raw_fuji,
        &rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
        18,
    )?;
    assert!(buf_fuji.width() > 0 && buf_fuji.height() > 0);
    Ok(())
}
