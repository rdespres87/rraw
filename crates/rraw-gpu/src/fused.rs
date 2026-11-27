#![forbid(unsafe_code)]

//! Fused Stages 5 → 6 → 7 GPU compute execution sharing device-resident ping-pong buffers
//! with a single `CommandEncoder` submission and a single final readback.

use std::sync::mpsc;

use wgpu::util::DeviceExt;

use crate::context::GpuContext;
use crate::error::GpuError;
use crate::stage5::{Stage5Params, Stage5Pipeline};
use crate::stage6::{SemanticMaskPass, Stage6IoBuffers, Stage6Params, Stage6Pipeline};
use crate::stage7::{Stage7IoBuffers, Stage7Params, Stage7Pipeline};

/// Request describing a fused Stages 5 → 6 → 7 preview execution on GPU.
#[derive(Debug, Clone, Copy)]
pub struct FusedPreviewRequest<'a> {
    pub linear_rgb: &'a [f32],
    pub stage5: Option<&'a Stage5Params>,
    pub stage6: Option<(&'a Stage6Params, Option<SemanticMaskPass<'a>>)>,
    pub stage7: Stage7Params,
}

/// Output of a fused Stages 5 → 6 → 7 GPU execution.
#[derive(Debug, Clone)]
pub struct FusedPreviewOutput {
    pub drt_rgb: Vec<f32>,
    pub histogram_lum: Vec<u32>,
    pub has_clipping: bool,
}

/// Validates fused preview buffer lengths and stage dimension consistency.
fn validate_fused_request(req: &FusedPreviewRequest<'_>) -> Result<usize, GpuError> {
    let count = (req.stage7.width as usize)
        .checked_mul(req.stage7.height as usize)
        .and_then(|px| px.checked_mul(3))
        .ok_or(GpuError::BufferLengthMismatch {
            expected: usize::MAX,
            actual: req.linear_rgb.len(),
        })?;
    if req.linear_rgb.len() != count {
        return Err(GpuError::BufferLengthMismatch {
            expected: count,
            actual: req.linear_rgb.len(),
        });
    }
    if let Some(s5) = req.stage5 {
        if s5.width != req.stage7.width || s5.height != req.stage7.height {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: (s5.width as usize)
                    .saturating_mul(s5.height as usize)
                    .saturating_mul(3),
            });
        }
    }
    if let Some((s6, _)) = req.stage6 {
        if s6.width != req.stage7.width || s6.height != req.stage7.height {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: (s6.width as usize)
                    .saturating_mul(s6.height as usize)
                    .saturating_mul(3),
            });
        }
    }
    Ok(count)
}

/// Maps all three staging buffers in one device poll and copies out the final results.
fn map_and_readback_fused(
    ctx: &GpuContext,
    staging_output: &wgpu::Buffer,
    staging_hist: &wgpu::Buffer,
    staging_clip: &wgpu::Buffer,
) -> Result<FusedPreviewOutput, GpuError> {
    let out_slice = staging_output.slice(..);
    let hist_slice = staging_hist.slice(..);
    let clip_slice = staging_clip.slice(..);

    let (tx_out, rx_out) = mpsc::channel();
    let (tx_hist, rx_hist) = mpsc::channel();
    let (tx_clip, rx_clip) = mpsc::channel();

    out_slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx_out.send(res);
    });
    hist_slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx_hist.send(res);
    });
    clip_slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx_clip.send(res);
    });

    let _ = ctx.device().poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });

    rx_out
        .recv()
        .map_err(|e| GpuError::BufferMapFailed(e.to_string()))?
        .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;
    rx_hist
        .recv()
        .map_err(|e| GpuError::BufferMapFailed(e.to_string()))?
        .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;
    rx_clip
        .recv()
        .map_err(|e| GpuError::BufferMapFailed(e.to_string()))?
        .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;

    let out_mapped = out_slice
        .get_mapped_range()
        .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;
    let hist_mapped = hist_slice
        .get_mapped_range()
        .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;
    let clip_mapped = clip_slice
        .get_mapped_range()
        .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;

    let drt_rgb: Vec<f32> = bytemuck::cast_slice(&out_mapped).to_vec();
    let histogram_lum: Vec<u32> = bytemuck::cast_slice(&hist_mapped).to_vec();
    let clip_vals: &[u32] = bytemuck::cast_slice(&clip_mapped);
    let has_clipping = clip_vals.first().is_some_and(|&v| v > 0);

    drop(out_mapped);
    drop(hist_mapped);
    drop(clip_mapped);

    staging_output.unmap();
    staging_hist.unmap();
    staging_clip.unmap();

    Ok(FusedPreviewOutput {
        drt_rgb,
        histogram_lum,
        has_clipping,
    })
}

