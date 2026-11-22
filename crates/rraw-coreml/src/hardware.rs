#![deny(unsafe_code)]

//! Hardware detection and compute unit management for Apple CoreML and ANE.

use std::fmt;

/// Apple CoreML compute units selector determining target execution hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreMlComputeUnits {
    /// Allows all compute units including Apple Neural Engine (ANE), GPU, and CPU.
    All,
    /// Bypasses GPU to preserve 100% Metal/WGPU headroom for rendering, routing to ANE + CPU.
    CpuAndNeuralEngine,
    /// Standard GPU + CPU fallback.
    CpuAndGpu,
    /// High-precision CPU only fallback.
    CpuOnly,
}

impl fmt::Display for CoreMlComputeUnits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::All => "All (ANE + GPU + CPU)",
            Self::CpuAndNeuralEngine => "CPU + Neural Engine",
            Self::CpuAndGpu => "CPU + GPU",
            Self::CpuOnly => "CPU Only",
        };
        write!(f, "{s}")
    }
}

/// Hardware execution capabilities of the host system for CoreML / ANE inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMlHardwareCapabilities {
    pub device_name: String,
    pub has_ane: bool,
    pub total_ane_cores: Option<u32>,
    pub recommended_compute_units: CoreMlComputeUnits,
}

impl CoreMlHardwareCapabilities {
    /// Detects host hardware capabilities and optimal CoreML execution targets.
    #[must_use]
    pub fn detect() -> Self {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            Self::detect_apple_silicon()
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            Self::detect_fallback()
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn detect_apple_silicon() -> Self {
        let brand = query_sysctl_by_name("machdep.cpu.brand_string")
            .or_else(|| query_sysctl_by_name("hw.model"))
            .unwrap_or_else(|| "Apple Silicon (ARM64)".to_string());

        Self {
            device_name: brand,
            has_ane: true,
            total_ane_cores: Some(16),
            recommended_compute_units: CoreMlComputeUnits::CpuAndNeuralEngine,
        }
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    fn detect_fallback() -> Self {
        Self {
            device_name: "Generic CPU / Emulated Host".to_string(),
            has_ane: false,
            total_ane_cores: None,
            recommended_compute_units: CoreMlComputeUnits::CpuOnly,
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
extern "C" {
    fn sysctlbyname(
        name: *const std::ffi::c_char,
        oldp: *mut std::ffi::c_void,
        oldlenp: *mut usize,
        newp: *mut std::ffi::c_void,
        newlen: usize,
    ) -> std::ffi::c_int;
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn query_sysctl_by_name(name: &str) -> Option<String> {
    use std::ffi::CString;

    let c_name = CString::new(name).ok()?;
    let mut len: usize = 0;

    // SAFETY:
    // First invocation queries required buffer capacity by passing NULL for oldp.
    // c_name is guaranteed to be a valid, NUL-terminated C string.
    #[allow(unsafe_code)]
    let ret = unsafe {
        sysctlbyname(
            c_name.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };

    if ret != 0 || len == 0 {
        return None;
    }

    let mut buf = vec![0u8; len];

    // SAFETY:
    // Second invocation populates `buf` up to capacity `len`.
    // buf allocation is strictly sized according to the previous sysctl call.
    #[allow(unsafe_code)]
    let ret = unsafe {
        sysctlbyname(
            c_name.as_ptr(),
            buf.as_mut_ptr().cast::<std::ffi::c_void>(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };

    if ret != 0 {
        return None;
    }

    // Strip trailing null characters if present
    while let Some(&0) = buf.last() {
        buf.pop();
    }

    String::from_utf8(buf).ok()
}
