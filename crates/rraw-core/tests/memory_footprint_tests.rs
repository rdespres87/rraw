use std::sync::Arc;

use rraw_core::{BayerOrigin, CfaPattern, CoreError, SignedCfaBuffer};

#[test]
fn signed_cfa_buffer_from_arc_is_zero_copy() -> Result<(), CoreError> {
    let total_samples = 1024; // Use representative slice for deterministic test
    let raw_vec: Vec<f32> = (0..total_samples)
        .map(|i| (i as f32) / 1000.0 - 0.05)
        .collect();
    let samples_arc: Arc<[f32]> = Arc::from(raw_vec);

    let cfa = SignedCfaBuffer::from_arc(
        32,
        32,
        CfaPattern::Bayer(BayerOrigin::Rggb),
        Arc::clone(&samples_arc),
    )?;

    assert_eq!(cfa.width(), 32);
    assert_eq!(cfa.height(), 32);
    assert_eq!(
        cfa.allocation_size_bytes(),
        1024 * std::mem::size_of::<f32>()
    );
    assert!(Arc::ptr_eq(cfa.samples_arc(), &samples_arc));

    // Cloning SignedCfaBuffer must be zero-copy O(1) Arc clone
    let cfa_cloned = cfa.clone();
    assert!(Arc::ptr_eq(cfa.samples_arc(), cfa_cloned.samples_arc()));

    Ok(())
}

#[test]
fn signed_cfa_buffer_new_allocation_size_calculation() -> Result<(), CoreError> {
    let raw_vec = vec![0.1_f32, 0.2, -0.05, 0.8];
    let cfa = SignedCfaBuffer::new(2, 2, CfaPattern::Bayer(BayerOrigin::Rggb), raw_vec)?;

    assert_eq!(cfa.allocation_size_bytes(), 4 * 4);
    assert_eq!(cfa.samples().len(), 4);
    assert!((cfa.samples()[2] - (-0.05)).abs() < 1e-6);

    let cloned = cfa.clone();
    assert!(Arc::ptr_eq(cfa.samples_arc(), cloned.samples_arc()));

    Ok(())
}
