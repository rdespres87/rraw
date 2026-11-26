#![forbid(unsafe_code)]

//! Pure-safe-Rust camera alias & rebadge normalizer for `rawler`.
//!
//! Many RAW files come from rebadged hardware (e.g., Hasselblad Lunar = Sony NEX-7,
//! Samsung GX-1L = Pentax K100D, Leica D-LUX 6 = Panasonic DMC-LX7) or regional
//! model variants / firmware mode strings whose exact `(Make, Model, Mode)` tuple is
//! absent from `rawler`'s built-in database even though the underlying sensor and
//! compression codec are identical.
//!
//! When `rawler::decode_file` fails, this module inspects the file header (TIFF,
//! Panasonic RW2 `IIU\0`, or Minolta MRW `\0MRM`), patches the in-memory `Make` /
//! `Model` tags to the hardware-identical sibling, decodes via `rawler::RawSource`,
//! and returns the original authentic `(make, model)` so caller metadata remains accurate.

use std::path::Path;

use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;

/// Decoded `rawler::RawImage` obtained via a hardware-identical camera alias,
/// paired with the original authentic make and model from the input file.
pub struct AliasDecodeResult {
    pub raw_image: rawler::RawImage,
    pub original_make: String,
    pub original_model: String,
}

type CameraId = (&'static str, &'static str);

/// Curated table of verified hardware-identical camera aliases:
/// `((original_make, original_model), (target_make, target_model))`.
///
/// Trailing spaces and NULs are stripped before lookup; leading spaces are
/// preserved for Hasselblad backs that emit `" Flash Sync"` / `" CFV-50c/Flash Sync"`.
const CAMERA_ALIASES: &[(CameraId, CameraId)] = &[
    // Rebadged cameras (exact same hardware)
    (("HASSELBLAD", "Lunar"), ("SONY", "NEX-7")),
    (
        ("SAMSUNG TECHWIN", "GX-1L"),
        ("PENTAX Corporation", "PENTAX K100D"),
    ),
    (("LEICA", "D-LUX 6"), ("Panasonic", "DMC-LX7")),
    // Regional / minor model-string variants
    (("Panasonic", "DMC-GM1S"), ("Panasonic", "DMC-GM1")),
    (("Panasonic", "DC-FZ45"), ("Panasonic", "DMC-FZ45")),
    (("Panasonic", "DC-FZ83"), ("Panasonic", "DC-FZ80")),
    (("PENTAX", "PENTAX K2000"), ("PENTAX", "PENTAX K-m")),
    (("SONY", "ILME-FX2"), ("SONY", "ILCE-7M4")),
    (("NIKON", "COOLPIX P7700"), ("NIKON", "COOLPIX P7800")),
    (
        ("NIKON CORPORATION", "NIKON 1 J4"),
        ("NIKON CORPORATION", "NIKON 1 V3"),
    ),
    (
        ("NIKON CORPORATION", "NIKON 1 S1"),
        ("NIKON CORPORATION", "NIKON 1 J2"),
    ),
    (
        ("NIKON CORPORATION", "NIKON D1H"),
        ("NIKON CORPORATION", "NIKON D1"),
    ),
    (
        ("Nikon", "Nikon COOLSCAN V ED"),
        ("Nikon", "Nikon SUPER COOLSCAN 5000 ED"),
    ),
    // Hasselblad backs / electronic shutter / flash-sync mode suffixes
    (
        ("Hasselblad", " Flash Sync"),
        ("Hasselblad", "Hasselblad H3D"),
    ),
    (
        ("Hasselblad", " CFV-50c/Flash Sync"),
        ("Hasselblad", "Hasselblad CFV-50"),
    ),
    (
        ("Hasselblad", "Hasselblad CFV-50c"),
        ("Hasselblad", "Hasselblad CFV-50"),
    ),
    (
        ("Hasselblad", "Hasselblad X1DM2-50c"),
        ("Hasselblad", "X1D II 50C"),
    ),
    (
        ("Hasselblad", "CFV 100C/Electronic Shutter"),
        ("Hasselblad", "X2D 100C"),
    ),
    // Medium-format & cinema siblings
    (("Phase One A/S", "P40+"), ("Phase One A/S", "P65+")),
    (("Kodak", "DCS520C"), ("Kodak", "DCS560C")),
    (("SAMSUNG", "EK-GN120"), ("SAMSUNG", "NX300")),
    (("Leaf", "Leaf AFi-II 12"), ("Leaf", "Credo 80")),
    // Konica Minolta / Minolta MRW siblings
    (
        ("KONICA MINOLTA", "DYNAX 5D"),
        ("KONICA MINOLTA", "DYNAX 7D"),
    ),
    (
        ("KONICA MINOLTA", "ALPHA SWEET DIGITAL"),
        ("KONICA MINOLTA", "DYNAX 7D"),
    ),
    (
        ("Minolta Co., Ltd.", "DiMAGE 5"),
        ("Minolta Co., Ltd.", "DiMAGE 7"),
    ),
];

