struct Stage7Uniforms {
    width: u32,
    height: u32,
    headroom: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Stage7Uniforms;
@group(0) @binding(1) var<storage, read> rgb_input: array<f32>;
@group(0) @binding(2) var<storage, read_write> rgb_output: array<f32>;
@group(0) @binding(3) var<storage, read_write> histogram: array<atomic<u32>, 256>;
@group(0) @binding(4) var<storage, read_write> clipping_alert: atomic<u32>;

// Parametric analytical AgX shoulder bounded by display headroom H
// Matches `rraw_color::apply_agx_edr` bit-for-bit.
fn agx_drt(x: f32, h: f32) -> f32 {
    if (x <= 0.0) {
        return 0.0;
    }
    let denom = x + h * ((1.0 + 0.5 * x) / (1.0 + 0.8 * x));
    return h * (x / denom);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;

    if (x >= uniforms.width || y >= uniforms.height) {
        return;
    }

    let idx = (y * uniforms.width + x) * 3u;
    let r_in = rgb_input[idx];
    let g_in = rgb_input[idx + 1u];
    let b_in = rgb_input[idx + 2u];

    let h = max(uniforms.headroom, 1.0);

    // Check clipping above available display headroom H
    if (r_in > h || g_in > h || b_in > h) {
        atomicStore(&clipping_alert, 1u);
    }

    // Apply AgX DRT tone mapping
    let r_out = agx_drt(r_in, h);
    let g_out = agx_drt(g_in, h);
    let b_out = agx_drt(b_in, h);

    rgb_output[idx] = r_out;
    rgb_output[idx + 1u] = g_out;
    rgb_output[idx + 2u] = b_out;

    // Luminance for 256-bin histogram
    let lum = 0.2126 * r_out + 0.7152 * g_out + 0.0722 * b_out;
    let normalized_lum = clamp(lum / h, 0.0, 1.0);
    let bin = clamp(u32(normalized_lum * 255.0), 0u, 255u);

    atomicAdd(&histogram[bin], 1u);
}
