#![forbid(unsafe_code)]

//! Disk-backed thumbnail cache for fast filmstrip and grid browsing.

use crate::error::StorageError;
use std::fs;
use std::path::{Path, PathBuf};

/// Mipmap level resolution for cached thumbnails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize {
    /// 160px longest edge (fast filmstrip preview).
    Size160,
    /// 320px longest edge (grid thumbnail).
    Size320,
}

impl ThumbnailSize {
    /// Dimension in pixels along the longest edge.
    #[must_use]
    pub const fn pixel_dimension(self) -> u32 {
        match self {
            Self::Size160 => 160,
            Self::Size320 => 320,
        }
    }

    /// File suffix identifying this thumbnail tier.
    #[must_use]
    pub const fn filename_suffix(self) -> &'static str {
        match self {
            Self::Size160 => "_160.bin",
            Self::Size320 => "_320.bin",
        }
    }
}

/// Disk cache manager for storing and retrieving rendered photo thumbnails.
#[derive(Debug, Clone)]
pub struct DiskThumbnailCache {
    cache_dir: PathBuf,
}

impl DiskThumbnailCache {
    /// Initializes thumbnail cache at `cache_dir`, creating the directory if missing.
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self, StorageError> {
        let dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        Ok(Self { cache_dir: dir })
    }

    fn path_for(&self, hash: &str, size: ThumbnailSize) -> PathBuf {
        let mut filename = hash.to_owned();
        filename.push_str(size.filename_suffix());
        self.cache_dir.join(filename)
    }

    /// Stores thumbnail bytes associated with a photo content hash.
    pub fn store_thumbnail(
        &self,
        hash: &str,
        size: ThumbnailSize,
        data: &[u8],
    ) -> Result<(), StorageError> {
        let path = self.path_for(hash, size);
        fs::write(path, data)?;
        Ok(())
    }

    /// Retrieves cached thumbnail bytes if present on disk.
    #[must_use]
    pub fn get_thumbnail(&self, hash: &str, size: ThumbnailSize) -> Option<Vec<u8>> {
        let path = self.path_for(hash, size);
        fs::read(path).ok()
    }
}
