#![forbid(unsafe_code)]

//! Unit and contract tests for dynamic CFA pattern resolution and phase alignment.

use rawler::cfa::CFA;
use rraw_core::{BayerOrigin, CfaPattern};
use rraw_decode::cfa::{
    align_crop_to_cfa_phase, resolve_cfa_pattern_from_rawler, SensorCropRect, SensorDimensions,
};

#[test]
fn test_cfa_pattern_resolution_bayer_variants() {
    let rggb = CFA::new("RGGB");
    assert_eq!(
        resolve_cfa_pattern_from_rawler(&rggb),
        CfaPattern::Bayer(BayerOrigin::Rggb)
    );

    let bggr = CFA::new("BGGR");
    assert_eq!(
        resolve_cfa_pattern_from_rawler(&bggr),
        CfaPattern::Bayer(BayerOrigin::Bggr)
    );

    let grbg = CFA::new("GRBG");
    assert_eq!(
        resolve_cfa_pattern_from_rawler(&grbg),
        CfaPattern::Bayer(BayerOrigin::Grbg)
    );

    let gbrg = CFA::new("GBRG");
    assert_eq!(
        resolve_cfa_pattern_from_rawler(&gbrg),
        CfaPattern::Bayer(BayerOrigin::Gbrg)
    );

    // Case insensitivity
    let mut lower_bggr = CFA::new("BGGR");
    lower_bggr.name = "bggr".to_string();
    assert_eq!(
        resolve_cfa_pattern_from_rawler(&lower_bggr),
        CfaPattern::Bayer(BayerOrigin::Bggr)
    );
}

#[test]
fn test_cfa_pattern_resolution_xtrans() {
    // 36-character standard Fujifilm X-Trans pattern string
    let xtrans_str = "GBGGRG RGRBGB GBGGRG GRGGBG BGBRGR GRGGBG".replace(' ', "");
    assert_eq!(xtrans_str.len(), 36);

    let xtrans_cfa = CFA::new(&xtrans_str);
    assert_eq!(xtrans_cfa.width, 6);
    assert_eq!(xtrans_cfa.height, 6);

    assert_eq!(
        resolve_cfa_pattern_from_rawler(&xtrans_cfa),
        CfaPattern::fujifilm_xtrans()
    );
}

#[test]
fn test_cfa_pattern_resolution_fallback_on_unrecognized() {
    let empty_cfa = CFA::new("");
    assert_eq!(
        resolve_cfa_pattern_from_rawler(&empty_cfa),
        CfaPattern::Bayer(BayerOrigin::Rggb)
    );
}

#[test]
fn test_align_crop_to_cfa_phase_bayer_and_xtrans() {
    let bayer = CfaPattern::Bayer(BayerOrigin::Rggb);
    let xtrans = CfaPattern::fujifilm_xtrans();

    let bounds = SensorDimensions {
        width: 1000,
        height: 1000,
    };

    // Bayer (period 2)
    let b = align_crop_to_cfa_phase(
        &bayer,
        SensorCropRect {
            x: 7,
            y: 9,
            width: 101,
            height: 203,
        },
        bounds,
    );
    assert_eq!(b.x % 2, 0);
    assert_eq!(b.y % 2, 0);
    assert_eq!(b.width % 2, 0);
    assert_eq!(b.height % 2, 0);
    assert_eq!(
        b,
        SensorCropRect {
            x: 6,
            y: 8,
            width: 100,
            height: 202,
        }
    );

    // X-Trans (period 6)
    let x = align_crop_to_cfa_phase(
        &xtrans,
        SensorCropRect {
            x: 7,
            y: 9,
            width: 501,
            height: 503,
        },
        bounds,
    );
    assert_eq!(x.x % 6, 0);
    assert_eq!(x.y % 6, 0);
    assert_eq!(x.width % 6, 0);
    assert_eq!(x.height % 6, 0);
    assert_eq!(
        x,
        SensorCropRect {
            x: 6,
            y: 6,
            width: 498,
            height: 498,
        }
    );
}
