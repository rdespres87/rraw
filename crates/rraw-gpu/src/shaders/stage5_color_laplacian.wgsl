struct Stage5Uniforms {
    width: u32,
    height: u32,
    mipmap_level: u32,
    effective_octaves: u32,
    local_contrast_strength: f32,
    enable_tone_curves: u32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Stage5Uniforms;
@group(0) @binding(1) var<storage, read> rgb_input: array<f32>;
@group(0) @binding(2) var<storage, read_write> rgb_output: array<f32>;
@group(0) @binding(3) var<storage, read> tone_curves_lut: array<vec4<f32>>;

fn sample_pixel(x: i32, y: i32) -> vec3<f32> {
    let cx = u32(clamp(x, 0, i32(uniforms.width) - 1));
    let cy = u32(clamp(y, 0, i32(uniforms.height) - 1));
    let idx = (cy * uniforms.width + cx) * 3u;
    return vec3<f32>(rgb_input[idx], rgb_input[idx + 1u], rgb_input[idx + 2u]);
}

fn sample_lut_channel(val: f32, ch: u32) -> f32 {
    let x = clamp(val, 0.0, 1.0) * 1023.0;
    let idx = min(u32(floor(x)), 1022u);
    let frac = x - f32(idx);
    let entry0 = tone_curves_lut[idx];
    let entry1 = tone_curves_lut[idx + 1u];
    var v0: f32;
    var v1: f32;
    if (ch == 0u) {
        v0 = entry0.r;
        v1 = entry1.r;
    } else if (ch == 1u) {
        v0 = entry0.g;
        v1 = entry1.g;
    } else if (ch == 2u) {
        v0 = entry0.b;
        v1 = entry1.b;
    } else {
        v0 = entry0.a;
        v1 = entry1.a;
    }
    return v0 * (1.0 - frac) + v1 * frac;
}

fn apply_tone_curves(rgb: vec3<f32>) -> vec3<f32> {
    let r = sample_lut_channel(rgb.r, 0u);
    let g = sample_lut_channel(rgb.g, 1u);
    let b = sample_lut_channel(rgb.b, 2u);

    // AP1 ACEScg Rec.2020/AP1 luma coefficients
    let cur_luma = 0.2722287 * r + 0.6740818 * g + 0.0536895 * b;
    let target_luma = sample_lut_channel(cur_luma, 3u);
    var luma_scale = 1.0;
    if (cur_luma > 1e-6) {
        luma_scale = clamp(target_luma / cur_luma, 0.0, 4.0);
    }
    return vec3<f32>(r * luma_scale, g * luma_scale, b * luma_scale);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;

    if (x >= uniforms.width || y >= uniforms.height) {
        return;
    }

    let ix = i32(x);
    let iy = i32(y);

    var current_rgb = sample_pixel(ix, iy);

    // Multi-scale Laplacian pyramid local contrast with effective octaves = N_octaves - L
    for (var k = 0u; k < uniforms.effective_octaves; k = k + 1u) {
        let step = i32(1u << k);

        // 5-tap separable-like cross blur
        let c0 = sample_pixel(ix - step, iy);
        let c1 = sample_pixel(ix + step, iy);
        let c2 = sample_pixel(ix, iy - step);
        let c3 = sample_pixel(ix, iy + step);

        let blurred = (c0 + c1 + c2 + c3 + 4.0 * current_rgb) * 0.125;
        let detail = current_rgb - blurred;

        // Compression / contrast enhancement
        let scale = uniforms.local_contrast_strength / (1.0 + f32(k) * 0.5);
        current_rgb = current_rgb + detail * scale * 0.1;
    }

    // Gamut Guard: strictly non-negative inside AP1 core
    current_rgb = max(current_rgb, vec3<f32>(0.0));

    // Stage 5b: PCHIP Tone Curves evaluation in VRAM
    if (uniforms.enable_tone_curves != 0u) {
        current_rgb = apply_tone_curves(current_rgb);
        current_rgb = max(current_rgb, vec3<f32>(0.0));
    }

    let out_idx = (y * uniforms.width + x) * 3u;
    rgb_output[out_idx] = current_rgb.r;
    rgb_output[out_idx + 1u] = current_rgb.g;
    rgb_output[out_idx + 2u] = current_rgb.b;
}
