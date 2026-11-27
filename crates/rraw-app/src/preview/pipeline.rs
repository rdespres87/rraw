#![forbid(unsafe_code)]

//! Unified color rendering pipeline, LUT evaluation, tone curves, and export rasterization.

use rraw_color::{apply_agx_edr, apply_opendrt, DisplayTransform, SoftProofResult};

use super::masks::eval_all_mask_adjustments_with_brush_grids;
use super::types::BaseRgbPreview;
use crate::state::*;

pub use super::render::{
    generate_preview_image, render_export_image, render_export_image_with_depth,
    render_preview_from_base, render_preview_from_base_profiled, ExportRasterBuffer,
};

/// Precomputed frame dimensions, optical distortion, lens vignetting, and geometric warp state.
#[derive(Debug, Clone, Copy)]
pub struct FrameGeometry {
    pub preview_w: usize,
    pub preview_h: usize,
    pub cx: f32,
    pub cy: f32,
    pub max_radius_sq: f32,
    pub k_dist: f32,
    pub v_gain: f32,
    pub cos_rot: f32,
    pub sin_rot: f32,
    pub flip_x: f32,
    pub flip_y: f32,
    pub keystone_v: f32,
    pub keystone_h: f32,
    pub auto_zoom: f32,
    pub is_identity_geom_base: bool,
}

/// Prevents magenta/violet color casts in clipped sensor highlights when white-balance
/// multipliers exceed the green channel (e.g. R=2.2, G=1.0, B=1.8).
///
/// Non-clipped pixels (`max(r, g, b) < 0.99`) and unclipped HDR float pixels (`> 1.30`)
/// are returned unchanged. Saturated sensor highlights (`0.99..=1.30`, covering sensors
/// whose ADC max exceeds nominal white level such as `4095 / 3800 = 1.08`) are smoothly
/// clamped to the minimum WB channel ceiling so clipped whites stay achromatic white.
#[inline]
#[must_use]
pub fn clip_saturated_sensor_highlights(rgb: [f32; 3], eff_wb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let peak = r.max(g).max(b);
    if !(0.99..=1.30).contains(&peak) {
        return rgb;
    }
    let min_wb = eff_wb[0].min(eff_wb[1]).min(eff_wb[2]);
    if !min_wb.is_finite() || min_wb <= 0.0 {
        return rgb;
    }
    let clamp_ch = |val: f32, wb: f32| -> f32 {
        if wb > 0.0 && val > 0.0 {
            let max_raw = min_wb / wb;
            val.min(max_raw)
        } else {
            val
        }
    };
    let t = ((peak - 0.99) / 0.01).clamp(0.0, 1.0);
    let cr = clamp_ch(r, eff_wb[0]);
    let cg = clamp_ch(g, eff_wb[1]);
    let cb = clamp_ch(b, eff_wb[2]);
    [r + (cr - r) * t, g + (cg - g) * t, b + (cb - b) * t]
}

/// Precomputed scalar/matrix grading uniforms shared across every pixel in a frame.
#[derive(Debug, Clone, Copy)]
pub struct GradingUniformPayload {
    pub combined_mat: [f32; 9],
    pub eff_wb: [f32; 3],
    pub has_tonal_adjustment: bool,
    pub has_curves: bool,
    pub base_contrast_factor: f32,
    pub base_vibrance: f32,
    pub user_vibrance: f32,
    pub user_sat: f32,
    pub has_creative_vignette: bool,
    pub cv_amt: f32,
    pub cv_mid: f32,
    pub cv_feather: f32,
    pub cv_round: f32,
}

/// Heap-allocated precomputed pixel, gain-map, curve-LUT, and spatial brush-grid buffers.
pub struct FramePrecomputedBuffers {
    pub processed_samples: Option<Vec<[f32; 3]>>,
    pub smart_lighting_map: Option<Vec<f32>>,
    pub curves_lut: Box<[[f32; 4]; 1024]>,
    pub brush_grids: Vec<Option<PrecomputedBrushGrid>>,
}

/// Single-slot thread-local cache for `LensfunDatabase::find_lens` + `interpolate_model`
/// results, keyed by `(override_name, focal_bits, aperture_bits)`.
struct CachedLensOverride {
    override_name: String,
    focal_bits: u32,
    aperture_bits: Option<u32>,
    kv: Option<(f32, f32)>,
}

thread_local! {
    static LENS_OVERRIDE_CACHE: std::cell::RefCell<Option<CachedLensOverride>> =
        const { std::cell::RefCell::new(None) };
}

/// Resolves `(distortion_k1, vignette_v1)` for a named lens override without
/// allocating a `MatchedLens` or re-scanning `LensfunDatabase` on repeated frames.
fn resolve_lens_override_kv(
    override_name: &str,
    focal: f32,
    aperture: Option<f32>,
) -> Option<(f32, f32)> {
    let focal_bits = focal.to_bits();
    let aperture_bits = aperture.map(f32::to_bits);

    let hit = LENS_OVERRIDE_CACHE.with_borrow(|c| {
        c.as_ref()
            .filter(|e| {
                e.override_name == override_name
                    && e.focal_bits == focal_bits
                    && e.aperture_bits == aperture_bits
            })
            .map(|e| e.kv)
    });
    if let Some(kv) = hit {
        return kv;
    }

    let db = rraw_optics::LensfunDatabase::global();
    let kv = db.find_lens(override_name).map(|lens| {
        let model = db.interpolate_model(lens, focal, aperture);
        (model.distortion.k1, model.vignette.v1)
    });

    let entry = CachedLensOverride {
        override_name: override_name.to_string(),
        focal_bits,
        aperture_bits,
        kv,
    };
    LENS_OVERRIDE_CACHE.with_borrow_mut(|c| *c = Some(entry));
    kv
}

