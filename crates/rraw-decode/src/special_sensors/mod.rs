#![forbid(unsafe_code)]

//! Specialized pure-Rust sensor pipelines for non-standard CFA geometries:
//! - Fujifilm SuperCCD 45° octagonal sensors (`fuji_rotation_width`).
//! - 4-color 2x2 CFA sensors (e.g., CYGM / GMYC / RGBE).
//! - Monochrome sensors and isolated Bayer hot-pixel suppression.

mod four_color;
mod monochrome;
mod superccd;
#[cfg(test)]
mod tests;

pub use four_color::{
    demosaic_four_color_cfa_to_linear_srgb, is_four_color_2x2_cfa, try_decode_canon_pro70_ciff,
};
pub use monochrome::{decode_monochrome_sensor_to_linear_rgb, detect_isolated_bayer_hot_pixels};
pub use superccd::{demosaic_and_rotate_fuji_superccd, fuji_rotated_dimensions};

/// Standard D65 linear sRGB to CIE XYZ 3x3 conversion matrix (row-major).
pub const SRGB_TO_XYZ_D65: [f32; 9] = [
    0.412_456_4,
    0.357_576_1,
    0.180_437_5,
    0.212_672_9,
    0.715_152_2,
    0.072_175_0,
    0.019_333_9,
    0.119_192,
    0.950_304_1,
];

/// Standard D65 CIE XYZ to linear sRGB 3x3 conversion matrix (row-major).
///
/// Passing this as `DecodedRaw::color_matrix` (`XYZ -> Camera`) alongside neutral `[1.0; 4]`
/// WB multipliers causes downstream `XYZ_TO_SRGB * inv(color_matrix)` calibration to collapse
/// to the exact 3x3 identity matrix for buffers already calibrated in linear sRGB.
pub const XYZ_TO_SRGB_D65: [f32; 9] = [
    3.240_454_2,
    -1.537_138_5,
    -0.498_531_4,
    -0.969_266,
    1.876_010_8,
    0.041_556,
    0.055_643_4,
    -0.204_025_9,
    1.057_225_2,
];
