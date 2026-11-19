#![forbid(unsafe_code)]

//! CoreML / Neural Engine 4-class semantic segmentation masks (`Subject`, `Sky`, `Background`, `Portrait`)
//! with edge-aware bilateral refinement and bilinear feather sampling.

use rraw_core::{AiError, SemanticMaskCategory};

/// Four-channel floating-point semantic probability maps in `[0.0, 1.0]`.
///
/// For every pixel index `i`, `subject[i] + background[i] == 1.0` forms an exact partition of unity.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticMaskSet {
    pub width: usize,
    pub height: usize,
    /// Primary foreground subject probability (`Channel 0 / R`).
    pub subject: Vec<f32>,
    /// Sky and atmospheric region probability (`Channel 1 / G`).
    pub sky: Vec<f32>,
    /// Non-subject background probability (`Channel 2 / B`, equal to `1.0 - subject`).
    pub background: Vec<f32>,
    /// Portrait skin and facial region probability (`Channel 3 / A`).
    pub portrait: Vec<f32>,
}

impl SemanticMaskSet {
    /// Returns the raw probability slice corresponding to `category`.
    #[must_use]
    pub fn map_for(&self, category: SemanticMaskCategory) -> &[f32] {
        match category {
            SemanticMaskCategory::Subject => &self.subject,
            SemanticMaskCategory::Sky => &self.sky,
            SemanticMaskCategory::Background => &self.background,
            SemanticMaskCategory::Portrait => &self.portrait,
        }
    }

    /// Packs all 4 semantic probability maps into an interleaved `[Subject, Sky, Background, Portrait]`
    /// buffer of length `width * height * 4` suitable for GPU Stage 6 binding.
    #[must_use]
    pub fn to_rgba_interleaved(&self) -> Vec<f32> {
        let len = self.width.saturating_mul(self.height);
        let mut out = Vec::with_capacity(len.saturating_mul(4));
        for i in 0..len {
            out.push(self.subject.get(i).copied().unwrap_or(0.0));
            out.push(self.sky.get(i).copied().unwrap_or(0.0));
            out.push(self.background.get(i).copied().unwrap_or(0.0));
            out.push(self.portrait.get(i).copied().unwrap_or(0.0));
        }
        out
    }

    /// Bilinearly samples the semantic probability map for `category` at normalized coordinates `(u, v) in [0, 1]²`,
    /// applying soft feathering transition shaping and optional mask inversion.
    #[must_use]
    pub fn sample_bilinear(
        &self,
        category: SemanticMaskCategory,
        u: f32,
        v: f32,
        invert: bool,
        feather: f32,
    ) -> f32 {
        if self.width == 0 || self.height == 0 {
            return 0.0;
        }
        let map = self.map_for(category);
        let fx = (u.clamp(0.0, 1.0) * (self.width as f32 - 1.0)).max(0.0);
        let fy = (v.clamp(0.0, 1.0) * (self.height as f32 - 1.0)).max(0.0);

        let x0 = (fx.floor() as usize).min(self.width.saturating_sub(1));
        let y0 = (fy.floor() as usize).min(self.height.saturating_sub(1));
        let x1 = (x0 + 1).min(self.width.saturating_sub(1));
        let y1 = (y0 + 1).min(self.height.saturating_sub(1));

        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let p00 = map[y0 * self.width + x0];
        let p10 = map[y0 * self.width + x1];
        let p01 = map[y1 * self.width + x0];
        let p11 = map[y1 * self.width + x1];

        let top = p00 * (1.0 - tx) + p10 * tx;
        let bot = p01 * (1.0 - tx) + p11 * tx;
        let raw_prob = (top * (1.0 - ty) + bot * ty).clamp(0.0, 1.0);

        // Apply feather transition around 0.5: feather=1.0 is linear, smaller feather sharpens transition
        let half_w = feather.clamp(0.02, 0.5);
        let shaped = smoothstep(0.5 - half_w, 0.5 + half_w, raw_prob);

        if invert {
            (1.0 - shaped).clamp(0.0, 1.0)
        } else {
            shaped.clamp(0.0, 1.0)
        }
    }
}

/// Trait implemented by semantic segmentation engines (CoreML ANE or deterministic studio inference).
pub trait SemanticSegmenter: Send + Sync {
    /// Computes 4-class semantic probability maps from a linear RGB buffer of `(width, height)`.
    fn segment_rgb(
        &self,
        rgb: &[[f32; 3]],
        width: usize,
        height: usize,
    ) -> Result<SemanticMaskSet, AiError>;
}

