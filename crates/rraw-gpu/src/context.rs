use crate::error::GpuError;

/// GPU execution context encapsulating `wgpu` adapter, device, and queue.
#[derive(Debug)]
pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuContext {
    /// Creates a new headless GPU context without an interactive display surface.
    ///
    /// Selects the highest performance available adapter (e.g. Metal on Apple Silicon).
    pub fn new_headless() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::default();

        let adapter_res =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            }))
            .or_else(|_| {
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                    apply_limit_buckets: false,
                }))
            });

        let adapter = adapter_res.map_err(|_| GpuError::AdapterNotFound)?;
        let adapter_limits = adapter.limits();

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rraw-headless-gpu-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                max_buffer_size: adapter_limits.max_buffer_size,
                max_storage_buffer_binding_size: adapter_limits.max_storage_buffer_binding_size,
                max_texture_dimension_2d: adapter_limits.max_texture_dimension_2d,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| GpuError::DeviceRequestFailed(e.to_string()))?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// Returns a reference to the active `wgpu::Instance`.
    #[must_use]
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// Returns a reference to the active `wgpu::Device`.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Returns a reference to the active `wgpu::Queue`.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Returns a reference to the active `wgpu::Adapter`.
    #[must_use]
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }
}
