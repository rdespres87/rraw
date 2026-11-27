#![forbid(unsafe_code)]

//! Minimal TIFF/DNG IFD parser and typed metadata sub-structures.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Endian {
    Little,
    Big,
}

impl Endian {
    #[inline]
    pub(super) fn u16(self, data: &[u8], off: usize) -> Option<u16> {
        let end = off.checked_add(2)?;
        let b = data.get(off..end)?;
        Some(match self {
            Self::Little => u16::from_le_bytes([b[0], b[1]]),
            Self::Big => u16::from_be_bytes([b[0], b[1]]),
        })
    }

    #[inline]
    pub(super) fn u32(self, data: &[u8], off: usize) -> Option<u32> {
        let end = off.checked_add(4)?;
        let b = data.get(off..end)?;
        Some(match self {
            Self::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Self::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    #[inline]
    pub(super) fn i32(self, data: &[u8], off: usize) -> Option<i32> {
        self.u32(data, off).map(|v| v as i32)
    }
}

#[derive(Debug, Clone)]
pub(super) struct IfdGeometry {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) bps: u16,
    pub(super) compression: u16,
    pub(super) photometric: u16,
    pub(super) spp: u16,
    pub(super) active_area: Option<[usize; 4]>,
    pub(super) cfa_pattern: Option<[u8; 4]>,
}

impl Default for IfdGeometry {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            bps: 0,
            compression: 0,
            photometric: 0,
            spp: 1,
            active_area: None,
            cfa_pattern: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct IfdStorage {
    pub(super) strip_offsets: Vec<usize>,
    pub(super) rows_per_strip: usize,
    pub(super) strip_byte_counts: Vec<usize>,
    pub(super) tile_width: usize,
    pub(super) tile_length: usize,
    pub(super) tile_offsets: Vec<usize>,
    pub(super) tile_byte_counts: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct IfdColor {
    pub(super) linearization_table: Option<Vec<u16>>,
    pub(super) black_level: Option<[f32; 4]>,
    pub(super) white_level: Option<[f32; 4]>,
    pub(super) color_matrix1: Option<[f32; 9]>,
    pub(super) color_matrix2: Option<[f32; 9]>,
    pub(super) calib_illuminant2: Option<u16>,
    pub(super) as_shot_neutral: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct IfdCamera {
    pub(super) make: Option<String>,
    pub(super) model: Option<String>,
    pub(super) orientation: Option<u16>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ParsedIfd {
    pub(super) new_subfile_type: u32,
    pub(super) sub_ifds: Vec<usize>,
    pub(super) geometry: IfdGeometry,
    pub(super) storage: IfdStorage,
    pub(super) color: IfdColor,
    pub(super) camera: IfdCamera,
}

fn read_u32_list(data: &[u8], e: Endian, typ: u16, count: usize, entry_off: usize) -> Vec<usize> {
    let elem_size = match typ {
        1 => 1,
        3 => 2,
        4 => 4,
        _ => return Vec::new(),
    };
    let total_bytes = count.saturating_mul(elem_size);
    let base = if total_bytes <= 4 {
        entry_off + 8
    } else {
        let Some(off) = e.u32(data, entry_off + 8) else {
            return Vec::new();
        };
        off as usize
    };
    if base.saturating_add(total_bytes) > data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let pos = base + i * elem_size;
        let val = match typ {
            1 => usize::from(data[pos]),
            3 => e.u16(data, pos).map_or(0, usize::from),
            4 => e.u32(data, pos).map_or(0, |v| v as usize),
            _ => 0,
        };
        out.push(val);
    }
    out
}

fn read_f32_list(data: &[u8], e: Endian, typ: u16, count: usize, entry_off: usize) -> Vec<f32> {
    let elem_size = match typ {
        1 => 1,
        3 => 2,
        4 => 4,
        5 | 10 => 8,
        _ => return Vec::new(),
    };
    let total_bytes = count.saturating_mul(elem_size);
    let base = if total_bytes <= 4 {
        entry_off + 8
    } else {
        let Some(off) = e.u32(data, entry_off + 8) else {
            return Vec::new();
        };
        off as usize
    };
    if base.saturating_add(total_bytes) > data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let pos = base + i * elem_size;
        let val = match typ {
            1 => f32::from(data[pos]),
            3 => e.u16(data, pos).map_or(0.0, f32::from),
            4 => e.u32(data, pos).map_or(0.0, |v| v as f32),
            5 => {
                let num = e.u32(data, pos).unwrap_or(0) as f64;
                let den = e.u32(data, pos + 4).unwrap_or(1) as f64;
                if den.abs() > 1e-12 {
                    (num / den) as f32
                } else {
                    0.0
                }
            }
            10 => {
                let num = f64::from(e.i32(data, pos).unwrap_or(0));
                let den = f64::from(e.i32(data, pos + 4).unwrap_or(1));
                if den.abs() > 1e-12 {
                    (num / den) as f32
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };
        out.push(val);
    }
    out
}

fn read_ascii(data: &[u8], e: Endian, count: usize, entry_off: usize) -> Option<String> {
    if count == 0 {
        return None;
    }
    let base = if count <= 4 {
        entry_off + 8
    } else {
        e.u32(data, entry_off + 8)? as usize
    };
    let slice = data.get(base..base.saturating_add(count))?;
    let trimmed = slice.split(|&b| b == 0).next().unwrap_or(slice);
    let s = String::from_utf8_lossy(trimmed).trim().to_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub(super) fn parse_ifd(data: &[u8], off: usize, e: Endian) -> Option<ParsedIfd> {
    let count = usize::from(e.u16(data, off)?);
    let mut ifd = ParsedIfd::default();

    for i in 0..count {
        let entry_off = off.checked_add(2)?.checked_add(i.checked_mul(12)?)?;
        if entry_off.checked_add(12).is_none_or(|end| end > data.len()) {
            break;
        }
        let tag = e.u16(data, entry_off)?;
        let typ = e.u16(data, entry_off + 2)?;
        let n = e.u32(data, entry_off + 4)? as usize;

        match tag {
            254 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.new_subfile_type = v as u32;
                }
            }
            256 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.geometry.width = v;
                }
            }
            257 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.geometry.height = v;
                }
            }
            258 => {
                // BitsPerSample has count=3 for RGB TIFFs (stored out-of-line when total_bytes > 4),
                // so pass the real count (capped at 16) rather than 1 to preserve pointer dereferencing.
                if let Some(&v) = read_u32_list(data, e, typ, n.min(16), entry_off).first() {
                    ifd.geometry.bps = v as u16;
                }
            }
            259 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.geometry.compression = v as u16;
                }
            }
            262 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.geometry.photometric = v as u16;
                }
            }
            271 => ifd.camera.make = read_ascii(data, e, n, entry_off),
            272 => ifd.camera.model = read_ascii(data, e, n, entry_off),
            273 => ifd.storage.strip_offsets = read_u32_list(data, e, typ, n, entry_off),
            274 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.camera.orientation = Some(v as u16);
                }
            }
            277 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.geometry.spp = v as u16;
                }
            }
            278 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.storage.rows_per_strip = v;
                }
            }
            279 => ifd.storage.strip_byte_counts = read_u32_list(data, e, typ, n, entry_off),
            322 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.storage.tile_width = v;
                }
            }
            323 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.storage.tile_length = v;
                }
            }
            324 => ifd.storage.tile_offsets = read_u32_list(data, e, typ, n, entry_off),
            325 => ifd.storage.tile_byte_counts = read_u32_list(data, e, typ, n, entry_off),
            330 => ifd.sub_ifds = read_u32_list(data, e, typ, n, entry_off),
            33422 => {
                let vals = read_u32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 4 {
                    ifd.geometry.cfa_pattern =
                        Some([vals[0] as u8, vals[1] as u8, vals[2] as u8, vals[3] as u8]);
                }
            }
            50712 => {
                let vals = read_u32_list(data, e, typ, n.min(65536), entry_off);
                if !vals.is_empty() {
                    ifd.color.linearization_table =
                        Some(vals.into_iter().map(|v| v as u16).collect());
                }
            }
            50714 => {
                let vals = read_f32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 1 {
                    ifd.color.black_level = Some([vals[0]; 4]);
                } else if vals.len() >= 4 {
                    ifd.color.black_level = Some([vals[0], vals[1], vals[2], vals[3]]);
                }
            }
            50717 => {
                let vals = read_f32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 1 {
                    ifd.color.white_level = Some([vals[0]; 4]);
                } else if vals.len() >= 4 {
                    ifd.color.white_level = Some([vals[0], vals[1], vals[2], vals[3]]);
                }
            }
            50721 => {
                let vals = read_f32_list(data, e, typ, n.min(9), entry_off);
                if vals.len() == 9 && vals.iter().all(|v| v.is_finite()) {
                    let mut m = [0.0_f32; 9];
                    m.copy_from_slice(&vals);
                    ifd.color.color_matrix1 = Some(m);
                }
            }
            50722 => {
                let vals = read_f32_list(data, e, typ, n.min(9), entry_off);
                if vals.len() == 9 && vals.iter().all(|v| v.is_finite()) {
                    let mut m = [0.0_f32; 9];
                    m.copy_from_slice(&vals);
                    ifd.color.color_matrix2 = Some(m);
                }
            }
            50779 => {
                if let Some(&v) = read_u32_list(data, e, typ, n.min(1), entry_off).first() {
                    ifd.color.calib_illuminant2 = Some(v as u16);
                }
            }
            50728 => {
                let vals = read_f32_list(data, e, typ, n.min(3), entry_off);
                if vals.len() == 3 && vals.iter().all(|&v| v.is_finite() && v > 0.0) {
                    ifd.color.as_shot_neutral = Some([vals[0], vals[1], vals[2]]);
                }
            }
            50829 => {
                let vals = read_u32_list(data, e, typ, n.min(4), entry_off);
                if vals.len() == 4 {
                    ifd.geometry.active_area = Some([vals[0], vals[1], vals[2], vals[3]]);
                }
            }
            _ => {}
        }
    }

    Some(ifd)
}
