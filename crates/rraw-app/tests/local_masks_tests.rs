#![forbid(unsafe_code)]

use rraw_app::state::{DarkroomGradingState, LocalMaskLayer, SidecarRecipe};

#[test]
fn test_linear_gradient_mask_weights() {
    // Linear gradient from top (y=0.0) to bottom (y=1.0)
    let mask = LocalMaskLayer::new_linear("Sky Gradient", [0.5, 0.0], [0.5, 1.0]);

    // Top: start of gradient should have weight ~ 0.0
    let w_top = mask.evaluate_weight(0.5, 0.0);
    assert!(w_top < 0.05, "Top should have near-zero weight: {w_top}");

    // Bottom: end of gradient should have weight ~ 1.0
    let w_bottom = mask.evaluate_weight(0.5, 1.0);
    assert!(
        w_bottom > 0.95,
        "Bottom should have near-full weight: {w_bottom}"
    );

    // Midpoint: weight should be around 0.5
    let w_mid = mask.evaluate_weight(0.5, 0.5);
    assert!(
        (w_mid - 0.5).abs() < 0.1,
        "Midpoint weight should be near 0.5: {w_mid}"
    );

    // Inverted mask: top should be near 1.0, bottom near 0.0
    let mut inv_mask = mask.clone();
    inv_mask.invert = true;
    assert!(inv_mask.evaluate_weight(0.5, 0.0) > 0.95);
    assert!(inv_mask.evaluate_weight(0.5, 1.0) < 0.05);
}

#[test]
fn test_radial_gradient_mask_weights() {
    // Radial gradient centered at [0.5, 0.5] with radius [0.3, 0.3]
    let mask = LocalMaskLayer::new_radial("Face Spotlight", [0.5, 0.5], [0.3, 0.3]);

    // Exact center should be 1.0
    let w_center = mask.evaluate_weight(0.5, 0.5);
    assert!(
        (w_center - 1.0).abs() < 1e-4,
        "Center should have full weight: {w_center}"
    );

    // Well outside the radius (e.g. at [0.9, 0.9]) should be 0.0
    let w_outside = mask.evaluate_weight(0.9, 0.9);
    assert!(
        w_outside < 1e-4,
        "Outside radius should have zero weight: {w_outside}"
    );

    // Inverted radial mask: center is 0.0, outside is 1.0
    let mut inv_mask = mask.clone();
    inv_mask.invert = true;
    assert!(inv_mask.evaluate_weight(0.5, 0.5) < 1e-4);
    assert!((inv_mask.evaluate_weight(0.9, 0.9) - 1.0).abs() < 1e-4);
}

#[test]
fn test_local_mask_neutral_and_disabled_state() {
    let mut mask = LocalMaskLayer::new_linear("Neutral Mask", [0.0, 0.0], [1.0, 1.0]);
    assert!(mask.is_neutral(), "Default new mask must be neutral");

    mask.exposure_ev = 1.0;
    assert!(
        !mask.is_neutral(),
        "Mask with non-zero exposure is not neutral"
    );

    mask.enabled = false;
    assert!(mask.is_neutral(), "Disabled mask must be neutral");
    assert_eq!(mask.evaluate_weight(0.5, 0.5), 0.0);
}

#[test]
fn test_local_masks_sidecar_recipe_roundtrip() {
    let mut grading = DarkroomGradingState::default();
    let mut mask1 = LocalMaskLayer::new_linear("Linear Sky", [0.5, 0.0], [0.5, 0.4]);
    mask1.exposure_ev = -1.5;
    mask1.contrast = 20.0;
    mask1.temperature_shift = -15.0;

    let mut mask2 = LocalMaskLayer::new_radial("Subject Light", [0.5, 0.6], [0.25, 0.35]);
    mask2.exposure_ev = 0.8;
    mask2.saturation = 10.0;

    grading.local_masks.push(mask1);
    grading.local_masks.push(mask2);

    let recipe = SidecarRecipe::from_state(&grading, 5);
    let mut restored = DarkroomGradingState::default();
    recipe.apply_to(&mut restored);

    assert_eq!(restored.local_masks.len(), 2);
    assert_eq!(restored.local_masks[0].name, "Linear Sky");
    assert!((restored.local_masks[0].exposure_ev - (-1.5)).abs() < 1e-4);
    assert_eq!(restored.local_masks[1].name, "Subject Light");
    assert!((restored.local_masks[1].exposure_ev - 0.8).abs() < 1e-4);
}
