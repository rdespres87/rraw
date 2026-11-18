// Stage 3: Regularized Richardson-Lucy Capture Sharpening WGSL Compute Shader

struct Uniforms {
    width: u32,
    height: u32,
    iterations: u32,
    base_psf_sigma: f32,
    epsilon: f32,
    deringing_threshold: f32,
    shot_r: f32,
    shot_g: f32,
    shot_b: f32,
    read_r: f32,
    read_g: f32,
    read_b: f32,
};

@group(0) @binding(0) var<uniform> params: Uniforms;
@group(0) @binding(1) var<storage, read> i_0: array<f32>;
@group(0) @binding(2) var<storage, read_write> current: array<f32>;
@group(0) @binding(3) var<storage, read_write> ratio_buf: array<f32>;
@group(0) @binding(4) var<storage, read_write> bounds_buf: array<f32>;
@group(0) @binding(5) var<storage, read_write> m_edge_buf: array<f32>;

fn clamp_coord(c: i32, max_val: u32) -> u32 {
    return u32(clamp(c, 0, i32(max_val) - 1));
}

fn get_idx(x: u32, y: u32, c: u32) -> u32 {
    return (y * params.width + x) * 3u + c;
}

fn get_psf_sigma(x: u32, y: u32) -> f32 {
    let cx = (f32(params.width) - 1.0) * 0.5;
    let cy = (f32(params.height) - 1.0) * 0.5;
    let dx = f32(x) - cx;
    let dy = f32(y) - cy;
    let max_r_sq = max(cx * cx + cy * cy, 1.0);
    let r_sq = min((dx * dx + dy * dy) / max_r_sq, 1.0);
    return params.base_psf_sigma * (1.0 + 0.5 * r_sq);
}

@compute @workgroup_size(16, 16, 1)
fn init_bounds_mask(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    if (x >= params.width || y >= params.height) {
        return;
    }

    let px_idx = y * params.width + x;

    // 1. Local min/max bounds (deringing envelope)
    for (var c: u32 = 0u; c < 3u; c = c + 1u) {
        var min_val: f32 = 1e30;
        var max_val: f32 = -1e30;
        for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
            for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
                let nx = clamp_coord(i32(x) + dx, params.width);
                let ny = clamp_coord(i32(y) + dy, params.height);
                let val = i_0[get_idx(nx, ny, c)];
                min_val = min(min_val, val);
                max_val = max(max_val, val);
            }
        }
        bounds_buf[px_idx * 6u + c * 2u] = min_val;
        bounds_buf[px_idx * 6u + c * 2u + 1u] = max_val;
    }

    // 2. Sobel gradient and noise regularizer threshold
    let shots = vec3<f32>(params.shot_r, params.shot_g, params.shot_b);
    let reads = vec3<f32>(params.read_r, params.read_g, params.read_b);

    for (var c: u32 = 0u; c < 3u; c = c + 1u) {
        var gx: f32 = 0.0;
        var gy: f32 = 0.0;

        for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
            for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
                let nx = clamp_coord(i32(x) + dx, params.width);
                let ny = clamp_coord(i32(y) + dy, params.height);
                let v = i_0[get_idx(nx, ny, c)];

                var kx: f32 = 0.0;
                var ky: f32 = 0.0;

                if (dx == -1 && dy == 0) { kx = -2.0; }
                else if (dx == 1 && dy == 0) { kx = 2.0; }
                else if (dx == -1 && (dy == -1 || dy == 1)) { kx = -1.0; }
                else if (dx == 1 && (dy == -1 || dy == 1)) { kx = 1.0; }

                if (dy == -1 && dx == 0) { ky = -2.0; }
                else if (dy == 1 && dx == 0) { ky = 2.0; }
                else if (dy == -1 && (dx == -1 || dx == 1)) { ky = -1.0; }
                else if (dy == 1 && (dx == -1 || dx == 1)) { ky = 1.0; }

                gx = gx + v * kx;
                gy = gy + v * ky;
            }
        }
        gx = gx / 8.0;
        gy = gy / 8.0;
        let grad_mag = sqrt(gx * gx + gy * gy);

        let val = i_0[px_idx * 3u + c];
        let var_noise = max(shots[c] * val + reads[c], 1e-12);
        let sigma_noise = sqrt(var_noise);
        let tau = 2.5 * sigma_noise;

        var mask: f32 = 0.0;
        if (grad_mag <= tau) {
            mask = 0.0;
        } else if (grad_mag >= 2.0 * tau) {
            mask = 1.0;
        } else {
            let norm = (grad_mag - tau) / (tau + 1e-12);
            mask = norm * norm * (3.0 - 2.0 * norm);
        }
        m_edge_buf[px_idx * 3u + c] = mask;
    }
}

