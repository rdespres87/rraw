#![forbid(unsafe_code)]

//! Catalog-free DAM index contracts and non-destructive `.rraw.json` / `.xmp` sidecar paths.

pub mod dam;
pub mod error;
pub mod thumbnail;
pub mod xmp;

pub use dam::{DamFilter, DamPhotoRecord, SqliteDamIndex, SyncAction};
pub use error::StorageError;
pub use thumbnail::{DiskThumbnailCache, ThumbnailSize};
pub use xmp::XmpEditor;

use std::path::{Path, PathBuf};

use rraw_core::PhotoMetadata;

/// Resolved sidecar paths derived from a read-only RAW source path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarPaths {
    pub raw_source: PathBuf,
    pub recipe_json: PathBuf,
    pub xmp_sidecar: PathBuf,
}

impl SidecarPaths {
    /// Derives `<photo>.rraw.json` and `<photo>.xmp` adjacent to `raw_source` without modifying it.
    #[must_use]
    pub fn from_raw_path(raw_source: &Path) -> Self {
        let mut recipe_os = raw_source.as_os_str().to_os_string();
        recipe_os.push(".rraw.json");
        let recipe_json = PathBuf::from(recipe_os);

        let xmp_sidecar = raw_source.with_extension("xmp");
        Self {
            raw_source: raw_source.to_path_buf(),
            recipe_json,
            xmp_sidecar,
        }
    }
}

/// Lightweight DAM summary row projected from canonical `PhotoMetadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamIndexSummary {
    pub camera_model: String,
    pub rating: u8,
    pub keyword_count: usize,
}

impl DamIndexSummary {
    /// Projects canonical metadata into an index summary row.
    #[must_use]
    pub fn from_metadata(meta: &PhotoMetadata) -> Self {
        Self {
            camera_model: meta.camera_model.clone(),
            rating: meta.rating.min(5),
            keyword_count: meta.keywords.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_core::{
        BayerOrigin, CfaPattern, ColorLabel, CoreError, NoiseProfile, NoiseProfileSource,
        PickStatus,
    };

    #[test]
    fn sidecar_paths_preserve_raw_source_and_append_expected_extensions() {
        let raw = Path::new("/Volumes/Photos/DSC0001.ARW");
        let paths = SidecarPaths::from_raw_path(raw);
        assert_eq!(paths.raw_source, raw);
        assert_eq!(
            paths.recipe_json,
            Path::new("/Volumes/Photos/DSC0001.ARW.rraw.json")
        );
        assert_eq!(paths.xmp_sidecar, Path::new("/Volumes/Photos/DSC0001.xmp"));

        let extensionless = Path::new("/Volumes/Photos/RAW_0042");
        let extless_paths = SidecarPaths::from_raw_path(extensionless);
        assert_eq!(extless_paths.raw_source, extensionless);
        assert_eq!(
            extless_paths.recipe_json,
            Path::new("/Volumes/Photos/RAW_0042.rraw.json")
        );
        assert_eq!(
            extless_paths.xmp_sidecar,
            Path::new("/Volumes/Photos/RAW_0042.xmp")
        );
    }

    #[test]
    fn dam_summary_clamps_rating_and_counts_keywords() -> Result<(), CoreError> {
        let mut meta = PhotoMetadata {
            camera_make: "Fujifilm".to_owned(),
            camera_model: "X-T5".to_owned(),
            lens_model: None,
            focal_length_mm: None,
            aperture_f_number: None,
            iso: Some(160),
            width: 7728,
            height: 5152,
            black_levels: [1024.0; 4],
            white_levels: [16383.0; 4],
            cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
            noise_profile: NoiseProfile::new(
                [0.001; 3],
                [0.0001; 3],
                NoiseProfileSource::CameraTable,
            )?,
            vignette_handled_by_gainmap: false,
            dng_warp_rectilinear: None,
            rating: 5,
            pick: PickStatus::Pick,
            label: ColorLabel::Green,
            keywords: vec!["landscape".to_owned(), "mountain".to_owned()],
        };
        let summary = DamIndexSummary::from_metadata(&meta);
        assert_eq!(summary.camera_model, "X-T5");
        assert_eq!(summary.rating, 5);
        assert_eq!(summary.keyword_count, 2);

        meta.rating = 255;
        let clamped = DamIndexSummary::from_metadata(&meta);
        assert_eq!(clamped.rating, 5);
        Ok(())
    }
}
