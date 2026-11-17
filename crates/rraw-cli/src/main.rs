#![forbid(unsafe_code)]

//! Lightweight CLI orchestrating Rraw CPU/GPU reference pipelines and golden tests.

use std::path::Path;

use rraw_ai::is_token_current;
use rraw_color::{effective_laplacian_octaves, DisplayTransform};
use rraw_core::{PhotoId, Stage0Epoch, Stage0Token, TileGeometry};
use rraw_decode::DecodeError;
use rraw_export::ExportSpec;
use rraw_gpu::{compute_vram_budget_bytes, GpuContextMode};
use rraw_optics::OpticalResolutionTier;
use rraw_storage::SidecarPaths;

/// Summarizes the static pipeline configuration across CLI-connected crates.
fn describe_pipeline_defaults(sample_path: &Path) -> String {
    let sidecars = SidecarPaths::from_raw_path(sample_path);
    let export_spec = ExportSpec::sdr_png16(DisplayTransform::AgX);
    let token = Stage0Token {
        photo_id: PhotoId(1),
        epoch: Stage0Epoch(1),
    };
    let octaves = effective_laplacian_octaves(6, 0);
    let vram_cap_mib = compute_vram_budget_bytes(16 * 1024 * 1024 * 1024) / (1024 * 1024);
    format!(
        "rraw-cli ready | gpu={:?} | optics={:?} | tile={}x{} | octaves={} | vram_cap={}MiB | export={:?} | token_ok={} | recipe={}",
        GpuContextMode::HeadlessOffscreen,
        OpticalResolutionTier::Tier1EmbeddedRawOpcodes,
        TileGeometry::TILE_SIZE,
        TileGeometry::TILE_SIZE,
        octaves,
        vram_cap_mib,
        export_spec.format,
        is_token_current(token, token),
        sidecars.recipe_json.display(),
    )
}

fn run() -> Result<(), DecodeError> {
    let summary = describe_pipeline_defaults(Path::new("sample.dng"));
    println!("{summary}");
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
    fn cli_pipeline_summary_reflects_frozen_tile_and_vram_contracts() {
        let text = describe_pipeline_defaults(Path::new("/tmp/test.arw"));
        assert!(text.contains("tile=504x504"));
        assert!(text.contains("vram_cap=2048MiB"));
        assert!(text.contains("token_ok=true"));
        assert!(text.contains("/tmp/test.arw.rraw.json"));
    }
}
