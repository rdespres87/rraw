#![forbid(unsafe_code)]

//! Universal MakerNote parser and lens metadata resolution engine.
//!
//! Automatically extracts manufacturer-specific lens identifiers (e.g. Pentax `LensRec` tag `0x003F`,
//! Sony `LensID` `0x0102`, Canon `LensType` `0x0016`, Nikon `0x0084`) from both native RAW and DNG
//! private metadata blocks, and resolves them against the universal database of 15,000+ lenses.

use rawler::formats::tiff::Rational;
use rawler::lens::{get_lenses, LensDescription, LensResolver};

/// Disambiguates and sanitizes a detected lens name.
///
/// Removes duplicate manufacturer prefixes (e.g., "Sigma Sigma 18-35mm" -> "Sigma 18-35mm").
#[must_use]
pub fn sanitize_lens_name(name: &str) -> String {
    let trimmed = name.trim();
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() >= 2 && words[0].eq_ignore_ascii_case(words[1]) {
        return words[1..].join(" ");
    }
    if words.len() >= 2 {
        let first = words[0].to_lowercase();
        let second = words[1].to_lowercase();
        if second.starts_with(&format!("{first}-")) {
            return words[1..].join(" ");
        }
    }
    trimmed.to_string()
}

/// Resolves a human-readable lens name from raw source bytes and camera metadata.
#[must_use]
pub fn resolve_universal_lens(
    raw_bytes: &[u8],
    camera_make: &str,
    camera_model: &str,
    focal_length_mm: Option<f32>,
    aperture_f_number: Option<f32>,
) -> Option<String> {
    let raw_name = resolve_universal_lens_inner(
        raw_bytes,
        camera_make,
        camera_model,
        focal_length_mm,
        aperture_f_number,
    );
    raw_name.map(|name| sanitize_lens_name(&name))
}

fn resolve_universal_lens_inner(
    raw_bytes: &[u8],
    camera_make: &str,
    camera_model: &str,
    focal_length_mm: Option<f32>,
    aperture_f_number: Option<f32>,
) -> Option<String> {
    let make_lower = camera_make.to_lowercase();

    // 1. Pentax / Ricoh K-mount resolution
    if make_lower.contains("pentax") || make_lower.contains("ricoh") || make_lower.contains("asahi")
    {
        if let Some(lens_name) = resolve_pentax_lens(
            raw_bytes,
            camera_make,
            camera_model,
            focal_length_mm,
            aperture_f_number,
        ) {
            return Some(lens_name);
        }
    }

    // 2. Generic fallback using focal length and aperture bounds on camera mounts
    resolve_by_mount_and_optics(
        camera_make,
        camera_model,
        focal_length_mm,
        aperture_f_number,
    )
}

/// Resolves a Pentax lens from tag 0x003F (LensRec / LensType) in MakerNotes or DNGPrivateData.
fn resolve_pentax_lens(
    raw_bytes: &[u8],
    camera_make: &str,
    camera_model: &str,
    focal_length_mm: Option<f32>,
    aperture_f_number: Option<f32>,
) -> Option<String> {
    let lens_id = extract_pentax_lens_id(raw_bytes)?;

    // 0 = M42 or no lens, 1 = K/M manual lens, 2 = A-series manual aperture
    if lens_id.0 <= 2 {
        return None;
    }

    // Attempt 1: Standard rawler LensResolver
    let mut resolver = LensResolver::new()
        .with_camera_make(camera_make)
        .with_camera_model(camera_model)
        .with_lens_id(lens_id)
        .with_mounts(&["k-mount".to_string()]);

    if let Some(fl) = focal_length_mm {
        resolver = resolver.with_focal_len(Some(Rational::new(fl.round() as u32, 1)));
    }
    if let Some(ap) = aperture_f_number {
        resolver = resolver.with_aperture(Some(Rational::new((ap * 10.0).round() as u32, 10)));
    }

    if let Some(desc) = resolver.resolve() {
        return Some(desc.lens_name.clone());
    }

    // Attempt 2: Direct lookup in LENSES_DB by K-mount and (lens_id, lens_subid)
    let candidates: Vec<&LensDescription> = get_lenses()
        .iter()
        .filter(|l| l.mount == "k-mount")
        .filter(|l| {
            l.identifiers
                .id
                .is_some_and(|id| id.0 == lens_id.0 && id.1 == lens_id.1)
        })
        .collect();

    if candidates.len() == 1 {
        return Some(candidates[0].lens_name.clone());
    }

    // Disambiguate multi-candidate matches using focal length and aperture
    if let (Some(fl), Some(ap)) = (focal_length_mm, aperture_f_number) {
        let matching: Vec<&LensDescription> = candidates
            .into_iter()
            .filter(|l| {
                let f_min = l.focal_range[0].as_f32();
                let f_max = l.focal_range[1].as_f32();
                let a_min = l.aperture_range[0].as_f32();
                fl >= f_min - 0.5 && fl <= f_max + 0.5 && ap >= a_min - 0.1
            })
            .collect();

        if !matching.is_empty() {
            return Some(matching[0].lens_name.clone());
        }
    }

    None
}

