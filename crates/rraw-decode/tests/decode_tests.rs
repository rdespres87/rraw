use rraw_core::{BayerOrigin, CfaPattern, NoiseProfileSource};
use rraw_decode::noise_profile::{estimate_noise_profile_wavelet_mad, parse_dng_noise_profile_tag};
use rraw_decode::opcodes::{parse_opcode_list, DngOpcode};
use rraw_decode::synthetic::SyntheticRawBuilder;

#[test]
fn test_dng_opcode_list_parsing_and_gainmap_flag() -> Result<(), Box<dyn std::error::Error>> {
    // Construct a synthetic DNG OpcodeList2 stream with:
    // Opcode 9: GainMap
    // Opcode 3: FixVignetteRadial
    let opcode_stream = SyntheticRawBuilder::build_synthetic_opcode_list_stream()?;
    let opcodes = parse_opcode_list(&opcode_stream)?;

    assert_eq!(opcodes.len(), 2);
    assert!(matches!(opcodes[0], DngOpcode::GainMap { .. }));
    assert!(matches!(opcodes[1], DngOpcode::FixVignetteRadial { .. }));

    let handles_gainmap = opcodes.iter().any(|op| {
        matches!(
            op,
            DngOpcode::GainMap { .. } | DngOpcode::FixVignetteRadial { .. }
        )
    });
    assert!(handles_gainmap);

    Ok(())
}

#[test]
fn test_dng_tag_0xc761_noise_profile_extraction() -> Result<(), Box<dyn std::error::Error>> {
    // Tag 0xc761 contains pairs of f64 (shot_noise_alpha, read_noise_sigma2) per active channel
    let raw_tag_data: [f64; 6] = [
        0.0012, 0.000045, // Channel 0 (Red)
        0.0009, 0.000030, // Channel 1 (Green)
        0.0015, 0.000060, // Channel 2 (Blue)
    ];

    let profile = parse_dng_noise_profile_tag(&raw_tag_data)?;
    assert_eq!(profile.source(), NoiseProfileSource::DngTag0xc761);
    assert!((profile.shot_gain_alpha()[0] - 0.0012).abs() < 1e-6);
    assert!((profile.read_variance_sigma2()[0] - 0.000045).abs() < 1e-7);

    // Degenerate/negative inputs must be rejected
    let bad_tag_data = [-0.01, 0.0001, 0.001, 0.0001, 0.001, 0.0001];
    assert!(parse_dng_noise_profile_tag(&bad_tag_data).is_err());

    Ok(())
}

#[test]
fn test_inspect_dng_crop_area() -> Result<(), Box<dyn std::error::Error>> {
    use rraw_decode::RawDecoder;
    let path = std::path::Path::new("/Users/rdespres/Desktop/test/IMGP4058.DNG");
    if path.exists() {
        let decoder = rraw_decode::PureRustRawDecoder::new();
        let decoded = decoder.decode_file(path)?;
        assert_eq!(decoded.metadata.width, 6016);
        assert_eq!(decoded.metadata.height, 4000);
        assert_eq!(decoded.cfa_signed.width(), 6016);
        assert_eq!(decoded.cfa_signed.height(), 4000);
        assert!(
            decoded.metadata.lens_model.is_some(),
            "Universal lens resolution must automatically extract lens model from DNG MakerNotes"
        );
        let lens = decoded
            .metadata
            .lens_model
            .as_ref()
            .ok_or("lens model must be present")?;
        assert!(lens.contains("PENTAX") || lens.contains("smc") || lens.contains("Sigma"));
    }
    Ok(())
}

#[test]
fn test_universal_makernote_lens_resolution_pentax_synthetic(
) -> Result<(), Box<dyn std::error::Error>> {
    use rraw_decode::makernotes::resolve_universal_lens;

    // Construct a synthetic Pentax MakerNote block with tag 0x003F = (8, 31) [Sigma 18-35mm F1.8 DC HSM]
    let mut synthetic_raw = Vec::new();
    synthetic_raw.extend_from_slice(b"II*\0\x08\0\0\0"); // TIFF Little Endian header
    synthetic_raw.extend_from_slice(b"PENTAX \0II"); // Pentax MakerNote header
    synthetic_raw.extend_from_slice(&1_u16.to_le_bytes()); // 1 entry in MakerNote IFD
    synthetic_raw.extend_from_slice(&0x003F_u16.to_le_bytes()); // Tag 0x003F (LensRec)
    synthetic_raw.extend_from_slice(&1_u16.to_le_bytes()); // Type BYTE
    synthetic_raw.extend_from_slice(&4_u32.to_le_bytes()); // Count 4
    synthetic_raw.extend_from_slice(&[8_u8, 31_u8, 0_u8, 0_u8]); // Value: (8, 31)

    let resolved = resolve_universal_lens(
        &synthetic_raw,
        "Pentax",
        "PENTAX K-1",
        Some(31.0),
        Some(1.8),
    );

    assert!(
        resolved.is_some(),
        "Synthetic Pentax MakerNote must resolve lens"
    );
    let name = resolved.ok_or("resolved lens must be present")?;
    assert_eq!(
        name, "Sigma 18-35mm F1.8 DC HSM",
        "Resolved lens must match Sigma 18-35mm F1.8 DC HSM without duplicate maker prefix (got: {name})"
    );
    Ok(())
}