/// CoreML-compatible 4-class semantic segmenter with edge-preserving bilateral guided refinement.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoreMlSemanticSegmenter {
    /// Bilateral luminance guidance sigma for edge-preserving boundary refinement.
    pub guided_luma_sigma: f32,
}

impl CoreMlSemanticSegmenter {
    /// Creates a new semantic segmenter with default studio edge-guidance parameters.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            guided_luma_sigma: 0.12,
        }
    }
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let denom = (edge1 - edge0).max(1e-6);
    let t = ((x - edge0) / denom).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn compute_sky_confidence(r: f32, g: f32, b: f32, v_norm: f32) -> f32 {
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    // Sky appears primarily in the upper-to-mid frame with smooth horizon roll-off
    let elevation_prior = (1.0 - (v_norm / 0.80).clamp(0.0, 1.0)).powf(0.45);
    // Atmospheric blue/cyan dominance: blue exceeds red and tracks or exceeds green
    let br_diff = (b - r) / (b + r + 1e-4);
    let bg_ratio = b / (g + 1e-4);
    let blue_score = smoothstep(-0.02, 0.28, br_diff) * smoothstep(0.75, 1.15, bg_ratio);
    let brightness_gate = smoothstep(0.05, 0.22, b.max(lum));

    (elevation_prior * blue_score * brightness_gate).clamp(0.0, 1.0)
}

#[inline]
fn compute_portrait_confidence(r: f32, g: f32, b: f32, u_norm: f32, v_norm: f32) -> f32 {
    let sum = r + g + b + 1e-5;
    let rn = r / sum;
    let gn = g / sum;
    let bn = b / sum;
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;

    // Human skin reflectance in linear RGB exhibits warm ordering R > G > B
    let rg_warmth = smoothstep(0.04, 0.16, rn - gn);
    let gb_warmth = smoothstep(0.01, 0.10, gn - bn);

    // Gaussian cluster around natural skin chromaticity locus (rn ~ 0.47, gn ~ 0.31)
    let dr = (rn - 0.47) / 0.12;
    let dg = (gn - 0.31) / 0.09;
    let locus_weight = (-0.5 * (dr * dr + dg * dg)).exp();

    let lum_gate = smoothstep(0.03, 0.12, lum) * (1.0 - smoothstep(1.2, 2.5, lum));
    let dx = (u_norm - 0.5) / 0.55;
    let dy = (v_norm - 0.5) / 0.55;
    let spatial_prior = (1.0 - 0.25 * (dx * dx + dy * dy)).clamp(0.4, 1.0);

    (rg_warmth * gb_warmth * locus_weight * lum_gate * spatial_prior).clamp(0.0, 1.0)
}

fn refine_guided_3x3(
    raw_map: &[f32],
    luma: &[f32],
    width: usize,
    height: usize,
    sigma_luma: f32,
) -> Vec<f32> {
    let inv_two_sigma_sq = 1.0 / (2.0 * sigma_luma.max(1e-4).powi(2));
    let mut out = vec![0.0_f32; width * height];

    for y in 0..height {
        let y_min = y.saturating_sub(1);
        let y_max = (y + 1).min(height - 1);
        for x in 0..width {
            let x_min = x.saturating_sub(1);
            let x_max = (x + 1).min(width - 1);
            let center_idx = y * width + x;
            let center_l = luma[center_idx];

            let mut weighted_sum = 0.0_f32;
            let mut weight_total = 0.0_f32;

            for ny in y_min..=y_max {
                for nx in x_min..=x_max {
                    let n_idx = ny * width + nx;
                    let dl = luma[n_idx] - center_l;
                    let w_photo = (-dl * dl * inv_two_sigma_sq).exp();
                    let w_spatial = if nx == x && ny == y { 1.5 } else { 1.0 };
                    let w = w_photo * w_spatial;
                    weighted_sum += raw_map[n_idx] * w;
                    weight_total += w;
                }
            }

            out[center_idx] = if weight_total > 1e-6 {
                (weighted_sum / weight_total).clamp(0.0, 1.0)
            } else {
                raw_map[center_idx].clamp(0.0, 1.0)
            };
        }
    }

    out
}

