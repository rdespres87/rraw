#![forbid(unsafe_code)]

use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Parameters governing Stage 3 regularized Richardson-Lucy capture sharpening.
#[derive(Debug, Clone, Copy)]
pub struct Stage3Params {
    pub width: u32,
    pub height: u32,
    pub iterations: u32,
    pub base_psf_sigma: f32,
    pub epsilon: f32,
    pub deringing_threshold: f32,
    pub shot_r: f32,
    pub shot_g: f32,
    pub shot_b: f32,
    pub read_r: f32,
    pub read_g: f32,
    pub read_b: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage3UniformsGpu {
    width: u32,
    height: u32,
    iterations: u32,
    base_psf_sigma: f32,
    epsilon: f32,
    deringing_threshold: f32,
    shot_r: f32,
    shot_g: f32,
    shot_b: f32,
    read_r: f32,
    read_g: f32,
    read_b: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
}

/// Headless GPU compute pipeline for Stage 3 regularized Richardson-Lucy deconvolution.
pub struct Stage3RlPipeline {
    pipeline_init: wgpu::ComputePipeline,
    pipeline_forward: wgpu::ComputePipeline,
    pipeline_backward: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Stage3RlPipeline {
    /// Compiles Stage 3 shader module and initializes compute pipelines.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage3_rl_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/stage3_rl.wgsl").into()),
            });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("stage3_bind_group_layout"),
                    entries: &[
                        // Binding 0: Uniforms
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
                        // Binding 1: i_0 (storage, read-only)
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
                        // Binding 2: current (storage, read_write)
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Binding 3: ratio_buf (storage, read_write)
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
                        // Binding 4: bounds_buf (storage, read_write)
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Binding 5: m_edge_buf (storage, read_write)
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
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
                    label: Some("stage3_pipeline_layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline_init =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("stage3_init_pipeline"),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some("init_bounds_mask"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                });

        let pipeline_forward =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("stage3_forward_pipeline"),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some("forward_convolve_and_ratio"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                });

        let pipeline_backward =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("stage3_backward_pipeline"),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some("backward_convolve_and_update"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                });

        Ok(Self {
            pipeline_init,
            pipeline_forward,
            pipeline_backward,
            bind_group_layout,
        })
    }

    /// Executes Stage 3 regularized Richardson-Lucy capture sharpening on the GPU.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        rgb_in: &[f32],
        params: &Stage3Params,
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

        let num_pixels = (params.width as usize).saturating_mul(params.height as usize);

        // Clamped initial non-negative buffer
        let clamped_in: Vec<f32> = rgb_in
            .iter()
            .map(|&v| if v.is_finite() && v > 0.0 { v } else { 0.0 })
            .collect();

        if params.iterations == 0 {
            return Ok(clamped_in);
        }

        let uniforms = Stage3UniformsGpu {
            width: params.width,
            height: params.height,
            iterations: params.iterations,
            base_psf_sigma: params.base_psf_sigma,
            epsilon: params.epsilon,
            deringing_threshold: params.deringing_threshold,
            shot_r: params.shot_r,
            shot_g: params.shot_g,
            shot_b: params.shot_b,
            read_r: params.read_r,
            read_g: params.read_g,
            read_b: params.read_b,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
        };

        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage3_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let i_0_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage3_i0_buffer"),
                contents: bytemuck::cast_slice(&clamped_in),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let current_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage3_current_buffer"),
                contents: bytemuck::cast_slice(&clamped_in),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            });

        let ratio_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage3_ratio_buffer"),
            size: (count * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let bounds_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage3_bounds_buffer"),
            size: (num_pixels * 6 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let m_edge_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage3_medge_buffer"),
            size: (count * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage3_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: i_0_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: current_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: ratio_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: bounds_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: m_edge_buffer.as_entire_binding(),
                },
            ],
        });

        let staging_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage3_staging_buffer"),
            size: (count * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage3_command_encoder"),
            });

        let workgroups_x = params.width.div_ceil(16);
        let workgroups_y = params.height.div_ceil(16);

        // Pass 1: Compute bounds and edge regularizer mask
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stage3_init_pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.pipeline_init);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        // Pass 2: Iterative Richardson-Lucy deconvolution
        for _ in 0..params.iterations {
            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("stage3_forward_pass"),
                    timestamp_writes: None,
                });
                compute_pass.set_pipeline(&self.pipeline_forward);
                compute_pass.set_bind_group(0, &bind_group, &[]);
                compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
            }
            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("stage3_backward_pass"),
                    timestamp_writes: None,
                });
                compute_pass.set_pipeline(&self.pipeline_backward);
                compute_pass.set_bind_group(0, &bind_group, &[]);
                compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
            }
        }

        encoder.copy_buffer_to_buffer(
            &current_buffer,
            0,
            &staging_buffer,
            0,
            (count * std::mem::size_of::<f32>()) as u64,
        );

        ctx.queue().submit(Some(encoder.finish()));

        // Readback from staging buffer
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
