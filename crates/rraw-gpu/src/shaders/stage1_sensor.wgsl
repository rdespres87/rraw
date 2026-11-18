struct Stage1Uniforms {
    width: u32,
    height: u32,
    cfa_type: u32,
    wavelet_level: u32,
    black_levels: vec4<f32>,
    white_levels: vec4<f32>,
    noise_alpha: vec4<f32>,
    noise_sigma2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Stage1Uniforms;
@group(0) @binding(1) var<storage, read> raw_input: array<u32>;
@group(0) @binding(2) var<storage, read_write> norm_output: array<f32>;
@group(0) @binding(3) var<storage, read_write> clip_mask_output: array<f32>;

fn get_bayer_channel(x: u32, y: u32, cfa: u32) -> u32 {
    let px = x % 2u;
    let py = y % 2u;
    // 0 = RGGB: (0,0)=R(0), (1,0)=Gr(1), (0,1)=Gb(2), (1,1)=B(3)
    if (cfa == 0u) {
        return py * 2u + px;
    } else if (cfa == 1u) { // BGGR
        return (1u - py) * 2u + (1u - px);
    } else if (cfa == 2u) { // GBRG
        return (1u - py) * 2u + px;
    } else { // GRBG
        return py * 2u + (1u - px);
    }
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;

    if (x >= uniforms.width || y >= uniforms.height) {
        return;
    }

    let idx = y * uniforms.width + x;
    let raw_val = f32(raw_input[idx]);
    let channel = get_bayer_channel(x, y, uniforms.cfa_type);

    let b_c = uniforms.black_levels[channel];
    let w_c = uniforms.white_levels[channel];
    let range = max(w_c - b_c, 1.0);

    // Defect repair: check 4 same-color neighbors (at distance 2)
    var repaired_raw = raw_val;
    if (x >= 2u && x + 2u < uniforms.width && y >= 2u && y + 2u < uniforms.height) {
        let n0 = f32(raw_input[(y - 2u) * uniforms.width + x]);
        let n1 = f32(raw_input[(y + 2u) * uniforms.width + x]);
        let n2 = f32(raw_input[y * uniforms.width + (x - 2u)]);
        let n3 = f32(raw_input[y * uniforms.width + (x + 2u)]);

        let avg_n = (n0 + n1 + n2 + n3) * 0.25;
        let max_n = max(max(n0, n1), max(n2, n3));
        let min_n = min(min(n0, n1), min(n2, n3));

        // Hot pixel outlier detection
        if (raw_val > max_n * 1.8 && raw_val > w_c * 1.2) {
            repaired_raw = avg_n;
        } else if (raw_val < min_n * 0.2 && raw_val < b_c * 0.5) {
            repaired_raw = avg_n;
        }
    }

    // Signed normalization: preserve negative values without zero-clipping DC bias
    let norm_val = (repaired_raw - b_c) / range;

    // Saturation clipping mask: 1.0 if pixel is at or above saturation threshold (0.999)
    let clip = select(0.0, 1.0, norm_val >= 0.999);

    norm_output[idx] = norm_val;
    clip_mask_output[idx] = clip;
}
