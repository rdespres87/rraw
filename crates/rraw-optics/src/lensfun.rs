#![forbid(unsafe_code)]

//! Official Lensfun XML database engine, parsing, and aberration interpolation.
//!
//! Provides pure-Rust XML parsing of the Lensfun lens database (both embedded
//! and discovered from system paths) and dynamic interpolation of polynomial distortion,
//! lateral TCA, and radial vignetting across focal lengths and apertures.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use flate2::read::GzDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;
use thiserror::Error;

use crate::db::{LensDatabase, MatchedLens};
use crate::model::{DistortionPoly, LateralTca, OpticalCorrectionModel, RadialVignette};
use rraw_core::PhotoMetadata;

/// Errors arising during Lensfun database discovery, decompression, or parsing.
#[derive(Debug, Error)]
pub enum LensfunError {
    #[error("I/O error reading Lensfun database: {0}")]
    Io(#[from] io::Error),

    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("Decompression error: {0}")]
    Decompress(String),

    #[error("Invalid calibration parameter: {0}")]
    InvalidParameter(String),
}

/// A single distortion calibration point at a given focal length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensfunDistortion {
    pub focal: f32,
    pub poly: DistortionPoly,
}

/// A single lateral TCA calibration point at a given focal length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensfunTca {
    pub focal: f32,
    pub ca: LateralTca,
}

/// A single radial vignetting calibration point at a given focal length, aperture, and distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensfunVignetting {
    pub focal: f32,
    pub aperture: f32,
    pub distance: f32,
    pub vignette: RadialVignette,
}

/// A complete lens calibration entry from the Lensfun database.
#[derive(Debug, Clone, PartialEq)]
pub struct LensfunLens {
    pub maker: String,
    pub model: String,
    pub mounts: Vec<String>,
    pub crop_factor: Option<f32>,
    pub distortions: Vec<LensfunDistortion>,
    pub tcas: Vec<LensfunTca>,
    pub vignettings: Vec<LensfunVignetting>,
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn normalize_string(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Embedded and external Lensfun lens aberration database.
#[derive(Debug, Default, Clone)]
pub struct LensfunDatabase {
    lenses: Vec<LensfunLens>,
}

static EMBEDDED_DB: OnceLock<LensfunDatabase> = OnceLock::new();
static GLOBAL_DB: OnceLock<LensfunDatabase> = OnceLock::new();

impl LensfunDatabase {
    /// Creates an empty database.
    #[must_use]
    pub const fn new() -> Self {
        Self { lenses: Vec::new() }
    }

    /// Number of loaded lenses in the database.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lenses.len()
    }

    /// Returns true if no lenses are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lenses.is_empty()
    }

    /// All lenses currently loaded in the database.
    #[must_use]
    pub fn lenses(&self) -> &[LensfunLens] {
        &self.lenses
    }

