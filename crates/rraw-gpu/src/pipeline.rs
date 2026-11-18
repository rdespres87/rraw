use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use rraw_core::{BayerOrigin, CfaPattern, NoiseProfile};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Parameters for Stage 1 GPU sensor preprocessing.
#[derive(Debug, Clone, Copy)]
pub struct Stage1Params {
    pub width: u32,
    pub height: u32,
    pub black_levels: [f32; 4],
    pub white_levels: [f32; 4],
    pub cfa_pattern: CfaPattern,
    pub noise_profile: NoiseProfile,
    pub wavelet_passes: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage1UniformsGpu {
    width: u32,
    height: u32,
    cfa_type: u32,
    wavelet_level: u32,
    black_levels: [f32; 4],
    white_levels: [f32; 4],
    noise_alpha: [f32; 4],
    noise_sigma2: [f32; 4],
}

/// Headless GPU compute pipeline for Stage 1.
pub struct Stage1Pipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Stage1Pipeline {
    /// Compiles Stage 1 shader and initializes compute pipeline.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage1_sensor_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/stage1_sensor.wgsl").into()),
            });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("stage1_bind_group_layout"),
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
                    ],
                });

        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("stage1_pipeline_layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline = ctx
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stage1_compute_pipeline"),
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

    /// Executes Stage 1 sensor normalization and defect repair on the GPU.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        raw_pixels: &[u16],
        params: &Stage1Params,
    ) -> Result<(Vec<f32>, Vec<f32>), GpuError> {
        let count = (params.width * params.height) as usize;
        if raw_pixels.len() != count {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: raw_pixels.len(),
            });
        }

        let cfa_type = match params.cfa_pattern {
            CfaPattern::Bayer(BayerOrigin::Rggb) => 0,
            CfaPattern::Bayer(BayerOrigin::Bggr) => 1,
            CfaPattern::Bayer(BayerOrigin::Gbrg) => 2,
            CfaPattern::Bayer(BayerOrigin::Grbg) => 3,
            CfaPattern::XTrans(_) => 4,
        };

        let raw_u32: Vec<u32> = raw_pixels.iter().map(|&p| u32::from(p)).collect();

        let uniforms = Stage1UniformsGpu {
            width: params.width,
            height: params.height,
            cfa_type,
            wavelet_level: params.wavelet_passes,
            black_levels: params.black_levels,
            white_levels: params.white_levels,
            noise_alpha: [
                params.noise_profile.shot_gain_alpha()[0],
                params.noise_profile.shot_gain_alpha()[1],
                params.noise_profile.shot_gain_alpha()[2],
                0.0,
            ],
            noise_sigma2: [
                params.noise_profile.read_variance_sigma2()[0],
                params.noise_profile.read_variance_sigma2()[1],
                params.noise_profile.read_variance_sigma2()[2],
                0.0,
            ],
        };

        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage1_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let input_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage1_raw_input_buffer"),
                contents: bytemuck::cast_slice(&raw_u32),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let buffer_size = (count * std::mem::size_of::<f32>()) as u64;

        let norm_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage1_norm_output_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let clip_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage1_clip_output_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage1_bind_group"),
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
                    resource: norm_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: clip_buffer.as_entire_binding(),
                },
            ],
        });

        let staging_norm = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage1_staging_norm"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let staging_clip = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage1_staging_clip"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage1_encoder"),
            });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stage1_compute_pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let workgroups_x = params.width.div_ceil(16);
            let workgroups_y = params.height.div_ceil(16);
            cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        encoder.copy_buffer_to_buffer(&norm_buffer, 0, &staging_norm, 0, buffer_size);
        encoder.copy_buffer_to_buffer(&clip_buffer, 0, &staging_clip, 0, buffer_size);

        ctx.queue().submit(Some(encoder.finish()));

        // Readback staging buffers
        let norm_slice = staging_norm.slice(..);
        let clip_slice = staging_clip.slice(..);

        let (tx_norm, rx_norm) = mpsc::channel();
        let (tx_clip, rx_clip) = mpsc::channel();

        norm_slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx_norm.send(res);
        });
        clip_slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx_clip.send(res);
        });

        let _ = ctx.device().poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        rx_norm
            .recv()
            .map_err(|e| GpuError::BufferMapFailed(e.to_string()))?
            .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;

        rx_clip
            .recv()
            .map_err(|e| GpuError::BufferMapFailed(e.to_string()))?
            .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;

        let norm_data = norm_slice
            .get_mapped_range()
            .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;
        let clip_data = clip_slice
            .get_mapped_range()
            .map_err(|e| GpuError::BufferMapFailed(format!("{e:?}")))?;

        let norm_vec: Vec<f32> = bytemuck::cast_slice(&norm_data).to_vec();
        let clip_vec: Vec<f32> = bytemuck::cast_slice(&clip_data).to_vec();

        drop(norm_data);
        drop(clip_data);
        staging_norm.unmap();
        staging_clip.unmap();

        Ok((norm_vec, clip_vec))
    }
}

