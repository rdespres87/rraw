#![forbid(unsafe_code)]

//! Semantic AI and local retouching mask layers, including $O(1)$ AABB-localized freehand brush grids.

use serde::{Deserialize, Serialize};

use crate::i18n::{Key, Language};

/// Cohesive local and AI semantic masking state, including interactive overlay and brush selection.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaskGradingState {
    pub semantic_masks: Vec<SemanticMaskLayer>,
    pub show_semantic_mask_overlay: Option<rraw_core::SemanticMaskCategory>,
    pub local_masks: Vec<LocalMaskLayer>,
    pub show_local_mask_overlay: Option<usize>,
    pub active_brush_mask_idx: Option<usize>,
}

impl MaskGradingState {
    /// Returns true if no semantic or local mask layers are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.semantic_masks.is_empty() && self.local_masks.is_empty()
    }

    /// Returns a mutable reference to the semantic mask layer for `category`, creating it if absent.
    pub fn upsert_semantic_mask(
        &mut self,
        category: rraw_core::SemanticMaskCategory,
    ) -> &mut SemanticMaskLayer {
        if let Some(pos) = self
            .semantic_masks
            .iter()
            .position(|m| m.category == category)
        {
            &mut self.semantic_masks[pos]
        } else {
            self.semantic_masks.push(SemanticMaskLayer::new(category));
            let last_idx = self.semantic_masks.len() - 1;
            &mut self.semantic_masks[last_idx]
        }
    }

    /// Removes the semantic mask layer for `category` if present.
    pub fn remove_semantic_mask(&mut self, category: rraw_core::SemanticMaskCategory) {
        self.semantic_masks.retain(|m| m.category != category);
        if self.show_semantic_mask_overlay == Some(category) {
            self.show_semantic_mask_overlay = None;
        }
    }
}

/// Local adjustment layer driven by a CoreML AI semantic segmentation mask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMaskLayer {
    /// Target semantic class (Subject, Sky, Background, Portrait).
    pub category: rraw_core::SemanticMaskCategory,
    /// Whether this semantic mask layer is active.
    pub enabled: bool,
    /// Whether to invert the semantic mask ($1.0 - p$).
    pub invert: bool,
    /// Boundary feather / soft transition width in `[0.02, 0.5]`.
    pub feather: f32,
    /// Overall layer opacity in `[0.0, 1.0]`.
    pub opacity: f32,
    /// Local exposure offset in EV (`-3.0..=+3.0`).
    pub exposure_ev: f32,
    /// Local contrast shift (`-100.0..=+100.0`).
    pub contrast: f32,
    /// Local saturation shift (`-100.0..=+100.0`).
    pub saturation: f32,
    /// Local temperature shift (`-100.0..=+100.0`).
    pub temperature_shift: f32,
}

impl SemanticMaskLayer {
    /// Creates a new enabled semantic mask layer for `category` with neutral grading offsets.
    #[must_use]
    pub const fn new(category: rraw_core::SemanticMaskCategory) -> Self {
        Self {
            category,
            enabled: true,
            invert: false,
            feather: 0.35,
            opacity: 1.0,
            exposure_ev: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature_shift: 0.0,
        }
    }

    /// Returns true if this mask layer contributes no effective color or exposure change.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        !self.enabled
            || self.opacity <= 1e-4
            || (self.exposure_ev.abs() < 1e-4
                && self.contrast.abs() < 1e-3
                && self.saturation.abs() < 1e-3
                && self.temperature_shift.abs() < 1e-3)
    }

    /// Human-readable French UI label for a semantic category.
    #[must_use]
    pub fn category_label(category: rraw_core::SemanticMaskCategory) -> &'static str {
        Self::category_label_lang(category, Language::Fr)
    }

    /// Language-aware UI label for a semantic category.
    #[must_use]
    pub fn category_label_lang(
        category: rraw_core::SemanticMaskCategory,
        lang: Language,
    ) -> &'static str {
        match category {
            rraw_core::SemanticMaskCategory::Subject => Key::SemanticMaskSubject.translate(lang),
            rraw_core::SemanticMaskCategory::Sky => Key::SemanticMaskSky.translate(lang),
            rraw_core::SemanticMaskCategory::Background => {
                Key::SemanticMaskBackground.translate(lang)
            }
            rraw_core::SemanticMaskCategory::Portrait => Key::SemanticMaskPortrait.translate(lang),
        }
    }
}

