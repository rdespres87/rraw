#![forbid(unsafe_code)]

//! Command-line reference runner, optical inspector, and golden test tool for Rraw.

use std::path::PathBuf;

use clap::Parser;
use rraw_color::pipeline::{CpuReferencePipeline, CpuRgbImage};
use rraw_core::{BayerOrigin, CfaPattern, Recipe, SignedCfaBuffer, WhiteBalanceMultipliers};
use rraw_decode::{DecodeError, PureRustRawDecoder, RawDecoder};

#[derive(Parser, Debug)]
#[command(
    name = "rraw-cli",
    version,
    about = "Pure-Rust command-line RAW photographic processor and reference pipeline"
)]
struct CliArgs {
    /// Input camera RAW file path (e.g. .dng, .arw, .nef, .cr2, .cr3).
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Output 16-bit RGB PNG image path.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Exposure compensation adjustment in EV stops.
    #[arg(short, long, default_value_t = 0.0)]
    exposure: f32,

    /// Extended Dynamic Range (EDR) active display headroom in [1.0, 16.0].
    #[arg(short = 'H', long, default_value_t = 1.0)]
    headroom: f32,

    /// Execute synthetic golden Bayer luminance ramp test without external file.
    #[arg(long)]
    synthetic_ramp: bool,
}

/// Runs the synthetic golden ramp reference pipeline, returning the rendered 16-bit image.
fn run_synthetic_golden_ramp(headroom: f32) -> Result<CpuRgbImage, DecodeError> {
    let width = 128;
    let height = 128;
    let wb = WhiteBalanceMultipliers::new(2.1, 1.0, 1.6)
        .map_err(|e| DecodeError::MalformedMetadata(e.to_string()))?;

    let mut samples = Vec::with_capacity(width * height);
    for y in 0..height {
        let l = (y as f32 / (height - 1) as f32) * 1.2;
        for x in 0..width {
            let sample = match (x % 2, y % 2) {
                (0, 0) => l / 2.1,
                (1, 0) => l / 1.0,
                (0, 1) => l / 1.0,
                (1, 1) => l / 1.6,
                _ => l,
            };
            samples.push(sample);
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)?;

    let mut recipe = Recipe::default();
    recipe.stage2_demosaic.wb_multipliers = wb;

    let pipeline = CpuReferencePipeline::new();
    let image = pipeline
        .execute(&cfa, &recipe, headroom)
        .map_err(|e| DecodeError::MalformedMetadata(e.to_string()))?;

    Ok(image)
}

fn run() -> Result<(), DecodeError> {
    let args = CliArgs::parse();

    if args.synthetic_ramp {
        println!("Running synthetic Bayer golden luminance ramp (128x128)...");
        let image = run_synthetic_golden_ramp(args.headroom)?;

        if let Some(out_path) = args.output {
            let png_bytes = image
                .to_png_16bit_bytes()
                .map_err(|e| DecodeError::MalformedMetadata(e.to_string()))?;
            std::fs::write(&out_path, &png_bytes).map_err(|e| DecodeError::Io(e.to_string()))?;
            println!(
                "Saved 16-bit PNG golden image: {} ({} bytes)",
                out_path.display(),
                png_bytes.len()
            );
        } else {
            println!(
                "Synthetic ramp passed: {}x{} RGB image rendered.",
                image.width(),
                image.height()
            );
        }
        return Ok(());
    }

    if let Some(input_path) = args.input {
        let decoder = PureRustRawDecoder::new();
        let decoded = decoder.decode_file(&input_path)?;
        println!(
            "Decoded RAW: {}x{}, camera: {} {}, CFA: {:?}",
            decoded.metadata.width,
            decoded.metadata.height,
            decoded.metadata.camera_make,
            decoded.metadata.camera_model,
            decoded.metadata.cfa_pattern,
        );

        let mut recipe = Recipe::default();
        recipe.stage5_grading.exposure_ev = args.exposure;

        let pipeline = CpuReferencePipeline::new();
        let image = pipeline
            .execute(&decoded.cfa_signed, &recipe, args.headroom)
            .map_err(|e| DecodeError::MalformedMetadata(e.to_string()))?;

        if let Some(out_path) = args.output {
            let png_bytes = image
                .to_png_16bit_bytes()
                .map_err(|e| DecodeError::MalformedMetadata(e.to_string()))?;
            std::fs::write(&out_path, &png_bytes).map_err(|e| DecodeError::Io(e.to_string()))?;
            println!(
                "Exported 16-bit PNG: {} ({} bytes)",
                out_path.display(),
                png_bytes.len()
            );
        }
    } else {
        println!("rraw-cli: No input file specified. Use --help for usage or --synthetic-ramp for golden test.");
    }

    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("rraw-cli error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_synthetic_golden_ramp_executes_cleanly() -> Result<(), DecodeError> {
        let image = run_synthetic_golden_ramp(1.0)?;
        assert_eq!(image.width(), 128);
        assert_eq!(image.height(), 128);
        let bytes = image
            .to_png_16bit_bytes()
            .map_err(|e| DecodeError::MalformedMetadata(e.to_string()))?;
        assert!(bytes.len() > 100);
        Ok(())
    }
}
