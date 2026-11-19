use crate::context::GpuContext;
use crate::error::GpuError;
use bytemuck::{Pod, Zeroable};
use rraw_color::effective_laplacian_octaves;
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Encodes an `f32` into an IEEE 754 half-precision float (`u16` bits).
#[must_use]
pub fn f32_to_f16(val: f32) -> u16 {
    let bits = val.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x007f_ffff;
    if exp <= 0 {
        sign
    } else if exp >= 31 {
        sign | 0x7c00 | if mant != 0 { 0x0200 } else { 0 }
    } else {
        sign | ((exp as u16) << 10) | ((mant >> 13) as u16)
    }
}

/// Parameters for Stage 5 GPU colorimetry, Laplacian pyramid local contrast, and 3D CLUT.
#[derive(Debug, Clone)]
pub struct Stage5Params {
    pub width: u32,
    pub height: u32,
    pub mipmap_level: u8,
    pub base_octaves: u8,
    pub local_contrast_strength: f32,
    pub tone_curves_lut: Option<Box<[[f32; 4]; 1024]>>,
    pub clut_3d: Option<rraw_color::ColorLut3d>,
    pub clut_3d_strength: f32,
}

impl Stage5Params {
    /// Constructs parameters with tone curves and 3D CLUT disabled.
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
            clut_3d: None,
            clut_3d_strength: 1.0,
        }
    }

    /// Attaches precomputed 1D tone curves LUT for direct GPU VRAM evaluation.
    #[must_use]
    pub fn with_tone_curves(mut self, lut: Box<[[f32; 4]; 1024]>) -> Self {
        self.tone_curves_lut = Some(lut);
        self
    }

    /// Attaches a 3D CLUT with blending strength in `[0.0, 1.0]`.
    #[must_use]
    pub fn with_clut_3d(mut self, clut: rraw_color::ColorLut3d, strength: f32) -> Self {
        self.clut_3d = Some(clut);
        self.clut_3d_strength = strength.clamp(0.0, 1.0);
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
    enable_clut_3d: u32,
    clut_3d_strength: f32,
}

/// Headless GPU compute pipeline for Stage 5.
pub struct Stage5Pipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    default_lut_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D3,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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

        // 2x2x2 identity LUT texture fallback in Rgba16Float
        let default_tex = ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("stage5_default_3d_lut"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 2,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let mut default_bytes = Vec::with_capacity(2 * 2 * 2 * 8);
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    let rf = r as f32;
                    let gf = g as f32;
                    let bf = b as f32;
                    default_bytes.extend_from_slice(&f32_to_f16(rf).to_ne_bytes());
                    default_bytes.extend_from_slice(&f32_to_f16(gf).to_ne_bytes());
                    default_bytes.extend_from_slice(&f32_to_f16(bf).to_ne_bytes());
                    default_bytes.extend_from_slice(&f32_to_f16(1.0).to_ne_bytes());
                }
            }
        }

        ctx.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &default_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &default_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(2 * 8),
                rows_per_image: Some(2),
            },
            wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 2,
            },
        );

        let default_lut_view = default_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = ctx.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("stage5_clut_3d_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            pipeline,
            bind_group_layout,
            default_lut_view,
            sampler,
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

        let (enable_clut_3d, clut_3d_strength, custom_lut_tex) = match params.clut_3d.as_ref() {
            Some(lut) => {
                let size = lut.size() as u32;
                let total_voxels = (size * size * size) as usize;
                let mut f16_bytes = Vec::with_capacity(total_voxels * 8);
                for node in lut.data() {
                    let r = f32_to_f16(node[0]);
                    let g = f32_to_f16(node[1]);
                    let b = f32_to_f16(node[2]);
                    let a = f32_to_f16(1.0);
                    f16_bytes.extend_from_slice(&r.to_ne_bytes());
                    f16_bytes.extend_from_slice(&g.to_ne_bytes());
                    f16_bytes.extend_from_slice(&b.to_ne_bytes());
                    f16_bytes.extend_from_slice(&a.to_ne_bytes());
                }

                let tex = ctx.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("stage5_active_3d_lut"),
                    size: wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: size,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D3,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });

                ctx.queue().write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &f16_bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size * 8),
                        rows_per_image: Some(size),
                    },
                    wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: size,
                    },
                );

                (1u32, params.clut_3d_strength.clamp(0.0, 1.0), Some(tex))
            }
            None => (0u32, 0.0f32, None),
        };

        let custom_view = custom_lut_tex
            .as_ref()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
        let lut_view_ref = match &custom_view {
            Some(view) => view,
            None => &self.default_lut_view,
        };

        let uniforms = Stage5UniformsGpu {
            width: params.width,
            height: params.height,
            mipmap_level: u32::from(params.mipmap_level),
            effective_octaves: u32::from(effective_octaves),
            local_contrast_strength: params.local_contrast_strength,
            enable_tone_curves,
            enable_clut_3d,
            clut_3d_strength,
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(lut_view_ref),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
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
