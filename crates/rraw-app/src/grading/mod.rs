#![forbid(unsafe_code)]

//! Modular grading parameter sub-structs and recipes.

pub mod color;
pub mod denoise;
pub mod geometry;
pub mod optics;
pub mod tone;

pub use color::{ColorGradingParams, ColorRecipeParams};
pub use denoise::{default_color_denoise, DenoiseSharpenParams};
pub use geometry::{default_one_f32, GeometryGradingParams, GeometryRecipeParams};
pub use optics::{
    default_100_f32, default_defringe_threshold, default_feather, default_midpoint,
    default_true_bool, OpticsGradingParams,
};
pub use tone::ToneGradingParams;
