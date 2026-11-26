#![forbid(unsafe_code)]

//! EXIF orientation transformations, TIFF tag parsing and directory scanning utilities.

use std::path::{Path, PathBuf};

/// Returns upright `(width, height)` after applying EXIF orientation (`5..=8` transpose dimensions).
#[must_use]
pub const fn oriented_dimensions(width: u32, height: u32, orientation: u16) -> (u32, u32) {
    if matches!(orientation, 5..=8) {
        (height, width)
    } else {
        (width, height)
    }
}

/// Transforms a row-major RGB buffer according to standard EXIF Orientation (`1..=8`).
#[must_use]
pub fn apply_exif_orientation_rgb(
    samples: &[[f32; 3]],
    width: usize,
    height: usize,
    orientation: u16,
) -> (usize, usize, Vec<[f32; 3]>) {
    if width == 0 || height == 0 || samples.len() < width.saturating_mul(height) {
        return (width, height, samples.to_vec());
    }
    match orientation {
        2 => {
            // Mirror Horizontal
            let mut out = vec![[0.0_f32; 3]; width * height];
            for y in 0..height {
                for x in 0..width {
                    out[y * width + x] = samples[y * width + (width - 1 - x)];
                }
            }
            (width, height, out)
        }
        3 => {
            // Rotate 180°
            let mut out = vec![[0.0_f32; 3]; width * height];
            for y in 0..height {
                for x in 0..width {
                    out[y * width + x] = samples[(height - 1 - y) * width + (width - 1 - x)];
                }
            }
            (width, height, out)
        }
        4 => {
            // Mirror Vertical
            let mut out = vec![[0.0_f32; 3]; width * height];
            for y in 0..height {
                for x in 0..width {
                    out[y * width + x] = samples[(height - 1 - y) * width + x];
                }
            }
            (width, height, out)
        }
        5 => {
            // Transpose: Mirror Horizontal + Rotate 270° CW -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    out[dy * out_w + dx] = samples[dx * width + dy];
                }
            }
            (out_w, out_h, out)
        }
        6 => {
            // Rotate 90° CW (Standard Portrait) -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    let sx = dy;
                    let sy = height - 1 - dx;
                    out[dy * out_w + dx] = samples[sy * width + sx];
                }
            }
            (out_w, out_h, out)
        }
        7 => {
            // Transverse: Mirror Horizontal + Rotate 90° CW -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    let sx = width - 1 - dy;
                    let sy = height - 1 - dx;
                    out[dy * out_w + dx] = samples[sy * width + sx];
                }
            }
            (out_w, out_h, out)
        }
        8 => {
            // Rotate 270° CW / 90° CCW (Portrait) -> (out_w, out_h) = (height, width)
            let out_w = height;
            let out_h = width;
            let mut out = vec![[0.0_f32; 3]; out_w * out_h];
            for dy in 0..out_h {
                for dx in 0..out_w {
                    let sx = width - 1 - dy;
                    let sy = dx;
                    out[dy * out_w + dx] = samples[sy * width + sx];
                }
            }
            (out_w, out_h, out)
        }
        _ => (width, height, samples.to_vec()),
    }
}

/// Applies EXIF Orientation (`1..=8`) to a `DynamicImage`.
#[must_use]
pub fn apply_exif_orientation_dyn_image(
    img: image::DynamicImage,
    orientation: u16,
) -> image::DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate270().fliph(),
        6 => img.rotate90(),
        7 => img.rotate90().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