/// Computes effective `(k_dist, v_gain)` for the current frame without cloning `MatchedLens`.
fn resolve_optical_distortion_and_vignette(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> (f32, f32) {
    match grading.optics.optical_tab {
        OpticalCorrectionTab::Profile => {
            if !grading.optics.lens_profile_auto {
                return (0.0, 0.0);
            }
            let base_kv = if let Some(override_name) = &grading.optics.lens_profile_override {
                resolve_lens_override_kv(
                    override_name,
                    base.focal_length_mm.unwrap_or(50.0),
                    base.aperture_f_number,
                )
                .or_else(|| {
                    base.matched_lens
                        .as_ref()
                        .map(|m| (m.model.distortion.k1, m.model.vignette.v1))
                })
            } else {
                base.matched_lens
                    .as_ref()
                    .map(|m| (m.model.distortion.k1, m.model.vignette.v1))
            };
            if let Some((k1, v1)) = base_kv {
                let dist_scale = (grading.optics.lens_distortion_strength / 100.0).max(0.0);
                let vig_scale = (grading.optics.lens_vignette_strength / 100.0).max(0.0);
                (k1 * dist_scale, v1 * vig_scale)
            } else {
                (0.0, 0.0)
            }
        }
        OpticalCorrectionTab::Manual => {
            let kd = if grading.optics.distortion_correction {
                (grading.optics.distortion_amount / 100.0) * 0.15
            } else {
                0.0
            };
            let vg = if grading.optics.vignette_correction {
                (grading.optics.vignette_amount / 100.0) * 0.65
            } else {
                0.0
            };
            (kd, vg)
        }
    }
}

/// Shared precomputed frame state for per-pixel evaluation across both interactive preview and export.
///
/// Guarantees 100% mathematical parity between `render_preview_from_base_profiled` and
/// `render_export_image_with_depth` while eliminating ~450 lines of duplicated pixel math.
pub struct PreparedGradingFrame<'a> {
    base: &'a BaseRgbPreview,
    grading: &'a DarkroomGradingState,
    geometry: FrameGeometry,
    uniforms: GradingUniformPayload,
    buffers: FramePrecomputedBuffers,
}