/// Parameters for Stage 2 GPU demosaicing and highlight reconstruction.
#[derive(Debug, Clone, Copy)]
pub struct Stage2Params {
    pub width: u32,
    pub height: u32,
    pub cfa_pattern: CfaPattern,
    pub wb_multipliers: [f32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Stage2UniformsGpu {
    width: u32,
    height: u32,
    cfa_type: u32,
    _pad: u32,
    wb_multipliers: [f32; 4],
}

/// Headless GPU compute pipeline for Stage 2.
pub struct Stage2Pipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Stage2Pipeline {
    /// Compiles Stage 2 shader and initializes compute pipeline.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage2_demosaic_hlr_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("shaders/stage2_demosaic_hlr.wgsl").into(),
                ),
            });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("stage2_bind_group_layout"),
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
                    label: Some("stage2_pipeline_layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline = ctx
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stage2_compute_pipeline"),
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

    /// Executes Stage 2 Bayer demosaicing and HLR on the GPU.
    pub fn execute(
        &self,
        ctx: &GpuContext,
        cfa: &[f32],
        clip_mask: &[f32],
        params: &Stage2Params,
    ) -> Result<Vec<f32>, GpuError> {
        let count = (params.width * params.height) as usize;
        if cfa.len() != count {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: cfa.len(),
            });
        }
        if clip_mask.len() != count {
            return Err(GpuError::BufferLengthMismatch {
                expected: count,
                actual: clip_mask.len(),
            });
        }

        let cfa_type = match params.cfa_pattern {
            CfaPattern::Bayer(BayerOrigin::Rggb) => 0,
            CfaPattern::Bayer(BayerOrigin::Bggr) => 1,
            CfaPattern::Bayer(BayerOrigin::Gbrg) => 2,
            CfaPattern::Bayer(BayerOrigin::Grbg) => 3,
            CfaPattern::XTrans(_) => 4,
        };

        let uniforms = Stage2UniformsGpu {
            width: params.width,
            height: params.height,
            cfa_type,
            _pad: 0,
            wb_multipliers: [
                params.wb_multipliers[0],
                params.wb_multipliers[1],
                params.wb_multipliers[2],
                1.0,
            ],
        };

        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage2_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let cfa_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage2_cfa_input_buffer"),
                contents: bytemuck::cast_slice(cfa),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let clip_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage2_clip_mask_input_buffer"),
                contents: bytemuck::cast_slice(clip_mask),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let output_size = (count * 3 * std::mem::size_of::<f32>()) as u64;

        let rgb_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage2_rgb_output_buffer"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_rgb = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage2_staging_rgb"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage2_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cfa_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: clip_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: rgb_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage2_encoder"),
            });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stage2_compute_pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let workgroups_x = params.width.div_ceil(16);
            let workgroups_y = params.height.div_ceil(16);
            cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        encoder.copy_buffer_to_buffer(&rgb_buffer, 0, &staging_rgb, 0, output_size);

        ctx.queue().submit(Some(encoder.finish()));

        let slice = staging_rgb.slice(..);
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
        staging_rgb.unmap();

        Ok(rgb_vec)
    }
}
