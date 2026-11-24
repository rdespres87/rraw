#![forbid(unsafe_code)]

use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use rraw_core::{MaskKind, Stage6MasksParams};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Parameters governing Stage 6 local geometric, parametric, and CoreML AI semantic masks.
#[derive(Debug, Clone)]
pub struct Stage6Params {
    pub width: u32,
    pub height: u32,
    pub masks: Stage6MasksParams,
}

/// Interleaved RGBA semantic probability map (`[Subject, Sky, Background, Portrait]` in `[0.0, 1.0]`)
/// passed to Stage 6 AI semantic mask evaluation.
#[derive(Debug, Clone, Copy)]
pub struct SemanticMaskPass<'a> {
    pub rgba_probabilities: &'a [f32],
    pub width: u32,
    pub height: u32,
}

impl SemanticMaskPass<'_> {
    /// Returns the expected interleaved RGBA buffer length (`width * height * 4`).
    #[must_use]
    pub const fn expected_len(&self) -> usize {
        (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(4)
    }

    /// Validates non-zero dimensions and exact RGBA buffer length.
    pub fn validate(&self) -> Result<(), GpuError> {
        let expected = self.expected_len();
        if self.width == 0 || self.height == 0 || self.rgba_probabilities.len() != expected {
            return Err(GpuError::BufferLengthMismatch {
                expected,
                actual: self.rgba_probabilities.len(),
            });
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage6UniformsGpu {
    width: u32,
    height: u32,
    mask_count: u32,
    sem_width: u32,
    sem_height: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuMaskData {
    mask_type: u32,
    exposure_offset_ev: f32,
    opacity: f32,
    temperature_shift: f32,
    param_vec0: [f32; 4],
    param_vec1: [f32; 4],
    param_vec2: [f32; 4],
}

impl Default for GpuMaskData {
    fn default() -> Self {
        Self {
            mask_type: 0,
            exposure_offset_ev: 0.0,
            opacity: 0.0,
            temperature_shift: 0.0,
            param_vec0: [0.0; 4],
            param_vec1: [0.0; 4],
            param_vec2: [0.0; 4],
        }
    }
}

/// Headless GPU compute pipeline for Stage 6 local geometric, parametric, and AI semantic masks.
pub struct Stage6Pipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Stage6Pipeline {
    /// Compiles Stage 6 shader and initializes compute pipeline.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage6_masks_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/stage6_masks.wgsl").into()),
            });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("stage6_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("stage6_pipeline_layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline = ctx
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stage6_compute_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        Ok(Self {
            pipeline,
            bind_group_layout,
        })
    }

    /// Executes Stage 6 local geometric and parametric masks on the GPU without external semantic maps.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        rgb_in: &[f32],
        params: &Stage6Params,
    ) -> Result<Vec<f32>, GpuError> {
        self.execute_with_semantics(ctx, rgb_in, params, None)
    }

    /// Executes Stage 6 local geometric, parametric, and CoreML AI semantic masks on the GPU.
    ///
    /// When `semantic_pass` is provided, its buffer must have length `width * height * 4` storing
    /// interleaved `[Subject, Sky, Background, Portrait]` probabilities in `[0.0, 1.0]`.
    pub fn execute_with_semantics(
        &self,
        ctx: &GpuContext,
        rgb_in: &[f32],
        params: &Stage6Params,
        semantic_pass: Option<SemanticMaskPass<'_>>,
    ) -> Result<Vec<f32>, GpuError> {
        let count = (params.width as usize)
            .saturating_mul(params.height as usize)
            .saturating_mul(3);
        if rgb_in.len() != count {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: rgb_in.len(),
            });
        }

        let (sem_slice, sem_width, sem_height) = if let Some(pass) = semantic_pass {
            pass.validate()?;
            (pass.rgba_probabilities, pass.width, pass.height)
        } else {
            (&[0.0_f32; 4][..], 1, 1)
        };

        let mut gpu_masks = Vec::new();
        for mask in &params.masks.masks {
            if !mask.enabled {
                continue;
            }
            let mut gm = GpuMaskData {
                exposure_offset_ev: mask.exposure_offset_ev,
                opacity: mask.opacity.clamp(0.0, 1.0),
                temperature_shift: mask.temperature_shift,
                ..Default::default()
            };
            let invert_flag = if mask.invert { 1.0 } else { 0.0 };
            match &mask.kind {
                MaskKind::LinearGradient { start, end } => {
                    gm.mask_type = 1;
                    gm.param_vec0 = [start[0], start[1], end[0], end[1]];
                    gm.param_vec1 = [0.0, invert_flag, mask.feather, 0.0];
                }
                MaskKind::RadialGradient {
                    center,
                    radius,
                    rotation_deg,
                } => {
                    gm.mask_type = 2;
                    gm.param_vec0 = [center[0], center[1], radius[0], radius[1]];
                    gm.param_vec1 = [rotation_deg.to_radians(), invert_flag, mask.feather, 0.0];
                }
                MaskKind::ParametricColor {
                    luminance,
                    chroma,
                    hue_deg,
                } => {
                    gm.mask_type = 3;
                    if let Some(l) = luminance {
                        gm.param_vec0 = [l.min_val, l.max_val, l.feather, 1.0];
                    }
                    if let Some(c) = chroma {
                        gm.param_vec1 = [c.min_val, c.max_val, c.feather, 1.0];
                    }
                    if let Some(h) = hue_deg {
                        gm.param_vec2 = [h.min_val, h.max_val, h.feather, 1.0];
                    }
                }
                MaskKind::SemanticAi {
                    category,
                    invert,
                    feather,
                } => {
                    gm.mask_type = 4;
                    gm.param_vec0 = [
                        category.channel_index() as f32,
                        if *invert { 1.0 } else { 0.0 },
                        feather.clamp(0.02, 0.5),
                        0.0,
                    ];
                }
            }
            gpu_masks.push(gm);
        }

        let mask_count = gpu_masks.len() as u32;
        if gpu_masks.is_empty() {
            gpu_masks.push(GpuMaskData::default());
        }

        let uniforms = Stage6UniformsGpu {
            width: params.width,
            height: params.height,
            mask_count,
            sem_width,
            sem_height,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };

        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage6_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let masks_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage6_masks_buffer"),
                contents: bytemuck::cast_slice(&gpu_masks),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let input_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage6_input_buffer"),
                contents: bytemuck::cast_slice(rgb_in),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let output_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage6_output_buffer"),
            size: (count * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let semantic_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage6_semantic_rgba_buffer"),
                contents: bytemuck::cast_slice(sem_slice),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let staging_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage6_staging_buffer"),
            size: (count * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage6_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: masks_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: semantic_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage6_command_encoder"),
            });

        let workgroups_x = params.width.div_ceil(16);
        let workgroups_y = params.height.div_ceil(16);

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stage6_compute_pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        encoder.copy_buffer_to_buffer(
            &output_buffer,
            0,
            &staging_buffer,
            0,
            (count * std::mem::size_of::<f32>()) as u64,
        );

        ctx.queue().submit(Some(encoder.finish()));

        let buffer_slice = staging_buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = sender.send(res);
        });

        let _ = ctx.device().poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        match receiver.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(GpuError::BufferMapFailed(format!("Map failed: {e:?}"))),
            Err(e) => return Err(GpuError::BufferMapFailed(format!("Channel failed: {e:?}"))),
        }

        let mapped = match buffer_slice.get_mapped_range() {
            Ok(view) => view,
            Err(e) => {
                return Err(GpuError::BufferMapFailed(format!(
                    "get_mapped_range failed: {e:?}"
                )))
            }
        };

        let result: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging_buffer.unmap();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_mask_pass_validation() {
        let valid_buf = vec![0.5_f32; 2 * 3 * 4];
        let valid = SemanticMaskPass {
            rgba_probabilities: &valid_buf,
            width: 2,
            height: 3,
        };
        assert_eq!(valid.expected_len(), 24);
        assert!(valid.validate().is_ok());

        let zero_dim = SemanticMaskPass {
            rgba_probabilities: &[],
            width: 0,
            height: 4,
        };
        assert!(matches!(
            zero_dim.validate(),
            Err(GpuError::BufferLengthMismatch {
                expected: 0,
                actual: 0
            })
        ));

        let short_buf = SemanticMaskPass {
            rgba_probabilities: &valid_buf[..12],
            width: 2,
            height: 3,
        };
        assert!(matches!(
            short_buf.validate(),
            Err(GpuError::BufferLengthMismatch {
                expected: 24,
                actual: 12
            })
        ));
    }
}
