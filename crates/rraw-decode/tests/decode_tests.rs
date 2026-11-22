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
