use rraw_ai::{CoreMlSemanticSegmenter, SemanticMaskSet, SemanticSegmenter};
use rraw_core::{AiError, SemanticMaskCategory};

#[test]
fn test_semantic_segmentation_landscape_sky_subject_background_partition() -> Result<(), AiError> {
    let width = 64_usize;
    let height = 64_usize;
    let mut rgb = vec![[0.08_f32, 0.14, 0.07]; width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if y < 24 {
                // Upper atmospheric blue sky
                rgb[idx] = [0.18, 0.45, 0.92];
            } else if (20..=44).contains(&x) && (26..=52).contains(&y) {
                // Central salient warm subject
                rgb[idx] = [0.75, 0.35, 0.18];
            }
        }
    }

    let segmenter = CoreMlSemanticSegmenter::new();
    let masks: SemanticMaskSet = segmenter.segment_rgb(&rgb, width, height)?;

    assert_eq!(masks.width, width);
    assert_eq!(masks.height, height);
    assert_eq!(masks.subject.len(), width * height);
    assert_eq!(masks.sky.len(), width * height);
    assert_eq!(masks.background.len(), width * height);
    assert_eq!(masks.portrait.len(), width * height);

    // Every probability must be finite and in [0.0, 1.0], and subject + background == 1.0
    for i in 0..(width * height) {
        for val in [
            masks.subject[i],
            masks.sky[i],
            masks.background[i],
            masks.portrait[i],
        ] {
            assert!(val.is_finite());
            assert!((0.0..=1.0).contains(&val));
        }
        assert!(
            (masks.subject[i] + masks.background[i] - 1.0).abs() < 1e-5,
            "subject + background must form a partition of unity"
        );
    }

    // Sky region (x=32, y=6)
    let sky_idx = 6 * width + 32;
    assert!(
        masks.sky[sky_idx] > 0.70,
        "expected high sky confidence at top sky pixel, got {}",
        masks.sky[sky_idx]
    );
    assert!(
        masks.subject[sky_idx] < 0.25,
        "expected low subject confidence in upper sky, got {}",
        masks.subject[sky_idx]
    );

    // Subject region (x=32, y=38)
    let subj_idx = 38 * width + 32;
    assert!(
        masks.subject[subj_idx] > 0.65,
        "expected high subject confidence at center subject, got {}",
        masks.subject[subj_idx]
    );
    assert!(
        masks.sky[subj_idx] < 0.20,
        "expected low sky confidence on warm subject, got {}",
        masks.sky[subj_idx]
    );

    Ok(())
}

#[test]
fn test_semantic_segmentation_portrait_skin_detection() -> Result<(), AiError> {
    let width = 48_usize;
    let height = 48_usize;
    let mut rgb = vec![[0.12_f32, 0.16, 0.24]; width * height];

    for y in 12..36 {
        for x in 14..34 {
            // Natural linear skin reflectance locus (R > G > B with warm chromaticity)
            rgb[y * width + x] = [0.62, 0.41, 0.29];
        }
    }

    let segmenter = CoreMlSemanticSegmenter::new();
    let masks = segmenter.segment_rgb(&rgb, width, height)?;

    let center_idx = 24 * width + 24;
    let corner_idx = 3 * width + 3;

    assert!(
        masks.portrait[center_idx] > 0.65,
        "expected high portrait skin probability at center, got {}",
        masks.portrait[center_idx]
    );
    assert!(
        masks.subject[center_idx] > 0.65,
        "portrait region must also contribute to salient subject mask, got {}",
        masks.subject[center_idx]
    );
    assert!(
        masks.portrait[corner_idx] < 0.15,
        "expected low portrait probability on cool background corner, got {}",
        masks.portrait[corner_idx]
    );

    Ok(())
}

#[test]
fn test_semantic_mask_set_bilinear_sampling_feather_invert_and_rgba_pack() -> Result<(), AiError> {
    let width = 16_usize;
    let height = 16_usize;
    let mut rgb = vec![[0.10_f32, 0.12, 0.15]; width * height];
    for y in 0..8 {
        for x in 0..width {
            rgb[y * width + x] = [0.20, 0.50, 0.95];
        }
    }

    let segmenter = CoreMlSemanticSegmenter::new();
    let masks = segmenter.segment_rgb(&rgb, width, height)?;

    let p_sky_top = masks.sample_bilinear(SemanticMaskCategory::Sky, 0.5, 0.15, false, 0.5);
    let p_sky_top_inv = masks.sample_bilinear(SemanticMaskCategory::Sky, 0.5, 0.15, true, 0.5);
    assert!((p_sky_top + p_sky_top_inv - 1.0).abs() < 1e-5);
    assert!(p_sky_top > 0.65);

    let packed = masks.to_rgba_interleaved();
    assert_eq!(packed.len(), width * height * 4);
    for i in 0..(width * height) {
        assert!((packed[i * 4] - masks.subject[i]).abs() < 1e-6);
        assert!((packed[i * 4 + 1] - masks.sky[i]).abs() < 1e-6);
        assert!((packed[i * 4 + 2] - masks.background[i]).abs() < 1e-6);
        assert!((packed[i * 4 + 3] - masks.portrait[i]).abs() < 1e-6);
    }

    Ok(())
}

#[test]
fn test_semantic_segmentation_invalid_dimensions_error() {
    let segmenter = CoreMlSemanticSegmenter::new();
    assert!(matches!(
        segmenter.segment_rgb(&[], 0, 16),
        Err(AiError::InvalidDimensions {
            width: 0,
            height: 16
        })
    ));
    assert!(matches!(
        segmenter.segment_rgb(&[[0.0, 0.0, 0.0]; 10], 4, 4),
        Err(AiError::InvalidInput(_))
    ));
}
