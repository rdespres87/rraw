#![forbid(unsafe_code)]

use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use rraw_core::{MaskKind, Stage6MasksParams};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Parameters governing Stage 6 local geometric and parametric masks.
#[derive(Debug, Clone)]
pub struct Stage6Params {
    pub width: u32,
    pub height: u32,
    pub masks: Stage6MasksParams,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage6UniformsGpu {
    width: u32,
    height: u32,
    mask_count: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuMaskData {
    mask_type: u32,
    exposure_offset_ev: f32,
    opacity: f32,
    _pad0: f32,
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
            _pad0: 0.0,
            param_vec0: [0.0; 4],
            param_vec1: [0.0; 4],
            param_vec2: [0.0; 4],
        }
    }
}

/// Headless GPU compute pipeline for Stage 6 local geometric and parametric masks.
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

    /// Executes Stage 6 local geometric and parametric masks on the GPU.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        rgb_in: &[f32],
        params: &Stage6Params,
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

        let mut gpu_masks = Vec::new();
        for mask in &params.masks.masks {
            if !mask.enabled {
                continue;
            }
            let mut gm = GpuMaskData {
                exposure_offset_ev: mask.exposure_offset_ev,
                opacity: mask.opacity.clamp(0.0, 1.0),
                ..Default::default()
            };
            match &mask.kind {
                MaskKind::LinearGradient { start, end } => {
                    gm.mask_type = 1;
                    gm.param_vec0 = [start[0], start[1], end[0], end[1]];
                }
                MaskKind::RadialGradient {
                    center,
                    radius,
                    rotation_deg,
                } => {
                    gm.mask_type = 2;
                    gm.param_vec0 = [center[0], center[1], radius[0], radius[1]];
                    gm.param_vec1 = [rotation_deg.to_radians(), 0.0, 0.0, 0.0];
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
            _pad: 0,
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
