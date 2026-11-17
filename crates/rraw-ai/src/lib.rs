#![forbid(unsafe_code)]

//! Asynchronous 504x504 tiled CoreML CFA denoising and epoch cancellation contracts.

use rraw_core::{AiError, CfaPattern, Stage0Token, TileGeometry};

/// Single fixed-size `504x504` tile descriptor generated for CFA-phase-preserving inference.
///
/// Every tile origin strictly satisfies `origin_x % 6 == 0 && origin_y % 6 == 0`
/// (divisible by both Bayer period `2` and Fujifilm X-Trans period `6`). Right and
/// bottom boundary tiles record `valid_width` / `valid_height` alongside `pad_right` /
/// `pad_bottom` so tensor assembly fills padding via phase-preserving `6x6` mirror
/// reflection to the fixed `504x504` ANE shape without ever shifting tile origin phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRegion {
    pub origin_x: usize,
    pub origin_y: usize,
    pub valid_width: usize,
    pub valid_height: usize,
    pub pad_right: usize,
    pub pad_bottom: usize,
}

/// Computes phase-preserving `504x504` tiles covering an image of `(width, height)`.
///
/// Advances strictly by `stride = geometry.stride()` (`456`, divisible by `6`) and never
/// steps backward onto unaligned edges, recording `pad_right` / `pad_bottom` instead.
pub fn plan_phase_preserving_tiles(
    width: usize,
    height: usize,
    pattern: &CfaPattern,
    geometry: TileGeometry,
) -> Result<Vec<TileRegion>, AiError> {
    if width == 0 || height == 0 {
        return Err(AiError::InvalidDimensions { width, height });
    }
    if !geometry.preserves_pattern(pattern)
        || !geometry
            .tile_size
            .is_multiple_of(TileGeometry::CFA_PHASE_ALIGNMENT)
        || !geometry
            .overlap
            .is_multiple_of(TileGeometry::CFA_PHASE_ALIGNMENT)
    {
        return Err(AiError::InvalidInput(format!(
            "tile geometry ({}, {}) does not preserve CFA phase",
            geometry.tile_size, geometry.overlap
        )));
    }

    let tile_size = geometry.tile_size;
    let stride = geometry.stride();

    let mut tiles = Vec::new();
    let mut y = 0_usize;
    loop {
        let mut x = 0_usize;
        loop {
            let valid_width = (width - x).min(tile_size);
            let valid_height = (height - y).min(tile_size);
            tiles.push(TileRegion {
                origin_x: x,
                origin_y: y,
                valid_width,
                valid_height,
                pad_right: tile_size - valid_width,
                pad_bottom: tile_size - valid_height,
            });
            if x.saturating_add(tile_size) >= width {
                break;
            }
            x += stride;
        }
        if y.saturating_add(tile_size) >= height {
            break;
        }
        y += stride;
    }

    Ok(tiles)
}

