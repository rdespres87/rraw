use super::*;
use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, NoiseProfile, NoiseProfileSource, PickStatus,
};

fn synthetic_metadata(width: usize, height: usize) -> Result<PhotoMetadata, CoreError> {
    Ok(PhotoMetadata {
        camera_make: "Synthetic".to_owned(),
        camera_model: "TestCam".to_owned(),
        lens_model: None,
        focal_length_mm: None,
        aperture_f_number: None,
        iso: Some(100),
        width,
        height,
        black_levels: [512.0; 4],
        white_levels: [16383.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile: NoiseProfile::new(
            [0.001; 3],
            [0.0001; 3],
            NoiseProfileSource::DngTag0xc761,
        )?,
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        rating: 0,
        pick: PickStatus::Unflagged,
        label: ColorLabel::None,
        keywords: Vec::new(),
    })
}

#[test]
fn signed_normalization_preserves_sub_black_samples() -> Result<(), DecodeError> {
    let meta = synthetic_metadata(2, 2)?;
    // Value 500 is below black level 512 and must remain strictly negative.
    let raw = [500_u16, 512_u16, 524_u16, 16383_u16];
    let signed = normalize_cfa_signed(&raw, 2, 2, &meta)?;
    assert!(signed.samples()[0] < 0.0);
    assert!((signed.samples()[1] - 0.0).abs() < 1e-7);
    assert!(signed.samples()[2] > 0.0);
    assert!((signed.samples()[3] - 1.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn signed_normalization_handles_xtrans_and_rejects_degenerate_span() -> Result<(), DecodeError> {
    let mut meta = synthetic_metadata(6, 1)?;
    meta.cfa_pattern = CfaPattern::fujifilm_xtrans();
    meta.black_levels = [100.0, 200.0, 300.0, 0.0];
    meta.white_levels = [1100.0, 1200.0, 1300.0, 0.0];
    // Row 0 of Fujifilm X-Trans: [G, B, G, G, R, G]
    let raw = [200_u16, 250_u16, 300_u16, 200_u16, 100_u16, 200_u16];
    let signed = normalize_cfa_signed(&raw, 6, 1, &meta)?;
    assert!((signed.samples()[0] - 0.0).abs() < 1e-6); // G at black=200
    assert!(signed.samples()[1] < 0.0); // B=250 < black=300
    assert!((signed.samples()[4] - 0.0).abs() < 1e-6); // R at black=100

    meta.white_levels[0] = meta.black_levels[0];
    assert!(matches!(
        normalize_cfa_signed(&raw, 6, 1, &meta),
        Err(DecodeError::MalformedMetadata(_))
    ));
    Ok(())
}

#[test]
fn signed_normalization_validates_dimensions_length_and_unvisited_channels(
) -> Result<(), DecodeError> {
    let meta_2x2 = synthetic_metadata(2, 2)?;

    // Buffer length mismatch
    assert_eq!(
        normalize_cfa_signed(&[512_u16; 3], 2, 2, &meta_2x2),
        Err(DecodeError::Core(CoreError::BufferLengthMismatch {
            expected: 4,
            actual: 3
        }))
    );

    // Dimension mismatch against metadata
    assert!(matches!(
        normalize_cfa_signed(&[512_u16; 4], 4, 1, &meta_2x2),
        Err(DecodeError::MalformedMetadata(_))
    ));

    // Zero dimensions
    let meta_zero = synthetic_metadata(0, 2)?;
    assert_eq!(
        normalize_cfa_signed(&[], 0, 2, &meta_zero),
        Err(DecodeError::Core(CoreError::InvalidDimensions {
            width: 0,
            height: 2
        }))
    );

    // Multiplication overflow
    let meta_overflow = synthetic_metadata(usize::MAX, 2)?;
    assert_eq!(
        normalize_cfa_signed(&[], usize::MAX, 2, &meta_overflow),
        Err(DecodeError::Core(CoreError::InvalidDimensions {
            width: usize::MAX,
            height: 2
        }))
    );

    // 1x1 Bayer image only visits channel 0, yet invalid channel 3 must be rejected upfront
    let mut meta_1x1 = synthetic_metadata(1, 1)?;
    meta_1x1.white_levels[3] = meta_1x1.black_levels[3];
    assert!(matches!(
        normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1),
        Err(DecodeError::MalformedMetadata(_))
    ));

    // 1x1 Bayer image with NaN in unvisited channel 2 must also be rejected upfront
    let mut meta_1x1_nan = synthetic_metadata(1, 1)?;
    meta_1x1_nan.black_levels[2] = f32::NAN;
    assert!(matches!(
        normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1_nan),
        Err(DecodeError::MalformedMetadata(_))
    ));

    // 1x1 X-Trans image visits only Green, yet invalid channel 2 (Blue) must be rejected
    let mut meta_1x1_xtrans = synthetic_metadata(1, 1)?;
    meta_1x1_xtrans.cfa_pattern = CfaPattern::fujifilm_xtrans();
    meta_1x1_xtrans.white_levels[2] = meta_1x1_xtrans.black_levels[2];
    assert!(matches!(
        normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1_xtrans),
        Err(DecodeError::MalformedMetadata(_))
    ));

    Ok(())
}

#[test]
fn test_decode_linear_rgb_synthetic() -> Result<(), Box<dyn std::error::Error>> {
    let cam = rawler::decoders::Camera::default();
    let pix = rawler::pixarray::PixU16::new_with(vec![1000u16; 16 * 16 * 3], 16 * 3, 16);
    let raw = rawler::RawImage::new(
        cam,
        pix,
        3,
        [1.0; 4],
        rawler::rawimage::RawPhotometricInterpretation::BlackIsZero,
        Some(rawler::rawimage::BlackLevel::new(&[100u16; 3], 1, 1, 3)),
        Some(rawler::rawimage::WhiteLevel::new(vec![4095; 3])),
        false,
    );
    let dec = super::decode_linear_rgb(std::path::Path::new("nonexistent_synth.dng"), &raw)?;
    assert_eq!(dec.metadata.width, 16);
    assert_eq!(dec.metadata.height, 16);
    assert!(dec.linear_rgb.is_some());
    Ok(())
}

#[test]
fn test_decode_bayer_or_xtrans_synthetic() -> Result<(), Box<dyn std::error::Error>> {
    let mut cam = rawler::decoders::Camera::default();
    let cfa = rawler::cfa::CFA::new("RGGB");
    cam.cfa = cfa.clone();
    let pix = rawler::pixarray::PixU16::new_with(vec![1000u16; 16 * 16], 16, 16);
    let raw = rawler::RawImage::new(
        cam,
        pix,
        1,
        [1.0; 4],
        rawler::rawimage::RawPhotometricInterpretation::Cfa(rawler::rawimage::CFAConfig::new(
            &cfa,
            &rawler::cfa::PlaneColor::default(),
        )),
        Some(rawler::rawimage::BlackLevel::new(&[100u16; 4], 2, 2, 1)),
        Some(rawler::rawimage::WhiteLevel::new(vec![4095; 4])),
        false,
    );
    let dec = super::decode_single_plane_cfa(std::path::Path::new("nonexistent_synth.dng"), &raw)?;
    assert_eq!(dec.cfa_signed.width(), 16);
    assert_eq!(dec.cfa_signed.height(), 16);
    Ok(())
}

#[test]
fn test_pure_rust_raw_decoder_fallback_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    let decoder = PureRustRawDecoder::new();
    assert!(decoder
        .decode_file(std::path::Path::new("definitely_missing_raw_file.dng"))
        .is_err());

    let img = image::RgbImage::from_pixel(4, 4, image::Rgb([128, 128, 128]));
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    let tf = TempFile(std::env::temp_dir().join(format!("rraw_disp_{}.dng", std::process::id())));
    std::fs::write(&tf.0, &png)?;
    let dec = decoder.decode_file(&tf.0)?;
    assert!(dec.is_linear_rgb());
    assert_eq!((dec.metadata.width, dec.metadata.height), (4, 4));

    const CIFF_DATA_END: usize = 26 + 1024 * 1940;
    let mut ciff = vec![0x55_u8; CIFF_DATA_END + 80];
    ciff[0..2].copy_from_slice(b"II");
    ciff[6..14].copy_from_slice(b"HEAPCCDR");
    ciff[CIFF_DATA_END..CIFF_DATA_END + 15].copy_from_slice(b"PowerShot Pro70");
    let tf_ciff =
        TempFile(std::env::temp_dir().join(format!("rraw_disp_ciff_{}.crw", std::process::id())));
    std::fs::write(&tf_ciff.0, &ciff)?;
    let dec_ciff = decoder.decode_file(&tf_ciff.0)?;
    assert!(dec_ciff.is_linear_rgb());

    let mut x3f = Vec::from(&b"FOVb"[..]);
    x3f.resize(40, 0);
    let ima_off = 40u32;
    let mut ima = Vec::new();
    ima.extend_from_slice(b"SECi");
    ima.extend_from_slice(&0u32.to_le_bytes());
    ima.extend_from_slice(&1u32.to_le_bytes());
    ima.extend_from_slice(&5u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&0u32.to_le_bytes());
    ima.resize(28 + 2048 + 16 * 16 * 4, 0);
    let ima_len = ima.len() as u32;
    x3f.extend_from_slice(&ima);
    let dir_off = x3f.len() as u32;
    x3f.extend_from_slice(b"SECd");
    x3f.extend_from_slice(&0u32.to_le_bytes());
    x3f.extend_from_slice(&1u32.to_le_bytes());
    x3f.extend_from_slice(&ima_off.to_le_bytes());
    x3f.extend_from_slice(&ima_len.to_le_bytes());
    x3f.extend_from_slice(b"IMA2");
    x3f.extend_from_slice(&dir_off.to_le_bytes());
    let tf_x3f =
        TempFile(std::env::temp_dir().join(format!("rraw_disp_x3f_{}.x3f", std::process::id())));
    std::fs::write(&tf_x3f.0, &x3f)?;
    let dec_x3f = decoder.decode_file(&tf_x3f.0)?;
    assert!(dec_x3f.is_linear_rgb());
    Ok(())
}

