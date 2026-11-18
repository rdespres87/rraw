// Stage 6: Post-Warp [0, 1]² Local Geometric & ACEScg Parametric Masks WGSL Compute Shader

struct Uniforms {
    width: u32,
    height: u32,
    mask_count: u32,
    _pad: u32,
};

struct GpuMaskData {
    mask_type: u32,          // 0 = Disabled, 1 = Linear, 2 = Radial, 3 = Parametric
    exposure_offset_ev: f32, // EV offset
    opacity: f32,            // [0.0, 1.0]
    _pad0: f32,
    param_vec0: vec4<f32>,   // Linear: [start.x, start.y, end.x, end.y], Radial: [cx, cy, rx, ry], Param: [lum_min, lum_max, lum_feather, has_lum]
    param_vec1: vec4<f32>,   // Radial: [rot_rad, 0, 0, 0], Param: [chroma_min, chroma_max, chroma_feather, has_chroma]
    param_vec2: vec4<f32>,   // Param: [hue_min, hue_max, hue_feather, has_hue]
};

@group(0) @binding(0) var<uniform> params: Uniforms;
@group(0) @binding(1) var<storage, read> masks: array<GpuMaskData>;
@group(0) @binding(2) var<storage, read> rgb_in: array<f32>;
@group(0) @binding(3) var<storage, read_write> rgb_out: array<f32>;

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

    for (var i: u32 = 0u; i < params.mask_count; i = i + 1u) {
        let mask = masks[i];
        if (mask.mask_type == 0u) {
            continue;
        }

        var weight: f32 = 0.0;

        if (mask.mask_type == 1u) {
            // Linear directional gradient
            let start = mask.param_vec0.xy;
            let end = mask.param_vec0.zw;
            let dir = end - start;
            let len_sq = max(dot(dir, dir), 1e-12);
            let t = clamp(dot(pos - start, dir) / len_sq, 0.0, 1.0);
            weight = t;
        } else if (mask.mask_type == 2u) {
            // Radial elliptical gradient with rotation
            let center = mask.param_vec0.xy;
            let radius = max(mask.param_vec0.zw, vec2<f32>(1e-6, 1e-6));
            let rot = mask.param_vec1.x;

            let delta = pos - center;
            let cos_r = cos(rot);
            let sin_r = sin(rot);

            let rx = (delta.x * cos_r + delta.y * sin_r) / radius.x;
            let ry = (-delta.x * sin_r + delta.y * cos_r) / radius.y;

            let dist = sqrt(rx * rx + ry * ry);
            let falloff = clamp(1.0 - dist, 0.0, 1.0);
            weight = falloff;
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
        }

        total_ev = total_ev + (weight * mask.opacity * mask.exposure_offset_ev);
    }

    let gain = exp2(total_ev);

    rgb_out[px_idx] = max(r_in * gain, 0.0);
    rgb_out[px_idx + 1u] = max(g_in * gain, 0.0);
    rgb_out[px_idx + 2u] = max(b_in * gain, 0.0);
}