#[test]
fn test_sanitize_lens_name_deduplication() {
    use rraw_decode::makernotes::sanitize_lens_name;
    assert_eq!(
        sanitize_lens_name("Sigma Sigma 18-35mm F1.8 DC HSM"),
        "Sigma 18-35mm F1.8 DC HSM"
    );
    assert_eq!(
        sanitize_lens_name("Canon Canon EF 24-70mm"),
        "Canon EF 24-70mm"
    );
    assert_eq!(
        sanitize_lens_name("smc PENTAX-DA* 300mm F4 ED [IF] SDM"),
        "smc PENTAX-DA* 300mm F4 ED [IF] SDM"
    );
}

#[test]
fn test_wavelet_mad_noise_profile_accuracy() -> Result<(), Box<dyn std::error::Error>> {
    // Generate a 256x256 synthetic flat CFA buffer with injected noise sigma = 0.02
    let width = 256;
    let height = 256;
    let target_sigma = 0.02_f32;
    let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);

    let synthetic_cfa = SyntheticRawBuilder::generate_flat_cfa_with_noise(
        width,
        height,
        pattern,
        0.18,
        target_sigma,
    )?;

    let estimated_profile = estimate_noise_profile_wavelet_mad(&synthetic_cfa)?;
    assert_eq!(
        estimated_profile.source(),
        NoiseProfileSource::WaveletMadFallback
    );

    for ch in 0..3 {
        let est_variance = estimated_profile.read_variance_sigma2()[ch];
        let est_sigma = est_variance.sqrt();
        // Tolerance: +/- 2% of 0.02
        let rel_error = (est_sigma - target_sigma).abs() / target_sigma;
        assert!(
            rel_error <= 0.02,
            "channel {ch} estimated sigma {est_sigma} diverged from target {target_sigma} by {rel_error:.4} (> 2%)"
        );
    }

    Ok(())
}

#[test]
fn test_signed_normalization_sub_black_zero_mean() -> Result<(), Box<dyn std::error::Error>> {
    let width = 128;
    let height = 128;
    let black_level = 512.0_f32;
    let white_level = 16383.0_f32;
    let noise_sigma = 8.0_f32;

    let (raw_samples, meta) = SyntheticRawBuilder::generate_dark_frame_raw(
        width,
        height,
        black_level,
        white_level,
        noise_sigma,
    )?;

    let normalized = rraw_decode::normalize_cfa_signed(&raw_samples, width, height, &meta)?;

    // Check that sub-black samples (< 512) are strictly negative
    let negative_samples_count = normalized.samples().iter().filter(|&&v| v < 0.0).count();
    assert!(
        negative_samples_count > (width * height) / 3,
        "expected Gaussian noise around black to produce abundant negative samples, got {negative_samples_count}"
    );

    // Compute mean of normalized noise around black: must be strictly centered at 0 (|mu| < 1e-4)
    let sum: f32 = normalized.samples().iter().sum();
    let mean = sum / (normalized.samples().len() as f32);
    assert!(
        mean.abs() < 1e-4,
        "normalized dark noise mean {mean} is not zero-centered (|mu| >= 1e-4)"
    );

    Ok(())
}

#[test]
fn test_normalize_cfa_signed_f32_preserves_sub_black_and_sanitizes_non_finite(
) -> Result<(), Box<dyn std::error::Error>> {
    let (_, meta) = SyntheticRawBuilder::generate_dark_frame_raw(4, 4, 0.1, 1.1, 0.0)?;
    let mut samples = vec![0.6_f32; 16];
    samples[0] = 0.05; // sub-black -> -0.05
    samples[1] = f32::NAN; // sanitized -> 0.0 -> (0.0 - 0.1) / 1.0 = -0.1
    samples[2] = f32::INFINITY; // sanitized -> 0.0 -> -0.1

    let normalized = rraw_decode::normalize_cfa_signed_f32(&samples, 4, 4, &meta)?;
    let out = normalized.samples();
    assert!((out[0] - (-0.05)).abs() < 1e-5);
    assert!(out[1].is_finite());
    assert!(out[2].is_finite());
    assert!((out[3] - 0.5).abs() < 1e-5);
    Ok(())
}