/// Checks whether a background AI inference token still matches the active `(PhotoId, Stage0Epoch)`.
#[must_use]
pub fn is_token_current(active: Stage0Token, worker: Stage0Token) -> bool {
    active == worker
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_core::{BayerOrigin, PhotoId, Stage0Epoch};

    fn assert_full_phase_preserving_coverage(
        width: usize,
        height: usize,
        geometry: TileGeometry,
        tiles: &[TileRegion],
    ) {
        assert!(!tiles.is_empty());

        let mut x_intervals: Vec<(usize, usize)> = Vec::new();
        let mut y_intervals: Vec<(usize, usize)> = Vec::new();

        for tile in tiles {
            assert_eq!(tile.origin_x % 6, 0);
            assert_eq!(tile.origin_y % 6, 0);
            assert_eq!(tile.origin_x % 2, 0);
            assert_eq!(tile.origin_y % 2, 0);
            assert!(tile.valid_width > 0);
            assert!(tile.valid_height > 0);
            assert_eq!(tile.valid_width + tile.pad_right, geometry.tile_size);
            assert_eq!(tile.valid_height + tile.pad_bottom, geometry.tile_size);
            assert!(tile.origin_x + tile.valid_width <= width);
            assert!(tile.origin_y + tile.valid_height <= height);

            x_intervals.push((tile.origin_x, tile.origin_x + tile.valid_width));
            y_intervals.push((tile.origin_y, tile.origin_y + tile.valid_height));
        }

        x_intervals.sort_unstable();
        y_intervals.sort_unstable();

        let mut covered_x = 0_usize;
        for (start, end) in x_intervals {
            assert!(start <= covered_x);
            covered_x = covered_x.max(end);
        }
        assert_eq!(covered_x, width);

        let mut covered_y = 0_usize;
        for (start, end) in y_intervals {
            assert!(start <= covered_y);
            covered_y = covered_y.max(end);
        }
        assert_eq!(covered_y, height);
    }

    #[test]
    fn tile_planning_preserves_phase_across_sensor_and_roi_dimensions() -> Result<(), AiError> {
        let geometry = TileGeometry::COREML;
        for pattern in [
            CfaPattern::fujifilm_xtrans(),
            CfaPattern::Bayer(BayerOrigin::Rggb),
        ] {
            for (w, h) in [
                (6000, 4000),
                (4000, 3000),
                (2000, 1500),
                (961, 701),
                (960, 960),
                (400, 400),
            ] {
                let tiles = plan_phase_preserving_tiles(w, h, &pattern, geometry)?;
                assert_full_phase_preserving_coverage(w, h, geometry, &tiles);
            }
        }
        Ok(())
    }

    #[test]
    fn exact_two_tile_span_960_has_zero_padding_and_961_pads_third_tile() -> Result<(), AiError> {
        let pattern = CfaPattern::fujifilm_xtrans();
        let geometry = TileGeometry::COREML;

        let tiles_960 = plan_phase_preserving_tiles(960, 504, &pattern, geometry)?;
        assert_eq!(tiles_960.len(), 2);
        assert_eq!(tiles_960[0].origin_x, 0);
        assert_eq!(tiles_960[0].pad_right, 0);
        assert_eq!(tiles_960[1].origin_x, 456);
        assert_eq!(tiles_960[1].valid_width, 504);
        assert_eq!(tiles_960[1].pad_right, 0);

        let tiles_961 = plan_phase_preserving_tiles(961, 504, &pattern, geometry)?;
        assert_eq!(tiles_961.len(), 3);
        assert_eq!(tiles_961[2].origin_x, 912);
        assert_eq!(tiles_961[2].origin_x % 6, 0);
        assert_eq!(tiles_961[2].valid_width, 49);
        assert_eq!(tiles_961[2].pad_right, 455);
        Ok(())
    }

    #[test]
    fn zero_dimensions_return_invalid_dimensions_error() {
        let pattern = CfaPattern::fujifilm_xtrans();
        let geometry = TileGeometry::COREML;
        assert_eq!(
            plan_phase_preserving_tiles(0, 504, &pattern, geometry),
            Err(AiError::InvalidDimensions {
                width: 0,
                height: 504
            })
        );
        assert_eq!(
            plan_phase_preserving_tiles(504, 0, &pattern, geometry),
            Err(AiError::InvalidDimensions {
                width: 504,
                height: 0
            })
        );
        assert_eq!(
            plan_phase_preserving_tiles(0, 0, &pattern, geometry),
            Err(AiError::InvalidDimensions {
                width: 0,
                height: 0
            })
        );
    }

    #[test]
    fn epoch_token_mismatch_triggers_cancellation() {
        let active = Stage0Token {
            photo_id: PhotoId(42),
            epoch: Stage0Epoch(7),
        };
        let stale_epoch = Stage0Token {
            photo_id: PhotoId(42),
            epoch: Stage0Epoch(6),
        };
        let other_photo = Stage0Token {
            photo_id: PhotoId(99),
            epoch: Stage0Epoch(7),
        };
        assert!(is_token_current(active, active));
        assert!(!is_token_current(active, stale_epoch));
        assert!(!is_token_current(active, other_photo));
    }
}