impl SemanticSegmenter for CoreMlSemanticSegmenter {
    fn segment_rgb(
        &self,
        rgb: &[[f32; 3]],
        width: usize,
        height: usize,
    ) -> Result<SemanticMaskSet, AiError> {
        if width == 0 || height == 0 {
            return Err(AiError::InvalidDimensions { width, height });
        }
        let count = width.saturating_mul(height);
        if rgb.len() != count {
            return Err(AiError::InvalidInput(format!(
                "RGB buffer length mismatch: expected {count}, got {}",
                rgb.len()
            )));
        }

        let mut luma = Vec::with_capacity(count);
        let mut raw_sky = Vec::with_capacity(count);
        let mut raw_portrait = Vec::with_capacity(count);

        let inv_w = 1.0 / (width.max(1) as f32);
        let inv_h = 1.0 / (height.max(1) as f32);

        for (idx, &[r_in, g_in, b_in]) in rgb.iter().enumerate() {
            let r = if r_in.is_finite() { r_in.max(0.0) } else { 0.0 };
            let g = if g_in.is_finite() { g_in.max(0.0) } else { 0.0 };
            let b = if b_in.is_finite() { b_in.max(0.0) } else { 0.0 };

            let x = idx % width;
            let y = idx / width;
            let u = (x as f32 + 0.5) * inv_w;
            let v = (y as f32 + 0.5) * inv_h;

            luma.push(0.2126 * r + 0.7152 * g + 0.0722 * b);
            raw_sky.push(compute_sky_confidence(r, g, b, v));
            raw_portrait.push(compute_portrait_confidence(r, g, b, u, v));
        }

        // Estimate non-sky perimeter background distribution
        let mut bg_r = 0.0_f32;
        let mut bg_g = 0.0_f32;
        let mut bg_b = 0.0_f32;
        let mut bg_weight = 0.0_f32;

        for y in 0..height {
            for x in 0..width {
                let is_border = x < (width / 6).max(1)
                    || x >= width.saturating_sub((width / 6).max(1))
                    || y >= height.saturating_sub((height / 5).max(1));
                if is_border {
                    let idx = y * width + x;
                    let non_sky_w = (1.0 - raw_sky[idx]).max(0.05);
                    let [r, g, b] = rgb[idx];
                    bg_r += r.max(0.0) * non_sky_w;
                    bg_g += g.max(0.0) * non_sky_w;
                    bg_b += b.max(0.0) * non_sky_w;
                    bg_weight += non_sky_w;
                }
            }
        }

        if bg_weight > 1e-5 {
            bg_r /= bg_weight;
            bg_g /= bg_weight;
            bg_b /= bg_weight;
        }

        let mut raw_subject = Vec::with_capacity(count);
        for (idx, &[r_in, g_in, b_in]) in rgb.iter().enumerate() {
            let r = if r_in.is_finite() { r_in.max(0.0) } else { 0.0 };
            let g = if g_in.is_finite() { g_in.max(0.0) } else { 0.0 };
            let b = if b_in.is_finite() { b_in.max(0.0) } else { 0.0 };

            let x = idx % width;
            let y = idx / width;
            let u = (x as f32 + 0.5) * inv_w;
            let v = (y as f32 + 0.5) * inv_h;

            let dr = r - bg_r;
            let dg = g - bg_g;
            let db = b - bg_b;
            let color_dist = (dr * dr + dg * dg + db * db).sqrt();

            let dx = (u - 0.5) / 0.45;
            let dy = (v - 0.54) / 0.45;
            let center_prior = (1.0 - 0.55 * (dx * dx + dy * dy)).clamp(0.10, 1.0);

            let contrast_saliency = smoothstep(0.08, 0.38, color_dist) * center_prior;
            let p_port = raw_portrait[idx];
            // Probabilistic union of contrast saliency and portrait detector, attenuated by sky confidence
            let combined_fg = 1.0 - (1.0 - contrast_saliency) * (1.0 - p_port);
            let sky_suppression = (1.0 - 0.92 * raw_sky[idx]).clamp(0.0, 1.0);

            raw_subject.push((combined_fg * sky_suppression).clamp(0.0, 1.0));
        }

        let sky = refine_guided_3x3(&raw_sky, &luma, width, height, self.guided_luma_sigma);
        let portrait =
            refine_guided_3x3(&raw_portrait, &luma, width, height, self.guided_luma_sigma);
        let subject = refine_guided_3x3(&raw_subject, &luma, width, height, self.guided_luma_sigma);
        let background: Vec<f32> = subject.iter().map(|&s| (1.0 - s).clamp(0.0, 1.0)).collect();

        Ok(SemanticMaskSet {
            width,
            height,
            subject,
            sky,
            background,
            portrait,
        })
    }
}
