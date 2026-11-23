#![forbid(unsafe_code)]

//! AI semantic segmentation masks inference and local adjustment evaluation.

use rraw_ai::{SemanticMaskSet, SemanticSegmenter};
use rraw_coreml::AneSemanticSegmenter;

use crate::state::{LocalMaskLayer, PrecomputedBrushGrid, SemanticMaskLayer};

/// Pre-infers 4-class CoreML AI semantic segmentation probability maps.
#[must_use]
pub fn infer_preview_semantic_masks(
    samples_rgb: &[[f32; 3]],
    width: usize,
    height: usize,
    as_shot_wb: [f32; 3],
    cam_to_srgb: [f32; 9],
) -> std::sync::Arc<SemanticMaskSet> {
    if width == 0 || height == 0 || samples_rgb.len() < width.saturating_mul(height) {
        return std::sync::Arc::new(SemanticMaskSet {
            width: 1,
            height: 1,
            subject: vec![0.0],
            sky: vec![0.0],
            background: vec![1.0],
            portrait: vec![0.0],
        });
    }

    let step = (width / 160).max(1);
    let sem_w = (width / step).max(1);
    let sem_h = (height / step).max(1);
    let mut cal_rgb = Vec::with_capacity(sem_w * sem_h);

    for sy in 0..sem_h {
        let py = (sy * step).min(height - 1);
        for sx in 0..sem_w {
            let px = (sx * step).min(width - 1);
            let [mr, mg, mb] = samples_rgb[py * width + px];
            let r_wb = mr * as_shot_wb[0];
            let g_wb = mg * as_shot_wb[1];
            let b_wb = mb * as_shot_wb[2];
            let r =
                (cam_to_srgb[0] * r_wb + cam_to_srgb[1] * g_wb + cam_to_srgb[2] * b_wb).max(0.0);
            let g =
                (cam_to_srgb[3] * r_wb + cam_to_srgb[4] * g_wb + cam_to_srgb[5] * b_wb).max(0.0);
            let b =
                (cam_to_srgb[6] * r_wb + cam_to_srgb[7] * g_wb + cam_to_srgb[8] * b_wb).max(0.0);
            cal_rgb.push([r, g, b]);
        }
    }

    let segmenter = AneSemanticSegmenter::new();
    let masks = segmenter
        .segment_rgb(&cal_rgb, sem_w, sem_h)
        .unwrap_or_else(|_| SemanticMaskSet {
            width: 1,
            height: 1,
            subject: vec![0.0],
            sky: vec![0.0],
            background: vec![1.0],
            portrait: vec![0.0],
        });
    std::sync::Arc::new(masks)
}

/// Evaluates accumulated local (semantic + geometric gradients) mask offsets `(ev, contrast, saturation, temp_shift)` at `(u, v)`.
#[inline]
#[must_use]
pub fn eval_all_mask_adjustments(
    masks_set: &SemanticMaskSet,
    semantic_layers: &[SemanticMaskLayer],
    local_masks: &[LocalMaskLayer],
    u: f32,
    v: f32,
) -> (f32, f32, f32, f32) {
    let mut ev = 0.0_f32;
    let mut contrast = 0.0_f32;
    let mut sat = 0.0_f32;
    let mut temp = 0.0_f32;

    for layer in semantic_layers {
        if layer.is_neutral() {
            continue;
        }
        let w = masks_set.sample_bilinear(layer.category, u, v, layer.invert, layer.feather)
            * layer.opacity.clamp(0.0, 1.0);
        if w > 1e-4 {
            ev += w * layer.exposure_ev;
            contrast += w * layer.contrast;
            sat += w * layer.saturation;
            temp += w * layer.temperature_shift;
        }
    }

    for mask in local_masks {
        if mask.is_neutral() {
            continue;
        }
        let w = mask.evaluate_weight(u, v);
        if w > 1e-4 {
            ev += w * mask.exposure_ev;
            contrast += w * mask.contrast;
            sat += w * mask.saturation;
            temp += w * mask.temperature_shift;
        }
    }

    (ev, contrast, sat, temp)
}

/// Evaluates accumulated local (semantic + geometric gradients + $O(1)$ precomputed brush grids)
/// mask offsets `(ev, contrast, saturation, temp_shift)` at `(u, v)`.
#[inline]
#[must_use]
pub fn eval_all_mask_adjustments_with_brush_grids(
    masks_set: &SemanticMaskSet,
    semantic_layers: &[SemanticMaskLayer],
    local_masks: &[LocalMaskLayer],
    brush_grids: &[Option<PrecomputedBrushGrid>],
    u: f32,
    v: f32,
) -> (f32, f32, f32, f32) {
    let mut ev = 0.0_f32;
    let mut contrast = 0.0_f32;
    let mut sat = 0.0_f32;
    let mut temp = 0.0_f32;

    for layer in semantic_layers {
        if layer.is_neutral() {
            continue;
        }
        let w = masks_set.sample_bilinear(layer.category, u, v, layer.invert, layer.feather)
            * layer.opacity.clamp(0.0, 1.0);
        if w > 1e-4 {
            ev += w * layer.exposure_ev;
            contrast += w * layer.contrast;
            sat += w * layer.saturation;
            temp += w * layer.temperature_shift;
        }
    }

    for (idx, mask) in local_masks.iter().enumerate() {
        if mask.is_neutral() {
            continue;
        }
        let w = if let Some(Some(grid)) = brush_grids.get(idx) {
            grid.sample(u, v)
        } else {
            mask.evaluate_weight(u, v)
        };
        if w > 1e-4 {
            ev += w * mask.exposure_ev;
            contrast += w * mask.contrast;
            sat += w * mask.saturation;
            temp += w * mask.temperature_shift;
        }
    }

    (ev, contrast, sat, temp)
}

/// Executes GPU Stage 6 local geometric and CoreML AI semantic masks on a GPU context.
pub fn execute_stage6_gpu(
    ctx: &rraw_gpu::GpuContext,
    rgb_in: &[f32],
    width: u32,
    height: u32,
    masks: &rraw_core::Stage6MasksParams,
    semantic_masks: &SemanticMaskSet,
) -> Result<Vec<f32>, rraw_gpu::GpuError> {
    let pipeline = rraw_gpu::Stage6Pipeline::new(ctx)?;
    let params = rraw_gpu::Stage6Params {
        width,
        height,
        masks: masks.clone(),
    };
    let sem_rgba = semantic_masks.to_rgba_interleaved();
    pipeline.execute_with_semantics(
        ctx,
        rgb_in,
        &params,
        Some((
            &sem_rgba,
            semantic_masks.width as u32,
            semantic_masks.height as u32,
        )),
    )
}