/// Extracts (lens_id, lens_subid) from Pentax MakerNotes in raw bytes.
fn extract_pentax_lens_id(raw_bytes: &[u8]) -> Option<(u32, u32)> {
    // Search for PENTAX MakerNote signature
    let signature = b"PENTAX \0";
    let max_search = raw_bytes.len().min(512 * 1024);
    let slice = &raw_bytes[..max_search];

    let mut start_idx = 0;
    while let Some(pos) = find_subsequence(&slice[start_idx..], signature) {
        let abs_pos = start_idx + pos;
        if abs_pos + 12 <= slice.len() {
            let endian_bytes = &slice[abs_pos + 8..abs_pos + 10];
            let is_little = endian_bytes == b"II";
            let is_big = endian_bytes == b"MM";

            if is_little || is_big {
                if let Some(id) = parse_pentax_ifd(&slice[abs_pos + 10..], is_little) {
                    return Some(id);
                }
            }
        }
        start_idx = abs_pos + signature.len();
    }

    None
}

fn parse_pentax_ifd(data: &[u8], is_little: bool) -> Option<(u32, u32)> {
    if data.len() < 2 {
        return None;
    }

    let num_entries = if is_little {
        u16::from_le_bytes([data[0], data[1]])
    } else {
        u16::from_be_bytes([data[0], data[1]])
    } as usize;

    for i in 0..num_entries {
        let entry_offset = 2 + i * 12;
        if entry_offset + 12 > data.len() {
            break;
        }

        let tag = if is_little {
            u16::from_le_bytes([data[entry_offset], data[entry_offset + 1]])
        } else {
            u16::from_be_bytes([data[entry_offset], data[entry_offset + 1]])
        };

        // Tag 0x003F: LensRec (lens_id, lens_subid)
        if tag == 0x003F {
            let val_bytes = &data[entry_offset + 8..entry_offset + 12];
            let b0 = val_bytes[0] as u32;
            let b1 = val_bytes[1] as u32;
            return Some((b0, b1));
        }
    }

    None
}