/// Single brush dab painted along a freehand stroke in normalized `[0, 1]²` coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BrushDab {
    pub pos: [f32; 2],
    pub radius: f32,
    pub flow: f32,
    pub erase: bool,
}

/// Kind and geometric coordinates of a local retouching mask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LocalMaskKind {
    /// Linear directional gradient in normalized `[0, 1]²` coordinates.
    LinearGradient { start: [f32; 2], end: [f32; 2] },
    /// Radial elliptical gradient in normalized `[0, 1]²` coordinates.
    RadialGradient {
        center: [f32; 2],
        radius: [f32; 2],
        rotation_deg: f32,
    },
    /// Freehand adjustment brush mask composed of vector dabs.
    FreehandBrush {
        dabs: Vec<BrushDab>,
        brush_radius: f32,
        brush_flow: f32,
        erase_mode: bool,
    },
}

/// Local adjustment mask layer (Linear Gradient, Radial Gradient, Freehand Brush).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalMaskLayer {
    pub id: String,
    pub name: String,
    pub kind: LocalMaskKind,
    pub enabled: bool,
    pub invert: bool,
    pub feather: f32,
    pub opacity: f32,
    pub exposure_ev: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature_shift: f32,
}

impl LocalMaskLayer {
    /// Creates a new linear gradient mask layer.
    pub fn new_linear(name: impl Into<String>, start: [f32; 2], end: [f32; 2]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Self {
            id: format!("lmask_lin_{ts}"),
            name: name.into(),
            kind: LocalMaskKind::LinearGradient { start, end },
            enabled: true,
            invert: false,
            feather: 0.5,
            opacity: 1.0,
            exposure_ev: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature_shift: 0.0,
        }
    }

    /// Creates a new radial gradient mask layer.
    pub fn new_radial(name: impl Into<String>, center: [f32; 2], radius: [f32; 2]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Self {
            id: format!("lmask_rad_{ts}"),
            name: name.into(),
            kind: LocalMaskKind::RadialGradient {
                center,
                radius,
                rotation_deg: 0.0,
            },
            enabled: true,
            invert: false,
            feather: 0.5,
            opacity: 1.0,
            exposure_ev: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature_shift: 0.0,
        }
    }

    /// Creates a new freehand adjustment brush mask layer.
    pub fn new_brush(name: impl Into<String>, brush_radius: f32, brush_flow: f32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Self {
            id: format!("lmask_brush_{ts}"),
            name: name.into(),
            kind: LocalMaskKind::FreehandBrush {
                dabs: Vec::new(),
                brush_radius: brush_radius.clamp(0.001, 0.5),
                brush_flow: brush_flow.clamp(0.05, 1.0),
                erase_mode: false,
            },
            enabled: true,
            invert: false,
            feather: 0.65,
            opacity: 1.0,
            exposure_ev: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature_shift: 0.0,
        }
    }