impl<'a> PreparedGradingFrame<'a> {
    /// Precomputes all frame-level matrices, LUTs, denoise/defringe buffers, AABB brush grids,
    /// and smart lighting gain maps required for parallel per-pixel evaluation.
    #[must_use]
    pub fn prepare(
        base: &'a BaseRgbPreview,
        grading: &'a DarkroomGradingState,
        allow_defringe_mask: bool,
    ) -> Self {
        let preview_w = base.width;
        let preview_h = base.height;

        let exposure_gain = 2.0_f32.powf(grading.tone.exposure_ev);
        let kelvin_ratio = (grading.color.kelvin / 5500.0).clamp(0.4, 2.5);
        let r_temp_scale = kelvin_ratio.powf(0.85);
        let b_temp_scale = (1.0 / kelvin_ratio).powf(0.85);
        let tint_scale = 1.0 - (grading.color.tint / 100.0);

        let eff_wb_r = base.as_shot_wb[0] * r_temp_scale;
        let eff_wb_g = base.as_shot_wb[1] * tint_scale;
        let eff_wb_b = base.as_shot_wb[2] * b_temp_scale;

        let mat = base.cam_to_srgb;
        let cx = preview_w as f32 * 0.5;
        let cy = preview_h as f32 * 0.5;
        let max_radius_sq = (cx * cx + cy * cy).max(1.0);

        let (k_dist, v_gain) = resolve_optical_distortion_and_vignette(base, grading);

        let rot_deg = grading.geometry.rotation_degrees;
        let rot_rad = rot_deg.to_radians();
        let cos_rot = rot_rad.cos();
        let sin_rot = rot_rad.sin();

        let flip_x = if grading.geometry.flip_horizontal {
            -1.0
        } else {
            1.0
        };
        let flip_y = if grading.geometry.flip_vertical {
            -1.0
        } else {
            1.0
        };

        let (keystone_v, keystone_h) = if grading.geometry.geometry_enabled {
            (
                grading.geometry.keystone_vertical / 100.0,
                grading.geometry.keystone_horizontal / 100.0,
            )
        } else {
            (0.0, 0.0)
        };

        let denoise_cfg = rraw_core::DenoiseConfig {
            chroma_amount: (grading.denoise_sharpen.color_denoise / 100.0).clamp(0.0, 1.0),
            luma_amount: (grading.denoise_sharpen.luma_denoise / 100.0).clamp(0.0, 1.0),
        };
        let defringe_cfg = rraw_optics::DefringeConfig {
            purple_amount: (grading.optics.defringe_purple_amount / 100.0).clamp(0.0, 1.0),
            green_amount: (grading.optics.defringe_green_amount / 100.0).clamp(0.0, 1.0),
            threshold: grading.optics.defringe_threshold.clamp(0.01, 0.50),
        };
        let need_ane = grading.denoise_sharpen.ane_neural_denoise
            && grading.denoise_sharpen.ane_denoise_strength > 1e-4;
        let need_denoise =
            denoise_cfg.chroma_amount > 1e-4 || denoise_cfg.luma_amount > 1e-4 || need_ane;
        let need_defringe = defringe_cfg.purple_amount > 1e-4 || defringe_cfg.green_amount > 1e-4;
        let show_defringe_mask = allow_defringe_mask && grading.optics.defringe_preview_mask;
        let sharpen_excess = (grading.denoise_sharpen.capture_sharpening_amount - 0.5).max(0.0);
        let need_sharpen = !show_defringe_mask && sharpen_excess > 1e-4;

        let processed_samples =
            if need_denoise || need_defringe || show_defringe_mask || need_sharpen {
                let mut s = base.samples_rgb.to_vec();
                if need_denoise {
                    rraw_core::denoise_rgb_with_wb(
                        &mut s,
                        preview_w,
                        preview_h,
                        &denoise_cfg,
                        Some([eff_wb_r, eff_wb_g, eff_wb_b]),
                    );
                    if need_ane {
                        let str_f =
                            (grading.denoise_sharpen.ane_denoise_strength / 100.0).clamp(0.0, 1.0);
                        apply_ane_neural_denoise_rgb(&mut s, preview_w, preview_h, str_f);
                    }
                }
                if need_sharpen && preview_w > 0 && preview_h > 0 {
                    if let (Ok(w_u32), Ok(h_u32), Ok(noise_profile)) = (
                        u32::try_from(preview_w),
                        u32::try_from(preview_h),
                        rraw_core::NoiseProfile::new(
                            [0.0002; 3],
                            [0.00005; 3],
                            rraw_core::NoiseProfileSource::WaveletMadFallback,
                        ),
                    ) {
                        let rl_params = rraw_optics::RichardsonLucyParams {
                            iterations: if sharpen_excess >= 1.0 { 2 } else { 1 },
                            epsilon: 1e-4,
                            base_psf_sigma: (0.45 + sharpen_excess * 0.45).clamp(0.35, 1.25),
                            deringing_threshold: 0.08,
                            noise_profile,
                        };
                        let mut flat_rgb = Vec::with_capacity(s.len().saturating_mul(3));
                        for px in &s {
                            flat_rgb.extend_from_slice(px);
                        }
                        let deconvolved = rraw_optics::execute_richardson_lucy(
                            &flat_rgb, w_u32, h_u32, &rl_params,
                        );
                        let blend = sharpen_excess.clamp(0.0, 1.0);
                        if deconvolved.len() == s.len().saturating_mul(3) {
                            for (i, px) in s.iter_mut().enumerate() {
                                px[0] = px[0] * (1.0 - blend) + deconvolved[i * 3] * blend;
                                px[1] = px[1] * (1.0 - blend) + deconvolved[i * 3 + 1] * blend;
                                px[2] = px[2] * (1.0 - blend) + deconvolved[i * 3 + 2] * blend;
                            }
                        }
                    }
                }
                if show_defringe_mask {
                    rraw_optics::defringe_mask_rgb(&mut s, preview_w, preview_h, &defringe_cfg);
                } else if need_defringe {
                    rraw_optics::defringe_rgb(&mut s, preview_w, preview_h, &defringe_cfg);
                }
                Some(s)
            } else {
                None
            };

        let has_creative_vignette = grading.effects.creative_vignette_amount.abs() > 0.1;
        let cv_amt = grading.effects.creative_vignette_amount / 100.0;
        let cv_mid = (grading.effects.creative_vignette_midpoint / 100.0).clamp(0.05, 0.95);
        let cv_feather = (grading.effects.creative_vignette_feather / 100.0).clamp(0.05, 0.95);
        let cv_round = (grading.effects.creative_vignette_roundness / 100.0).clamp(-1.0, 1.0);

        let curves_lut = rraw_color::curves::generate_1d_lut_1024(&grading.tone_curves);
        let has_curves = !grading.tone_curves.is_identity();
        let brush_grids: Vec<Option<PrecomputedBrushGrid>> = grading
            .masks
            .local_masks
            .iter()
            .map(PrecomputedBrushGrid::from_layer)
            .collect();

        let opt_zoom = if k_dist > 1e-5 {
            let mut z = 1.0 + k_dist * 0.8;
            for _ in 0..4 {
                let f = z * z * z - z * z - k_dist;
                let fp = 3.0 * z * z - 2.0 * z;
                if fp.abs() > 1e-6 {
                    z -= f / fp;
                }
            }
            z.max(1.0)
        } else if k_dist < -1e-5 {
            let min_warp = 1.0 + k_dist * (cx * cx / max_radius_sq);
            (1.0 / min_warp.max(0.1)).max(1.0)
        } else {
            1.0
        };

        let geom_zoom = if grading.geometry.geometry_enabled && grading.geometry.auto_crop_geometry
        {
            let abs_rot = rot_deg.abs().to_radians();
            let rot_zoom = abs_rot.cos() + abs_rot.sin() * (cy / cx.max(1.0));
            let k_zoom = 1.0
                + (grading
                    .geometry
                    .keystone_vertical
                    .abs()
                    .max(grading.geometry.keystone_horizontal.abs())
                    / 100.0)
                    * 0.5;
            (rot_zoom * k_zoom).max(1.0)
        } else {
            1.0
        };
        let auto_zoom = geom_zoom * opt_zoom;

        let is_identity_geom_base = rot_deg.abs() < 1e-4
            && k_dist.abs() < 1e-5
            && keystone_v.abs() < 1e-5
            && keystone_h.abs() < 1e-5
            && flip_x > 0.0
            && flip_y > 0.0
            && (auto_zoom - 1.0).abs() < 1e-4;

        let has_tonal_adjustment = grading.tone.shadows.abs() > 0.01
            || grading.tone.highlights.abs() > 0.01
            || grading.tone.blacks.abs() > 0.01;

        let combined_mat = [
            mat[0] * eff_wb_r * exposure_gain,
            mat[1] * eff_wb_g * exposure_gain,
            mat[2] * eff_wb_b * exposure_gain,
            mat[3] * eff_wb_r * exposure_gain,
            mat[4] * eff_wb_g * exposure_gain,
            mat[5] * eff_wb_b * exposure_gain,
            mat[6] * eff_wb_r * exposure_gain,
            mat[7] * eff_wb_g * exposure_gain,
            mat[8] * eff_wb_b * exposure_gain,
        ];

        let base_contrast_factor = 1.0 + (grading.tone.contrast / 100.0).clamp(-0.8, 2.0);
        let base_vibrance = 0.10_f32;
        let user_vibrance = grading.tone.vibrance / 100.0;
        let user_sat = grading.tone.saturation / 100.0;

        let effective_samples = processed_samples.as_deref().unwrap_or(&base.samples_rgb);
        let smart_lighting_map: Option<Vec<f32>> = if grading.smart_lighting.is_active() {
            let n_pix = preview_w * preview_h;
            let mut rgb_linear = Vec::with_capacity(n_pix);
            for s in effective_samples.iter().take(n_pix) {
                let [r, g, b] =
                    clip_saturated_sensor_highlights(*s, [eff_wb_r, eff_wb_g, eff_wb_b]);
                let r_scaled =
                    (combined_mat[0] * r + combined_mat[1] * g + combined_mat[2] * b).max(0.0);
                let g_scaled =
                    (combined_mat[3] * r + combined_mat[4] * g + combined_mat[5] * b).max(0.0);
                let b_scaled =
                    (combined_mat[6] * r + combined_mat[7] * g + combined_mat[8] * b).max(0.0);
                rgb_linear.push([r_scaled, g_scaled, b_scaled]);
            }
            if rgb_linear.len() == n_pix {
                let orig_lumas: Vec<f32> = rgb_linear
                    .iter()
                    .map(|rgb| rraw_color::smart_lighting::aces_cg_luminance(*rgb).max(0.0))
                    .collect();
                if rraw_color::smart_lighting::apply_smart_lighting_acescg(
                    &mut rgb_linear,
                    preview_w,
                    preview_h,
                    &grading.smart_lighting,
                )
                .is_ok()
                {
                    let gains: Vec<f32> = rgb_linear
                        .iter()
                        .enumerate()
                        .map(|(i, mapped)| {
                            let orig_y = orig_lumas[i];
                            let mapped_y =
                                rraw_color::smart_lighting::aces_cg_luminance(*mapped).max(0.0);
                            if orig_y > 1e-7 {
                                mapped_y / orig_y
                            } else {
                                1.0
                            }
                        })
                        .collect();
                    Some(gains)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        Self {
            base,
            grading,
            geometry: FrameGeometry {
                preview_w,
                preview_h,
                cx,
                cy,
                max_radius_sq,
                k_dist,
                v_gain,
                cos_rot,
                sin_rot,
                flip_x,
                flip_y,
                keystone_v,
                keystone_h,
                auto_zoom,
                is_identity_geom_base,
            },
            uniforms: GradingUniformPayload {
                combined_mat,
                eff_wb: [eff_wb_r, eff_wb_g, eff_wb_b],
                has_tonal_adjustment,
                has_curves,
                base_contrast_factor,
                base_vibrance,
                user_vibrance,
                user_sat,
                has_creative_vignette,
                cv_amt,
                cv_mid,
                cv_feather,
                cv_round,
            },
            buffers: FramePrecomputedBuffers {
                processed_samples,
                smart_lighting_map,
                curves_lut,
                brush_grids,
            },
        }
    }

    /// Returns the precomputed frame geometry and optical warp parameters.
    #[inline]
    #[must_use]
    pub const fn geometry(&self) -> &FrameGeometry {
        &self.geometry
    }

    /// Returns the precomputed scalar/matrix grading uniforms for this frame.
    #[inline]
    #[must_use]
    pub const fn uniforms(&self) -> &GradingUniformPayload {
        &self.uniforms
    }

    /// Returns the precomputed spatial brush acceleration grids.
    #[inline]
    #[must_use]
    pub fn brush_grids(&self) -> &[Option<PrecomputedBrushGrid>] {
        &self.buffers.brush_grids
    }

    /// Returns the effective linear RGB samples (after optional denoise/defringe pre-processing).
    #[inline]
    #[must_use]
    pub fn effective_samples(&self) -> &[[f32; 3]] {
        self.buffers
            .processed_samples
            .as_deref()
            .unwrap_or(&self.base.samples_rgb)
    }

    /// Returns the precomputed Smart Lighting per-pixel luminance gain map, if active.
    #[inline]
    #[must_use]
    pub fn smart_lighting_map(&self) -> Option<&[f32]> {
        self.buffers.smart_lighting_map.as_deref()
    }

    /// Evaluates the pre-Stage-5 linear RGB value at `(px, py)` including optical lens distortion,
    /// radial vignetting, geometric warp (rotation, flip, keystone), white balance, exposure,
    /// camera-to-sRGB matrix, Smart Lighting gain map, and shadows/highlights/blacks tonal shift.
    #[inline]
    #[must_use]
    pub fn evaluate_pre_stage5_linear(&self, px: usize, py: usize) -> [f32; 3] {
        let geom = &self.geometry;
        let u = &self.uniforms;
        let effective_samples = self.effective_samples();
        let max_sample_idx = effective_samples.len().saturating_sub(1);

        let (sample_idx, v_mult) = if geom.is_identity_geom_base {
            let s_idx = (py * geom.preview_w + px).min(max_sample_idx);
            let v = if geom.v_gain > 1e-4 {
                let kx = px as f32 - geom.cx;
                let ky = py as f32 - geom.cy;
                let norm_r2 = (kx * kx + ky * ky) / geom.max_radius_sq;
                1.0 + geom.v_gain * norm_r2 + (geom.v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };
            (s_idx, v)
        } else {
            let orig_x = px as f32;
            let orig_y = py as f32;
            let raw_dx = (orig_x - geom.cx) * geom.flip_x;
            let raw_dy = (orig_y - geom.cy) * geom.flip_y;
            let rx = raw_dx * geom.cos_rot + raw_dy * geom.sin_rot;
            let ry = -raw_dx * geom.sin_rot + raw_dy * geom.cos_rot;
            let v_scale = (1.0 + geom.keystone_v * (ry / geom.cy)).max(0.1);
            let kx = (rx / v_scale) / geom.auto_zoom;
            let h_scale = (1.0 + geom.keystone_h * (kx / geom.cx)).max(0.1);
            let ky = (ry / h_scale) / geom.auto_zoom;

            let s_idx = if geom.k_dist.abs() > 1e-5 {
                let r2 = (kx * kx + ky * ky) / geom.max_radius_sq;
                let warp = 1.0 + geom.k_dist * r2;
                let warped_x = ((geom.cx + kx * warp).round() as isize)
                    .clamp(0, geom.preview_w as isize - 1) as usize;
                let warped_y = ((geom.cy + ky * warp).round() as isize)
                    .clamp(0, geom.preview_h as isize - 1) as usize;
                warped_y * geom.preview_w + warped_x
            } else {
                let sx = ((geom.cx + kx).round() as isize).clamp(0, geom.preview_w as isize - 1)
                    as usize;
                let sy = ((geom.cy + ky).round() as isize).clamp(0, geom.preview_h as isize - 1)
                    as usize;
                sy * geom.preview_w + sx
            };

            let v = if geom.v_gain > 1e-4 {
                let norm_r2 = (kx * kx + ky * ky) / geom.max_radius_sq;
                1.0 + geom.v_gain * norm_r2 + (geom.v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };
            (s_idx.min(max_sample_idx), v)
        };

        let [mean_r, mean_g, mean_b] =
            clip_saturated_sensor_highlights(effective_samples[sample_idx], u.eff_wb);
        let mut r_s =
            (u.combined_mat[0] * mean_r + u.combined_mat[1] * mean_g + u.combined_mat[2] * mean_b)
                .max(0.0)
                * v_mult;
        let mut g_s =
            (u.combined_mat[3] * mean_r + u.combined_mat[4] * mean_g + u.combined_mat[5] * mean_b)
                .max(0.0)
                * v_mult;
        let mut b_s =
            (u.combined_mat[6] * mean_r + u.combined_mat[7] * mean_g + u.combined_mat[8] * mean_b)
                .max(0.0)
                * v_mult;

        if let Some(gains) = self.smart_lighting_map() {
            let sl_idx = sample_idx.min(gains.len().saturating_sub(1));
            let g = gains[sl_idx];
            r_s *= g;
            g_s *= g;
            b_s *= g;
        }

        if u.has_tonal_adjustment {
            let lum = 0.2126 * r_s + 0.7152 * g_s + 0.0722 * b_s;
            let y_norm = lum / (lum + 0.18);
            let shadow_shift = (self.grading.tone.shadows / 100.0) * 2.5 * (1.0 - y_norm).powi(2);
            let highlight_shift = (self.grading.tone.highlights / 100.0) * 2.5 * y_norm.powi(2);
            let blacks_shift = (self.grading.tone.blacks / 100.0) * 1.5 * (1.0 - y_norm).powi(4);
            let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);
            [
                (r_s * tone_gain).max(0.0),
                (g_s * tone_gain).max(0.0),
                (b_s * tone_gain).max(0.0),
            ]
        } else {
            [r_s, g_s, b_s]
        }
    }

    /// Returns the base preview width in pixels.
    #[inline]
    #[must_use]
    pub const fn preview_w(&self) -> usize {
        self.geometry.preview_w
    }

    /// Returns the base preview height in pixels.
    #[inline]
    #[must_use]
    pub const fn preview_h(&self) -> usize {
        self.geometry.preview_h
    }

    /// Returns true if the frame has identity geometry (no rotation, distortion, keystone, flip, or auto-zoom).
    #[inline]
    #[must_use]
    pub const fn is_identity_geom_base(&self) -> bool {
        self.geometry.is_identity_geom_base
    }

    /// Evaluates the full per-pixel optical, tonal, curve, 3D LUT, local mask, DRT, and soft-proofing chain.
    ///
    /// Returns `([r_post, g_post, b_post], soft_proof_result, u_norm, v_norm)` prior to display OETF quantization.
    #[inline]
    #[must_use]
    pub fn evaluate_pixel(
        &self,
        orig_x: f32,
        orig_y: f32,
        identity_xy: Option<(usize, usize)>,
    ) -> ([f32; 3], SoftProofResult, f32, f32) {
        let geom = &self.geometry;
        let u = &self.uniforms;
        let effective_samples = self
            .buffers
            .processed_samples
            .as_deref()
            .unwrap_or(&self.base.samples_rgb);
        let max_sample_idx = effective_samples.len().saturating_sub(1);

        let (sample_idx, v_mult, u_norm, v_norm) = if let Some((ix, iy)) = identity_xy {
            let s_idx = (iy * geom.preview_w + ix).min(max_sample_idx);
            let v = if geom.v_gain > 1e-4 {
                let kx = ix as f32 - geom.cx;
                let ky = iy as f32 - geom.cy;
                let norm_r2 = (kx * kx + ky * ky) / geom.max_radius_sq;
                1.0 + geom.v_gain * norm_r2 + (geom.v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };
            let un = (ix as f32 + 0.5) / (geom.preview_w.max(1) as f32);
            let vn = (iy as f32 + 0.5) / (geom.preview_h.max(1) as f32);
            (s_idx, v, un, vn)
        } else {
            let raw_dx = (orig_x - geom.cx) * geom.flip_x;
            let raw_dy = (orig_y - geom.cy) * geom.flip_y;
            let rx = raw_dx * geom.cos_rot + raw_dy * geom.sin_rot;
            let ry = -raw_dx * geom.sin_rot + raw_dy * geom.cos_rot;
            let v_scale = (1.0 + geom.keystone_v * (ry / geom.cy)).max(0.1);
            let kx = (rx / v_scale) / geom.auto_zoom;
            let h_scale = (1.0 + geom.keystone_h * (kx / geom.cx)).max(0.1);
            let ky = (ry / h_scale) / geom.auto_zoom;

            let s_idx = if geom.k_dist.abs() > 1e-5 {
                let r2 = (kx * kx + ky * ky) / geom.max_radius_sq;
                let warp = 1.0 + geom.k_dist * r2;
                let warped_x = ((geom.cx + kx * warp).round() as isize)
                    .clamp(0, geom.preview_w as isize - 1) as usize;
                let warped_y = ((geom.cy + ky * warp).round() as isize)
                    .clamp(0, geom.preview_h as isize - 1) as usize;
                warped_y * geom.preview_w + warped_x
            } else {
                let sx = ((geom.cx + kx).round() as isize).clamp(0, geom.preview_w as isize - 1)
                    as usize;
                let sy = ((geom.cy + ky).round() as isize).clamp(0, geom.preview_h as isize - 1)
                    as usize;
                sy * geom.preview_w + sx
            };

            let v = if geom.v_gain > 1e-4 {
                let norm_r2 = (kx * kx + ky * ky) / geom.max_radius_sq;
                1.0 + geom.v_gain * norm_r2 + (geom.v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };
            let un = (orig_x / (geom.preview_w.max(1) as f32)).clamp(0.0, 1.0);
            let vn = (orig_y / (geom.preview_h.max(1) as f32)).clamp(0.0, 1.0);
            (s_idx.min(max_sample_idx), v, un, vn)
        };

        let [mean_r, mean_g, mean_b] =
            clip_saturated_sensor_highlights(effective_samples[sample_idx], u.eff_wb);

        let mut r_scaled =
            (u.combined_mat[0] * mean_r + u.combined_mat[1] * mean_g + u.combined_mat[2] * mean_b)
                .max(0.0)
                * v_mult;
        let mut g_scaled =
            (u.combined_mat[3] * mean_r + u.combined_mat[4] * mean_g + u.combined_mat[5] * mean_b)
                .max(0.0)
                * v_mult;
        let mut b_scaled =
            (u.combined_mat[6] * mean_r + u.combined_mat[7] * mean_g + u.combined_mat[8] * mean_b)
                .max(0.0)
                * v_mult;

        if let Some(gains) = &self.buffers.smart_lighting_map {
            let sl_idx = sample_idx.min(gains.len().saturating_sub(1));
            let g = gains[sl_idx];
            r_scaled *= g;
            g_scaled *= g;
            b_scaled *= g;
        }

        let (r_toned, g_toned, b_toned) = if u.has_tonal_adjustment {
            let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
            let y_norm = lum / (lum + 0.18);
            let shadow_weight = (1.0 - y_norm).powi(2);
            let highlight_weight = y_norm.powi(2);
            let shadow_shift = (self.grading.tone.shadows / 100.0) * 2.5 * shadow_weight;
            let highlight_shift = (self.grading.tone.highlights / 100.0) * 2.5 * highlight_weight;
            let blacks_weight = (1.0 - y_norm).powi(4);
            let blacks_shift = (self.grading.tone.blacks / 100.0) * 1.5 * blacks_weight;
            let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);
            (
                (r_scaled * tone_gain).max(0.0),
                (g_scaled * tone_gain).max(0.0),
                (b_scaled * tone_gain).max(0.0),
            )
        } else {
            (r_scaled, g_scaled, b_scaled)
        };

        let (r_curved, g_curved, b_curved) = if u.has_curves {
            let c = rraw_color::curves::apply_tone_curves_lut(
                glam::Vec3::new(r_toned, g_toned, b_toned),
                &self.buffers.curves_lut,
            );
            (c.x, c.y, c.z)
        } else {
            (r_toned, g_toned, b_toned)
        };

        let (r_lut, g_lut, b_lut) = if let Some(clut) = &self.grading.custom_clut {
            let out = clut
                .lut
                .sample_tetrahedral(glam::Vec3::new(r_curved, g_curved, b_curved));
            let w = clut.strength.clamp(0.0, 1.0);
            (
                r_curved * (1.0 - w) + out.x * w,
                g_curved * (1.0 - w) + out.y * w,
                b_curved * (1.0 - w) + out.z * w,
            )
        } else {
            (r_curved, g_curved, b_curved)
        };

        let (sem_ev, sem_contrast, sem_sat, sem_temp) = if !self.grading.masks.is_empty() {
            eval_all_mask_adjustments_with_brush_grids(
                &self.base.semantic_masks,
                &self.grading.masks.semantic_masks,
                &self.grading.masks.local_masks,
                &self.buffers.brush_grids,
                u_norm,
                v_norm,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let (r_sem, g_sem, b_sem) = if sem_ev.abs() > 1e-4 || sem_temp.abs() > 1e-3 {
            let g_ev = 2.0_f32.powf(sem_ev);
            let r_t = (1.0 + sem_temp * 0.004).max(0.1);
            let b_t = (1.0 - sem_temp * 0.004).max(0.1);
            (
                (r_lut * g_ev * r_t).max(0.0),
                (g_lut * g_ev).max(0.0),
                (b_lut * g_ev * b_t).max(0.0),
            )
        } else {
            (r_lut, g_lut, b_lut)
        };

        let (r_drt, g_drt, b_drt) = match self.grading.color.display_transform {
            DisplayTransform::AgX => (
                apply_agx_edr(r_sem, 1.0),
                apply_agx_edr(g_sem, 1.0),
                apply_agx_edr(b_sem, 1.0),
            ),
            DisplayTransform::OpenDrt => (
                apply_opendrt(r_sem, 1.0),
                apply_opendrt(g_sem, 1.0),
                apply_opendrt(b_sem, 1.0),
            ),
        };

        let eff_factor =
            (u.base_contrast_factor * (1.0 + (sem_contrast / 100.0).clamp(-0.8, 2.0))).max(0.05);
        let r_c = ((r_drt - 0.18) * eff_factor + 0.18).max(0.0);
        let g_c = ((g_drt - 0.18) * eff_factor + 0.18).max(0.0);
        let b_c = ((b_drt - 0.18) * eff_factor + 0.18).max(0.0);

        let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
        let max_val = r_c.max(g_c).max(b_c);
        let min_val = r_c.min(g_c).min(b_c);
        let current_sat = if max_val > 1e-4 {
            (max_val - min_val) / max_val
        } else {
            0.0
        };
        let vib_weight = (1.0 - current_sat).clamp(0.0, 1.0);
        let eff_sat = ((1.0 + u.user_sat + (sem_sat / 100.0))
            * (1.0 + (u.base_vibrance + u.user_vibrance) * vib_weight))
            .max(0.0);

        let r_s = lum_c + (r_c - lum_c) * eff_sat;
        let g_s = lum_c + (g_c - lum_c) * eff_sat;
        let b_s = lum_c + (b_c - lum_c) * eff_sat;

        let (r_s, g_s, b_s) = if u.has_creative_vignette {
            let u_rel = (orig_x - geom.cx) / geom.cx.max(1.0);
            let v_rel = (orig_y - geom.cy) / geom.cy.max(1.0);
            let r_ellip = (u_rel * u_rel + v_rel * v_rel).sqrt();
            let r_circ = ((orig_x - geom.cx).powi(2) + (orig_y - geom.cy).powi(2)).sqrt()
                / geom.cx.min(geom.cy).max(1.0);
            let r_rect = u_rel.abs().max(v_rel.abs());
            let r_dist = if u.cv_round > 0.0 {
                r_ellip * (1.0 - u.cv_round) + r_circ * u.cv_round
            } else {
                r_ellip * (1.0 + u.cv_round) + r_rect * (-u.cv_round)
            };
            let t = ((r_dist - u.cv_mid) / (u.cv_feather.max(0.05) * 1.2)).clamp(0.0, 1.0);
            let smooth_t = t * t * (3.0 - 2.0 * t);
            let cv_mult = (1.0 + u.cv_amt * smooth_t).max(0.0);
            (r_s * cv_mult, g_s * cv_mult, b_s * cv_mult)
        } else {
            (r_s, g_s, b_s)
        };

        let sp_res = rraw_color::evaluate_soft_proof(
            glam::Vec3::new(r_s, g_s, b_s),
            &self.grading.soft_proof,
        );

        let rgb_post = if self.grading.soft_proof.enabled {
            [
                sp_res.proofed_rgb.x,
                sp_res.proofed_rgb.y,
                sp_res.proofed_rgb.z,
            ]
        } else {
            [r_s.max(0.0), g_s.max(0.0), b_s.max(0.0)]
        };

        (rgb_post, sp_res, u_norm, v_norm)
    }

    /// Applies the display toe and gamma 2.2 OETF in floating point `[0.0, 1.0]`.
    #[inline]
    #[must_use]
    pub fn map_display_oetf(x: f32) -> f32 {
        let clamped = x.clamp(0.0, 1.0);
        let toe = if clamped < 0.35 {
            clamped.powf(1.08)
        } else {
            clamped
        };
        toe.powf(1.0 / 2.2)
    }

    /// Quantizes a single post-DRT linear channel to 8-bit display space.
    #[inline]
    #[must_use]
    pub fn to_u8_fast(x: f32) -> u8 {
        (Self::map_display_oetf(x) * 255.0 + 0.5).clamp(0.0, 255.0) as u8
    }

    /// Converts post-DRT `[r_post, g_post, b_post]` to 8-bit display RGB `[u8; 3]`, honoring monochrome luminance mixing.
    #[inline]
    #[must_use]
    pub fn quantize_display_u8(&self, rgb_post: [f32; 3]) -> [u8; 3] {
        let ur = Self::to_u8_fast(rgb_post[0]);
        let ug = Self::to_u8_fast(rgb_post[1]);
        let ub = Self::to_u8_fast(rgb_post[2]);
        if self.grading.color.is_monochrome {
            let lum_val = ((ur as u32 * 54 + ug as u32 * 183 + ub as u32 * 19) >> 8) as u8;
            [lum_val, lum_val, lum_val]
        } else {
            [ur, ug, ub]
        }
    }

    /// Converts post-DRT `[r_post, g_post, b_post]` to 16-bit display RGB `[u16; 3]`, honoring monochrome luminance mixing.
    #[inline]
    #[must_use]
    pub fn quantize_display_u16(&self, rgb_post: [f32; 3]) -> [u16; 3] {
        let fr = Self::map_display_oetf(rgb_post[0]);
        let fg = Self::map_display_oetf(rgb_post[1]);
        let fb = Self::map_display_oetf(rgb_post[2]);
        let [mr, mg, mb] = if self.grading.color.is_monochrome {
            let lum_val = (fr * 54.0 + fg * 183.0 + fb * 19.0) / 256.0;
            [lum_val, lum_val, lum_val]
        } else {
            [fr, fg, fb]
        };
        [
            (mr * 65535.0).clamp(0.0, 65535.0).round() as u16,
            (mg * 65535.0).clamp(0.0, 65535.0).round() as u16,
            (mb * 65535.0).clamp(0.0, 65535.0).round() as u16,
        ]
    }
}

/// Applies Apple Neural Engine / CoreML structure-preserving luminance denoising to linear RGB samples.
fn apply_ane_neural_denoise_rgb(
    samples: &mut [[f32; 3]],
    width: usize,
    height: usize,
    strength: f32,
) {
    if width == 0
        || height == 0
        || samples.is_empty()
        || samples.len() != width.saturating_mul(height)
        || strength <= 1e-4
    {
        return;
    }
    let count = samples.len();
    let mut luma = Vec::with_capacity(count);
    for &[r, g, b] in samples.iter() {
        let r_v = if r.is_finite() { r.max(0.0) } else { 0.0 };
        let g_v = if g.is_finite() { g.max(0.0) } else { 0.0 };
        let b_v = if b.is_finite() { b.max(0.0) } else { 0.0 };
        luma.push(0.2126 * r_v + 0.7152 * g_v + 0.0722 * b_v);
    }

    let engine = rraw_coreml::CoreMlEngine::new();
    if let Ok(denoised_luma) = engine.run_tile_denoise(&luma, width, height) {
        for (i, sample) in samples.iter_mut().enumerate() {
            if let Some(&dl) = denoised_luma.get(i) {
                let orig_l = luma[i];
                if orig_l > 1e-5 {
                    let target_l = (orig_l + (dl - orig_l) * strength).max(0.0);
                    let scale = target_l / orig_l;
                    sample[0] = (sample[0] * scale).max(0.0);
                    sample[1] = (sample[1] * scale).max(0.0);
                    sample[2] = (sample[2] * scale).max(0.0);
                } else {
                    let diff = (dl - orig_l) * strength;
                    sample[0] = (sample[0] + diff).max(0.0);
                    sample[1] = (sample[1] + diff).max(0.0);
                    sample[2] = (sample[2] + diff).max(0.0);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
