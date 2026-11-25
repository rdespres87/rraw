use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Parameters for Stage 7 GPU AgX display tone mapping and scopes.
#[derive(Debug, Clone, Copy)]
pub struct Stage7Params {
    pub width: u32,
    pub height: u32,
    pub headroom: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage7UniformsGpu {
    width: u32,
    height: u32,
    headroom: f32,
    _pad: f32,
}

/// Device-resident storage buffers bound by a Stage 7 compute dispatch.
pub struct Stage7IoBuffers<'a> {
    pub input: &'a wgpu::Buffer,
    pub output: &'a wgpu::Buffer,
    pub histogram: &'a wgpu::Buffer,
    pub clipping: &'a wgpu::Buffer,
}

/// Headless GPU compute pipeline for Stage 7.
pub struct Stage7Pipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Stage7Pipeline {
    /// Compiles Stage 7 shader and initializes compute pipeline.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage7_display_scopes_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("shaders/stage7_display_scopes.wgsl").into(),
                ),
            });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("stage7_bind_group_layout"),
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
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
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
                    label: Some("stage7_pipeline_layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline = ctx
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stage7_compute_pipeline"),
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

    /// Encodes Stage 7 AgX tone mapping and scope reductions directly into `encoder`
    /// using device-resident buffers (zero intermediate CPU readback).
    pub fn encode_into(
        &self,
        ctx: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        buffers: Stage7IoBuffers<'_>,
        params: &Stage7Params,
    ) {
        let uniforms = Stage7UniformsGpu {
            width: params.width,
            height: params.height,
            headroom: params.headroom,
            _pad: 0.0,
        };

        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage7_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage7_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffers.output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: buffers.histogram.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: buffers.clipping.as_entire_binding(),
                },
            ],
        });

        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("stage7_compute_pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        let workgroups_x = params.width.div_ceil(16);
        let workgroups_y = params.height.div_ceil(16);
        cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
    }

    /// Executes Stage 7 AgX tone mapping and scope reductions on the GPU.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        rgb_in: &[f32],
        params: &Stage7Params,
    ) -> Result<(Vec<f32>, Vec<u32>, bool), GpuError> {
        let count = (params.width * params.height * 3) as usize;
        if rgb_in.len() != count {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: rgb_in.len(),
            });
        }

        let input_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage7_rgb_input_buffer"),
                contents: bytemuck::cast_slice(rgb_in),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let buffer_size = (count * std::mem::size_of::<f32>()) as u64;

        let output_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage7_rgb_output_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let hist_zeros = [0_u32; 256];
        let hist_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage7_hist_buffer"),
                contents: bytemuck::cast_slice(&hist_zeros),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });

        let clip_zeros = [0_u32; 1];
        let clip_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage7_clip_alert_buffer"),
                contents: bytemuck::cast_slice(&clip_zeros),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });

        let staging_output = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage7_staging_output"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let staging_hist = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage7_staging_hist"),
            size: (256 * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let staging_clip = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage7_staging_clip"),
            size: std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage7_encoder"),
            });

        self.encode_into(
            ctx,
            &mut encoder,
            Stage7IoBuffers {
                input: &input_buffer,
                output: &output_buffer,
                histogram: &hist_buffer,
                clipping: &clip_buffer,
            },
            params,
        );

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_output, 0, buffer_size);
        encoder.copy_buffer_to_buffer(
            &hist_buffer,
            0,
            &staging_hist,
            0,
            (256 * std::mem::size_of::<u32>()) as u64,
        );
        encoder.copy_buffer_to_buffer(
            &clip_buffer,
            0,
            &staging_clip,
            0,
            std::mem::size_of::<u32>() as u64,
        );

        ctx.queue().submit(Some(encoder.finish()));

        // Map all 3 staging buffers
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

        let rgb_vec: Vec<f32> = bytemuck::cast_slice(&out_mapped).to_vec();
        let hist_vec: Vec<u32> = bytemuck::cast_slice(&hist_mapped).to_vec();
        let clip_val: Vec<u32> = bytemuck::cast_slice(&clip_mapped).to_vec();

        drop(out_mapped);
        drop(hist_mapped);
        drop(clip_mapped);

        staging_output.unmap();
        staging_hist.unmap();
        staging_clip.unmap();

        let has_clipping = clip_val.first().is_some_and(|&v| v > 0);

        Ok((rgb_vec, hist_vec, has_clipping))
    }
}
