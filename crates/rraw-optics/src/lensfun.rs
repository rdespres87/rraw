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
                    let text = e
                        .decode()
                        .map_err(quick_xml::Error::from)?
                        .trim()
                        .to_string();
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
            if full_name.contains(&norm_query)
                || norm_model.contains(&norm_query)
                || norm_query.contains(&norm_model)
            {
                return Some(lens);
            }
        }

        None
    }

    /// Returns up to `limit` lens names matching a user search query for manual profile selection.
    #[must_use]
    pub fn search_lenses(&self, query: &str, limit: usize) -> Vec<String> {
        let q = normalize_string(query);
        let mut results = Vec::new();
        for lens in &self.lenses {
            let full_name = format!("{} {}", lens.maker, lens.model);
            let norm = normalize_string(&full_name);
            if q.is_empty() || norm.contains(&q) {
                results.push(full_name);
                if results.len() >= limit {
                    break;
                }
            }
        }
        results
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
        // Priority 0: DNG Opcode 1 WarpRectilinear (if present in embedded metadata)
        if let Some(dng_warp) = &metadata.dng_warp_rectilinear {
            let model = OpticalCorrectionModel::from_dng_warp_rectilinear(
                dng_warp.planes,
                &dng_warp.coefficients,
            );
            return MatchedLens {
                profile_name: format!("DNG Opcode (WarpRectilinear, {} plans)", dng_warp.planes),
                maker: "DNG Embedded".to_string(),
                model,
                is_exact_match: true,
            };
        }

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
                let profile_name = if lens
                    .model
                    .to_lowercase()
                    .starts_with(&lens.maker.to_lowercase())
                {
                    lens.model.clone()
                } else {
                    format!("{} {}", lens.maker, lens.model)
                };
                return MatchedLens {
                    profile_name,
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_XML: &str = r#"
    <lensdatabase>
      <lens>
        <maker>TestMaker</maker>
        <model>TestZoom 19-63mm f/3.1</model>
        <mount>E-mount</mount>
        <cropfactor>1.0</cropfactor>
        <calibration>
          <distortion model="ptlens" focal="24" a="0.01" b="-0.02" c="0.005" />
          <distortion model="poly3" focal="50" k1="-0.01" />
          <distortion model="poly5" focal="70" k1="0.005" k2="-0.002" />
          <distortion model="unknown" focal="85" k1="0.01" />
          <distortion model="ptlens" focal="100" a="0.5" b="0.5" c="0.0" />
          <distortion model="poly3" focal="0" k1="0.1" />
          <tca model="poly3" focal="24" vr="1.0004" vb="0.9996" br="0.0002" bb="-0.0002" />
          <tca model="linear" focal="70" kr="1.0001" kb="0.9999" />
          <tca model="linear" focal="0" kr="1.0" kb="1.0" />
          <vignetting model="pa" focal="24" aperture="2.8" distance="1000" k1="-0.4" k2="0.1" k3="-0.02" />
          <vignetting model="pa" focal="24" aperture="8.0" distance="1000" k1="-0.1" k2="0.02" k3="0.0" />
          <vignetting model="pa" focal="70" aperture="2.8" distance="1000" k1="-0.3" k2="0.05" k3="0.0" />
          <vignetting model="pa" focal="70" aperture="8.0" distance="1000" k1="-0.05" k2="0.0" k3="0.0" />
          <vignetting model="pa" focal="0" aperture="2.8" k1="-0.1" />
        </calibration>
      </lens>
    </lensdatabase>
    "#;

    #[test]
    fn parses_xml_merges_searches_and_interpolates_all_models() -> Result<(), LensfunError> {
        let empty = LensfunDatabase::new();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(empty.lenses().is_empty());
        assert!(empty.find_lens("").is_none());
        assert!(empty.find_lens("Missing").is_none());

        let mut db = LensfunDatabase::from_xml_str(SAMPLE_XML)?;
        assert_eq!(db.len(), 1);
        assert!(!db.is_empty());

        // Merging duplicate keeps length 1; merging distinct lens adds it
        db.merge(LensfunDatabase::from_xml_str(SAMPLE_XML)?);
        assert_eq!(db.len(), 1);
        let extra = LensfunDatabase::from_xml_str(
            r#"<lensdatabase><lens><maker>Other</maker><model>Other 35mm</model><calibration/></lens></lensdatabase>"#,
        )?;
        db.merge(extra);
        assert_eq!(db.len(), 2);

        // Exact model, full name, and substring matching + search_lenses
        assert!(db.find_lens("TestZoom 19-63mm f/3.1").is_some());
        assert!(db.find_lens("TestMaker TestZoom 19-63mm f/3.1").is_some());
        assert!(db.find_lens("TestZoom").is_some());
        assert_eq!(db.search_lenses("", 1).len(), 1);
        assert_eq!(db.search_lenses("testzoom", 5).len(), 1);

        // Interpolate below min, mid-range, and above max for focal & aperture
        if let Some(lens) = db.find_lens("TestZoom") {
            let m_low = db.interpolate_model(lens, 16.0, Some(1.4));
            let m_mid = db.interpolate_model(lens, 45.0, Some(5.6));
            let m_high = db.interpolate_model(lens, 200.0, Some(16.0));
            assert!(m_low.vignette.v1 >= 0.0);
            assert!(m_mid.vignette.v1 >= 0.0);
            assert!(m_high.vignette.v1 >= 0.0);
            assert!(m_mid.lateral_ca.scale_r > 0.99);
        }

        // Empty calibration lens interpolation returns defaults
        if let Some(other) = db.find_lens("Other 35mm") {
            let m_empty = db.interpolate_model(other, 35.0, None);
            assert_eq!(m_empty.distortion, DistortionPoly::default());
            assert_eq!(m_empty.lateral_ca, LateralTca::default());
            assert_eq!(m_empty.vignette, RadialVignette::default());
        }

        Ok(())
    }

    fn sample_metadata() -> PhotoMetadata {
        PhotoMetadata {
            camera_make: "TestMake".to_owned(),
            camera_model: "TestBody".to_owned(),
            lens_model: None,
            focal_length_mm: None,
            aperture_f_number: None,
            iso: None,
            width: 64,
            height: 64,
            black_levels: [0.0; 4],
            white_levels: [16383.0; 4],
            cfa_pattern: rraw_core::CfaPattern::Bayer(rraw_core::BayerOrigin::Rggb),
            noise_profile: rraw_core::NoiseProfile::default(),
            vignette_handled_by_gainmap: false,
            dng_warp_rectilinear: None,
            rating: 0,
            pick: rraw_core::PickStatus::Unflagged,
            label: rraw_core::ColorLabel::None,
            keywords: Vec::new(),
        }
    }

    #[test]
    fn match_metadata_handles_dng_warp_lensfun_and_fallback() -> Result<(), LensfunError> {
        let db = LensfunDatabase::from_xml_str(SAMPLE_XML)?;

        // Priority 0: DNG WarpRectilinear
        let meta_dng = PhotoMetadata {
            dng_warp_rectilinear: Some(rraw_core::DngWarpRectilinearData {
                planes: 1,
                coefficients: vec![1.0, -0.02, 0.01, 0.0, 0.0, 0.0],
            }),
            ..sample_metadata()
        };
        let matched_dng = db.match_metadata(&meta_dng);
        assert_eq!(matched_dng.maker, "DNG Embedded");
        assert!(matched_dng.is_exact_match);

        // Priority 2: Lensfun DB match (model without maker prefix & with maker prefix)
        let meta_lf = PhotoMetadata {
            lens_model: Some("TestZoom 19-63mm f/3.1".to_owned()),
            focal_length_mm: Some(50.0),
            aperture_f_number: Some(4.0),
            ..sample_metadata()
        };
        let matched_lf = db.match_metadata(&meta_lf);
        assert_eq!(matched_lf.profile_name, "TestMaker TestZoom 19-63mm f/3.1");
        assert!(matched_lf.is_exact_match);

        let db_prefixed = LensfunDatabase::from_xml_str(
            r#"<lensdatabase><lens><maker>Acme</maker><model>Acme 85mm f/1.8</model><calibration><vignetting model="pa" focal="85" aperture="1.8" k1="-0.2"/></calibration></lens></lensdatabase>"#,
        )?;
        let meta_pref = PhotoMetadata {
            lens_model: Some("Acme 85mm f/1.8".to_owned()),
            focal_length_mm: Some(85.0),
            aperture_f_number: Some(2.8),
            ..sample_metadata()
        };
        let matched_pref = db_prefixed.match_metadata(&meta_pref);
        assert_eq!(matched_pref.profile_name, "Acme 85mm f/1.8");

        // Fallback when unknown lens
        let meta_unknown = sample_metadata();
        let _ = db.match_metadata(&meta_unknown);
        let _ = LensfunDatabase::discover_system_paths();
        assert!(!LensfunDatabase::embedded().is_empty());
        assert!(!LensfunDatabase::global().is_empty());
        let _ = LensfunDatabase::from_xml_str("<lensdatabase><unclosed");
        let deg_xml = r#"<lensdatabase><lens><maker>M</maker><model>D</model><calibration><distortion model="ptlens" focal="50" a="0.5" b="0.5" c="0.0"/><distortion model="poly3" focal="35" k1="1.0"/><vignetting model="pa" focal="50" aperture="2.8" distance="500" k1="-0.2"/><vignetting model="pa" focal="50" aperture="2.8" distance="2000" k1="-0.1"/></calibration></lens></lensdatabase>"#;
        let deg_db = LensfunDatabase::from_xml_str(deg_xml)?;
        if let Some(l) = deg_db.find_lens("D") {
            let m = deg_db.interpolate_model(l, 42.0, Some(2.8));
            assert!(m.vignette.v1.is_finite());
        }
        Ok(())
    }
}
