//! Analytical 24 ColorChecker patches and CIEDE2000 color difference metric.

use glam::{Mat3, Vec3};

use crate::ColorError;

/// Matrix converting ACEScg AP1 back to CIE XYZ D60.
/// Inverse of MAT_XYZ_D60_TO_ACESCG.
pub const MAT_ACESCG_TO_XYZ_D60: Mat3 = Mat3::from_cols(
    Vec3::new(0.662_454_2, 0.272_228_7, -0.005_528_4),
    Vec3::new(0.134_004_8, 0.674_081_8, 0.004_025_2),
    Vec3::new(0.156_187_7, 0.053_689_5, 1.010_339_1),
);

/// Canonical 24 ColorChecker patch names.
pub const COLORCHECKER_24_NAMES: [&str; 24] = [
    "dark_skin",
    "light_skin",
    "blue_sky",
    "foliage",
    "blue_flower",
    "bluish_green",
    "orange",
    "purplish_blue",
    "moderate_red",
    "purple",
    "yellow_green",
    "orange_yellow",
    "blue",
    "green",
    "red",
    "yellow",
    "magenta",
    "cyan",
    "white",
    "neutral_8",
    "neutral_6_5",
    "neutral_5",
    "neutral_3_5",
    "black",
];

/// Analytical CIE XYZ D60 coordinates for the 24 ColorChecker patches under D60 illuminant.
const COLORCHECKER_XYZ_D60: [[f32; 3]; 24] = [
    [0.1147, 0.1008, 0.0658], // 1. Dark skin
    [0.3986, 0.3582, 0.2642], // 2. Light skin
    [0.1878, 0.1933, 0.3275], // 3. Blue sky
    [0.1132, 0.1330, 0.0706], // 4. Foliage
    [0.2625, 0.2442, 0.4354], // 5. Blue flower
    [0.3201, 0.4286, 0.4079], // 6. Bluish green
    [0.3908, 0.3012, 0.0841], // 7. Orange
    [0.1378, 0.1205, 0.3592], // 8. Purplish blue
    [0.3021, 0.1983, 0.1447], // 9. Moderate red
    [0.0863, 0.0644, 0.1396], // 10. Purple
    [0.3705, 0.4435, 0.1189], // 11. Yellow green
    [0.4789, 0.4312, 0.1025], // 12. Orange yellow
    [0.0768, 0.0579, 0.2471], // 13. Blue
    [0.1601, 0.2317, 0.0984], // 14. Green
    [0.2074, 0.1206, 0.0526], // 15. Red
    [0.6012, 0.5982, 0.1119], // 16. Yellow
    [0.3052, 0.1985, 0.3045], // 17. Magenta
    [0.1602, 0.1979, 0.3642], // 18. Cyan
    [0.9135, 0.9000, 0.8258], // 19. White 9.5
    [0.5976, 0.5890, 0.5404], // 20. Neutral 8
    [0.3657, 0.3605, 0.3308], // 21. Neutral 6.5
    [0.1998, 0.1970, 0.1807], // 22. Neutral 5
    [0.0908, 0.0895, 0.0821], // 23. Neutral 3.5
    [0.0315, 0.0310, 0.0284], // 24. Black 2
];

/// Returns the analytic XYZ D60 and CIELAB D60 coordinates for a given patch index `0..24`.
pub fn colorchecker_patch_xyz(index: usize) -> Result<(Vec3, [f32; 3]), ColorError> {
    if index >= 24 {
        return Err(ColorError::InvalidKelvin {
            value: index as f64,
        });
    }

    let raw = COLORCHECKER_XYZ_D60[index];
    let xyz = Vec3::new(raw[0], raw[1], raw[2]);
    let lab = xyz_to_lab(xyz);
    Ok((xyz, lab))
}

/// Converts CIE XYZ D60 to CIELAB coordinates assuming D60 reference white.
fn xyz_to_lab(xyz: Vec3) -> [f32; 3] {
    // D60 reference white normalized Y = 1.0: Xn = 0.32168 / 0.33767, Yn = 1.0, Zn = (1 - 0.32168 - 0.33767) / 0.33767
    let xn = 0.952_646_1;
    let yn = 1.0;
    let zn = 1.008_825_2;

    let f = |t: f32| -> f32 {
        let delta = 6.0 / 29.0;
        if t > delta * delta * delta {
            t.cbrt()
        } else {
            t / (3.0 * delta * delta) + 4.0 / 29.0
        }
    };

    let fx = f(xyz.x / xn);
    let fy = f(xyz.y / yn);
    let fz = f(xyz.z / zn);

    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);

    [l, a, b]
}

