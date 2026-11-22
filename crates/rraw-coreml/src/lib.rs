#![deny(unsafe_code)]

//! Isolated macOS CoreML and Apple Neural Engine (ANE) hardware bridge.
//!
//! This crate provides safe, zero-overhead Rust bindings to Apple CoreML and the
//! Apple Neural Engine (ANE) for 504x504 CFA RAW tile denoising and 4-class semantic
//! segmentation.
//!
//! Like `rraw-macos-edr`, this crate uses `#![deny(unsafe_code)]` with targeted,
//! strictly audited `#[allow(unsafe_code)]` and `// SAFETY:` proofs for low-level
//! system calls (such as sysctl hardware detection and Objective-C CoreML invocation).

pub mod denoiser;
pub mod descriptor;
pub mod engine;
pub mod hardware;
pub mod segmentation;

pub use denoiser::AneRawDenoiser;
pub use descriptor::{CoreMlError, CoreMlModelDescriptor};
pub use engine::CoreMlEngine;
pub use hardware::{CoreMlComputeUnits, CoreMlHardwareCapabilities};
pub use segmentation::AneSemanticSegmenter;