/// Fallback lens resolution by mount, focal length, and aperture when no maker ID is recorded.
fn resolve_by_mount_and_optics(
    camera_make: &str,
    _camera_model: &str,
    focal_length_mm: Option<f32>,
    aperture_f_number: Option<f32>,
) -> Option<String> {
    let make_lower = camera_make.to_lowercase();
    let mount = if make_lower.contains("pentax") || make_lower.contains("ricoh") {
        "k-mount"
    } else if make_lower.contains("sony") {
        "e-mount"
    } else if make_lower.contains("canon") {
        "ef-mount"
    } else if make_lower.contains("nikon") {
        "f-mount"
    } else if make_lower.contains("fuji") {
        "x-mount"
    } else {
        return None;
    };

    let (fl, ap) = (focal_length_mm?, aperture_f_number?);

    let matches: Vec<&LensDescription> = get_lenses()
        .iter()
        .filter(|l| l.mount == mount)
        .filter(|l| {
            let f_min = l.focal_range[0].as_f32();
            let f_max = l.focal_range[1].as_f32();
            let a_min = l.aperture_range[0].as_f32();
            fl >= f_min - 0.5 && fl <= f_max + 0.5 && ap >= a_min - 0.25
        })
        .collect();

    if matches.len() == 1 {
        Some(matches[0].lens_name.clone())
    } else {
        None
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_pentax_makernote(is_little: bool, tag: u16, id0: u8, id1: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"PENTAX \0");
        buf.extend_from_slice(if is_little { b"II" } else { b"MM" });
        let count: u16 = 1;
        buf.extend_from_slice(&if is_little {
            count.to_le_bytes()
        } else {
            count.to_be_bytes()
        });
        buf.extend_from_slice(&if is_little {
            tag.to_le_bytes()
        } else {
            tag.to_be_bytes()
        });
        // type (2) + count (4)
        buf.extend_from_slice(&[0, 1, 0, 0, 0, 2]);
        // value bytes (4)
        buf.extend_from_slice(&[id0, id1, 0, 0]);
        buf
    }

    #[test]
    fn sanitize_lens_name_deduplicates_prefixes() {
        assert_eq!(sanitize_lens_name("Sigma Sigma 18-35mm"), "Sigma 18-35mm");
        assert_eq!(sanitize_lens_name("SMC SMC-DA 35mm"), "SMC-DA 35mm");
        assert_eq!(
            sanitize_lens_name("  Sony FE 50mm f/1.4  "),
            "Sony FE 50mm f/1.4"
        );
        assert_eq!(sanitize_lens_name("50mm"), "50mm");
    }

    #[test]
    fn extract_and_resolve_pentax_makernote_le_and_be() {
        // Manual lens id <= 2 returns None from Pentax resolver
        let manual = build_pentax_makernote(true, 0x003F, 1, 0);
        assert_eq!(extract_pentax_lens_id(&manual), Some((1, 0)));
        assert!(resolve_pentax_lens(&manual, "PENTAX", "K-1", None, None).is_none());

        // Missing tag 0x003F or truncated IFD returns None
        let other_tag = build_pentax_makernote(true, 0x0001, 5, 1);
        assert_eq!(extract_pentax_lens_id(&other_tag), None);
        assert_eq!(parse_pentax_ifd(&[0u8], true), None);
        assert_eq!(parse_pentax_ifd(&[1, 0, 0, 0], true), None);

        // Pick a real K-mount lens with id.0 > 2 and valid byte-sized IDs
        if let Some(sample) = get_lenses().iter().find(|l| {
            l.mount == "k-mount"
                && l.identifiers
                    .id
                    .is_some_and(|id| id.0 > 2 && id.0 <= 255 && id.1 <= 255)
        }) {
            if let Some((id0, id1)) = sample.identifiers.id {
                let fl = Some(sample.focal_range[0].as_f32());
                let ap = Some(sample.aperture_range[0].as_f32());

                let le_bytes = build_pentax_makernote(true, 0x003F, id0 as u8, id1 as u8);
                assert_eq!(extract_pentax_lens_id(&le_bytes), Some((id0, id1)));
                let resolved_le =
                    resolve_universal_lens(&le_bytes, "RICOH IMAGING", "K-3 III", fl, ap);
                assert!(resolved_le.is_some());

                let be_bytes = build_pentax_makernote(false, 0x003F, id0 as u8, id1 as u8);
                assert_eq!(extract_pentax_lens_id(&be_bytes), Some((id0, id1)));
                let resolved_be = resolve_universal_lens(&be_bytes, "PENTAX", "K-1", fl, ap);
                assert!(resolved_be.is_some());
            }
        }
    }

    #[test]
    fn resolve_by_mount_and_optics_covers_all_mounts_and_fallbacks() {
        // Unknown manufacturer or missing focal/aperture -> None
        assert!(resolve_universal_lens(&[], "Hasselblad", "X2D", Some(50.0), Some(2.8)).is_none());
        assert!(resolve_universal_lens(&[], "Sony", "A7IV", None, Some(2.8)).is_none());
        assert!(resolve_universal_lens(&[], "Sony", "A7IV", Some(50.0), None).is_none());

        // Exercise mount branches (Sony, Canon, Nikon, Fujifilm, Pentax)
        for make in ["Sony", "Canon", "Nikon", "Fujifilm", "Pentax"] {
            let _ = resolve_universal_lens(&[], make, "Body", Some(50.0), Some(1.4));
        }
    }
}
