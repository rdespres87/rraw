use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use rraw_optics::UvLutTable;
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Parameters for Stage 4 GPU optical warp and vignetting.
#[derive(Debug, Clone, Copy)]
pub struct Stage4Params {
    pub width: u32,
    pub height: u32,
    pub apply_radial_vignette: bool,
    pub vignette_v1: f32,
    pub vignette_v2: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage4UniformsGpu {
    width: u32,
    height: u32,
    lut_dimension: u32,
    apply_radial_vignette: u32,
    vignette_v1: f32,
    vignette_v2: f32,
    _pad0: f32,
    _pad1: f32,
}

/// Headless GPU compute pipeline for Stage 4 Catmull-Rom optical warp.
pub struct Stage4WarpPipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Stage4WarpPipeline {
    /// Compiles Stage 4 shader and initializes compute pipeline.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage4_warp_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/stage4_warp.wgsl").into()),
            });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("stage4_bind_group_layout"),
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
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
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
                    label: Some("stage4_pipeline_layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline = ctx
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stage4_compute_pipeline"),
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

    /// Executes Stage 4 Catmull-Rom optical warp on the GPU.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        rgb_in: &[f32],
        lut: &UvLutTable,
        params: &Stage4Params,
    ) -> Result<Vec<f32>, GpuError> {
        let count = (params.width * params.height * 3) as usize;
        if rgb_in.len() != count {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: rgb_in.len(),
            });
        }

        let uniforms = Stage4UniformsGpu {
            width: params.width,
            height: params.height,
            lut_dimension: lut.dimension as u32,
            apply_radial_vignette: if params.apply_radial_vignette { 1 } else { 0 },
            vignette_v1: params.vignette_v1,
            vignette_v2: params.vignette_v2,
            _pad0: 0.0,
            _pad1: 0.0,
        };

        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage4_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let input_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage4_rgb_input_buffer"),
                contents: bytemuck::cast_slice(rgb_in),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let lut_r_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage4_lut_r_buffer"),
                contents: bytemuck::cast_slice(&lut.uv_r),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let lut_g_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage4_lut_g_buffer"),
                contents: bytemuck::cast_slice(&lut.uv_g),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let lut_b_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage4_lut_b_buffer"),
                contents: bytemuck::cast_slice(&lut.uv_b),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let buffer_size = (count * std::mem::size_of::<f32>()) as u64;

        let output_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage4_rgb_output_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage4_staging_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage4_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lut_r_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: lut_g_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: lut_b_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage4_encoder"),
            });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stage4_compute_pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let workgroups_x = params.width.div_ceil(16);
            let workgroups_y = params.height.div_ceil(16);
            cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, buffer_size);
        ctx.queue().submit(Some(encoder.finish()));

        let slice = staging_buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });

        let _ = ctx.device().poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        rx.recv()
            .map_err(|e| GpuError::BufferMapFailed(e.to_string()))?
            .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;

        let mapped = slice
            .get_mapped_range()
            .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;
        let rgb_vec: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging_buffer.unmap();

        Ok(rgb_vec)
    }
}