#[test]
fn test_normalize_linear_rgb_signed_u16_and_f32() -> Result<(), Box<dyn std::error::Error>> {
    // 2x2 interleaved RGB u16 with per-channel black/white levels
    let black = [100.0_f32, 200.0, 300.0];
    let white = [1100.0_f32, 1200.0, 1300.0];
    let raw_u16: Vec<u16> = vec![
        50, 200, 1300, // pixel 0: R sub-black (-0.05), G=0.0, B=1.0
        600, 700, 800, // pixel 1: (0.5, 0.5, 0.5)
        1100, 1200, 300, // pixel 2: (1.0, 1.0, 0.0)
        100, 200, 300, // pixel 3: (0.0, 0.0, 0.0)
    ];

    let buf = rraw_decode::normalize_linear_rgb_signed(&raw_u16, 2, 2, black, white)?;
    assert_eq!(buf.width(), 2);
    assert_eq!(buf.height(), 2);
    let px = buf.samples();
    assert!((px[0][0] - (-0.05)).abs() < 1e-5);
    assert!(px[0][1].abs() < 1e-5);
    assert!((px[0][2] - 1.0).abs() < 1e-5);
    assert!((px[1][0] - 0.5).abs() < 1e-5);
    assert!((px[1][1] - 0.5).abs() < 1e-5);
    assert!((px[1][2] - 0.5).abs() < 1e-5);

    // Floating-point normalization must sanitize NaN/Inf to 0.0
    let raw_f32: Vec<f32> = vec![f32::NAN, f32::INFINITY, 0.5, 0.25, 0.75, 1.0];
    let buf_f32 = rraw_decode::normalize_linear_rgb_signed_f32(&raw_f32, 2, 1, [0.0; 3], [1.0; 3])?;
    assert_eq!(buf_f32.samples()[0], [0.0, 0.0, 0.5]);
    assert_eq!(buf_f32.samples()[1], [0.25, 0.75, 1.0]);

    // Degenerate channel span and length mismatch must be rejected
    assert!(rraw_decode::normalize_linear_rgb_signed(&raw_u16[..5], 2, 2, black, white).is_err());
    assert!(rraw_decode::normalize_linear_rgb_signed(
        &raw_u16,
        2,
        2,
        [100.0, 200.0, 300.0],
        [100.0, 1200.0, 1300.0]
    )
    .is_err());

    Ok(())
}

#[test]
fn test_decode_real_linear_rgb_cpp3_samples_when_present() -> Result<(), Box<dyn std::error::Error>>
{
    use rraw_decode::RawDecoder;

    let candidates = [
        "/Users/rdespres/Projets/raw-pixls-samples/Canon/Canon - EOS 40D - sRAW2 (sRAW) (3:2).CR2",
        "/Users/rdespres/Projets/photo_data/Apple/iPhone 12 Pro/IMG_1361.DNG",
        "/Users/rdespres/Projets/photo_data/Adobe DNG Converter/Canon EOS 5D Mark III/5G4A9395-uncompressed.DNG",
    ];
    let decoder = rraw_decode::PureRustRawDecoder::new();
    for path_str in candidates {
        let path = std::path::Path::new(path_str);
        if !path.exists() {
            continue;
        }
        let decoded = decoder.decode_file(path)?;
        assert!(
            decoded.is_linear_rgb(),
            "expected cpp=3 Linear RGB for {path_str}"
        );
        if path_str.contains("sRAW") {
            assert_eq!(
                decoded.metadata.black_levels, [0.0; 4],
                "Canon sRAW YCbCr->RGB output must not subtract 14-bit Bayer black pedestal"
            );
        }
        let rgb = decoded
            .linear_rgb
            .as_ref()
            .ok_or("linear_rgb buffer must be populated")?;
        assert!(rgb.width() > 0 && rgb.height() > 0);
        assert_eq!(rgb.samples().len(), rgb.width() * rgb.height());
        assert!(rgb
            .samples()
            .iter()
            .take(64)
            .all(|px| px[0].is_finite() && px[1].is_finite() && px[2].is_finite()));
    }
    Ok(())
}