    /// Appends a brush dab at `(u, v)` if sufficiently spaced from the previous dab.
    pub fn add_brush_dab(&mut self, u: f32, v: f32) -> bool {
        if let LocalMaskKind::FreehandBrush {
            dabs,
            brush_radius,
            brush_flow,
            erase_mode,
        } = &mut self.kind
        {
            let r = brush_radius.clamp(0.001, 0.5);
            let min_dist_sq = (r * 0.25).powi(2);
            if let Some(last) = dabs.last() {
                let dx = u - last.pos[0];
                let dy = v - last.pos[1];
                if dx * dx + dy * dy < min_dist_sq && last.erase == *erase_mode {
                    return false;
                }
            }
            dabs.push(BrushDab {
                pos: [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)],
                radius: r,
                flow: brush_flow.clamp(0.05, 1.0),
                erase: *erase_mode,
            });
            true
        } else {
            false
        }
    }

    /// Forces adding a brush dab at `(u, v)` without spacing deduplication.
    pub fn add_brush_dab_forced(&mut self, u: f32, v: f32) -> bool {
        if let LocalMaskKind::FreehandBrush {
            dabs,
            brush_radius,
            brush_flow,
            erase_mode,
        } = &mut self.kind
        {
            dabs.push(BrushDab {
                pos: [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)],
                radius: brush_radius.clamp(0.001, 0.5),
                flow: brush_flow.clamp(0.05, 1.0),
                erase: *erase_mode,
            });
            true
        } else {
            false
        }
    }

    /// Sets the brush erase mode if this layer is a freehand brush.
    pub fn set_brush_erase_mode(&mut self, erase: bool) {
        if let LocalMaskKind::FreehandBrush { erase_mode, .. } = &mut self.kind {
            *erase_mode = erase;
        }
    }

    /// Returns true if this mask contributes zero effective changes.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        !self.enabled
            || self.opacity <= 1e-4
            || (self.exposure_ev.abs() < 1e-4
                && self.contrast.abs() < 1e-3
                && self.saturation.abs() < 1e-3
                && self.temperature_shift.abs() < 1e-3)
    }

    /// Evaluates the normalized mask weight at normalized coordinates `(u, v) \in [0, 1]²`.
    #[must_use]
    pub fn evaluate_weight(&self, u: f32, v: f32) -> f32 {
        if !self.enabled || self.opacity <= 1e-4 {
            return 0.0;
        }

        let raw_w = match &self.kind {
            LocalMaskKind::LinearGradient { start, end } => {
                let dx = end[0] - start[0];
                let dy = end[1] - start[1];
                let len_sq = (dx * dx + dy * dy).max(1e-12);
                let proj = ((u - start[0]) * dx + (v - start[1]) * dy) / len_sq;
                let t = proj.clamp(0.0, 1.0);
                let f = self.feather.clamp(0.05, 1.0);
                if f > 0.95 {
                    t
                } else {
                    t * t * (3.0 - 2.0 * t)
                }
            }
            LocalMaskKind::RadialGradient {
                center,
                radius,
                rotation_deg,
            } => {
                let rad = rotation_deg.to_radians();
                let cos_r = rad.cos();
                let sin_r = rad.sin();
                let dx = u - center[0];
                let dy = v - center[1];
                let rx = (dx * cos_r + dy * sin_r) / radius[0].max(1e-4);
                let ry = (-dx * sin_r + dy * cos_r) / radius[1].max(1e-4);
                let dist = (rx * rx + ry * ry).sqrt();

                let f = self.feather.clamp(0.05, 1.0);
                let inner = (1.0 - f).max(0.0);
                if dist <= inner {
                    1.0
                } else if dist >= 1.0 {
                    0.0
                } else {
                    let t = (dist - inner) / (1.0 - inner).max(1e-4);
                    let s = t * t * (3.0 - 2.0 * t);
                    1.0 - s
                }
            }
            LocalMaskKind::FreehandBrush { dabs, .. } => {
                if dabs.is_empty() {
                    0.0
                } else {
                    let f = self.feather.clamp(0.05, 1.0);
                    let inner_ratio = (1.0 - f).max(0.0);
                    let inner_ratio_sq = inner_ratio * inner_ratio;
                    let inv_feather_span = 1.0 / (1.0 - inner_ratio).max(1e-4);
                    let mut acc = 0.0_f32;
                    for dab in dabs {
                        let r = dab.radius.max(1e-4);
                        let dx = (u - dab.pos[0]).abs();
                        if dx >= r {
                            continue;
                        }
                        let dy = (v - dab.pos[1]).abs();
                        if dy >= r {
                            continue;
                        }
                        let dist_sq = dx * dx + dy * dy;
                        let r_sq = r * r;
                        if dist_sq >= r_sq {
                            continue;
                        }
                        let dab_w = if dist_sq <= inner_ratio_sq * r_sq {
                            1.0
                        } else {
                            let dist_norm = dist_sq.sqrt() / r;
                            let t = (dist_norm - inner_ratio) * inv_feather_span;
                            1.0 - t * t * (3.0 - 2.0 * t)
                        };
                        let contrib = dab_w * dab.flow.clamp(0.05, 1.0);
                        if dab.erase {
                            acc = (acc - contrib).max(0.0);
                        } else {
                            acc = (acc + contrib).min(1.0);
                        }
                    }
                    acc
                }
            }
        };

        let w = if self.invert {
            (1.0 - raw_w).clamp(0.0, 1.0)
        } else {
            raw_w.clamp(0.0, 1.0)
        };

        w * self.opacity.clamp(0.0, 1.0)
    }
}

