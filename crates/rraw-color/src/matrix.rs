//! Dual-illuminant DNG spectral colorimetry matrices into `ACEScg` (`AP1` D60).

use glam::{Mat3, Vec3};

use crate::ColorError;

/// Matrix converting CIE XYZ D60 to ACEScg (AP1 primaries with ACES D60 white point).
/// SMPTE ST 2065-1 standard matrix (row-major).
pub const MAT_XYZ_D60_TO_ACESCG: Mat3 = Mat3::from_cols(
    Vec3::new(1.641_023_4, -0.663_662_9, 0.011_721_8),
    Vec3::new(-0.324_803_3, 1.615_332_1, -0.008_284_4),
    Vec3::new(-0.236_424_7, 0.016_756_3, 0.988_394_9),
);

/// Bradford cone response matrix M_cone.
const MAT_BRADFORD_CONE: Mat3 = Mat3::from_cols(
    Vec3::new(0.8951, -0.7502, 0.0389),
    Vec3::new(0.2664, 1.7135, -0.0685),
    Vec3::new(-0.1614, 0.0367, 1.0296),
);

/// Inverse Bradford cone response matrix M_cone^-1.
const MAT_BRADFORD_CONE_INV: Mat3 = Mat3::from_cols(
    Vec3::new(0.986_992_9, 0.432_305_3, -0.008_528_7),
    Vec3::new(-0.147_054_3, 0.518_360_3, 0.040_042_8),
    Vec3::new(0.159_962_7, 0.049_291_2, 0.968_486_7),
);

/// Computes the Bradford chromatic adaptation matrix adapting from `src_xy` to `dst_xy`.
#[must_use]
pub fn bradford_adaptation(src_x: f64, src_y: f64, dst_x: f64, dst_y: f64) -> Mat3 {
    let src_xyz = Vec3::new(
        (src_x / src_y) as f32,
        1.0,
        ((1.0 - src_x - src_y) / src_y) as f32,
    );
    let dst_xyz = Vec3::new(
        (dst_x / dst_y) as f32,
        1.0,
        ((1.0 - dst_x - dst_y) / dst_y) as f32,
    );

    let src_rgb = MAT_BRADFORD_CONE * src_xyz;
    let dst_rgb = MAT_BRADFORD_CONE * dst_xyz;

    let scale = Vec3::new(
        dst_rgb.x / src_rgb.x,
        dst_rgb.y / src_rgb.y,
        dst_rgb.z / src_rgb.z,
    );

    let mat_scale = Mat3::from_diagonal(scale);
    MAT_BRADFORD_CONE_INV * mat_scale * MAT_BRADFORD_CONE
}

/// Converts a CIE XYZ D60 color vector to ACEScg (AP1).
#[must_use]
pub fn xyz_d60_to_acescg(xyz: Vec3) -> Vec3 {
    MAT_XYZ_D60_TO_ACESCG * xyz
}

/// Path A: Converts white-balanced camera RGB to `ACEScg` using `ForwardMatrix` (to XYZ D50)
/// followed by Bradford adaptation to D60 and conversion to `ACEScg` AP1.
pub fn cam_wb_to_acescg_matrix_forward(forward_matrix: &Mat3) -> Result<Mat3, ColorError> {
    // D50 (x = 0.34567, y = 0.35850) -> D60 (x = 0.32168, y = 0.33767)
    let bradford_d50_to_d60 = bradford_adaptation(0.34567, 0.35850, 0.32168, 0.33767);
    Ok(MAT_XYZ_D60_TO_ACESCG * bradford_d50_to_d60 * (*forward_matrix))
}

/// Path B: Converts white-balanced camera RGB to `ACEScg` using `ColorMatrix` fallback:
/// inverts ColorMatrix, undoes white-balance multipliers diag(w)^-1, adapts scene WP -> D60,
/// and converts to `ACEScg` AP1.
pub fn cam_wb_to_acescg_matrix_inverse_colormatrix(
    color_matrix: &Mat3,
    wb_multipliers: [f32; 3],
    scene_wp_x: f64,
    scene_wp_y: f64,
) -> Result<Mat3, ColorError> {
    let cm_inv = color_matrix.inverse();
    let wb_inv = Mat3::from_diagonal(Vec3::new(
        1.0 / wb_multipliers[0],
        1.0 / wb_multipliers[1],
        1.0 / wb_multipliers[2],
    ));

    let bradford_scene_to_d60 = bradford_adaptation(scene_wp_x, scene_wp_y, 0.32168, 0.33767);
    Ok(MAT_XYZ_D60_TO_ACESCG * bradford_scene_to_d60 * cm_inv * wb_inv)
}