/// Executes optional Stage 5 (curves / 3D CLUT), optional Stage 6 (local & AI semantic masks),
/// and Stage 7 (AgX DRT & scopes) in one GPU command submission with zero intermediate readbacks.
pub fn execute_stages5_to_7_fused(
    ctx: &GpuContext,
    stage5: &Stage5Pipeline,
    stage6: &Stage6Pipeline,
    stage7: &Stage7Pipeline,
    req: &FusedPreviewRequest<'_>,
) -> Result<FusedPreviewOutput, GpuError> {
    let count = validate_fused_request(req)?;
    if count == 0 {
        return Ok(FusedPreviewOutput {
            drt_rgb: Vec::new(),
            histogram_lum: vec![0_u32; 256],
            has_clipping: false,
        });
    }

    let pixel_bytes = count
        .checked_mul(std::mem::size_of::<f32>())
        .and_then(|b| u64::try_from(b).ok())
        .ok_or(GpuError::BufferLengthMismatch {
            expected: usize::MAX,
            actual: count,
        })?;
    let hist_bytes = (256 * std::mem::size_of::<u32>()) as u64;
    let clip_bytes = std::mem::size_of::<u32>() as u64;

    let buf_a = ctx
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fused_rgb_buf_a"),
            contents: bytemuck::cast_slice(req.linear_rgb),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });

    let buf_b = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("fused_rgb_buf_b"),
        size: pixel_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let hist_zeros = [0_u32; 256];
    let hist_buffer = ctx
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fused_hist_buffer"),
            contents: bytemuck::cast_slice(&hist_zeros),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });

    let clip_zeros = [0_u32; 1];
    let clip_buffer = ctx
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fused_clip_buffer"),
            contents: bytemuck::cast_slice(&clip_zeros),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });

    let staging_output = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("fused_staging_output"),
        size: pixel_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let staging_hist = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("fused_staging_hist"),
        size: hist_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let staging_clip = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("fused_staging_clip"),
        size: clip_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("fused_stages5_to_7_encoder"),
        });

    let mut active_is_b = false;

    if let Some(s5) = req.stage5 {
        stage5.encode_into(ctx, &mut encoder, &buf_a, &buf_b, s5);
        active_is_b = true;
    }

    if let Some((s6, sem)) = req.stage6 {
        let (in_buf, out_buf) = if active_is_b {
            (&buf_b, &buf_a)
        } else {
            (&buf_a, &buf_b)
        };
        stage6.encode_with_semantics_into(
            ctx,
            &mut encoder,
            Stage6IoBuffers {
                input: in_buf,
                output: out_buf,
            },
            s6,
            sem,
        )?;
        active_is_b = !active_is_b;
    }

    let (stage7_in, stage7_out) = if active_is_b {
        (&buf_b, &buf_a)
    } else {
        (&buf_a, &buf_b)
    };
    stage7.encode_into(
        ctx,
        &mut encoder,
        Stage7IoBuffers {
            input: stage7_in,
            output: stage7_out,
            histogram: &hist_buffer,
            clipping: &clip_buffer,
        },
        &req.stage7,
    );

    encoder.copy_buffer_to_buffer(stage7_out, 0, &staging_output, 0, pixel_bytes);
    encoder.copy_buffer_to_buffer(&hist_buffer, 0, &staging_hist, 0, hist_bytes);
    encoder.copy_buffer_to_buffer(&clip_buffer, 0, &staging_clip, 0, clip_bytes);

    ctx.queue().submit(Some(encoder.finish()));

    map_and_readback_fused(ctx, &staging_output, &staging_hist, &staging_clip)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rraw_color::ColorLut3d;
    use rraw_core::{GeometricMask, MaskKind, Stage6MasksParams};

    use super::*;

    #[test]
    fn fused_stages5_to_7_matches_sequential_execution_and_shares_arc_clut(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Ok(ctx) = GpuContext::new_headless() else {
            return Ok(());
        };
        let stage5 = Stage5Pipeline::new(&ctx)?;
        let stage6 = Stage6Pipeline::new(&ctx)?;
        let stage7 = Stage7Pipeline::new(&ctx)?;

        let width = 8_u32;
        let height = 8_u32;
        let mut linear = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                linear.push(x as f32 / (width - 1) as f32 * 1.4);
                linear.push(y as f32 / (height - 1) as f32 * 0.9);
                linear.push(0.35);
            }
        }

        let clut = Arc::new(ColorLut3d::identity(9)?);
        let s5_params =
            Stage5Params::new(width, height, 0, 0, 0.0).with_clut_3d_arc(Arc::clone(&clut), 0.75);
        assert_eq!(Arc::strong_count(&clut), 2);

        let mut masks = Stage6MasksParams::default();
        masks.masks.push(GeometricMask {
            id: "fused_mask".to_string(),
            kind: MaskKind::LinearGradient {
                start: [0.0, 0.0],
                end: [1.0, 1.0],
            },
            exposure_offset_ev: 0.35,
            opacity: 0.8,
            invert: false,
            feather: 0.4,
            temperature_shift: 10.0,
            enabled: true,
        });
        let s6_params = Stage6Params {
            width,
            height,
            masks,
        };
        let s7_params = Stage7Params {
            width,
            height,
            headroom: 4.0,
        };

        let seq_s5 = stage5.execute(&ctx, &linear, &s5_params)?;
        let seq_s6 = stage6.execute_with_semantics(&ctx, &seq_s5, &s6_params, None)?;
        let (seq_s7, seq_hist, seq_clip) = stage7.execute(&ctx, &seq_s6, &s7_params)?;

        let req = FusedPreviewRequest {
            linear_rgb: &linear,
            stage5: Some(&s5_params),
            stage6: Some((&s6_params, None)),
            stage7: s7_params,
        };
        let fused = execute_stages5_to_7_fused(&ctx, &stage5, &stage6, &stage7, &req)?;

        assert_eq!(fused.drt_rgb.len(), seq_s7.len());
        for (a, b) in fused.drt_rgb.iter().zip(seq_s7.iter()) {
            assert!((a - b).abs() < 1e-6, "fused mismatch: {a} vs {b}");
        }
        assert_eq!(fused.histogram_lum, seq_hist);
        assert_eq!(fused.has_clipping, seq_clip);
        Ok(())
    }
}
