struct Stage5Uniforms {
    width: u32,
    height: u32,
    mipmap_level: u32,
    effective_octaves: u32,
    local_contrast_strength: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Stage5Uniforms;
@group(0) @binding(1) var<storage, read> rgb_input: array<f32>;
@group(0) @binding(2) var<storage, read_write> rgb_output: array<f32>;

fn sample_pixel(x: i32, y: i32) -> vec3<f32> {
    let cx = u32(clamp(x, 0, i32(uniforms.width) - 1));
    let cy = u32(clamp(y, 0, i32(uniforms.height) - 1));
    let idx = (cy * uniforms.width + cx) * 3u;
    return vec3<f32>(rgb_input[idx], rgb_input[idx + 1u], rgb_input[idx + 2u]);
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

    let out_idx = (y * uniforms.width + x) * 3u;
    rgb_output[out_idx] = current_rgb.r;
    rgb_output[out_idx + 1u] = current_rgb.g;
    rgb_output[out_idx + 2u] = current_rgb.b;
}