    /// Loads the embedded official compressed Lensfun catalog (> 1,000 lenses).
    #[must_use]
    pub fn embedded() -> &'static Self {
        EMBEDDED_DB.get_or_init(|| {
            const GZ_BYTES: &[u8] = include_bytes!("../data/lensfun_database.xml.gz");
            let decoder = GzDecoder::new(GZ_BYTES);
            Self::from_reader(decoder).unwrap_or_default()
        })
    }

    /// Global singleton instance with embedded catalog and discovered system XML updates.
    #[must_use]
    pub fn global() -> &'static Self {
        GLOBAL_DB.get_or_init(Self::load_with_system)
    }

    /// Loads the embedded catalog and merges any external XML files found on the host system.
    #[must_use]
    pub fn load_with_system() -> Self {
        let mut db = Self::embedded().clone();
        for dir in Self::discover_system_paths() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().is_some_and(|ext| ext == "xml") {
                        if let Ok(file) = File::open(&path) {
                            if let Ok(external_db) = Self::from_reader(BufReader::new(file)) {
                                db.merge(external_db);
                            }
                        }
                    }
                }
            }
        }
        db
    }

    /// Discovers system directories where Lensfun XML databases reside.
    #[must_use]
    pub fn discover_system_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(env_path) = std::env::var("RRAW_LENSFUN_PATH") {
            let p = PathBuf::from(env_path);
            if p.is_dir() {
                paths.push(p);
            }
        }

        if let Ok(home) = std::env::var("HOME") {
            let user_app_support =
                PathBuf::from(home).join("Library/Application Support/rraw/lensfun");
            if user_app_support.is_dir() {
                paths.push(user_app_support);
            }
        }

        let candidates = [
            "/opt/homebrew/share/lensfun/version_1",
            "/opt/homebrew/share/lensfun/version_2",
            "/usr/local/share/lensfun/version_1",
            "/usr/local/share/lensfun/version_2",
            "/usr/share/lensfun/version_1",
            "/usr/share/lensfun/version_2",
            "/var/lib/lensfun-updates/version_1",
            "/var/lib/lensfun-updates/version_2",
        ];

        for &candidate in &candidates {
            let p = Path::new(candidate);
            if p.is_dir() {
                paths.push(p.to_path_buf());
            }
        }

        paths
    }

    /// Parses a Lensfun database from an XML string.
    pub fn from_xml_str(xml: &str) -> Result<Self, LensfunError> {
        Self::from_reader(xml.as_bytes())
    }

    /// Parses a Lensfun database from any readable byte stream.
    pub fn from_reader<R: Read>(reader: R) -> Result<Self, LensfunError> {
        let mut xml_reader = Reader::from_reader(BufReader::new(reader));
        xml_reader.config_mut().trim_text(true);

        let mut buf = Vec::with_capacity(1024);
        let mut lenses = Vec::new();

        let mut current_lens: Option<LensfunLens> = None;
        let mut in_calibration = false;
        let mut current_tag = String::new();

        loop {
            match xml_reader.read_event_into(&mut buf)? {
                Event::Start(ref e) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    current_tag = tag_name.clone();

                    match tag_name.as_str() {
                        "lens" => {
                            current_lens = Some(LensfunLens {
                                maker: String::new(),
                                model: String::new(),
                                mounts: Vec::new(),
                                crop_factor: None,
                                distortions: Vec::new(),
                                tcas: Vec::new(),
                                vignettings: Vec::new(),
                            });
                        }
                        "calibration" => {
                            in_calibration = true;
                        }
                        _ => {}
                    }
                }
                Event::Empty(ref e) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if in_calibration {
                        if let Some(ref mut lens) = current_lens {
                            match tag_name.as_str() {
                                "distortion" => {
                                    if let Some(dist) = Self::parse_distortion(e) {
                                        lens.distortions.push(dist);
                                    }
                                }
                                "tca" => {
                                    if let Some(tca) = Self::parse_tca(e) {
                                        lens.tcas.push(tca);
                                    }
                                }
                                "vignetting" => {
                                    if let Some(vig) = Self::parse_vignetting(e) {
                                        lens.vignettings.push(vig);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Text(ref e) => {
                    let text = e.unescape()?.trim().to_string();
                    if let Some(ref mut lens) = current_lens {
                        match current_tag.as_str() {
                            "maker" if !text.is_empty() => {
                                if lens.maker.is_empty() {
                                    lens.maker = text;
                                }
                            }
                            "model" if !text.is_empty() => {
                                if lens.model.is_empty() {
                                    lens.model = text;
                                }
                            }
                            "mount" if !text.is_empty() => {
                                lens.mounts.push(text);
                            }
                            "cropfactor" => {
                                if let Ok(val) = text.parse::<f32>() {
                                    lens.crop_factor = Some(val);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Event::End(ref e) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag_name.as_str() {
                        "calibration" => {
                            in_calibration = false;
                        }
                        "lens" => {
                            if let Some(mut lens) = current_lens.take() {
                                lens.distortions.sort_by(|a, b| a.focal.total_cmp(&b.focal));
                                lens.tcas.sort_by(|a, b| a.focal.total_cmp(&b.focal));
                                lens.vignettings.sort_by(|a, b| {
                                    a.focal
                                        .total_cmp(&b.focal)
                                        .then(a.aperture.total_cmp(&b.aperture))
                                });
                                lenses.push(lens);
                            }
                        }
                        _ => {}
                    }
                    current_tag.clear();
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }

        Ok(Self { lenses })
    }

    fn parse_distortion(e: &quick_xml::events::BytesStart<'_>) -> Option<LensfunDistortion> {
        let mut model = String::new();
        let mut focal = 0.0_f32;
        let mut k1 = 0.0_f32;
        let mut k2 = 0.0_f32;
        let mut a = 0.0_f32;
        let mut b = 0.0_f32;
        let mut c = 0.0_f32;

        for attr in e.attributes().flatten() {
            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
            let val = String::from_utf8_lossy(&attr.value).to_string();
            match key.as_str() {
                "model" => model = val,
                "focal" => focal = val.parse().unwrap_or(0.0),
                "k1" => k1 = val.parse().unwrap_or(0.0),
                "k2" => k2 = val.parse().unwrap_or(0.0),
                "a" => a = val.parse().unwrap_or(0.0),
                "b" => b = val.parse().unwrap_or(0.0),
                "c" => c = val.parse().unwrap_or(0.0),
                _ => {}
            }
        }

        if focal <= 0.0 {
            return None;
        }

        let poly = match model.as_str() {
            "ptlens" => {
                let d = 1.0 - a - b - c;
                if d.abs() > 1e-4 {
                    let g05 = (a * 0.125 + b * 0.25 + c * 0.5) / d;
                    let g10 = (a + b + c) / d;
                    let p2 = (g10 - 4.0 * g05) / 0.75;
                    let p1 = g10 - p2;
                    DistortionPoly {
                        k1: p1,
                        k2: p2,
                        k3: 0.0,
                    }
                } else {
                    DistortionPoly::default()
                }
            }
            "poly3" => {
                let denom = 1.0 - k1;
                let eff_k1 = if denom.abs() > 1e-4 { k1 / denom } else { k1 };
                DistortionPoly {
                    k1: eff_k1,
                    k2: 0.0,
                    k3: 0.0,
                }
            }
            "poly5" => DistortionPoly { k1, k2, k3: 0.0 },
            _ => DistortionPoly::default(),
        };

        Some(LensfunDistortion { focal, poly })
    }

    fn parse_tca(e: &quick_xml::events::BytesStart<'_>) -> Option<LensfunTca> {
        let mut focal = 0.0_f32;
        let mut vr = 1.0_f32;
        let mut vb = 1.0_f32;
        let mut br = 0.0_f32;
        let mut bb = 0.0_f32;

        for attr in e.attributes().flatten() {
            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
            let val = String::from_utf8_lossy(&attr.value).to_string();
            match key.as_str() {
                "focal" => focal = val.parse().unwrap_or(0.0),
                "vr" | "kr" => vr = val.parse().unwrap_or(1.0),
                "vb" | "kb" => vb = val.parse().unwrap_or(1.0),
                "br" => br = val.parse().unwrap_or(0.0),
                "bb" => bb = val.parse().unwrap_or(0.0),
                _ => {}
            }
        }

        if focal <= 0.0 {
            return None;
        }

        let scale_r = vr + br * 0.5;
        let scale_b = vb + bb * 0.5;

        Some(LensfunTca {
            focal,
            ca: LateralTca { scale_r, scale_b },
        })
    }

    fn parse_vignetting(e: &quick_xml::events::BytesStart<'_>) -> Option<LensfunVignetting> {
        let mut focal = 0.0_f32;
        let mut aperture = 2.8_f32;
        let mut distance = 1000.0_f32;
        let mut k1 = 0.0_f32;
        let mut k2 = 0.0_f32;
        let mut k3 = 0.0_f32;

        for attr in e.attributes().flatten() {
            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
            let val = String::from_utf8_lossy(&attr.value).to_string();
            match key.as_str() {
                "focal" => focal = val.parse().unwrap_or(0.0),
                "aperture" => aperture = val.parse().unwrap_or(2.8),
                "distance" => distance = val.parse().unwrap_or(1000.0),
                "k1" => k1 = val.parse().unwrap_or(0.0),
                "k2" => k2 = val.parse().unwrap_or(0.0),
                "k3" => k3 = val.parse().unwrap_or(0.0),
                _ => {}
            }
        }

        if focal <= 0.0 {
            return None;
        }

        let v_atten = |r: f32| -> f32 {
            let r2 = r * r;
            let r4 = r2 * r2;
            let r6 = r4 * r2;
            (1.0 + k1 * r2 + k2 * r4 + k3 * r6).max(0.05)
        };

        let g05 = 1.0 / v_atten(0.5) - 1.0;
        let g10 = 1.0 / v_atten(1.0) - 1.0;
        let v2 = (g10 - 4.0 * g05) / 0.75;
        let v1 = g10 - v2;

        Some(LensfunVignetting {
            focal,
            aperture,
            distance,
            vignette: RadialVignette {
                v1: v1.max(0.0),
                v2: v2.max(0.0),
                v3: 0.0,
            },
        })
    }

    /// Merges an external database, skipping duplicates.
    pub fn merge(&mut self, other: Self) {
        for other_lens in other.lenses {
            if !self
                .lenses
                .iter()
                .any(|l| l.maker == other_lens.maker && l.model == other_lens.model)
            {
                self.lenses.push(other_lens);
            }
        }
    }

    /// Searches for a lens profile matching the given query (exact or fuzzy).
    #[must_use]
    pub fn find_lens(&self, query: &str) -> Option<&LensfunLens> {
        let norm_query = normalize_string(query);
        if norm_query.is_empty() {
            return None;
        }

        // 1. Exact normalized model or full name match
        for lens in &self.lenses {
            let norm_model = normalize_string(&lens.model);
            if norm_model == norm_query {
                return Some(lens);
            }
            let full_name = format!("{} {}", lens.maker, lens.model);
            if normalize_string(&full_name) == norm_query {
                return Some(lens);
            }
        }

        // 2. Substring containment
        for lens in &self.lenses {
            let norm_model = normalize_string(&lens.model);
            let full_name = normalize_string(&format!("{} {}", lens.maker, lens.model));
            if full_name.contains(&norm_query) || norm_query.contains(&norm_model) {
                return Some(lens);
            }
        }

        None
    }

    /// Interpolates an [`OpticalCorrectionModel`] for the given lens, focal length, and aperture.
    #[must_use]
    pub fn interpolate_model(
        &self,
        lens: &LensfunLens,
        focal_mm: f32,
        aperture: Option<f32>,
    ) -> OpticalCorrectionModel {
        let distortion = self.interpolate_distortion(&lens.distortions, focal_mm);
        let lateral_ca = self.interpolate_tca(&lens.tcas, focal_mm);
        let vignette = self.interpolate_vignetting(&lens.vignettings, focal_mm, aperture);

        OpticalCorrectionModel {
            distortion,
            lateral_ca,
            vignette,
        }
    }

    fn interpolate_distortion(&self, points: &[LensfunDistortion], focal: f32) -> DistortionPoly {
        if points.is_empty() {
            return DistortionPoly::default();
        }
        if points.len() == 1 || focal <= points[0].focal {
            return points[0].poly;
        }
        let last = points.len() - 1;
        if focal >= points[last].focal {
            return points[last].poly;
        }

        let mut idx = 0;
        while idx < last && points[idx + 1].focal < focal {
            idx += 1;
        }

        let p0 = &points[idx];
        let p1 = &points[idx + 1];
        let span = (p1.focal - p0.focal).max(1e-4);
        let t = ((focal - p0.focal) / span).clamp(0.0, 1.0);

        DistortionPoly {
            k1: lerp(p0.poly.k1, p1.poly.k1, t),
            k2: lerp(p0.poly.k2, p1.poly.k2, t),
            k3: lerp(p0.poly.k3, p1.poly.k3, t),
        }
    }

    fn interpolate_tca(&self, points: &[LensfunTca], focal: f32) -> LateralTca {
        if points.is_empty() {
            return LateralTca::default();
        }
        if points.len() == 1 || focal <= points[0].focal {
            return points[0].ca;
        }
        let last = points.len() - 1;
        if focal >= points[last].focal {
            return points[last].ca;
        }

        let mut idx = 0;
        while idx < last && points[idx + 1].focal < focal {
            idx += 1;
        }

        let p0 = &points[idx];
        let p1 = &points[idx + 1];
        let span = (p1.focal - p0.focal).max(1e-4);
        let t = ((focal - p0.focal) / span).clamp(0.0, 1.0);

        LateralTca {
            scale_r: lerp(p0.ca.scale_r, p1.ca.scale_r, t),
            scale_b: lerp(p0.ca.scale_b, p1.ca.scale_b, t),
        }
    }

    fn interpolate_vignetting(
        &self,
        points: &[LensfunVignetting],
        focal: f32,
        aperture: Option<f32>,
    ) -> RadialVignette {
        if points.is_empty() {
            return RadialVignette::default();
        }

        let target_ap = aperture.unwrap_or(2.8);

        // Filter or interpolate per focal point
        if points.len() == 1 {
            return points[0].vignette;
        }

        // Find samples closest in focal length
        let mut focals: Vec<f32> = points.iter().map(|p| p.focal).collect();
        focals.sort_by(|a, b| a.total_cmp(b));
        focals.dedup();

        let interp_ap_at_focal = |f: f32| -> RadialVignette {
            let f_points: Vec<&LensfunVignetting> = points
                .iter()
                .filter(|p| (p.focal - f).abs() < 1e-2)
                .collect();
            if f_points.is_empty() {
                return RadialVignette::default();
            }
            if f_points.len() == 1 || target_ap <= f_points[0].aperture {
                return f_points[0].vignette;
            }
            let last_idx = f_points.len() - 1;
            if target_ap >= f_points[last_idx].aperture {
                return f_points[last_idx].vignette;
            }

            let mut a_idx = 0;
            while a_idx < last_idx && f_points[a_idx + 1].aperture < target_ap {
                a_idx += 1;
            }
            let ap0 = f_points[a_idx];
            let ap1 = f_points[a_idx + 1];
            let a_span = (ap1.aperture - ap0.aperture).max(0.1);
            let t = ((target_ap - ap0.aperture) / a_span).clamp(0.0, 1.0);

            RadialVignette {
                v1: lerp(ap0.vignette.v1, ap1.vignette.v1, t),
                v2: lerp(ap0.vignette.v2, ap1.vignette.v2, t),
                v3: lerp(ap0.vignette.v3, ap1.vignette.v3, t),
            }
        };

        if focal <= focals[0] {
            return interp_ap_at_focal(focals[0]);
        }
        let last_f_idx = focals.len() - 1;
        if focal >= focals[last_f_idx] {
            return interp_ap_at_focal(focals[last_f_idx]);
        }

        let mut f_idx = 0;
        while f_idx < last_f_idx && focals[f_idx + 1] < focal {
            f_idx += 1;
        }

        let f0 = focals[f_idx];
        let f1 = focals[f_idx + 1];
        let f_span = (f1 - f0).max(1e-4);
        let t = ((focal - f0) / f_span).clamp(0.0, 1.0);

        let v0 = interp_ap_at_focal(f0);
        let v1 = interp_ap_at_focal(f1);

        RadialVignette {
            v1: lerp(v0.v1, v1.v1, t),
            v2: lerp(v0.v2, v1.v2, t),
            v3: lerp(v0.v3, v1.v3, t),
        }
    }

    /// Matches a photo's metadata against the Lensfun database.
    #[must_use]
    pub fn match_metadata(&self, metadata: &PhotoMetadata) -> MatchedLens {
        let focal = metadata.focal_length_mm.unwrap_or(50.0);
        let aperture = metadata.aperture_f_number;

        if let Some(lens_name) = &metadata.lens_model {
            // Priority 1: Curated hand-calibrated profiles in LensDatabase
            if let Some(curated) = LensDatabase::find_lens(lens_name) {
                let model = curated.interpolate_model(focal, aperture);
                return MatchedLens {
                    profile_name: curated.name.to_string(),
                    maker: curated.maker.to_string(),
                    model,
                    is_exact_match: true,
                };
            }

            // Priority 2: Full Lensfun database (> 1,000 lenses)
            if let Some(lens) = self.find_lens(lens_name) {
                let model = self.interpolate_model(lens, focal, aperture);
                return MatchedLens {
                    profile_name: format!("{} {}", lens.maker, lens.model),
                    maker: lens.maker.clone(),
                    model,
                    is_exact_match: true,
                };
            }
        }

        // Fallback to static catalog or generic
        LensDatabase::match_metadata(metadata)
    }
}