#[must_use]
pub fn parse_tiff_ifd0_orientation(tiff: &[u8]) -> Option<u16> {
    if tiff.len() < 8 {
        return None;
    }
    let is_le = match &tiff[0..4] {
        [b'I', b'I', 42, 0] => true,
        [b'M', b'M', 0, 42] => false,
        _ => return None,
    };
    let read_u16 = |off: usize| -> Option<u16> {
        let slice = tiff.get(off..off + 2)?;
        Some(if is_le {
            u16::from_le_bytes([slice[0], slice[1]])
        } else {
            u16::from_be_bytes([slice[0], slice[1]])
        })
    };
    let read_u32 = |off: usize| -> Option<u32> {
        let slice = tiff.get(off..off + 4)?;
        Some(if is_le {
            u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
        } else {
            u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
        })
    };

    let mut ifd_offset = read_u32(4)? as usize;
    let mut ifd_count = 0;

    while ifd_offset != 0 && ifd_count < 8 {
        ifd_count += 1;
        let entry_count = read_u16(ifd_offset)? as usize;
        let entries_base = ifd_offset.checked_add(2)?;

        let mut exif_subifd_offset = None;

        for i in 0..entry_count.min(128) {
            let entry_off = entries_base.checked_add(i * 12)?;
            let tag = read_u16(entry_off)?;
            if tag == 0x0112 {
                let val = read_u16(entry_off + 8)?;
                if (1..=8).contains(&val) {
                    return Some(val);
                }
            } else if tag == 0x8769 {
                exif_subifd_offset = read_u32(entry_off + 8);
            }
        }

        // Traverse Exif SubIFD (tag 0x8769) if orientation was not found in this IFD
        if let Some(sub_off) = exif_subifd_offset {
            let sub_off = sub_off as usize;
            if let Some(sub_count) = read_u16(sub_off) {
                if let Some(sub_base) = sub_off.checked_add(2) {
                    for i in 0..(sub_count as usize).min(128) {
                        if let Some(entry_off) = sub_base.checked_add(i * 12) {
                            if let Some(tag) = read_u16(entry_off) {
                                if tag == 0x0112 {
                                    if let Some(val) = read_u16(entry_off + 8) {
                                        if (1..=8).contains(&val) {
                                            return Some(val);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Offset to next IFD in chain
        if let Some(next_off_pos) = entries_base.checked_add(entry_count * 12) {
            ifd_offset = read_u32(next_off_pos).unwrap_or(0) as usize;
        } else {
            break;
        }
    }

    None
}

/// Canonical list of RAW sensor file extensions supported by `rraw-decode`.
pub const RAW_EXTENSIONS: &[&str] = &[
    "dng", "arw", "cr2", "cr3", "crw", "nef", "nrw", "raf", "rw2", "rwl", "orf", "ori", "pef",
    "iiq", "srw", "mrw", "3fr", "fff", "kdc", "dcr", "mos", "erf", "ari", "arq", "mef", "sr2",
    "srf", "raw", "gpr", "x3f",
];

/// Canonical list of all image extensions (RAW + raster) accepted by file dialogs and folder scans.
pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
    "dng", "arw", "cr2", "cr3", "crw", "nef", "nrw", "raf", "rw2", "rwl", "orf", "ori", "pef",
    "iiq", "srw", "mrw", "3fr", "fff", "kdc", "dcr", "mos", "erf", "ari", "arq", "mef", "sr2",
    "srf", "raw", "gpr", "x3f", "jpg", "jpeg", "png", "tif", "tiff",
];

/// Checks if the given file extension corresponds to a RAW sensor format.
#[must_use]
pub fn is_raw_extension(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    RAW_EXTENSIONS.contains(&lower.as_str())
}

/// Reads the EXIF `Orientation` tag (`0x0112`, values `1..=8`) from a JPEG or TIFF/RAW file header.
#[must_use]
pub fn read_exif_orientation(path: &Path) -> u16 {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return 1;
    };
    let mut buf = vec![0_u8; 262_144];
    let Ok(n) = file.read(&mut buf) else {
        return 1;
    };
    let data = &buf[..n];
    if data.len() < 4 {
        return 1;
    }

    // 1. Direct TIFF / DNG / RAW container
    if let Some(orient) = parse_tiff_ifd0_orientation(data) {
        return orient;
    }

    // 2. JPEG container (0xFFD8 followed by APP markers)
    if data[0] == 0xFF && data[1] == 0xD8 {
        let mut pos = 2_usize;
        while pos + 4 <= data.len() && data[pos] == 0xFF {
            let marker = data[pos + 1];
            if marker == 0xDA || marker == 0xD9 {
                break;
            }
            let seg_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
            if seg_len < 2 || pos + 2 + seg_len > data.len() {
                break;
            }
            if marker == 0xE1 && seg_len >= 8 {
                let payload = &data[pos + 4..pos + 2 + seg_len];
                if payload.starts_with(b"Exif\0\0") {
                    if let Some(orient) = parse_tiff_ifd0_orientation(&payload[6..]) {
                        return orient;
                    }
                }
            }
            pos += 2 + seg_len;
        }
    }

    1
}

/// Scans a filesystem directory for supported RAW and raster image files.
#[must_use]
pub fn scan_directory_for_images(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_ascii_lowercase();
                if SUPPORTED_IMAGE_EXTENSIONS.contains(&ext_lower.as_str()) {
                    entries.push(path);
                }
            }
        }
    }

    entries.sort();
    entries
}

/// Re-scans `dir` and updates `current_entries` in-place if files were added or removed.
/// Returns `true` when the list of images changed.
pub fn sync_directory_entries(dir: &Path, current_entries: &mut Vec<PathBuf>) -> bool {
    let latest = scan_directory_for_images(dir);
    if latest != *current_entries {
        *current_entries = latest;
        true
    } else {
        false
    }
}