const TAG_MAKE: u16 = 0x010F;
const TAG_MODEL: u16 = 0x0110;
const TIFF_TYPE_ASCII: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    #[inline]
    fn u16(self, data: &[u8], off: usize) -> Option<u16> {
        let end = off.checked_add(2)?;
        let b = data.get(off..end)?;
        Some(match self {
            Self::Little => u16::from_le_bytes([b[0], b[1]]),
            Self::Big => u16::from_be_bytes([b[0], b[1]]),
        })
    }

    #[inline]
    fn u32(self, data: &[u8], off: usize) -> Option<u32> {
        let end = off.checked_add(4)?;
        let b = data.get(off..end)?;
        Some(match self {
            Self::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Self::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    #[inline]
    fn write_u32(self, data: &mut [u8], off: usize, val: u32) -> Option<()> {
        let end = off.checked_add(4)?;
        let dst = data.get_mut(off..end)?;
        let bytes = match self {
            Self::Little => val.to_le_bytes(),
            Self::Big => val.to_be_bytes(),
        };
        dst.copy_from_slice(&bytes);
        Some(())
    }
}

fn lookup_alias(make: &str, model: &str) -> Option<(&'static str, &'static str)> {
    let make_clean = make.trim_end_matches(['\0', ' ']);
    let model_clean = model.trim_end_matches(['\0', ' ']);
    CAMERA_ALIASES
        .iter()
        .find(|((m, mdl), _)| *m == make_clean && *mdl == model_clean)
        .map(|(_, target)| *target)
}

#[derive(Debug, Clone, Copy)]
struct AsciiEntryLoc {
    entry_off: usize,
    data_off: usize,
    count: usize,
}

fn read_ascii_entry(
    data: &[u8],
    tiff_base: usize,
    endian: Endian,
    entry_off: usize,
) -> Option<(String, AsciiEntryLoc)> {
    let typ = endian.u16(data, entry_off.checked_add(2)?)?;
    if typ != TIFF_TYPE_ASCII {
        return None;
    }
    let count = endian.u32(data, entry_off.checked_add(4)?)? as usize;
    if count == 0 || count > 256 {
        return None;
    }
    let data_off = if count <= 4 {
        entry_off.checked_add(8)?
    } else {
        let rel = endian.u32(data, entry_off.checked_add(8)?)? as usize;
        tiff_base.checked_add(rel)?
    };
    let end = data_off.checked_add(count)?;
    let slice = data.get(data_off..end)?;
    let nul_pos = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    let text = String::from_utf8_lossy(&slice[..nul_pos]).into_owned();
    Some((
        text,
        AsciiEntryLoc {
            entry_off,
            data_off,
            count,
        },
    ))
}

fn locate_tiff_make_model(
    data: &[u8],
    tiff_base: usize,
    allow_panasonic_rw2: bool,
) -> Option<(Endian, String, AsciiEntryLoc, String, AsciiEntryLoc)> {
    let hdr_end = tiff_base.checked_add(8)?;
    let hdr = data.get(tiff_base..hdr_end)?;
    let endian = match &hdr[0..2] {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return None,
    };
    let magic = endian.u16(data, tiff_base.checked_add(2)?)?;
    let valid_magic =
        magic == 42 || (allow_panasonic_rw2 && endian == Endian::Little && magic == 85);
    if !valid_magic {
        return None;
    }
    let ifd0_rel = endian.u32(data, tiff_base.checked_add(4)?)? as usize;
    let ifd0_off = tiff_base.checked_add(ifd0_rel)?;
    let entry_count = usize::from(endian.u16(data, ifd0_off)?);
    if entry_count == 0 || entry_count > 512 {
        return None;
    }

    let mut make_found = None;
    let mut model_found = None;
    for i in 0..entry_count {
        let step = i.checked_mul(12)?;
        let entry_off = ifd0_off.checked_add(2)?.checked_add(step)?;
        let tag = endian.u16(data, entry_off)?;
        if tag == TAG_MAKE {
            make_found = read_ascii_entry(data, tiff_base, endian, entry_off);
        } else if tag == TAG_MODEL {
            model_found = read_ascii_entry(data, tiff_base, endian, entry_off);
        }
        if make_found.is_some() && model_found.is_some() {
            break;
        }
    }

    let (make, make_loc) = make_found?;
    let (model, model_loc) = model_found?;
    Some((endian, make, make_loc, model, model_loc))
}

fn patch_tiff_ascii_eof(
    buf: &mut Vec<u8>,
    endian: Endian,
    loc: AsciiEntryLoc,
    replacement: &str,
) -> Option<()> {
    let bytes = replacement.as_bytes();
    let count = bytes.len().checked_add(1)?;
    let count_u32 = u32::try_from(count).ok()?;

    if count <= 4 {
        endian.write_u32(buf, loc.entry_off.checked_add(4)?, count_u32)?;
        let val_off = loc.entry_off.checked_add(8)?;
        let val_end = val_off.checked_add(4)?;
        let slot = buf.get_mut(val_off..val_end)?;
        slot.fill(0);
        slot[..bytes.len()].copy_from_slice(bytes);
    } else {
        let new_off = u32::try_from(buf.len()).ok()?;
        buf.extend_from_slice(bytes);
        buf.push(0);
        // Keep even alignment for strict TIFF readers
        if !buf.len().is_multiple_of(2) {
            buf.push(0);
        }
        endian.write_u32(buf, loc.entry_off.checked_add(4)?, count_u32)?;
        endian.write_u32(buf, loc.entry_off.checked_add(8)?, new_off)?;
    }
    Some(())
}

fn patch_tiff_ascii_in_place(
    buf: &mut [u8],
    endian: Endian,
    loc: AsciiEntryLoc,
    replacement: &str,
) -> Option<()> {
    let bytes = replacement.as_bytes();
    let needed = bytes.len().checked_add(1)?;
    if needed > loc.count {
        return None;
    }
    let end = loc.data_off.checked_add(loc.count)?;
    let slot = buf.get_mut(loc.data_off..end)?;
    slot.fill(0);
    slot[..bytes.len()].copy_from_slice(bytes);
    let count_u32 = u32::try_from(needed).ok()?;
    endian.write_u32(buf, loc.entry_off.checked_add(4)?, count_u32)?;
    Some(())
}

/// Attempts to patch a TIFF / Panasonic RW2 buffer's Make/Model if it matches `CAMERA_ALIASES`.
/// Returns `(patched_bytes, original_make, original_model)`.
fn try_patch_tiff_alias(data: &[u8]) -> Option<(Vec<u8>, String, String)> {
    let (endian, make, make_loc, model, model_loc) = locate_tiff_make_model(data, 0, true)?;
    let (target_make, target_model) = lookup_alias(&make, &model)?;

    let mut patched = data.to_vec();
    patch_tiff_ascii_eof(&mut patched, endian, make_loc, target_make)?;
    patch_tiff_ascii_eof(&mut patched, endian, model_loc, target_model)?;
    Some((
        patched,
        make.trim_end_matches(['\0', ' ']).to_owned(),
        model.trim_end_matches(['\0', ' ']).to_owned(),
    ))
}

/// Attempts to patch a Minolta MRW (`\0MRM`) container's embedded `TTW` TIFF block in-place.
fn try_patch_mrw_alias(data: &[u8]) -> Option<(Vec<u8>, String, String)> {
    if data.len() < 16 || &data[0..4] != b"\x00MRM" {
        return None;
    }
    let data_offset = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let header_end = data_offset.checked_add(8)?.min(data.len());

    let mut pos = 8_usize;
    let mut ttw_base = None;
    while pos.checked_add(8)? <= header_end {
        let tag = data.get(pos..pos + 4)?;
        let len = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;
        let payload_start = pos.checked_add(8)?;
        let payload_end = payload_start.checked_add(len)?;
        if payload_end > header_end {
            break;
        }
        if tag == b"\x00TTW" {
            ttw_base = Some(payload_start);
            break;
        }
        pos = payload_end;
    }

    let tiff_base = ttw_base?;
    let (endian, make, make_loc, model, model_loc) =
        locate_tiff_make_model(data, tiff_base, false)?;
    let (target_make, target_model) = lookup_alias(&make, &model)?;

    let mut patched = data.to_vec();
    patch_tiff_ascii_in_place(&mut patched, endian, make_loc, target_make)?;
    patch_tiff_ascii_in_place(&mut patched, endian, model_loc, target_model)?;
    Some((
        patched,
        make.trim_end_matches(['\0', ' ']).to_owned(),
        model.trim_end_matches(['\0', ' ']).to_owned(),
    ))
}

/// Attempts to decode a RAW file rejected by `rawler` by patching a known hardware-identical
/// camera alias in memory and delegating decoding to `rawler::RawSource`.
pub fn try_decode_with_alias(path: &Path) -> Option<AliasDecodeResult> {
    let data = std::fs::read(path).ok()?;
    let (patched, original_make, original_model) =
        try_patch_mrw_alias(&data).or_else(|| try_patch_tiff_alias(&data))?;

    let raw_source = RawSource::new_from_slice(&patched);
    let raw_image = rawler::decode(&raw_source, &RawDecodeParams::default()).ok()?;
    Some(AliasDecodeResult {
        raw_image,
        original_make,
        original_model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_tiff(endian: Endian, magic: u16, make: &str, model: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        match endian {
            Endian::Little => {
                buf.extend_from_slice(b"II");
                buf.extend_from_slice(&magic.to_le_bytes());
                buf.extend_from_slice(&8_u32.to_le_bytes());
            }
            Endian::Big => {
                buf.extend_from_slice(b"MM");
                buf.extend_from_slice(&magic.to_be_bytes());
                buf.extend_from_slice(&8_u32.to_be_bytes());
            }
        }
        // IFD0 at offset 8: 2 entries -> 2 + 24 + 4 = 30 bytes (strings start at offset 38)
        let make_bytes = make.as_bytes();
        let model_bytes = model.as_bytes();
        let make_count = (make_bytes.len() + 1) as u32;
        let model_count = (model_bytes.len() + 1) as u32;
        let make_off = 38_u32;
        let model_off = make_off + make_count;

        let push_u16 = |b: &mut Vec<u8>, v: u16| match endian {
            Endian::Little => b.extend_from_slice(&v.to_le_bytes()),
            Endian::Big => b.extend_from_slice(&v.to_be_bytes()),
        };
        let push_u32 = |b: &mut Vec<u8>, v: u32| match endian {
            Endian::Little => b.extend_from_slice(&v.to_le_bytes()),
            Endian::Big => b.extend_from_slice(&v.to_be_bytes()),
        };

        push_u16(&mut buf, 2);
        // Make entry
        push_u16(&mut buf, TAG_MAKE);
        push_u16(&mut buf, TIFF_TYPE_ASCII);
        push_u32(&mut buf, make_count);
        push_u32(&mut buf, make_off);
        // Model entry
        push_u16(&mut buf, TAG_MODEL);
        push_u16(&mut buf, TIFF_TYPE_ASCII);
        push_u32(&mut buf, model_count);
        push_u32(&mut buf, model_off);
        // Next IFD = 0
        push_u32(&mut buf, 0);

        buf.extend_from_slice(make_bytes);
        buf.push(0);
        buf.extend_from_slice(model_bytes);
        buf.push(0);
        buf
    }

    #[test]
    fn test_tiff_alias_patch_rw2_and_be() {
        // Panasonic RW2 magic (85) LE: LEICA D-LUX 6 -> Panasonic DMC-LX7 (longer string -> EOF append)
        let rw2 = build_synthetic_tiff(Endian::Little, 85, "LEICA", "D-LUX 6");
        let patched = try_patch_tiff_alias(&rw2);
        assert!(patched.is_some());
        if let Some((out, orig_make, orig_model)) = patched {
            assert_eq!(orig_make, "LEICA");
            assert_eq!(orig_model, "D-LUX 6");
            let parsed = locate_tiff_make_model(&out, 0, true);
            assert!(parsed.is_some());
            if let Some((_, new_make, _, new_model, _)) = parsed {
                assert_eq!(new_make, "Panasonic");
                assert_eq!(new_model, "DMC-LX7");
            }
        }

        // Big-endian TIFF: SAMSUNG TECHWIN GX-1L -> PENTAX Corporation / PENTAX K100D
        let pef = build_synthetic_tiff(Endian::Big, 42, "SAMSUNG TECHWIN", "GX-1L              ");
        let patched = try_patch_tiff_alias(&pef);
        assert!(patched.is_some());
        if let Some((out, orig_make, orig_model)) = patched {
            assert_eq!(orig_make, "SAMSUNG TECHWIN");
            assert_eq!(orig_model, "GX-1L");
            let parsed = locate_tiff_make_model(&out, 0, false);
            assert!(parsed.is_some());
            if let Some((_, new_make, _, new_model, _)) = parsed {
                assert_eq!(new_make, "PENTAX Corporation");
                assert_eq!(new_model, "PENTAX K100D");
            }
        }
    }

    #[test]
    fn test_mrw_alias_patch_in_place() {
        let ttw_tiff = build_synthetic_tiff(Endian::Big, 42, "Minolta Co., Ltd.", "DiMAGE 5");
        let mut mrw = Vec::new();
        mrw.extend_from_slice(b"\x00MRM");
        let block_total = 8 + ttw_tiff.len();
        mrw.extend_from_slice(&(block_total as u32).to_be_bytes());
        mrw.extend_from_slice(b"\x00TTW");
        mrw.extend_from_slice(&(ttw_tiff.len() as u32).to_be_bytes());
        mrw.extend_from_slice(&ttw_tiff);
        // Dummy pixel payload after header
        mrw.extend_from_slice(&[0_u8; 16]);

        let patched = try_patch_mrw_alias(&mrw);
        assert!(patched.is_some());
        if let Some((out, orig_make, orig_model)) = patched {
            assert_eq!(
                out.len(),
                mrw.len(),
                "MRW in-place patch must preserve file size"
            );
            assert_eq!(orig_make, "Minolta Co., Ltd.");
            assert_eq!(orig_model, "DiMAGE 5");
            let parsed = locate_tiff_make_model(&out, 16, false);
            assert!(parsed.is_some());
            if let Some((_, new_make, _, new_model, _)) = parsed {
                assert_eq!(new_make, "Minolta Co., Ltd.");
                assert_eq!(new_model, "DiMAGE 7");
            }
        }
    }
}
