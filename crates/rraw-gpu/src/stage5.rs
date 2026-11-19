use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use rraw_color::effective_laplacian_octaves;
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Parameters for Stage 5 GPU colorimetry and Laplacian pyramid local contrast.
#[derive(Debug, Clone)]
pub struct Stage5Params {
    pub width: u32,
    pub height: u32,
    pub mipmap_level: u8,
    pub base_octaves: u8,
    pub local_contrast_strength: f32,
    pub tone_curves_lut: Option<Box<[[f32; 4]; 1024]>>,
}

impl Stage5Params {
    /// Constructs parameters with tone curves disabled.
    #[must_use]
    pub const fn new(
        width: u32,
        height: u32,
        mipmap_level: u8,
        base_octaves: u8,
        local_contrast_strength: f32,
    ) -> Self {
        Self {
            width,
            height,
            mipmap_level,
            base_octaves,
            local_contrast_strength,
            tone_curves_lut: None,
        }
    }

    /// Attaches precomputed 1D tone curves LUT for direct GPU VRAM evaluation.
    #[must_use]
    pub fn with_tone_curves(mut self, lut: Box<[[f32; 4]; 1024]>) -> Self {
        self.tone_curves_lut = Some(lut);
        self
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage5UniformsGpu {
    width: u32,
    height: u32,
    mipmap_level: u32,
    effective_octaves: u32,
    local_contrast_strength: f32,
    enable_tone_curves: u32,
    _pad0: f32,
    _pad1: f32,
}

/// Headless GPU compute pipeline for Stage 5.
pub struct Stage5Pipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Stage5Pipeline {
    /// Compiles Stage 5 shader and initializes compute pipeline.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage5_color_laplacian_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("shaders/stage5_color_laplacian.wgsl").into(),
                ),
            });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("stage5_bind_group_layout"),
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
                    label: Some("stage5_pipeline_layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline = ctx
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stage5_compute_pipeline"),
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

    /// Executes Stage 5 Laplacian pyramid on the GPU.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        rgb_in: &[f32],
        params: &Stage5Params,
    ) -> Result<Vec<f32>, GpuError> {
        let count = (params.width * params.height * 3) as usize;
        if rgb_in.len() != count {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: rgb_in.len(),
            });
        }

        let effective_octaves =
            effective_laplacian_octaves(params.base_octaves, params.mipmap_level);

        let (enable_tone_curves, lut_bytes): (u32, &[u8]) = match params.tone_curves_lut.as_ref() {
            Some(lut) => (1, bytemuck::cast_slice(lut.as_slice())),
            None => (0, bytemuck::cast_slice(&[0.0_f32; 4096])),
        };

        let uniforms = Stage5UniformsGpu {
            width: params.width,
            height: params.height,
            mipmap_level: u32::from(params.mipmap_level),
            effective_octaves: u32::from(effective_octaves),
            local_contrast_strength: params.local_contrast_strength,
            enable_tone_curves,
            _pad0: 0.0,
            _pad1: 0.0,
        };

        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage5_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let input_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage5_rgb_input_buffer"),
                contents: bytemuck::cast_slice(rgb_in),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let lut_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage5_tone_curves_lut_buffer"),
                contents: lut_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });

        let buffer_size = (count * std::mem::size_of::<f32>()) as u64;

        let output_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage5_rgb_output_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage5_staging_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage5_bind_group"),
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
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: lut_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage5_encoder"),
            });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stage5_compute_pass"),
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
