#![forbid(unsafe_code)]

use rraw_optics::defringe::{defringe_rgb, DefringeConfig};

#[test]
fn test_defringe_neutral_when_amount_zero() {
    let width = 8;
    let height = 8;
    let initial: Vec<[f32; 3]> = (0..width * height)
        .map(|i| {
            let v = (i as f32) / 64.0;
            [v, v * 0.8, v * 1.2]
        })
        .collect();

    let mut samples = initial.clone();
    let config = DefringeConfig {
        purple_amount: 0.0,
        green_amount: 0.0,
        threshold: 0.08,
    };

    defringe_rgb(&mut samples, width, height, &config);
    assert_eq!(samples, initial);
}

#[test]
fn test_defringe_suppresses_purple_fringe_on_contrast_edge() {
    let width = 16;
    let height = 16;
    let mut samples = vec![[0.0_f32; 3]; width * height];

    // Create high-contrast edge:
    // Left: dark tree branch (Y ~ 0.08)
    // Middle (x = 7): purple fringe on edge (R=0.55, G=0.12, B=0.65)
    // Right: bright sky (Y ~ 0.95)
    for y in 0..height {
        for x in 0..width {
            if x < 7 {
                samples[y * width + x] = [0.08, 0.08, 0.08];
            } else if x == 7 {
                samples[y * width + x] = [0.55, 0.12, 0.65];
            } else {
                samples[y * width + x] = [0.95, 0.95, 0.95];
            }
        }
    }

    let config = DefringeConfig {
        purple_amount: 1.0,
        green_amount: 0.0,
        threshold: 0.10,
    };

    defringe_rgb(&mut samples, width, height, &config);

    // At x = 7, the purple fringe must be significantly desaturated
    for y in 2..(height - 2) {
        let fringe_px = samples[y * width + 7];
        let excess_purple =
            (fringe_px[0] - fringe_px[1]).max(0.0) + (fringe_px[2] - fringe_px[1]).max(0.0);
        assert!(
            excess_purple < 0.25,
            "Purple fringe was not suppressed: pixel is {fringe_px:?}, excess is {excess_purple}"
        );
    }
}

#[test]
fn test_defringe_suppresses_green_fringe_on_contrast_edge() {
    let width = 16;
    let height = 16;
    let mut samples = vec![[0.0_f32; 3]; width * height];

    // Create high-contrast edge:
    // Left: dark branch (Y ~ 0.10)
    // Middle (x = 7): green fringe on edge (R=0.12, G=0.68, B=0.15)
    // Right: bright sky (Y ~ 0.90)
    for y in 0..height {
        for x in 0..width {
            if x < 7 {
                samples[y * width + x] = [0.10, 0.10, 0.10];
            } else if x == 7 {
                samples[y * width + x] = [0.12, 0.68, 0.15];
            } else {
                samples[y * width + x] = [0.90, 0.90, 0.90];
            }
        }
    }

    let config = DefringeConfig {
        purple_amount: 0.0,
        green_amount: 1.0,
        threshold: 0.10,
    };

    defringe_rgb(&mut samples, width, height, &config);

    // At x = 7, the green fringe must be significantly desaturated
    for y in 2..(height - 2) {
        let fringe_px = samples[y * width + 7];
        let excess_green = (fringe_px[1] - fringe_px[0].max(fringe_px[2])).max(0.0);
        assert!(
            excess_green < 0.20,
            "Green fringe was not suppressed: pixel is {fringe_px:?}, excess green is {excess_green}"
        );
    }
}

#[test]
fn test_defringe_preserves_genuine_saturated_colors_away_from_contrast_edges() {
    let width = 12;
    let height = 12;
    // Uniform purple patch (e.g. purple flower or dress) with no contrast edge
    let mut samples = vec![[0.60_f32, 0.15_f32, 0.70_f32]; width * height];

    let config = DefringeConfig {
        purple_amount: 1.0,
        green_amount: 1.0,
        threshold: 0.10,
    };

    defringe_rgb(&mut samples, width, height, &config);

    // Interior pixels must remain saturated purple since edge contrast is zero
    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let px = samples[y * width + x];
            assert!(
                (px[0] - 0.60).abs() < 1e-4
                    && (px[1] - 0.15).abs() < 1e-4
                    && (px[2] - 0.70).abs() < 1e-4,
                "Flat saturated purple was erroneously modified: {px:?}"
            );
        }
    }
}

#[test]
fn test_defringe_handles_small_images_without_panic() {
    let mut small_1x1 = vec![[0.5, 0.5, 0.5]];
    let config = DefringeConfig {
        purple_amount: 1.0,
        green_amount: 1.0,
        threshold: 0.10,
    };
    defringe_rgb(&mut small_1x1, 1, 1, &config);
    assert_eq!(small_1x1, vec![[0.5, 0.5, 0.5]]);

    let mut small_2x2 = vec![[0.1, 0.2, 0.3]; 4];
    defringe_rgb(&mut small_2x2, 2, 2, &config);
    assert_eq!(small_2x2.len(), 4);
}