/// Pre-rasterized 2D lookup grid with AABB bounding box for $O(1)$ per-pixel freehand brush evaluation.
#[derive(Debug, Clone)]
pub struct PrecomputedBrushGrid {
    width: usize,
    height: usize,
    aabb: [f32; 4],
    invert: bool,
    opacity: f32,
    weights: Vec<f32>,
}

impl PrecomputedBrushGrid {
    /// Grid resolution along each axis (512x512 localized over AABB).
    pub const DEFAULT_DIM: usize = 512;

    /// Builds a pre-rasterized brush grid if `layer` is an active `FreehandBrush` mask.
    #[must_use]
    pub fn from_layer(layer: &LocalMaskLayer) -> Option<Self> {
        if !layer.enabled || layer.opacity <= 1e-4 {
            return None;
        }
        let LocalMaskKind::FreehandBrush { dabs, .. } = &layer.kind else {
            return None;
        };
        let opacity = layer.opacity.clamp(0.0, 1.0);
        if dabs.is_empty() {
            return Some(Self {
                width: 1,
                height: 1,
                aabb: [1.0, 1.0, 0.0, 0.0],
                invert: layer.invert,
                opacity,
                weights: vec![if layer.invert { opacity } else { 0.0 }],
            });
        }

        let mut min_u = 1.0_f32;
        let mut min_v = 1.0_f32;
        let mut max_u = 0.0_f32;
        let mut max_v = 0.0_f32;
        for dab in dabs {
            let r = dab.radius.max(1e-4);
            min_u = min_u.min((dab.pos[0] - r).max(0.0));
            min_v = min_v.min((dab.pos[1] - r).max(0.0));
            max_u = max_u.max((dab.pos[0] + r).min(1.0));
            max_v = max_v.max((dab.pos[1] + r).min(1.0));
        }

        let span_u = (max_u - min_u).max(1e-4);
        let span_v = (max_v - min_v).max(1e-4);
        let aabb = [min_u, min_v, max_u, max_v];

        let dim = Self::DEFAULT_DIM;
        let mut raw_grid = vec![0.0_f32; dim * dim];
        let f = layer.feather.clamp(0.05, 1.0);
        let inner_ratio = (1.0 - f).max(0.0);
        let inner_ratio_sq = inner_ratio * inner_ratio;
        let inv_feather_span = 1.0 / (1.0 - inner_ratio).max(1e-4);
        let scale = (dim - 1) as f32;

        // Splat each dab onto the AABB-localized grid: higher spatial precision where dabs actually exist
        for dab in dabs {
            let r = dab.radius.max(1e-4);
            let r_sq = r * r;
            let inner_sq = inner_ratio_sq * r_sq;
            let flow = dab.flow.clamp(0.05, 1.0);

            let rel_u0 = ((dab.pos[0] - r - min_u) / span_u).clamp(0.0, 1.0);
            let rel_u1 = ((dab.pos[0] + r - min_u) / span_u).clamp(0.0, 1.0);
            let rel_v0 = ((dab.pos[1] - r - min_v) / span_v).clamp(0.0, 1.0);
            let rel_v1 = ((dab.pos[1] + r - min_v) / span_v).clamp(0.0, 1.0);

            let x0 = ((rel_u0 * scale).floor() as usize).min(dim - 1);
            let x1 = ((rel_u1 * scale).ceil() as usize).min(dim - 1);
            let y0 = ((rel_v0 * scale).floor() as usize).min(dim - 1);
            let y1 = ((rel_v1 * scale).ceil() as usize).min(dim - 1);

            for gy in y0..=y1 {
                let v = min_v + (gy as f32 / scale) * span_v;
                let dy = v - dab.pos[1];
                let dy_sq = dy * dy;
                let row_off = gy * dim;
                for gx in x0..=x1 {
                    let u = min_u + (gx as f32 / scale) * span_u;
                    let dx = u - dab.pos[0];
                    let dist_sq = dx * dx + dy_sq;
                    if dist_sq >= r_sq {
                        continue;
                    }
                    let dab_w = if dist_sq <= inner_sq {
                        1.0
                    } else {
                        let dist_norm = dist_sq.sqrt() / r;
                        let t = (dist_norm - inner_ratio) * inv_feather_span;
                        1.0 - t * t * (3.0 - 2.0 * t)
                    };
                    let contrib = dab_w * flow;
                    let cell = &mut raw_grid[row_off + gx];
                    if dab.erase {
                        *cell = (*cell - contrib).max(0.0);
                    } else {
                        *cell = (*cell + contrib).min(1.0);
                    }
                }
            }
        }

        let opacity = layer.opacity.clamp(0.0, 1.0);
        for cell in &mut raw_grid {
            let w = if layer.invert {
                (1.0 - *cell).clamp(0.0, 1.0)
            } else {
                (*cell).clamp(0.0, 1.0)
            };
            *cell = w * opacity;
        }

        Some(Self {
            width: dim,
            height: dim,
            aabb,
            invert: layer.invert,
            opacity,
            weights: raw_grid,
        })
    }

