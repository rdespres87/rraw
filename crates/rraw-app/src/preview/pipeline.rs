#![forbid(unsafe_code)]

//! Color rendering pipeline, LUT evaluation, tone curves and export rasterization.

use egui::Color32;
use rayon::prelude::*;
use rraw_color::{apply_agx_edr, apply_opendrt, DisplayTransform};
use rraw_core::SignedCfaBuffer;

use super::masks::eval_all_mask_adjustments;
use super::types::BaseRgbPreview;
use crate::state::*;

/// Evaluates darkroom grading, optical distortion, vignetting, and histogram from `BaseRgbPreview`.
///
/// Executes in < 1 ms over contiguous RAM without re-sampling the raw sensor matrix.
#[must_use]
pub fn render_preview_from_base(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
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

    let (k_dist, v_gain) = match grading.optics.optical_tab {
        OpticalCorrectionTab::Profile => {
            let active_matched = if let Some(override_name) = &grading.optics.lens_profile_override
            {
                let db = rraw_optics::LensfunDatabase::global();
                let focal = base.focal_length_mm.unwrap_or(50.0);
                let aperture = base.aperture_f_number;
                db.find_lens(override_name)
                    .map(|lens| rraw_optics::MatchedLens {
                        profile_name: format!("{} {}", lens.maker, lens.model),
                        maker: lens.maker.clone(),
                        model: db.interpolate_model(lens, focal, aperture),
                        is_exact_match: true,
                    })
                    .or_else(|| base.matched_lens.clone())
            } else {
                base.matched_lens.clone()
            };

            if let Some(matched) = &active_matched {
                if grading.optics.lens_profile_auto {
                    let dist_scale = (grading.optics.lens_distortion_strength / 100.0).max(0.0);
                    let vig_scale = (grading.optics.lens_vignette_strength / 100.0).max(0.0);
                    (
                        matched.model.distortion.k1 * dist_scale,
                        matched.model.vignette.v1 * vig_scale,
                    )
                } else {
                    (0.0, 0.0)
                }
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
    };

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
    let show_defringe_mask = grading.optics.defringe_preview_mask;

    let processed_samples = if need_denoise || need_defringe || show_defringe_mask {
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
                let str_f = (grading.denoise_sharpen.ane_denoise_strength / 100.0).clamp(0.0, 1.0);
                apply_ane_neural_denoise_rgb(&mut s, preview_w, preview_h, str_f);
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
    let effective_samples = processed_samples.as_deref().unwrap_or(&base.samples_rgb);

    let has_creative_vignette = grading.effects.creative_vignette_amount.abs() > 0.1;
    let cv_amt = grading.effects.creative_vignette_amount / 100.0;
    let cv_mid = (grading.effects.creative_vignette_midpoint / 100.0).clamp(0.05, 0.95);
    let cv_feather = (grading.effects.creative_vignette_feather / 100.0).clamp(0.05, 0.95);
    let cv_round = (grading.effects.creative_vignette_roundness / 100.0).clamp(-1.0, 1.0);

    let curves_lut = rraw_color::curves::generate_1d_lut_1024(&grading.tone_curves);
    let has_curves = !grading.tone_curves.is_identity();

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

    let geom_zoom = if grading.geometry.geometry_enabled && grading.geometry.auto_crop_geometry {
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

    let is_identity_geom = rot_deg.abs() < 1e-4
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

    let factor = 1.0 + (grading.tone.contrast / 100.0).clamp(-0.8, 2.0);
    let base_vibrance = 0.10_f32;
    let user_vibrance = grading.tone.vibrance / 100.0;
    let user_sat = grading.tone.saturation / 100.0;

    let to_u8_fast = |x: f32| -> u8 {
        let clamped = x.clamp(0.0, 1.0);
        let toe = if clamped < 0.35 {
            clamped.powf(1.08)
        } else {
            clamped
        };
        let mapped = toe.powf(1.0 / 2.2);
        (mapped * 255.0 + 0.5).clamp(0.0, 255.0) as u8
    };

    let smart_lighting_map: Option<Vec<f32>> = if grading.smart_lighting.is_active() {
        let n_pix = preview_w * preview_h;
        let mut rgb_linear = Vec::with_capacity(n_pix);
        for s in effective_samples.iter().take(n_pix) {
            let [r, g, b] = *s;
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

    struct RowHistogram {
        hist_r: [u32; 256],
        hist_g: [u32; 256],
        hist_b: [u32; 256],
        hist_lum: [u32; 256],
        shadow_clipped: u32,
        highlight_clipped: u32,
        out_of_gamut_count: u32,
    }

    let mut color_pixels = vec![Color32::TRANSPARENT; preview_w * preview_h];

    let row_histograms: Vec<RowHistogram> = color_pixels
        .par_chunks_exact_mut(preview_w)
        .enumerate()
        .map(|(py, row_pixels)| {
            let mut row_r = [0_u32; 256];
            let mut row_g = [0_u32; 256];
            let mut row_b = [0_u32; 256];
            let mut row_lum = [0_u32; 256];
            let mut row_sc = 0_u32;
            let mut row_hc = 0_u32;
            let mut row_oog = 0_u32;

            let raw_dy = (py as f32 - cy) * flip_y;
            let row_identity_offset = py * preview_w;

            for (px, out_pixel) in row_pixels.iter_mut().enumerate() {
                let (sample_idx, v_mult) = if is_identity_geom {
                    let s_idx =
                        (row_identity_offset + px).min(effective_samples.len().saturating_sub(1));
                    let v = if v_gain > 1e-4 {
                        let kx = px as f32 - cx;
                        let ky = py as f32 - cy;
                        let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                        1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
                    } else {
                        1.0
                    };
                    (s_idx, v)
                } else {
                    let raw_dx = (px as f32 - cx) * flip_x;
                    let rx = raw_dx * cos_rot + raw_dy * sin_rot;
                    let ry = -raw_dx * sin_rot + raw_dy * cos_rot;
                    let v_scale = (1.0 + keystone_v * (ry / cy)).max(0.1);
                    let kx = (rx / v_scale) / auto_zoom;
                    let h_scale = (1.0 + keystone_h * (kx / cx)).max(0.1);
                    let ky = (ry / h_scale) / auto_zoom;

                    let s_idx = if k_dist.abs() > 1e-5 {
                        let r2 = (kx * kx + ky * ky) / max_radius_sq;
                        let warp = 1.0 + k_dist * r2;
                        let warped_x = ((cx + kx * warp).round() as isize)
                            .clamp(0, preview_w as isize - 1)
                            as usize;
                        let warped_y = ((cy + ky * warp).round() as isize)
                            .clamp(0, preview_h as isize - 1)
                            as usize;
                        warped_y * preview_w + warped_x
                    } else {
                        let sx =
                            ((cx + kx).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                        let sy =
                            ((cy + ky).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                        sy * preview_w + sx
                    };

                    let v = if v_gain > 1e-4 {
                        let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                        1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
                    } else {
                        1.0
                    };
                    (s_idx.min(effective_samples.len().saturating_sub(1)), v)
                };

                let [mean_r, mean_g, mean_b] = effective_samples[sample_idx];

                let mut r_scaled = (combined_mat[0] * mean_r
                    + combined_mat[1] * mean_g
                    + combined_mat[2] * mean_b)
                    .max(0.0)
                    * v_mult;
                let mut g_scaled = (combined_mat[3] * mean_r
                    + combined_mat[4] * mean_g
                    + combined_mat[5] * mean_b)
                    .max(0.0)
                    * v_mult;
                let mut b_scaled = (combined_mat[6] * mean_r
                    + combined_mat[7] * mean_g
                    + combined_mat[8] * mean_b)
                    .max(0.0)
                    * v_mult;

                if let Some(gains) = &smart_lighting_map {
                    let sl_idx = sample_idx.min(gains.len().saturating_sub(1));
                    let g = gains[sl_idx];
                    r_scaled *= g;
                    g_scaled *= g;
                    b_scaled *= g;
                }

                let (r_toned, g_toned, b_toned) = if has_tonal_adjustment {
                    let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
                    let y_norm = lum / (lum + 0.18);
                    let shadow_weight = (1.0 - y_norm).powi(2);
                    let highlight_weight = y_norm.powi(2);
                    let shadow_shift = (grading.tone.shadows / 100.0) * 2.5 * shadow_weight;
                    let highlight_shift =
                        (grading.tone.highlights / 100.0) * 2.5 * highlight_weight;
                    let blacks_weight = (1.0 - y_norm).powi(4);
                    let blacks_shift = (grading.tone.blacks / 100.0) * 1.5 * blacks_weight;
                    let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);
                    (
                        (r_scaled * tone_gain).max(0.0),
                        (g_scaled * tone_gain).max(0.0),
                        (b_scaled * tone_gain).max(0.0),
                    )
                } else {
                    (r_scaled, g_scaled, b_scaled)
                };

                let (r_curved, g_curved, b_curved) = if has_curves {
                    let c = rraw_color::curves::apply_tone_curves_lut(
                        glam::Vec3::new(r_toned, g_toned, b_toned),
                        &curves_lut,
                    );
                    (c.x, c.y, c.z)
                } else {
                    (r_toned, g_toned, b_toned)
                };

                let (r_lut, g_lut, b_lut) = if let Some(clut) = &grading.custom_clut {
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

                let u_norm = (px as f32 + 0.5) / (preview_w.max(1) as f32);
                let v_norm = (py as f32 + 0.5) / (preview_h.max(1) as f32);

                let (sem_ev, sem_contrast, sem_sat, sem_temp) =
                    if !grading.semantic_masks.is_empty() || !grading.local_masks.is_empty() {
                        eval_all_mask_adjustments(
                            &base.semantic_masks,
                            &grading.semantic_masks,
                            &grading.local_masks,
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

                let (r_drt, g_drt, b_drt) = match grading.color.display_transform {
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
                    (factor * (1.0 + (sem_contrast / 100.0).clamp(-0.8, 2.0))).max(0.05);
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
                let eff_sat = ((1.0 + user_sat + (sem_sat / 100.0))
                    * (1.0 + (base_vibrance + user_vibrance) * vib_weight))
                    .max(0.0);

                let r_s = lum_c + (r_c - lum_c) * eff_sat;
                let g_s = lum_c + (g_c - lum_c) * eff_sat;
                let b_s = lum_c + (b_c - lum_c) * eff_sat;

                let (r_s, g_s, b_s) = if has_creative_vignette {
                    let u_rel = (px as f32 - cx) / cx.max(1.0);
                    let v_rel = (py as f32 - cy) / cy.max(1.0);
                    let r_ellip = (u_rel * u_rel + v_rel * v_rel).sqrt();
                    let r_circ = ((px as f32 - cx).powi(2) + (py as f32 - cy).powi(2)).sqrt()
                        / cx.min(cy).max(1.0);
                    let r_rect = u_rel.abs().max(v_rel.abs());
                    let r_dist = if cv_round > 0.0 {
                        r_ellip * (1.0 - cv_round) + r_circ * cv_round
                    } else {
                        r_ellip * (1.0 + cv_round) + r_rect * (-cv_round)
                    };
                    let t = ((r_dist - cv_mid) / (cv_feather.max(0.05) * 1.2)).clamp(0.0, 1.0);
                    let smooth_t = t * t * (3.0 - 2.0 * t);
                    let cv_mult = (1.0 + cv_amt * smooth_t).max(0.0);
                    (r_s * cv_mult, g_s * cv_mult, b_s * cv_mult)
                } else {
                    (r_s, g_s, b_s)
                };

                let sp_res = rraw_color::evaluate_soft_proof(
                    glam::Vec3::new(r_s, g_s, b_s),
                    &grading.soft_proof,
                );
                if sp_res.is_out_of_gamut {
                    row_oog += 1;
                }

                let (r_post, g_post, b_post) = if grading.soft_proof.enabled {
                    (
                        sp_res.proofed_rgb.x,
                        sp_res.proofed_rgb.y,
                        sp_res.proofed_rgb.z,
                    )
                } else {
                    (r_s.max(0.0), g_s.max(0.0), b_s.max(0.0))
                };

                let (mut u8_r, mut u8_g, mut u8_b) = if grading.color.is_monochrome {
                    let lum_val = ((to_u8_fast(r_post) as u32 * 54
                        + to_u8_fast(g_post) as u32 * 183
                        + to_u8_fast(b_post) as u32 * 19)
                        >> 8) as u8;
                    (lum_val, lum_val, lum_val)
                } else {
                    (to_u8_fast(r_post), to_u8_fast(g_post), to_u8_fast(b_post))
                };

                if grading.soft_proof.show_gamut_warning && sp_res.is_out_of_gamut {
                    let w = (0.55 + sp_res.gamut_excursion * 1.6).clamp(0.55, 0.88);
                    // High-contrast electric magenta/cyan zebra warning
                    let is_stripe = ((px + py) & 2) == 0;
                    let (alert_r, alert_g, alert_b) = if is_stripe {
                        (255.0_f32, 0.0_f32, 220.0_f32)
                    } else {
                        (0.0_f32, 245.0_f32, 255.0_f32)
                    };
                    u8_r = ((u8_r as f32) * (1.0 - w) + alert_r * w).clamp(0.0, 255.0) as u8;
                    u8_g = ((u8_g as f32) * (1.0 - w) + alert_g * w).clamp(0.0, 255.0) as u8;
                    u8_b = ((u8_b as f32) * (1.0 - w) + alert_b * w).clamp(0.0, 255.0) as u8;
                }

                if let Some(overlay_cat) = grading.show_semantic_mask_overlay {
                    let (inv, fth) = grading
                        .semantic_masks
                        .iter()
                        .find(|m| m.category == overlay_cat)
                        .map_or((false, 0.35), |m| (m.invert, m.feather));
                    let mask_w =
                        base.semantic_masks
                            .sample_bilinear(overlay_cat, u_norm, v_norm, inv, fth)
                            * 0.65;
                    if mask_w > 1e-3 {
                        u8_r = ((u8_r as f32) * (1.0 - mask_w) + 255.0 * mask_w).clamp(0.0, 255.0)
                            as u8;
                        u8_g = ((u8_g as f32) * (1.0 - mask_w) + 40.0 * mask_w).clamp(0.0, 255.0)
                            as u8;
                        u8_b = ((u8_b as f32) * (1.0 - mask_w) + 115.0 * mask_w).clamp(0.0, 255.0)
                            as u8;
                    }
                }

                if let Some(mask_idx) = grading.show_local_mask_overlay {
                    if let Some(m) = grading.local_masks.get(mask_idx) {
                        let mask_w = m.evaluate_weight(u_norm, v_norm) * 0.65;
                        if mask_w > 1e-3 {
                            u8_r = ((u8_r as f32) * (1.0 - mask_w) + 255.0 * mask_w)
                                .clamp(0.0, 255.0) as u8;
                            u8_g = ((u8_g as f32) * (1.0 - mask_w) + 40.0 * mask_w)
                                .clamp(0.0, 255.0) as u8;
                            u8_b = ((u8_b as f32) * (1.0 - mask_w) + 115.0 * mask_w)
                                .clamp(0.0, 255.0) as u8;
                        }
                    }
                }

                row_r[u8_r as usize] += 1;
                row_g[u8_g as usize] += 1;
                row_b[u8_b as usize] += 1;

                let lum = (((u8_r as u32 * 54) + (u8_g as u32 * 183) + (u8_b as u32 * 19)) >> 8)
                    .min(255) as usize;
                row_lum[lum] += 1;

                if lum <= 1 {
                    row_sc += 1;
                }
                if lum >= 250 || u8_r >= 253 || u8_g >= 253 || u8_b >= 253 {
                    row_hc += 1;
                }

                *out_pixel = Color32::from_rgb(u8_r, u8_g, u8_b);
            }

            RowHistogram {
                hist_r: row_r,
                hist_g: row_g,
                hist_b: row_b,
                hist_lum: row_lum,
                shadow_clipped: row_sc,
                highlight_clipped: row_hc,
                out_of_gamut_count: row_oog,
            }
        })
        .collect();

    let mut hist_r = [0_u32; 256];
    let mut hist_g = [0_u32; 256];
    let mut hist_b = [0_u32; 256];
    let mut hist_lum = [0_u32; 256];
    let mut shadow_clipped = 0_u32;
    let mut highlight_clipped = 0_u32;
    let mut out_of_gamut_count = 0_u32;

    for row in row_histograms {
        for i in 0..256 {
            hist_r[i] += row.hist_r[i];
            hist_g[i] += row.hist_g[i];
            hist_b[i] += row.hist_b[i];
            hist_lum[i] += row.hist_lum[i];
        }
        shadow_clipped += row.shadow_clipped;
        highlight_clipped += row.highlight_clipped;
        out_of_gamut_count += row.out_of_gamut_count;
    }

    let max_count = hist_lum[1..255].iter().copied().max().unwrap_or(1).max(1);

    let hist_data = HistogramData {
        r: hist_r,
        g: hist_g,
        b: hist_b,
        lum: hist_lum,
        max_count,
        shadow_clipped,
        highlight_clipped,
        out_of_gamut_count,
    };

    (
        egui::ColorImage::new([preview_w, preview_h], color_pixels),
        hist_data,
    )
}

/// Renders the fully developed photographic raster for export, applying all darkroom grades,
/// cropping boundaries, geometric corrections, color transform, and scaling.
#[must_use]
pub fn render_export_image(
    base: &BaseRgbPreview,
    grading: &DarkroomGradingState,
    scale: f32,
) -> (usize, usize, Vec<u8>, Vec<u16>) {
    let preview_w = base.width;
    let preview_h = base.height;

    let (crop_x0, crop_y0, crop_x1, crop_y1) = if grading.geometry.crop_enabled {
        let x0 = (grading.geometry.crop_left.clamp(0.0, 0.95) * preview_w as f32).round() as usize;
        let y0 = (grading.geometry.crop_top.clamp(0.0, 0.95) * preview_h as f32).round() as usize;
        let x1 = (grading.geometry.crop_right.clamp(0.05, 1.0) * preview_w as f32)
            .round()
            .max(x0 as f32 + 1.0) as usize;
        let y1 = (grading.geometry.crop_bottom.clamp(0.05, 1.0) * preview_h as f32)
            .round()
            .max(y0 as f32 + 1.0) as usize;
        (
            x0.min(preview_w.saturating_sub(1)),
            y0.min(preview_h.saturating_sub(1)),
            x1.min(preview_w),
            y1.min(preview_h),
        )
    } else {
        (0, 0, preview_w, preview_h)
    };

    let cropped_w = (crop_x1 - crop_x0).max(1);
    let cropped_h = (crop_y1 - crop_y0).max(1);

    let out_w = ((cropped_w as f32 * scale).round() as usize).max(1);
    let out_h = ((cropped_h as f32 * scale).round() as usize).max(1);

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

    let (k_dist, v_gain) = match grading.optics.optical_tab {
        OpticalCorrectionTab::Profile => {
            let active_matched = if let Some(override_name) = &grading.optics.lens_profile_override
            {
                let db = rraw_optics::LensfunDatabase::global();
                let focal = base.focal_length_mm.unwrap_or(50.0);
                let aperture = base.aperture_f_number;
                db.find_lens(override_name)
                    .map(|lens| rraw_optics::MatchedLens {
                        profile_name: format!("{} {}", lens.maker, lens.model),
                        maker: lens.maker.clone(),
                        model: db.interpolate_model(lens, focal, aperture),
                        is_exact_match: true,
                    })
                    .or_else(|| base.matched_lens.clone())
            } else {
                base.matched_lens.clone()
            };

            if let Some(matched) = &active_matched {
                if grading.optics.lens_profile_auto {
                    let dist_scale = (grading.optics.lens_distortion_strength / 100.0).max(0.0);
                    let vig_scale = (grading.optics.lens_vignette_strength / 100.0).max(0.0);
                    (
                        matched.model.distortion.k1 * dist_scale,
                        matched.model.vignette.v1 * vig_scale,
                    )
                } else {
                    (0.0, 0.0)
                }
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
    };

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
    let show_defringe_mask = false;

    let processed_samples = if need_denoise || need_defringe || show_defringe_mask {
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
                let str_f = (grading.denoise_sharpen.ane_denoise_strength / 100.0).clamp(0.0, 1.0);
                apply_ane_neural_denoise_rgb(&mut s, preview_w, preview_h, str_f);
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
    let effective_samples = processed_samples.as_deref().unwrap_or(&base.samples_rgb);

    let has_creative_vignette = grading.effects.creative_vignette_amount.abs() > 0.1;
    let cv_amt = grading.effects.creative_vignette_amount / 100.0;
    let cv_mid = (grading.effects.creative_vignette_midpoint / 100.0).clamp(0.05, 0.95);
    let cv_feather = (grading.effects.creative_vignette_feather / 100.0).clamp(0.05, 0.95);
    let cv_round = (grading.effects.creative_vignette_roundness / 100.0).clamp(-1.0, 1.0);

    let curves_lut = rraw_color::curves::generate_1d_lut_1024(&grading.tone_curves);
    let has_curves = !grading.tone_curves.is_identity();

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

    let geom_zoom = if grading.geometry.geometry_enabled && grading.geometry.auto_crop_geometry {
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

    let mut u8_bytes = Vec::with_capacity(out_w * out_h * 3);
    let mut u16_samples = Vec::with_capacity(out_w * out_h * 3);

    let smart_lighting_map: Option<Vec<f32>> = if grading.smart_lighting.is_active() {
        let n_pix = preview_w * preview_h;
        let mut rgb_linear = Vec::with_capacity(n_pix);
        for s in effective_samples.iter().take(n_pix) {
            let [r, g, b] = *s;
            let r_wb = r * eff_wb_r;
            let g_wb = g * eff_wb_g;
            let b_wb = b * eff_wb_b;
            let r_linear = (mat[0] * r_wb + mat[1] * g_wb + mat[2] * b_wb).max(0.0);
            let g_linear = (mat[3] * r_wb + mat[4] * g_wb + mat[5] * b_wb).max(0.0);
            let b_linear = (mat[6] * r_wb + mat[7] * g_wb + mat[8] * b_wb).max(0.0);
            rgb_linear.push([
                r_linear * exposure_gain,
                g_linear * exposure_gain,
                b_linear * exposure_gain,
            ]);
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

    for oy in 0..out_h {
        let orig_y = crop_y0 as f32 + (oy as f32 / out_h as f32) * cropped_h as f32;
        let raw_dy = (orig_y - cy) * flip_y;

        for ox in 0..out_w {
            let orig_x = crop_x0 as f32 + (ox as f32 / out_w as f32) * cropped_w as f32;
            let raw_dx = (orig_x - cx) * flip_x;

            let rx = raw_dx * cos_rot + raw_dy * sin_rot;
            let ry = -raw_dx * sin_rot + raw_dy * cos_rot;

            let v_scale = (1.0 + keystone_v * (ry / cy)).max(0.1);
            let kx = (rx / v_scale) / auto_zoom;
            let h_scale = (1.0 + keystone_h * (kx / cx)).max(0.1);
            let ky = (ry / h_scale) / auto_zoom;

            let sample_idx = if k_dist.abs() > 1e-5 {
                let r2 = (kx * kx + ky * ky) / max_radius_sq;
                let warp = 1.0 + k_dist * r2;
                let warped_x =
                    ((cx + kx * warp).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let warped_y =
                    ((cy + ky * warp).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                warped_y * preview_w + warped_x
            } else {
                let sx = ((cx + kx).round() as isize).clamp(0, preview_w as isize - 1) as usize;
                let sy = ((cy + ky).round() as isize).clamp(0, preview_h as isize - 1) as usize;
                sy * preview_w + sx
            };

            let [mean_r, mean_g, mean_b] = effective_samples[sample_idx];

            let v_mult = if v_gain > 1e-4 {
                let norm_r2 = (kx * kx + ky * ky) / max_radius_sq;
                1.0 + v_gain * norm_r2 + (v_gain * 0.4) * (norm_r2 * norm_r2)
            } else {
                1.0
            };

            let r_wb = mean_r * eff_wb_r;
            let g_wb = mean_g * eff_wb_g;
            let b_wb = mean_b * eff_wb_b;

            let r_linear = (mat[0] * r_wb + mat[1] * g_wb + mat[2] * b_wb).max(0.0);
            let g_linear = (mat[3] * r_wb + mat[4] * g_wb + mat[5] * b_wb).max(0.0);
            let b_linear = (mat[6] * r_wb + mat[7] * g_wb + mat[8] * b_wb).max(0.0);

            let mut r_scaled = (r_linear * exposure_gain * v_mult).max(0.0);
            let mut g_scaled = (g_linear * exposure_gain * v_mult).max(0.0);
            let mut b_scaled = (b_linear * exposure_gain * v_mult).max(0.0);

            if let Some(gains) = &smart_lighting_map {
                let sl_idx = sample_idx.min(gains.len().saturating_sub(1));
                let g = gains[sl_idx];
                r_scaled *= g;
                g_scaled *= g;
                b_scaled *= g;
            }

            let lum = 0.2126 * r_scaled + 0.7152 * g_scaled + 0.0722 * b_scaled;
            let y_norm = lum / (lum + 0.18);
            let shadow_weight = (1.0 - y_norm).powi(2);
            let highlight_weight = y_norm.powi(2);
            let shadow_shift = (grading.tone.shadows / 100.0) * 2.5 * shadow_weight;
            let highlight_shift = (grading.tone.highlights / 100.0) * 2.5 * highlight_weight;
            let blacks_weight = (1.0 - y_norm).powi(4);
            let blacks_shift = (grading.tone.blacks / 100.0) * 1.5 * blacks_weight;
            let tone_gain = 2.0_f32.powf(shadow_shift + highlight_shift + blacks_shift);

            let r_toned = (r_scaled * tone_gain).max(0.0);
            let g_toned = (g_scaled * tone_gain).max(0.0);
            let b_toned = (b_scaled * tone_gain).max(0.0);

            let (r_toned, g_toned, b_toned) = if has_curves {
                let c = rraw_color::curves::apply_tone_curves_lut(
                    glam::Vec3::new(r_toned, g_toned, b_toned),
                    &curves_lut,
                );
                (c.x, c.y, c.z)
            } else {
                (r_toned, g_toned, b_toned)
            };

            let (r_lut, g_lut, b_lut) = if let Some(clut) = &grading.custom_clut {
                let out = clut
                    .lut
                    .sample_tetrahedral(glam::Vec3::new(r_toned, g_toned, b_toned));
                let w = clut.strength.clamp(0.0, 1.0);
                (
                    r_toned * (1.0 - w) + out.x * w,
                    g_toned * (1.0 - w) + out.y * w,
                    b_toned * (1.0 - w) + out.z * w,
                )
            } else {
                (r_toned, g_toned, b_toned)
            };

            let u_norm = (orig_x / (preview_w.max(1) as f32)).clamp(0.0, 1.0);
            let v_norm = (orig_y / (preview_h.max(1) as f32)).clamp(0.0, 1.0);
            let (sem_ev, sem_contrast, sem_sat, sem_temp) =
                if !grading.semantic_masks.is_empty() || !grading.local_masks.is_empty() {
                    eval_all_mask_adjustments(
                        &base.semantic_masks,
                        &grading.semantic_masks,
                        &grading.local_masks,
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

            let (r_drt, g_drt, b_drt) = match grading.color.display_transform {
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

            let factor = (1.0 + (grading.tone.contrast / 100.0).clamp(-0.8, 2.0))
                * (1.0 + (sem_contrast / 100.0).clamp(-0.8, 2.0));
            let r_c = ((r_drt - 0.18) * factor + 0.18).max(0.0);
            let g_c = ((g_drt - 0.18) * factor + 0.18).max(0.0);
            let b_c = ((b_drt - 0.18) * factor + 0.18).max(0.0);

            let lum_c = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;
            let max_val = r_c.max(g_c).max(b_c);
            let min_val = r_c.min(g_c).min(b_c);
            let current_sat = if max_val > 1e-4 {
                (max_val - min_val) / max_val
            } else {
                0.0
            };
            let base_vibrance = 0.10_f32;
            let user_vibrance = grading.tone.vibrance / 100.0;
            let user_sat = grading.tone.saturation / 100.0;
            let vib_weight = (1.0 - current_sat).clamp(0.0, 1.0);
            let eff_sat = ((1.0 + user_sat + (sem_sat / 100.0))
                * (1.0 + (base_vibrance + user_vibrance) * vib_weight))
                .max(0.0);

            let r_s = (lum_c + (r_c - lum_c) * eff_sat).max(0.0);
            let g_s = (lum_c + (g_c - lum_c) * eff_sat).max(0.0);
            let b_s = (lum_c + (b_c - lum_c) * eff_sat).max(0.0);

            let (r_s, g_s, b_s) = if has_creative_vignette {
                let u_rel = (orig_x - cx) / cx.max(1.0);
                let v_rel = (orig_y - cy) / cy.max(1.0);
                let r_ellip = (u_rel * u_rel + v_rel * v_rel).sqrt();
                let r_circ =
                    ((orig_x - cx).powi(2) + (orig_y - cy).powi(2)).sqrt() / cx.min(cy).max(1.0);
                let r_rect = u_rel.abs().max(v_rel.abs());
                let r_dist = if cv_round > 0.0 {
                    r_ellip * (1.0 - cv_round) + r_circ * cv_round
                } else {
                    r_ellip * (1.0 + cv_round) + r_rect * (-cv_round)
                };
                let t = ((r_dist - cv_mid) / (cv_feather.max(0.05) * 1.2)).clamp(0.0, 1.0);
                let smooth_t = t * t * (3.0 - 2.0 * t);
                let cv_mult = (1.0 + cv_amt * smooth_t).max(0.0);
                (r_s * cv_mult, g_s * cv_mult, b_s * cv_mult)
            } else {
                (r_s, g_s, b_s)
            };

            let map_f32 = |x: f32| -> f32 {
                let clamped = x.clamp(0.0, 1.0);
                let toe = if clamped < 0.35 {
                    clamped.powf(1.08)
                } else {
                    clamped
                };
                toe.powf(1.0 / 2.2)
            };

            let (f_r, f_g, f_b) = if grading.color.is_monochrome {
                let lum_val = map_f32(0.2126 * r_s + 0.7152 * g_s + 0.0722 * b_s);
                (lum_val, lum_val, lum_val)
            } else {
                (map_f32(r_s), map_f32(g_s), map_f32(b_s))
            };

            u8_bytes.push((f_r * 255.0).clamp(0.0, 255.0) as u8);
            u8_bytes.push((f_g * 255.0).clamp(0.0, 255.0) as u8);
            u8_bytes.push((f_b * 255.0).clamp(0.0, 255.0) as u8);

            u16_samples.push((f_r * 65535.0).clamp(0.0, 65535.0).round() as u16);
            u16_samples.push((f_g * 65535.0).clamp(0.0, 65535.0).round() as u16);
            u16_samples.push((f_b * 65535.0).clamp(0.0, 65535.0).round() as u16);
        }
    }

    (out_w, out_h, u8_bytes, u16_samples)
}

/// Convenience helper evaluating preview and histogram directly from CFA.
#[must_use]
pub fn generate_preview_image(
    cfa: &SignedCfaBuffer,
    grading: &DarkroomGradingState,
) -> (egui::ColorImage, HistogramData) {
    let base = BaseRgbPreview::from_cfa(cfa);
    render_preview_from_base(&base, grading)
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
                    // Ratio-preserving luminance scaling: guarantees zero chromaticity or saturation shift
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
