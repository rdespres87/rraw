use rraw_gpu::GpuContext;

#[test]
fn test_gpu_context_adapter_limits_support_large_buffers() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(_) => return, // Headless GPU may not be available on some CI runners
    };

    let limits = ctx.device().limits();
    // Verify device was not clamped to default 128 MB storage buffer limit if adapter supports more
    let adapter_limits = ctx.adapter().limits();
    assert_eq!(
        limits.max_storage_buffer_binding_size,
        adapter_limits.max_storage_buffer_binding_size
    );
    assert_eq!(limits.max_buffer_size, adapter_limits.max_buffer_size);
}
