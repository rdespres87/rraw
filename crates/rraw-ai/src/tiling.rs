#![forbid(unsafe_code)]

//! Phase-preserving tile slicer and cosine-window blender.

use crate::TileRegion;
use rraw_core::TileGeometry;
use std::f32::consts::PI;

/// Slices raw sensor frames into fixed-size tiles with phase-preserving boundary reflection.
#[derive(Debug, Clone, Copy)]
pub struct TileSlicer {
    geometry: TileGeometry,
}

impl TileSlicer {
    /// Constructs a tile slicer with the specified tile geometry.
    #[must_use]
    pub const fn new(geometry: TileGeometry) -> Self {
        Self { geometry }
    }

    /// Extracts a fixed `(tile_size * tile_size)` tensor tile preserving CFA phase.
    ///
    /// Out-of-bounds boundary pixels on right and bottom edges are filled via periodic
    /// reflection stepping by multiples of `6` (LCM of Bayer `2` and X-Trans `6`),
    /// guaranteeing identical color filter phase across all synthetic padding.
    #[must_use]
    pub fn extract_tile(
        &self,
        image: &[f32],
        width: usize,
        height: usize,
        region: &TileRegion,
    ) -> Vec<f32> {
        let size = self.geometry.tile_size;
        let mut tile = Vec::with_capacity(size.saturating_mul(size));

        for ty in 0..size {
            let gy = region.origin_y.saturating_add(ty);
            let py = if gy < height {
                gy
            } else {
                let mut ry = gy;
                while ry >= height {
                    ry = ry.saturating_sub(TileGeometry::CFA_PHASE_ALIGNMENT);
                }
                ry
            };

            for tx in 0..size {
                let gx = region.origin_x.saturating_add(tx);
                let px = if gx < width {
                    gx
                } else {
                    let mut rx = gx;
                    while rx >= width {
                        rx = rx.saturating_sub(TileGeometry::CFA_PHASE_ALIGNMENT);
                    }
                    rx
                };

                let src_idx = py.saturating_mul(width).saturating_add(px);
                let val = if src_idx < image.len() {
                    image[src_idx]
                } else {
                    0.0
                };
                tile.push(val);
            }
        }

        tile
    }
}

/// Accumulates tiles and blends overlapping boundaries using 2D separable $\cos^2$ windows.
#[derive(Debug, Clone)]
pub struct TileBlender {
    width: usize,
    height: usize,
    geometry: TileGeometry,
    accum: Vec<f32>,
    weight_accum: Vec<f32>,
}

impl TileBlender {
    /// Initializes an empty blender for an image of `(width, height)`.
    #[must_use]
    pub fn new(width: usize, height: usize, geometry: TileGeometry) -> Self {
        let count = width.saturating_mul(height);
        Self {
            width,
            height,
            geometry,
            accum: vec![0.0; count],
            weight_accum: vec![0.0; count],
        }
    }

    /// Accumulates a processed `(tile_size * tile_size)` tile into the full-frame canvas.
    pub fn accumulate_tile(&mut self, tile: &[f32], region: &TileRegion) {
        let size = self.geometry.tile_size;
        let overlap = self.geometry.overlap;
        let o_f32 = overlap as f32;

        let valid_w = region.valid_width.min(size);
        let valid_h = region.valid_height.min(size);

        for ty in 0..valid_h {
            let gy = region.origin_y.saturating_add(ty);
            if gy >= self.height {
                continue;
            }

            // Top boundary overlap transition
            let wy = if region.origin_y > 0 && ty < overlap {
                let theta = (PI * 0.5) * ((ty as f32 + 0.5) / o_f32);
                let s = theta.sin();
                s * s
            // Bottom boundary overlap transition
            } else if region.origin_y.saturating_add(size) < self.height && ty >= size - overlap {
                let d = (size - 1 - ty) as f32;
                let theta = (PI * 0.5) * ((d + 0.5) / o_f32);
                let s = theta.sin();
                s * s
            } else {
                1.0
            };

            for tx in 0..valid_w {
                let gx = region.origin_x.saturating_add(tx);
                if gx >= self.width {
                    continue;
                }

                // Left boundary overlap transition
                let wx = if region.origin_x > 0 && tx < overlap {
                    let theta = (PI * 0.5) * ((tx as f32 + 0.5) / o_f32);
                    let s = theta.sin();
                    s * s
                // Right boundary overlap transition
                } else if region.origin_x.saturating_add(size) < self.width && tx >= size - overlap
                {
                    let d = (size - 1 - tx) as f32;
                    let theta = (PI * 0.5) * ((d + 0.5) / o_f32);
                    let s = theta.sin();
                    s * s
                } else {
                    1.0
                };

                let weight = wx * wy;
                let tile_idx = ty.saturating_mul(size).saturating_add(tx);
                let canvas_idx = gy.saturating_mul(self.width).saturating_add(gx);

                if tile_idx < tile.len() && canvas_idx < self.accum.len() {
                    self.accum[canvas_idx] += tile[tile_idx] * weight;
                    self.weight_accum[canvas_idx] += weight;
                }
            }
        }
    }

    /// Normalizes accumulated values by the sum of weights, yielding seamless full-frame output.
    #[must_use]
    pub fn finalize(self) -> Vec<f32> {
        let mut output = Vec::with_capacity(self.accum.len());
        for i in 0..self.accum.len() {
            let w = self.weight_accum[i];
            let val = if w > 1e-12 {
                self.accum[i] / w
            } else {
                self.accum[i]
            };
            output.push(val);
        }
        output
    }
}
