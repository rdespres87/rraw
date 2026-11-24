// Stage 6: Post-Warp [0, 1]² Local Geometric, ACEScg Parametric & CoreML AI Semantic Masks WGSL Compute Shader

struct Uniforms {
    width: u32,
    height: u32,
    mask_count: u32,
    sem_width: u32,
    sem_height: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

struct GpuMaskData {
    mask_type: u32,          // 0 = Disabled, 1 = Linear, 2 = Radial, 3 = Parametric, 4 = SemanticAi
    exposure_offset_ev: f32, // EV offset
    opacity: f32,            // [0.0, 1.0]
    temperature_shift: f32,  // Local color temperature shift [-100, 100]
    param_vec0: vec4<f32>,   // Linear: [start.x, start.y, end.x, end.y], Radial: [cx, cy, rx, ry], Param: [lum_min, lum_max, lum_feather, has_lum], SemanticAi: [cat_idx, invert, feather, 0]
    param_vec1: vec4<f32>,   // Linear: [0, invert, feather, 0], Radial: [rot_rad, invert, feather, 0], Param: [chroma_min, chroma_max, chroma_feather, has_chroma]
    param_vec2: vec4<f32>,   // Param: [hue_min, hue_max, hue_feather, has_hue]
};

@group(0) @binding(0) var<uniform> params: Uniforms;
@group(0) @binding(1) var<storage, read> masks: array<GpuMaskData>;
@group(0) @binding(2) var<storage, read> rgb_in: array<f32>;
@group(0) @binding(3) var<storage, read_write> rgb_out: array<f32>;
@group(0) @binding(4) var<storage, read> semantic_rgba: array<vec4<f32>>;

fn extract_channel(v: vec4<f32>, c: u32) -> f32 {
    if (c == 0u) {
        return v.x;
    } else if (c == 1u) {
        return v.y;
    } else if (c == 2u) {
        return v.z;
    } else {
        return v.w;
    }
}

fn sample_semantic_bilinear(pos: vec2<f32>, cat_idx: u32, invert: bool, feather: f32) -> f32 {
    if (params.sem_width == 0u || params.sem_height == 0u) {
        return 0.0;
    }
    let sw = f32(params.sem_width);
    let sh = f32(params.sem_height);
    let fx = max(clamp(pos.x, 0.0, 1.0) * (sw - 1.0), 0.0);
    let fy = max(clamp(pos.y, 0.0, 1.0) * (sh - 1.0), 0.0);

    let x0 = min(u32(floor(fx)), params.sem_width - 1u);
    let y0 = min(u32(floor(fy)), params.sem_height - 1u);
    let x1 = min(x0 + 1u, params.sem_width - 1u);
    let y1 = min(y0 + 1u, params.sem_height - 1u);

    let tx = fx - f32(x0);
    let ty = fy - f32(y0);

    let p00 = extract_channel(semantic_rgba[y0 * params.sem_width + x0], cat_idx);
    let p10 = extract_channel(semantic_rgba[y0 * params.sem_width + x1], cat_idx);
    let p01 = extract_channel(semantic_rgba[y1 * params.sem_width + x0], cat_idx);
    let p11 = extract_channel(semantic_rgba[y1 * params.sem_width + x1], cat_idx);

    let top = mix(p00, p10, tx);
    let bot = mix(p01, p11, tx);
    let raw_prob = clamp(mix(top, bot, ty), 0.0, 1.0);

    let half_w = clamp(feather, 0.02, 0.5);
    let shaped = smoothstep(0.5 - half_w, 0.5 + half_w, raw_prob);
    if (invert) {
        return clamp(1.0 - shaped, 0.0, 1.0);
    } else {
        return clamp(shaped, 0.0, 1.0);
    }
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    if (x >= params.width || y >= params.height) {
        return;
    }

    let px_idx = (y * params.width + x) * 3u;
    let r_in = rgb_in[px_idx];
    let g_in = rgb_in[px_idx + 1u];
    let b_in = rgb_in[px_idx + 2u];

    let u = (f32(x) + 0.5) / f32(params.width);
    let v = (f32(y) + 0.5) / f32(params.height);
    let pos = vec2<f32>(u, v);

    // ACEScg luminance (AP1 coefficients)
    let y_aces = 0.2722287 * r_in + 0.6740818 * g_in + 0.0536895 * b_in;

    var total_ev: f32 = 0.0;
    var total_temp: f32 = 0.0;

    for (var i: u32 = 0u; i < params.mask_count; i = i + 1u) {
        let mask = masks[i];
        if (mask.mask_type == 0u) {
            continue;
        }

        var weight: f32 = 0.0;

        if (mask.mask_type == 1u) {
            // Linear directional gradient with optional smoothstep feather and inversion
            let start = mask.param_vec0.xy;
            let end = mask.param_vec0.zw;
            let dir = end - start;
            let len_sq = max(dot(dir, dir), 1e-12);
            let t = clamp(dot(pos - start, dir) / len_sq, 0.0, 1.0);
            let f = mask.param_vec1.z;
            if (f > 0.0 && f <= 0.95) {
                weight = t * t * (3.0 - 2.0 * t);
            } else {
                weight = t;
            }
            if (mask.param_vec1.y > 0.5) {
                weight = clamp(1.0 - weight, 0.0, 1.0);
            }
        } else if (mask.mask_type == 2u) {
            // Radial elliptical gradient with rotation, optional smoothstep feather, and inversion
            let center = mask.param_vec0.xy;
            let radius = max(mask.param_vec0.zw, vec2<f32>(1e-6, 1e-6));
            let rot = mask.param_vec1.x;

            let delta = pos - center;
            let cos_r = cos(rot);
            let sin_r = sin(rot);

            let rx = (delta.x * cos_r + delta.y * sin_r) / radius.x;
            let ry = (-delta.x * sin_r + delta.y * cos_r) / radius.y;

            let dist = sqrt(rx * rx + ry * ry);
            let f = mask.param_vec1.z;
            if (f > 0.0) {
                let fc = clamp(f, 0.05, 1.0);
                let inner = max(1.0 - fc, 0.0);
                if (dist <= inner) {
                    weight = 1.0;
                } else if (dist >= 1.0) {
                    weight = 0.0;
                } else {
                    let t = (dist - inner) / max(1.0 - inner, 1e-4);
                    let s = t * t * (3.0 - 2.0 * t);
                    weight = 1.0 - s;
                }
            } else {
                weight = clamp(1.0 - dist, 0.0, 1.0);
            }
            if (mask.param_vec1.y > 0.5) {
                weight = clamp(1.0 - weight, 0.0, 1.0);
            }
        } else if (mask.mask_type == 3u) {
            // Parametric ACEScg qualifier
            var p_weight: f32 = 1.0;

            // Luminance qualification
            if (mask.param_vec0.w > 0.5) {
                let min_val = mask.param_vec0.x;
                let max_val = mask.param_vec0.y;
                let feather = max(mask.param_vec0.z, 1e-6);

                let w_low = clamp((y_aces - (min_val - feather)) / feather, 0.0, 1.0);
                let w_high = clamp(((max_val + feather) - y_aces) / feather, 0.0, 1.0);
                p_weight = p_weight * w_low * w_high;
            }

            weight = p_weight;
        } else if (mask.mask_type == 4u) {
            // CoreML AI semantic segmentation mask
            let cat_idx = u32(round(mask.param_vec0.x));
            let invert = mask.param_vec0.y > 0.5;
            let feather = mask.param_vec0.z;
            weight = sample_semantic_bilinear(pos, cat_idx, invert, feather);
        }

        let w_eff = weight * mask.opacity;
        total_ev = total_ev + (w_eff * mask.exposure_offset_ev);
        total_temp = total_temp + (w_eff * mask.temperature_shift);
    }

    let gain = exp2(total_ev);
    let r_t = max(1.0 + total_temp * 0.004, 0.1);
    let b_t = max(1.0 - total_temp * 0.004, 0.1);

    rgb_out[px_idx] = max(r_in * gain * r_t, 0.0);
    rgb_out[px_idx + 1u] = max(g_in * gain, 0.0);
    rgb_out[px_idx + 2u] = max(b_in * gain * b_t, 0.0);
}