/// Computes the CIEDE2000 total color difference Delta E 00 between an ACEScg color and expected Lab.
#[must_use]
pub fn delta_e_00(acescg: Vec3, expected_lab: [f32; 3]) -> f32 {
    let xyz = MAT_ACESCG_TO_XYZ_D60 * acescg;
    let actual_lab = xyz_to_lab(xyz);

    ciede2000(actual_lab, expected_lab)
}

/// Standard CIEDE2000 formula implementation (Sharma et al. 2005).
fn ciede2000(lab1: [f32; 3], lab2: [f32; 3]) -> f32 {
    let (l1, a1, b1) = (lab1[0] as f64, lab1[1] as f64, lab1[2] as f64);
    let (l2, a2, b2) = (lab2[0] as f64, lab2[1] as f64, lab2[2] as f64);

    let avg_l = (l1 + l2) * 0.5;
    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let avg_c = (c1 + c2) * 0.5;

    let g = 0.5 * (1.0 - (avg_c.powi(7) / (avg_c.powi(7) + 25.0_f64.powi(7))).sqrt());
    let a1_prime = (1.0 + g) * a1;
    let a2_prime = (1.0 + g) * a2;

    let c1_prime = (a1_prime * a1_prime + b1 * b1).sqrt();
    let c2_prime = (a2_prime * a2_prime + b2 * b2).sqrt();
    let avg_c_prime = (c1_prime + c2_prime) * 0.5;

    let h1_prime = b1.atan2(a1_prime).to_degrees().rem_euclid(360.0);
    let h2_prime = b2.atan2(a2_prime).to_degrees().rem_euclid(360.0);

    let avg_h_prime = if (c1_prime * c2_prime).abs() < 1e-9 {
        h1_prime + h2_prime
    } else if (h1_prime - h2_prime).abs() <= 180.0 {
        (h1_prime + h2_prime) * 0.5
    } else if h1_prime + h2_prime < 360.0 {
        (h1_prime + h2_prime + 360.0) * 0.5
    } else {
        (h1_prime + h2_prime - 360.0) * 0.5
    };

    let delta_h_prime = if (c1_prime * c2_prime).abs() < 1e-9 {
        0.0
    } else if (h2_prime - h1_prime).abs() <= 180.0 {
        h2_prime - h1_prime
    } else if h2_prime <= h1_prime {
        h2_prime - h1_prime + 360.0
    } else {
        h2_prime - h1_prime - 360.0
    };

    let delta_l_prime = l2 - l1;
    let delta_c_prime = c2_prime - c1_prime;
    let delta_big_h_prime =
        2.0 * (c1_prime * c2_prime).sqrt() * (delta_h_prime.to_radians() * 0.5).sin();

    let t = 1.0 - 0.17 * ((avg_h_prime - 30.0).to_radians()).cos()
        + 0.24 * ((2.0 * avg_h_prime).to_radians()).cos()
        + 0.32 * ((3.0 * avg_h_prime + 6.0).to_radians()).cos()
        - 0.20 * ((4.0 * avg_h_prime - 63.0).to_radians()).cos();

    let s_l = 1.0 + (0.015 * (avg_l - 50.0).powi(2)) / (20.0 + (avg_l - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * avg_c_prime;
    let s_h = 1.0 + 0.015 * avg_c_prime * t;

    let delta_theta = 30.0 * (-(((avg_h_prime - 275.0) / 25.0).powi(2))).exp();
    let r_c = 2.0 * (avg_c_prime.powi(7) / (avg_c_prime.powi(7) + 25.0_f64.powi(7))).sqrt();
    let r_t = -r_c * (2.0 * delta_theta.to_radians()).sin();

    let term_l = delta_l_prime / s_l;
    let term_c = delta_c_prime / s_c;
    let term_h = delta_big_h_prime / s_h;

    (term_l * term_l + term_c * term_c + term_h * term_h + r_t * term_c * term_h).sqrt() as f32
}