@compute @workgroup_size(16, 16, 1)
fn forward_convolve_and_ratio(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    if (x >= params.width || y >= params.height) {
        return;
    }

    let sigma = max(get_psf_sigma(x, y), 0.1);
    let two_sigma_sq = 2.0 * sigma * sigma;

    var weight_sum: f32 = 0.0;
    var accum_r: f32 = 0.0;
    var accum_g: f32 = 0.0;
    var accum_b: f32 = 0.0;

    for (var dy: i32 = -2; dy <= 2; dy = dy + 1) {
        for (var dx: i32 = -2; dx <= 2; dx = dx + 1) {
            let nx = clamp_coord(i32(x) + dx, params.width);
            let ny = clamp_coord(i32(y) + dy, params.height);
            let d_sq = f32(dx * dx + dy * dy);
            let w = exp(-d_sq / two_sigma_sq);
            weight_sum = weight_sum + w;

            let s_idx = (ny * params.width + nx) * 3u;
            accum_r = accum_r + current[s_idx] * w;
            accum_g = accum_g + current[s_idx + 1u] * w;
            accum_b = accum_b + current[s_idx + 2u] * w;
        }
    }

    let inv_sum = 1.0 / max(weight_sum, 1e-12);
    let e_r = accum_r * inv_sum;
    let e_g = accum_g * inv_sum;
    let e_b = accum_b * inv_sum;

    let d_idx = (y * params.width + x) * 3u;
    ratio_buf[d_idx] = i_0[d_idx] / (e_r + params.epsilon);
    ratio_buf[d_idx + 1u] = i_0[d_idx + 1u] / (e_g + params.epsilon);
    ratio_buf[d_idx + 2u] = i_0[d_idx + 2u] / (e_b + params.epsilon);
}

@compute @workgroup_size(16, 16, 1)
fn backward_convolve_and_update(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    if (x >= params.width || y >= params.height) {
        return;
    }

    let px_idx = y * params.width + x;
    let sigma = max(get_psf_sigma(x, y), 0.1);
    let two_sigma_sq = 2.0 * sigma * sigma;

    var weight_sum: f32 = 0.0;
    var accum_r: f32 = 0.0;
    var accum_g: f32 = 0.0;
    var accum_b: f32 = 0.0;

    for (var dy: i32 = -2; dy <= 2; dy = dy + 1) {
        for (var dx: i32 = -2; dx <= 2; dx = dx + 1) {
            let nx = clamp_coord(i32(x) + dx, params.width);
            let ny = clamp_coord(i32(y) + dy, params.height);
            let d_sq = f32(dx * dx + dy * dy);
            let w = exp(-d_sq / two_sigma_sq);
            weight_sum = weight_sum + w;

            let s_idx = (ny * params.width + nx) * 3u;
            accum_r = accum_r + ratio_buf[s_idx] * w;
            accum_g = accum_g + ratio_buf[s_idx + 1u] * w;
            accum_b = accum_b + ratio_buf[s_idx + 2u] * w;
        }
    }

    let inv_sum = 1.0 / max(weight_sum, 1e-12);
    let corr_r = accum_r * inv_sum;
    let corr_g = accum_g * inv_sum;
    let corr_b = accum_b * inv_sum;

    let d_idx = px_idx * 3u;
    let mask_r = m_edge_buf[d_idx];
    let mask_g = m_edge_buf[d_idx + 1u];
    let mask_b = m_edge_buf[d_idx + 2u];

    let b_r = 1.0 + mask_r * (corr_r - 1.0);
    let b_g = 1.0 + mask_g * (corr_g - 1.0);
    let b_b = 1.0 + mask_b * (corr_b - 1.0);

    let u_r = current[d_idx] * b_r;
    let u_g = current[d_idx + 1u] * b_g;
    let u_b = current[d_idx + 2u] * b_b;

    let min_r = bounds_buf[px_idx * 6u] * (1.0 - params.deringing_threshold);
    let max_r = bounds_buf[px_idx * 6u + 1u] * (1.0 + params.deringing_threshold);
    let min_g = bounds_buf[px_idx * 6u + 2u] * (1.0 - params.deringing_threshold);
    let max_g = bounds_buf[px_idx * 6u + 3u] * (1.0 + params.deringing_threshold);
    let min_b = bounds_buf[px_idx * 6u + 4u] * (1.0 - params.deringing_threshold);
    let max_b = bounds_buf[px_idx * 6u + 5u] * (1.0 + params.deringing_threshold);

    current[d_idx] = max(clamp(u_r, max(min_r, 0.0), max_r), 0.0);
    current[d_idx + 1u] = max(clamp(u_g, max(min_g, 0.0), max_g), 0.0);
    current[d_idx + 2u] = max(clamp(u_b, max(min_b, 0.0), max_b), 0.0);
}
