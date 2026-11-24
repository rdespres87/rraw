#![forbid(unsafe_code)]

//! Non-destructive `<photo>.rraw.json` sidecar serialization and atomic persistence.

use std::path::Path;

use rraw_storage::SidecarPaths;
use serde::{Deserialize, Serialize};

use crate::grading::*;
use crate::mask_layers::{LocalMaskLayer, SemanticMaskLayer};
use crate::presets::GradingPreset;
use crate::state::{CustomClutState, DarkroomGradingState};

/// Serialized recipe saved into adjacent `<photo>.rraw.json` sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarRecipe {
    pub schema_version: u32,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub rating: u8,
    #[serde(flatten)]
    pub tone: ToneGradingParams,
    #[serde(flatten)]
    pub color: ColorRecipeParams,
    #[serde(flatten)]
    pub denoise_sharpen: DenoiseSharpenParams,
    #[serde(flatten)]
    pub optics: OpticsGradingParams,
    #[serde(flatten)]
    pub effects: EffectsGradingParams,
    #[serde(flatten)]
    pub geometry: GeometryRecipeParams,
    #[serde(default)]
    pub tone_curves: rraw_core::curves::ToneCurvesRecipe,
    pub custom_lut_path: Option<String>,
    #[serde(default = "default_one_f32")]
    pub custom_lut_strength: f32,
    #[serde(default)]
    pub semantic_masks: Vec<SemanticMaskLayer>,
    #[serde(default)]
    pub local_masks: Vec<LocalMaskLayer>,
    #[serde(default)]
    pub soft_proof: rraw_color::SoftProofConfig,
    #[serde(default)]
    pub smart_lighting: rraw_core::SmartLightingParams,
}

impl SidecarRecipe {
    #[must_use]
    pub fn from_state(grading: &DarkroomGradingState, rating: u8) -> Self {
        Self {
            schema_version: 1,
            preset: Some(grading.preset.id_str().to_string()),
            rating,
            tone: grading.tone.clone(),
            color: ColorRecipeParams::from(&grading.color),
            denoise_sharpen: grading.denoise_sharpen.clone(),
            optics: grading.optics.clone(),
            effects: grading.effects.clone(),
            geometry: GeometryRecipeParams::from(&grading.geometry),
            tone_curves: grading.tone_curves.clone(),
            custom_lut_path: grading
                .custom_clut
                .as_ref()
                .and_then(|c| c.path.as_ref().map(|p| p.to_string_lossy().to_string())),
            custom_lut_strength: grading.custom_clut.as_ref().map_or(1.0, |c| c.strength),
            semantic_masks: grading.masks.semantic_masks.clone(),
            local_masks: grading.masks.local_masks.clone(),
            soft_proof: grading.soft_proof,
            smart_lighting: grading.smart_lighting.clone(),
        }
    }

    pub fn apply_to(&self, grading: &mut DarkroomGradingState) {
        grading.tone = self.tone.clone();
        self.color.apply_to(&mut grading.color);
        grading.denoise_sharpen = self.denoise_sharpen.clone();
        grading.optics = self.optics.clone();
        grading.effects = self.effects.clone();
        self.geometry.apply_to(&mut grading.geometry);
        grading.tone_curves = self.tone_curves.clone();
        if let Some(path_str) = &self.custom_lut_path {
            let path = std::path::PathBuf::from(path_str);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(lut) = rraw_color::ColorLut3d::from_cube_str(&content) {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Custom LUT")
                            .to_string();
                        grading.custom_clut = Some(CustomClutState {
                            name,
                            path: Some(path),
                            lut: std::sync::Arc::new(lut),
                            strength: self.custom_lut_strength.clamp(0.0, 1.0),
                        });
                    }
                }
            }
        } else {
            grading.custom_clut = None;
        }
        grading.masks.semantic_masks = self.semantic_masks.clone();
        grading.masks.local_masks = self.local_masks.clone();
        grading.soft_proof = self.soft_proof;
        grading.smart_lighting = self.smart_lighting.clone();
        if let Some(ref pid) = self.preset {
            if let Some(preset) = GradingPreset::from_id_str(pid) {
                grading.preset = preset;
            }
        }
    }
}

/// Writes `bytes` to `target` atomically using a temporary adjacent file and POSIX/OS rename.
pub fn atomic_write_bytes(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("sidecar");
    let tmp_path = target.with_file_name(format!(".{file_name}.tmp.{pid}.{seq}"));

    let mut file = std::fs::File::create(&tmp_path)?;
    if let Err(err) = file
        .write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
    {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }
    drop(file);

    if let Err(err) = std::fs::rename(&tmp_path, target) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

/// Writes `content` to `target` atomically using a temporary adjacent file and POSIX/OS rename.
pub fn atomic_write_str(target: &Path, content: &str) -> std::io::Result<()> {
    atomic_write_bytes(target, content.as_bytes())
}

pub fn save_photo_sidecars(photo_path: &Path, grading: &DarkroomGradingState, rating: u8) {
    let sidecars = SidecarPaths::from_raw_path(photo_path);

    // 1. Ensure canonical .rraw directory exists
    if let Some(parent) = sidecars.recipe_json.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // 2. Write canonical <photo>.rraw.json recipe atomically inside .rraw/
    let recipe = SidecarRecipe::from_state(grading, rating);
    if let Ok(json_str) = serde_json::to_string_pretty(&recipe) {
        if let Err(err) = atomic_write_str(&sidecars.recipe_json, &json_str) {
            eprintln!(
                "rraw: failed to persist recipe sidecar {}: {err}",
                sidecars.recipe_json.display()
            );
        }
    }
}

/// Attempts to load non-destructive settings from canonical `.rraw/<photo>.rraw.json`.
#[must_use]
pub fn load_photo_sidecars(photo_path: &Path) -> (Option<DarkroomGradingState>, Option<u8>) {
    let sidecars = SidecarPaths::from_raw_path(photo_path);
    let mut grading = None;
    let mut rating = None;

    // Primary source of truth: canonical .rraw/<photo>.rraw.json
    if sidecars.recipe_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&sidecars.recipe_json) {
            if let Ok(recipe) = serde_json::from_str::<SidecarRecipe>(&content) {
                let mut state = DarkroomGradingState::default();
                recipe.apply_to(&mut state);
                rating = Some(recipe.rating);
                grading = Some(state);
            }
        }
    }

    (grading, rating)
}