#[test]
fn test_decode_single_plane_cfa_special_sensors() -> Result<(), Box<dyn std::error::Error>> {
    let p = std::path::Path::new("synth_special.raf");

    let mut cam = rawler::decoders::Camera::default();
    let pix = rawler::pixarray::PixU16::new_with(vec![1500u16; 256], 16, 16);
    let raw_mono = rawler::RawImage::new(
        cam.clone(),
        pix,
        1,
        [1.0; 4],
        rawler::rawimage::RawPhotometricInterpretation::BlackIsZero,
        Some(rawler::rawimage::BlackLevel::new(&[100u16; 4], 2, 2, 1)),
        Some(rawler::rawimage::WhiteLevel::new(vec![4095; 4])),
        false,
    );
    let dec_mono = super::decode_single_plane_cfa(p, &raw_mono)?;
    assert!(dec_mono.is_linear_rgb());

    let cfa = rawler::cfa::CFA::new("RGGB");
    cam.cfa = cfa.clone();
    let pix2 = rawler::pixarray::PixU16::new_with(vec![1500u16; 1024], 32, 32);
    let mut raw_fuji = rawler::RawImage::new(
        cam.clone(),
        pix2,
        1,
        [1.0; 4],
        rawler::rawimage::RawPhotometricInterpretation::Cfa(rawler::rawimage::CFAConfig::new(
            &cfa,
            &rawler::cfa::PlaneColor::default(),
        )),
        Some(rawler::rawimage::BlackLevel::new(&[100u16; 4], 2, 2, 1)),
        Some(rawler::rawimage::WhiteLevel::new(vec![4095; 4])),
        false,
    );
    raw_fuji.fuji_rotation_width = Some(20);
    let dec_fuji = super::decode_single_plane_cfa(p, &raw_fuji)?;
    assert!(dec_fuji.is_linear_rgb());

    let cfa4 = rawler::cfa::CFA::new("RGBE");
    cam.cfa = cfa4.clone();
    let pix3 = rawler::pixarray::PixU16::new_with(vec![1500u16; 256], 16, 16);
    let mut raw4 = rawler::RawImage::new(
        cam,
        pix3,
        1,
        [1.0; 4],
        rawler::rawimage::RawPhotometricInterpretation::Cfa(rawler::rawimage::CFAConfig::new(
            &cfa4,
            &rawler::cfa::PlaneColor::default(),
        )),
        Some(rawler::rawimage::BlackLevel::new(&[100u16; 4], 2, 2, 1)),
        Some(rawler::rawimage::WhiteLevel::new(vec![4095; 4])),
        false,
    );
    raw4.color_matrix.insert(
        rawler::imgop::xyz::Illuminant::D65,
        vec![0.8, 0.2, 0.0, 0.1, 0.7, 0.2, 0.0, 0.3, 0.7, 0.5, 0.4, 0.1],
    );
    let dec4 = super::decode_single_plane_cfa(p, &raw4)?;
    assert!(dec4.is_linear_rgb());
    Ok(())
}
