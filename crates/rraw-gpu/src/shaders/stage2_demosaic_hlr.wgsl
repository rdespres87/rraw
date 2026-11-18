struct Stage2Uniforms {
    width: u32,
    height: u32,
    cfa_type: u32,
    _pad: u32,
    wb_multipliers: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Stage2Uniforms;
@group(0) @binding(1) var<storage, read> cfa_input: array<f32>;
@group(0) @binding(2) var<storage, read> clip_mask_input: array<f32>;
@group(0) @binding(3) var<storage, read_write> rgb_output: array<f32>;

fn sample_cfa(x: i32, y: i32) -> f32 {
    let cx = u32(clamp(x, 0, i32(uniforms.width) - 1));
    let cy = u32(clamp(y, 0, i32(uniforms.height) - 1));
    return cfa_input[cy * uniforms.width + cx];
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

    let px = x % 2u;
    let py = y % 2u;

    var raw_r: f32 = 0.0;
    var raw_g: f32 = 0.0;
    var raw_b: f32 = 0.0;

    // RGGB Bayer pattern with Hamilton-Adams directional edge-adaptive interpolation
    if (py == 0u && px == 0u) {
        // Red pixel (R known, interpolate G then B)
        raw_r = sample_cfa(ix, iy);

        // Directional Green interpolation
        let g_left = sample_cfa(ix - 1, iy);
        let g_right = sample_cfa(ix + 1, iy);
        let g_up = sample_cfa(ix, iy - 1);
        let g_down = sample_cfa(ix, iy + 1);

        let r_left2 = sample_cfa(ix - 2, iy);
        let r_right2 = sample_cfa(ix + 2, iy);
        let r_up2 = sample_cfa(ix, iy - 2);
        let r_down2 = sample_cfa(ix, iy + 2);

        let d_h = abs(g_left - g_right) + abs(2.0 * raw_r - r_left2 - r_right2);
        let d_v = abs(g_up - g_down) + abs(2.0 * raw_r - r_up2 - r_down2);

        let g_h = (g_left + g_right) * 0.5 + (2.0 * raw_r - r_left2 - r_right2) * 0.25;
        let g_v = (g_up + g_down) * 0.5 + (2.0 * raw_r - r_up2 - r_down2) * 0.25;

        if (d_h < d_v * 0.95) {
            raw_g = g_h;
        } else if (d_v < d_h * 0.95) {
            raw_g = g_v;
        } else {
            raw_g = (g_h + g_v) * 0.5;
        }

        // Blue interpolation along diagonals guided by green difference
        let b_ul = sample_cfa(ix - 1, iy - 1);
        let b_ur = sample_cfa(ix + 1, iy - 1);
        let b_ll = sample_cfa(ix - 1, iy + 1);
        let b_lr = sample_cfa(ix + 1, iy + 1);

        let g_ul = (sample_cfa(ix - 1, iy) + sample_cfa(ix, iy - 1)) * 0.5;
        let g_ur = (sample_cfa(ix + 1, iy) + sample_cfa(ix, iy - 1)) * 0.5;
        let g_ll = (sample_cfa(ix - 1, iy) + sample_cfa(ix, iy + 1)) * 0.5;
        let g_lr = (sample_cfa(ix + 1, iy) + sample_cfa(ix, iy + 1)) * 0.5;

        let d_d1 = abs(b_ul - b_lr) + abs(2.0 * raw_g - g_ul - g_lr);
        let d_d2 = abs(b_ur - b_ll) + abs(2.0 * raw_g - g_ur - g_ll);

        let b_d1 = (b_ul + b_lr) * 0.5 + (2.0 * raw_g - g_ul - g_lr) * 0.25;
        let b_d2 = (b_ur + b_ll) * 0.5 + (2.0 * raw_g - g_ur - g_ll) * 0.25;

        if (d_d1 < d_d2 * 0.95) {
            raw_b = b_d1;
        } else if (d_d2 < d_d1 * 0.95) {
            raw_b = b_d2;
        } else {
            raw_b = (b_d1 + b_d2) * 0.5;
        }

    } else if (py == 0u && px == 1u) {
        // Green (Gr) pixel on Red row (G known, horizontal R, vertical B)
        raw_g = sample_cfa(ix, iy);

        let r_left = sample_cfa(ix - 1, iy);
        let r_right = sample_cfa(ix + 1, iy);
        let g_left2 = sample_cfa(ix - 2, iy);
        let g_right2 = sample_cfa(ix + 2, iy);
        raw_r = (r_left + r_right) * 0.5 + (2.0 * raw_g - g_left2 - g_right2) * 0.25;

        let b_up = sample_cfa(ix, iy - 1);
        let b_down = sample_cfa(ix, iy + 1);
        let g_up2 = sample_cfa(ix, iy - 2);
        let g_down2 = sample_cfa(ix, iy + 2);
        raw_b = (b_up + b_down) * 0.5 + (2.0 * raw_g - g_up2 - g_down2) * 0.25;

    } else if (py == 1u && px == 0u) {
        // Green (Gb) pixel on Blue row (G known, vertical R, horizontal B)
        raw_g = sample_cfa(ix, iy);

        let r_up = sample_cfa(ix, iy - 1);
        let r_down = sample_cfa(ix, iy + 1);
        let g_up2 = sample_cfa(ix, iy - 2);
        let g_down2 = sample_cfa(ix, iy + 2);
        raw_r = (r_up + r_down) * 0.5 + (2.0 * raw_g - g_up2 - g_down2) * 0.25;

        let b_left = sample_cfa(ix - 1, iy);
        let b_right = sample_cfa(ix + 1, iy);
        let g_left2 = sample_cfa(ix - 2, iy);
        let g_right2 = sample_cfa(ix + 2, iy);
        raw_b = (b_left + b_right) * 0.5 + (2.0 * raw_g - g_left2 - g_right2) * 0.25;

    } else {
        // Blue pixel (B known, interpolate G then R)
        raw_b = sample_cfa(ix, iy);

        // Directional Green interpolation
        let g_left = sample_cfa(ix - 1, iy);
        let g_right = sample_cfa(ix + 1, iy);
        let g_up = sample_cfa(ix, iy - 1);
        let g_down = sample_cfa(ix, iy + 1);

        let b_left2 = sample_cfa(ix - 2, iy);
        let b_right2 = sample_cfa(ix + 2, iy);
        let b_up2 = sample_cfa(ix, iy - 2);
        let b_down2 = sample_cfa(ix, iy + 2);

        let d_h = abs(g_left - g_right) + abs(2.0 * raw_b - b_left2 - b_right2);
        let d_v = abs(g_up - g_down) + abs(2.0 * raw_b - b_up2 - b_down2);

        let g_h = (g_left + g_right) * 0.5 + (2.0 * raw_b - b_left2 - b_right2) * 0.25;
        let g_v = (g_up + g_down) * 0.5 + (2.0 * raw_b - b_up2 - b_down2) * 0.25;

        if (d_h < d_v * 0.95) {
            raw_g = g_h;
        } else if (d_v < d_h * 0.95) {
            raw_g = g_v;
        } else {
            raw_g = (g_h + g_v) * 0.5;
        }

        // Red interpolation along diagonals guided by green difference
        let r_ul = sample_cfa(ix - 1, iy - 1);
        let r_ur = sample_cfa(ix + 1, iy - 1);
        let r_ll = sample_cfa(ix - 1, iy + 1);
        let r_lr = sample_cfa(ix + 1, iy + 1);

        let g_ul = (sample_cfa(ix - 1, iy) + sample_cfa(ix, iy - 1)) * 0.5;
        let g_ur = (sample_cfa(ix + 1, iy) + sample_cfa(ix, iy - 1)) * 0.5;
        let g_ll = (sample_cfa(ix - 1, iy) + sample_cfa(ix, iy + 1)) * 0.5;
        let g_lr = (sample_cfa(ix + 1, iy) + sample_cfa(ix, iy + 1)) * 0.5;

        let d_d1 = abs(r_ul - r_lr) + abs(2.0 * raw_g - g_ul - g_lr);
        let d_d2 = abs(r_ur - r_ll) + abs(2.0 * raw_g - g_ur - g_ll);

        let r_d1 = (r_ul + r_lr) * 0.5 + (2.0 * raw_g - g_ul - g_lr) * 0.25;
        let r_d2 = (r_ur + r_ll) * 0.5 + (2.0 * raw_g - g_ur - g_ll) * 0.25;

        if (d_d1 < d_d2 * 0.95) {
            raw_r = r_d1;
        } else if (d_d2 < d_d1 * 0.95) {
            raw_r = r_d2;
        } else {
            raw_r = (r_d1 + r_d2) * 0.5;
        }
    }

    // 1. Unclamped White Balance: sat_c = w_c * 1.0
    let sat_r = uniforms.wb_multipliers.x;
    let sat_g = uniforms.wb_multipliers.y;
    let sat_b = uniforms.wb_multipliers.z;

    var r = raw_r * sat_r;
    var g = raw_g * sat_g;
    var b = raw_b * sat_b;

    // 2. Highlight Reconstruction (HLR)
    let thresh = 0.99;
    let r_clipped = r >= thresh * sat_r;
    let g_clipped = g >= thresh * sat_g;
    let b_clipped = b >= thresh * sat_b;

    if (r_clipped || g_clipped || b_clipped) {
        if (!g_clipped) {
            // Green is valid: guide R and B
            if (r_clipped) {
                r = max(r, g);
            }
            if (b_clipped) {
                b = max(b, g);
            }
        } else if (!r_clipped && !b_clipped) {
            // R and B valid, G clipped: average R and B
            g = (r + b) * 0.5;
        } else {
            // Multiple or all channels clipped: equalize to max to prevent magenta cast
            let max_val = max(r, max(g, b));
            r = max_val;
            g = max_val;
            b = max_val;
        }
    }

    let out_idx = (y * uniforms.width + x) * 3u;
    rgb_output[out_idx] = r;
    rgb_output[out_idx + 1u] = g;
    rgb_output[out_idx + 2u] = b;
}