    /// Evaluates the brush weight at `(u, v)` in $O(1)$ time using AABB rejection and bilinear interpolation.
    #[inline]
    #[must_use]
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if u < self.aabb[0] || u > self.aabb[2] || v < self.aabb[1] || v > self.aabb[3] {
            return if self.invert { self.opacity } else { 0.0 };
        }
        if self.width <= 1 || self.height <= 1 {
            return self.weights.first().copied().unwrap_or(0.0);
        }
        let span_u = (self.aabb[2] - self.aabb[0]).max(1e-4);
        let span_v = (self.aabb[3] - self.aabb[1]).max(1e-4);
        let rel_u = ((u - self.aabb[0]) / span_u).clamp(0.0, 1.0);
        let rel_v = ((v - self.aabb[1]) / span_v).clamp(0.0, 1.0);

        let fx = rel_u * (self.width - 1) as f32;
        let fy = rel_v * (self.height - 1) as f32;
        let x0 = (fx.floor() as usize).min(self.width - 1);
        let y0 = (fy.floor() as usize).min(self.height - 1);
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let w00 = self.weights[y0 * self.width + x0];
        let w10 = self.weights[y0 * self.width + x1];
        let w01 = self.weights[y1 * self.width + x0];
        let w11 = self.weights[y1 * self.width + x1];

        let top = w00 * (1.0 - tx) + w10 * tx;
        let bot = w01 * (1.0 - tx) + w11 * tx;
        top * (1.0 - ty) + bot * ty
    }
}
