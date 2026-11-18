struct Stage4Uniforms {
    width: u32,
    height: u32,
    lut_dimension: u32,
    apply_radial_vignette: u32,
    vignette_v1: f32,
    vignette_v2: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Stage4Uniforms;
@group(0) @binding(1) var<storage, read> rgb_input: array<f32>;
@group(0) @binding(2) var<storage, read> lut_r: array<f32>;
@group(0) @binding(3) var<storage, read> lut_g: array<f32>;
@group(0) @binding(4) var<storage, read> lut_b: array<f32>;
@group(0) @binding(5) var<storage, read_write> rgb_output: array<f32>;

fn sample_channel(x: i32, y: i32, channel: u32) -> f32 {
    let cx = u32(clamp(x, 0, i32(uniforms.width) - 1));
    let cy = u32(clamp(y, 0, i32(uniforms.height) - 1));
    return rgb_input[(cy * uniforms.width + cx) * 3u + channel];
}

// 1D Catmull-Rom cubic interpolation weights
fn catmull_rom_weights(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    let w0 = -0.5 * t + t2 - 0.5 * t3;
    let w1 = 1.0 - 2.5 * t2 + 1.5 * t3;
    let w2 = 0.5 * t + 2.0 * t2 - 1.5 * t3;
    let w3 = -0.5 * t2 + 0.5 * t3;
    return vec4<f32>(w0, w1, w2, w3);
}

// 2D Catmull-Rom bicubic sampling with clamp to edge
fn sample_bicubic(u: f32, v: f32, channel: u32) -> f32 {
    let px = u * f32(uniforms.width) - 0.5;
    let py = v * f32(uniforms.height) - 0.5;

    let x0 = i32(floor(px));
    let y0 = i32(floor(py));

    let fx = px - f32(x0);
    let fy = py - f32(y0);

    let wx = catmull_rom_weights(fx);
    let wy = catmull_rom_weights(fy);

    var sum: f32 = 0.0;
    for (var j = 0; j < 4; j = j + 1) {
        let sample_y = y0 - 1 + j;
        var row_sum: f32 = 0.0;
        for (var i = 0; i < 4; i = i + 1) {
            let sample_x = x0 - 1 + i;
            row_sum = row_sum + sample_channel(sample_x, sample_y, channel) * wx[i];
        }
        sum = sum + row_sum * wy[j];
    }
    return sum;
}

// Bilinear lookup into UV table
fn sample_lut(x: f32, y: f32, lut_select: u32) -> vec2<f32> {
    let dim = f32(uniforms.lut_dimension);
    let lx = clamp(x * (dim - 1.0), 0.0, dim - 1.0);
    let ly = clamp(y * (dim - 1.0), 0.0, dim - 1.0);

    let x0 = u32(floor(lx));
    let y0 = u32(floor(ly));
    let x1 = min(x0 + 1u, uniforms.lut_dimension - 1u);
    let y1 = min(y0 + 1u, uniforms.lut_dimension - 1u);

    let fx = lx - f32(x0);
    let fy = ly - f32(y0);

    let idx00 = (y0 * uniforms.lut_dimension + x0) * 2u;
    let idx10 = (y0 * uniforms.lut_dimension + x1) * 2u;
    let idx01 = (y1 * uniforms.lut_dimension + x0) * 2u;
    let idx11 = (y1 * uniforms.lut_dimension + x1) * 2u;

    var uv00: vec2<f32>;
    var uv10: vec2<f32>;
    var uv01: vec2<f32>;
    var uv11: vec2<f32>;

    if (lut_select == 0u) {
        uv00 = vec2<f32>(lut_r[idx00], lut_r[idx00 + 1u]);
        uv10 = vec2<f32>(lut_r[idx10], lut_r[idx10 + 1u]);
        uv01 = vec2<f32>(lut_r[idx01], lut_r[idx01 + 1u]);
        uv11 = vec2<f32>(lut_r[idx11], lut_r[idx11 + 1u]);
    } else if (lut_select == 1u) {
        uv00 = vec2<f32>(lut_g[idx00], lut_g[idx00 + 1u]);
        uv10 = vec2<f32>(lut_g[idx10], lut_g[idx10 + 1u]);
        uv01 = vec2<f32>(lut_g[idx01], lut_g[idx01 + 1u]);
        uv11 = vec2<f32>(lut_g[idx11], lut_g[idx11 + 1u]);
    } else {
        uv00 = vec2<f32>(lut_b[idx00], lut_b[idx00 + 1u]);
        uv10 = vec2<f32>(lut_b[idx10], lut_b[idx10 + 1u]);
        uv01 = vec2<f32>(lut_b[idx01], lut_b[idx01 + 1u]);
        uv11 = vec2<f32>(lut_b[idx11], lut_b[idx11 + 1u]);
    }

    let top = mix(uv00, uv10, fx);
    let bot = mix(uv01, uv11, fx);
    return mix(top, bot, fy);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;

    if (x >= uniforms.width || y >= uniforms.height) {
        return;
    }

    let norm_x = (f32(x) + 0.5) / f32(uniforms.width);
    let norm_y = (f32(y) + 0.5) / f32(uniforms.height);

    // Sample distorted UV coordinates per channel
    let uv_r_coord = sample_lut(norm_x, norm_y, 0u);
    let uv_g_coord = sample_lut(norm_x, norm_y, 1u);
    let uv_b_coord = sample_lut(norm_x, norm_y, 2u);

    // Bicubic Catmull-Rom sampling
    var val_r = sample_bicubic(uv_r_coord.x, uv_r_coord.y, 0u);
    var val_g = sample_bicubic(uv_g_coord.x, uv_g_coord.y, 1u);
    var val_b = sample_bicubic(uv_b_coord.x, uv_b_coord.y, 2u);

    // Critical Invariant: positive half-space clamping (zero negative values from overshoot lobes)
    val_r = max(val_r, 0.0);
    val_g = max(val_g, 0.0);
    val_b = max(val_b, 0.0);

    // Double-vignetting prevention: apply radial vignette ONLY if not handled by GainMap
    if (uniforms.apply_radial_vignette == 1u) {
        let u_cent = norm_x - 0.5;
        let v_cent = norm_y - 0.5;
        let r2 = u_cent * u_cent + v_cent * v_cent;
        let r4 = r2 * r2;
        let vig = max(1.0 + uniforms.vignette_v1 * r2 + uniforms.vignette_v2 * r4, 0.0);
        val_r = val_r * vig;
        val_g = val_g * vig;
        val_b = val_b * vig;
    }

    let out_idx = (y * uniforms.width + x) * 3u;
    rgb_output[out_idx] = val_r;
    rgb_output[out_idx + 1u] = val_g;
    rgb_output[out_idx + 2u] = val_b;
}
