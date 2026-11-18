use rraw_color::pipeline::{CpuReferencePipeline, CpuRgbImage};
use rraw_core::{BayerOrigin, CfaPattern, Recipe, SignedCfaBuffer, WhiteBalanceMultipliers};

#[test]
fn test_cpu_pipeline_neutral_ramp_preserves_neutrality_without_magenta_cast(
) -> Result<(), Box<dyn std::error::Error>> {
    let width = 64;
    let height = 64;
    let wb = WhiteBalanceMultipliers::new(2.1, 1.0, 1.6)?;

    // Create a synthetic Bayer RGGB linear luminance ramp from 0.0 to 1.2
    // For a neutral scene, physical raw samples before white balance are:
    // raw_c = L / w_c
    // so after multiplying by w_c, every channel receives exactly L.
    let mut samples = Vec::with_capacity(width * height);
    for y in 0..height {
        let l = (y as f32 / (height - 1) as f32) * 1.2; // linear luminance 0.0 .. 1.2
        for x in 0..width {
            let sample = match (x % 2, y % 2) {
                (0, 0) => l / 2.1, // R
                (1, 0) => l / 1.0, // Gr
                (0, 1) => l / 1.0, // Gb
                (1, 1) => l / 1.6, // B
                _ => l,
            };
            samples.push(sample);
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)?;

    let mut recipe = Recipe::default();
    recipe.stage2_demosaic.wb_multipliers = wb;

    let pipeline = CpuReferencePipeline::new();
    let rgb_image: CpuRgbImage = pipeline.execute(&cfa, &recipe, 1.0)?;

    assert_eq!(rgb_image.width(), width);
    assert_eq!(rgb_image.height(), height);

    // Verify neutrality: for every pixel within valid region (avoiding boundary 2px),
    // R, G, B must be equal within 1e-4 up to saturation ceiling
    for y in 2..height - 2 {
        for x in 2..width - 2 {
            let [r, g, b] = rgb_image.pixel(x, y);
            assert!(
                (r - g).abs() < 1e-4,
                "neutrality violation R vs G at ({x}, {y}): R={r}, G={g}, diff={}",
                (r - g).abs()
            );
            assert!(
                (b - g).abs() < 1e-4,
                "neutrality violation B vs G at ({x}, {y}): B={b}, G={g}, diff={}",
                (b - g).abs()
            );
        }
    }

    Ok(())
}

#[test]
fn test_cpu_pipeline_png_16bit_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let width = 32;
    let height = 32;
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let v = (x + y) as f32 / ((width + height) as f32);
            pixels.push([v, v, v]);
        }
    }

    let image = CpuRgbImage::new(width, height, pixels)?;
    let png_bytes = image.to_png_16bit_bytes()?;

    assert!(png_bytes.len() > 8);
    // Standard PNG signature: 137 80 78 71 13 10 26 10
    assert_eq!(&png_bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);

    // Decode header using `png` crate
    let cursor = std::io::Cursor::new(png_bytes);
    let decoder = png::Decoder::new(cursor);
    let reader = decoder.read_info()?;
    let info = reader.info();

    assert_eq!(info.width, width as u32);
    assert_eq!(info.height, height as u32);
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Sixteen);

    Ok(())
}
