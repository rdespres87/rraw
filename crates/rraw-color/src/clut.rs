#![forbid(unsafe_code)]

//! 3D Color Look-Up Table (CLUT) tetrahedral interpolation and film simulation engine.
//!
//! Provides zero-allocation tetrahedral and trilinear 3D LUT sampling, `.cube` file format
//! parsing/exporting, and mathematical film simulation generators (Classic Chrome, Portra 400,
//! Velvia 50, Tri-X 400, and Ilford HP5).

use crate::ColorError;

/// Supported film simulation profiles for algorithmic 3D LUT generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilmSimulationProfile {
    /// Fujifilm Classic Chrome: muted saturation, hard shadow contrast, cyan-tinted sky bias.
    ClassicChrome,
    /// Kodak Portra 400: warm natural skin tones, soft pastel contrast, gentle highlight roll-off.
    Portra400,
    /// Fujifilm Velvia 50: high saturation, punchy slide contrast, deep inky blacks and vivid skies.
    Velvia50,
    /// Kodak Tri-X 400: high-contrast panchromatic photojournalism monochrome with rich punch.
    TriX400,
    /// Ilford HP5 Plus: versatile panchromatic monochrome with soft midtones and wide latitude.
    IlfordHp5,
}

impl FilmSimulationProfile {
    /// List of all built-in film simulation profiles.
    pub const ALL: [Self; 5] = [
        Self::ClassicChrome,
        Self::Portra400,
        Self::Velvia50,
        Self::TriX400,
        Self::IlfordHp5,
    ];

    /// Display label for UI presentation.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClassicChrome => "Fujifilm Classic Chrome",
            Self::Portra400 => "Kodak Portra 400",
            Self::Velvia50 => "Fujifilm Velvia 50",
            Self::TriX400 => "Kodak Tri-X 400",
            Self::IlfordHp5 => "Ilford HP5 Plus",
        }
    }
}

/// A 3D Color Look-Up Table (CLUT) with $N \times N \times N$ discrete RGB nodes.
///
/// Nodes are stored in standard `.cube` row-major order:
/// `index(r, g, b) = r + g * N + b * N * N` (Red changes fastest, then Green, then Blue).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorLut3d {
    size: usize,
    data: Vec<[f32; 3]>,
}

impl ColorLut3d {
    /// Minimum allowed LUT dimension along each axis.
    pub const MIN_SIZE: usize = 2;
    /// Maximum allowed LUT dimension along each axis (e.g. 65x65x65).
    pub const MAX_SIZE: usize = 65;

    /// Creates an identity 3D LUT of dimension `size` mapping `[r, g, b] -> [r, g, b]`.
    pub fn identity(size: usize) -> Result<Self, ColorError> {
        if !(Self::MIN_SIZE..=Self::MAX_SIZE).contains(&size) {
            return Err(ColorError::InvalidLutSize { size });
        }

        let total = size * size * size;
        let mut data = Vec::with_capacity(total);
        let denom = (size - 1) as f32;

        for b_idx in 0..size {
            let b = b_idx as f32 / denom;
            for g_idx in 0..size {
                let g = g_idx as f32 / denom;
                for r_idx in 0..size {
                    let r = r_idx as f32 / denom;
                    data.push([r, g, b]);
                }
            }
        }

        Ok(Self { size, data })
    }

    /// Constructs a `ColorLut3d` from raw node data, verifying dimensions and numerical validity.
    pub fn from_data(size: usize, data: Vec<[f32; 3]>) -> Result<Self, ColorError> {
        if !(Self::MIN_SIZE..=Self::MAX_SIZE).contains(&size) {
            return Err(ColorError::InvalidLutSize { size });
        }
        let expected = size * size * size;
        if data.len() != expected {
            return Err(ColorError::LutDataLengthMismatch {
                actual: data.len(),
                expected,
            });
        }
        for (i, node) in data.iter().enumerate() {
            if !node[0].is_finite() || !node[1].is_finite() || !node[2].is_finite() {
                return Err(ColorError::InvalidCubeFile {
                    reason: format!("non-finite float detected at node index {i}"),
                });
            }
        }

        Ok(Self { size, data })
    }

    /// Returns the 1D dimension $N$ along each axis.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Returns the raw node slice.
    #[must_use]
    pub fn data(&self) -> &[[f32; 3]] {
        &self.data
    }

    /// Safely fetches a node value at integer lattice coordinates `(r, g, b)`.
    #[inline]
    fn get_node(&self, r: usize, g: usize, b: usize) -> glam::Vec3 {
        let idx = r + g * self.size + b * self.size * self.size;
        match self.data.get(idx) {
            Some(&p) => glam::Vec3::new(p[0], p[1], p[2]),
            None => glam::Vec3::ZERO,
        }
    }

    /// Evaluates the 3D LUT at continuous coordinates using zero-allocation tetrahedral interpolation.
    ///
    /// Tetrahedral interpolation decomposes each sub-cube into 6 tetrahedra and samples only 4 vertices,
    /// avoiding diagonal color shift and preserving the neutral gray axis.
    #[inline]
    #[must_use]
    pub fn sample_tetrahedral(&self, rgb: glam::Vec3) -> glam::Vec3 {
        let n = self.size;
        let denom = (n - 1) as f32;

        let r_clamped = rgb.x.clamp(0.0, 1.0);
        let g_clamped = rgb.y.clamp(0.0, 1.0);
        let b_clamped = rgb.z.clamp(0.0, 1.0);

        let rx = r_clamped * denom;
        let gy = g_clamped * denom;
        let bz = b_clamped * denom;

        let max_idx = n.saturating_sub(2);
        let i = (rx.floor() as usize).min(max_idx);
        let j = (gy.floor() as usize).min(max_idx);
        let k = (bz.floor() as usize).min(max_idx);

        let dr = rx - i as f32;
        let dg = gy - j as f32;
        let db = bz - k as f32;

        let v000 = self.get_node(i, j, k);
        let v111 = self.get_node(i + 1, j + 1, k + 1);

        if dr >= dg {
            if dg >= db {
                // Case 1: dr >= dg >= db
                let v100 = self.get_node(i + 1, j, k);
                let v110 = self.get_node(i + 1, j + 1, k);
                v000 * (1.0 - dr) + v100 * (dr - dg) + v110 * (dg - db) + v111 * db
            } else if dr >= db {
                // Case 2: dr >= db > dg
                let v100 = self.get_node(i + 1, j, k);
                let v101 = self.get_node(i + 1, j, k + 1);
                v000 * (1.0 - dr) + v100 * (dr - db) + v101 * (db - dg) + v111 * dg
            } else {
                // Case 5: db > dr >= dg
                let v001 = self.get_node(i, j, k + 1);
                let v101 = self.get_node(i + 1, j, k + 1);
                v000 * (1.0 - db) + v001 * (db - dr) + v101 * (dr - dg) + v111 * dg
            }
        } else if dg >= db {
            if dr >= db {
                // Case 3: dg > dr >= db
                let v010 = self.get_node(i, j + 1, k);
                let v110 = self.get_node(i + 1, j + 1, k);
                v000 * (1.0 - dg) + v010 * (dg - dr) + v110 * (dr - db) + v111 * db
            } else {
                // Case 4: dg >= db > dr
                let v010 = self.get_node(i, j + 1, k);
                let v011 = self.get_node(i, j + 1, k + 1);
                v000 * (1.0 - dg) + v010 * (dg - db) + v011 * (db - dr) + v111 * dr
            }
        } else {
            // Case 6: db > dg > dr
            let v001 = self.get_node(i, j, k + 1);
            let v011 = self.get_node(i, j + 1, k + 1);
            v000 * (1.0 - db) + v001 * (db - dg) + v011 * (dg - dr) + v111 * dr
        }
    }

    /// Evaluates the 3D LUT at continuous coordinates using standard trilinear interpolation (8 vertices).
    #[inline]
    #[must_use]
    pub fn sample_trilinear(&self, rgb: glam::Vec3) -> glam::Vec3 {
        let n = self.size;
        let denom = (n - 1) as f32;

        let r_clamped = rgb.x.clamp(0.0, 1.0);
        let g_clamped = rgb.y.clamp(0.0, 1.0);
        let b_clamped = rgb.z.clamp(0.0, 1.0);

        let rx = r_clamped * denom;
        let gy = g_clamped * denom;
        let bz = b_clamped * denom;

        let max_idx = n.saturating_sub(2);
        let i = (rx.floor() as usize).min(max_idx);
        let j = (gy.floor() as usize).min(max_idx);
        let k = (bz.floor() as usize).min(max_idx);

        let dr = rx - i as f32;
        let dg = gy - j as f32;
        let db = bz - k as f32;

        let v000 = self.get_node(i, j, k);
        let v100 = self.get_node(i + 1, j, k);
        let v010 = self.get_node(i, j + 1, k);
        let v110 = self.get_node(i + 1, j + 1, k);
        let v001 = self.get_node(i, j, k + 1);
        let v101 = self.get_node(i + 1, j, k + 1);
        let v011 = self.get_node(i, j + 1, k + 1);
        let v111 = self.get_node(i + 1, j + 1, k + 1);

        let c00 = v000.lerp(v100, dr);
        let c01 = v001.lerp(v101, dr);
        let c10 = v010.lerp(v110, dr);
        let c11 = v011.lerp(v111, dr);

        let c0 = c00.lerp(c10, dg);
        let c1 = c01.lerp(c11, dg);

        c0.lerp(c1, db)
    }

    /// Parses an Adobe / DaVinci `.cube` formatted string into a `ColorLut3d`.
    pub fn from_cube_str(content: &str) -> Result<Self, ColorError> {
        let mut size = None;
        let mut nodes = Vec::new();

        for (line_num, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with("LUT_3D_SIZE") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 2 {
                    return Err(ColorError::InvalidCubeFile {
                        reason: format!("malformed LUT_3D_SIZE header at line {line_num}"),
                    });
                }
                let parsed_size =
                    parts[1]
                        .parse::<usize>()
                        .map_err(|e| ColorError::InvalidCubeFile {
                            reason: format!("invalid integer size `{}`: {e}", parts[1]),
                        })?;
                if !(Self::MIN_SIZE..=Self::MAX_SIZE).contains(&parsed_size) {
                    return Err(ColorError::InvalidLutSize { size: parsed_size });
                }
                size = Some(parsed_size);
                nodes.reserve(parsed_size * parsed_size * parsed_size);
                continue;
            }

            if line.starts_with("TITLE")
                || line.starts_with("DOMAIN_MIN")
                || line.starts_with("DOMAIN_MAX")
                || line.starts_with("LUT_1D_SIZE")
            {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 3 {
                let r = parts[0]
                    .parse::<f32>()
                    .map_err(|e| ColorError::InvalidCubeFile {
                        reason: format!("invalid float R `{}` at line {line_num}: {e}", parts[0]),
                    })?;
                let g = parts[1]
                    .parse::<f32>()
                    .map_err(|e| ColorError::InvalidCubeFile {
                        reason: format!("invalid float G `{}` at line {line_num}: {e}", parts[1]),
                    })?;
                let b = parts[2]
                    .parse::<f32>()
                    .map_err(|e| ColorError::InvalidCubeFile {
                        reason: format!("invalid float B `{}` at line {line_num}: {e}", parts[2]),
                    })?;

                nodes.push([r, g, b]);
            }
        }

        let lut_size = size.ok_or_else(|| ColorError::InvalidCubeFile {
            reason: "missing `LUT_3D_SIZE` header in .cube file".to_string(),
        })?;

        Self::from_data(lut_size, nodes)
    }

    /// Serializes the 3D LUT into standard Adobe / DaVinci `.cube` format.
    #[must_use]
    pub fn to_cube_str(&self, title: &str) -> String {
        let mut out = String::with_capacity(32 + self.data.len() * 32);
        out.push_str("# Created by Rraw (https://github.com/rdespres/rraw)\n");
        out.push_str(&format!("TITLE \"{title}\"\n"));
        out.push_str(&format!("LUT_3D_SIZE {}\n", self.size));
        out.push_str("DOMAIN_MIN 0.0 0.0 0.0\n");
        out.push_str("DOMAIN_MAX 1.0 1.0 1.0\n\n");

        for node in &self.data {
            out.push_str(&format!("{:.6} {:.6} {:.6}\n", node[0], node[1], node[2]));
        }

        out
    }

    /// Algorithmic film simulation profile generator.
    ///
    /// Generates high-fidelity 3D LUTs modeling the spectral emulsion characteristics of legendary films.
    pub fn generate_film_profile(
        profile: FilmSimulationProfile,
        size: usize,
    ) -> Result<Self, ColorError> {
        if !(Self::MIN_SIZE..=Self::MAX_SIZE).contains(&size) {
            return Err(ColorError::InvalidLutSize { size });
        }

        let total = size * size * size;
        let mut data = Vec::with_capacity(total);
        let denom = (size - 1) as f32;

        for b_idx in 0..size {
            let b = b_idx as f32 / denom;
            for g_idx in 0..size {
                let g = g_idx as f32 / denom;
                for r_idx in 0..size {
                    let r = r_idx as f32 / denom;
                    let input = glam::Vec3::new(r, g, b);
                    let transformed = match profile {
                        FilmSimulationProfile::ClassicChrome => Self::apply_classic_chrome(input),
                        FilmSimulationProfile::Portra400 => Self::apply_portra_400(input),
                        FilmSimulationProfile::Velvia50 => Self::apply_velvia_50(input),
                        FilmSimulationProfile::TriX400 => Self::apply_tri_x_400(input),
                        FilmSimulationProfile::IlfordHp5 => Self::apply_ilford_hp5(input),
                    };
                    data.push([transformed.x, transformed.y, transformed.z]);
                }
            }
        }

        Ok(Self { size, data })
    }

    // --- Algorithmic Film Emulation Transforms ---

    /// Fujifilm Classic Chrome emulation:
    /// Muted saturation, enhanced hard shadow contrast, slight cyan bias in skies, warm skin protection.
    fn apply_classic_chrome(rgb: glam::Vec3) -> glam::Vec3 {
        let lum = 0.2126 * rgb.x + 0.7152 * rgb.y + 0.0722 * rgb.z;

        // Hard shadow contrast with inflection around 0.4
        let curved_lum = if lum < 0.4 {
            lum * (0.85 + 0.375 * (lum / 0.4))
        } else {
            0.4 * 1.0 + (lum - 0.4) * 1.05
        }
        .clamp(0.0, 1.0);

        let lum_scale = if lum > 1e-5 { curved_lum / lum } else { 1.0 };
        let toned = rgb * lum_scale;

        // Muted global saturation (-18%)
        let sat_factor = 0.82;
        let mut out = glam::Vec3::new(
            curved_lum + (toned.x - curved_lum) * sat_factor,
            curved_lum + (toned.y - curved_lum) * sat_factor,
            curved_lum + (toned.z - curved_lum) * sat_factor,
        );

        // Sky cyan shift: when blue > red, nudge blue towards cyan
        if out.z > out.x {
            let sky_weight = (out.z - out.x).clamp(0.0, 1.0) * (out.z).clamp(0.0, 1.0);
            out.y += 0.04 * sky_weight; // boost green slightly in cyan sky
            out.x -= 0.02 * sky_weight; // attenuate red
        }

        out.clamp(glam::Vec3::ZERO, glam::Vec3::ONE)
    }

    /// Kodak Portra 400 emulation:
    /// Pastel low-contrast, gentle lifted toe, warm golden highlights, flattering portrait tones.
    fn apply_portra_400(rgb: glam::Vec3) -> glam::Vec3 {
        // Lifted toe: blacks never hit pure 0.0 (soft black floor at 0.02)
        let lift = 0.02;
        let r_lifted = lift + (1.0 - lift) * rgb.x.powf(0.96);
        let g_lifted = lift + (1.0 - lift) * rgb.y.powf(0.97);
        let b_lifted = lift + (1.0 - lift) * rgb.z.powf(1.02);

        // Gentle warm cast in midtones (Portra golden warmth)
        let lum = 0.2126 * r_lifted + 0.7152 * g_lifted + 0.0722 * b_lifted;
        let warm_weight = 4.0 * lum * (1.0 - lum).max(0.0); // bell curve peaked at 0.5

        let r_warm = r_lifted + 0.035 * warm_weight;
        let g_warm = g_lifted + 0.010 * warm_weight;
        let b_warm = (b_lifted - 0.025 * warm_weight).max(0.0);

        glam::Vec3::new(r_warm, g_warm, b_warm).clamp(glam::Vec3::ZERO, glam::Vec3::ONE)
    }

    /// Fujifilm Velvia 50 emulation:
    /// High-saturation positive slide film with deep blacks, high micro-contrast, emerald greens and vivid blues.
    fn apply_velvia_50(rgb: glam::Vec3) -> glam::Vec3 {
        // Steep S-curve
        let s_curve = |x: f32| -> f32 {
            if x <= 0.5 {
                2.0 * x * x
            } else {
                1.0 - 2.0 * (1.0 - x) * (1.0 - x)
            }
        };

        let r_c = s_curve(rgb.x.clamp(0.0, 1.0));
        let g_c = s_curve(rgb.y.clamp(0.0, 1.0));
        let b_c = s_curve(rgb.z.clamp(0.0, 1.0));

        let lum = 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c;

        // Vivid saturation boost (+30%)
        let sat_boost = 1.30;
        let r_sat = lum + (r_c - lum) * sat_boost;
        let g_sat = lum + (g_c - lum) * sat_boost;
        let b_sat = lum + (b_c - lum) * sat_boost;

        // Emerald greens and rich sky blues
        let mut out = glam::Vec3::new(r_sat, g_sat, b_sat);
        if out.y > out.x && out.y > out.z {
            out.y *= 1.05; // boost foliage greens
        }

        out.clamp(glam::Vec3::ZERO, glam::Vec3::ONE)
    }

    /// Kodak Tri-X 400 emulation:
    /// High-contrast panchromatic monochrome, rich shadow punch, silvery highlights.
    fn apply_tri_x_400(rgb: glam::Vec3) -> glam::Vec3 {
        // Tri-X panchromatic sensitivity weights
        let pan_lum = 0.26 * rgb.x + 0.65 * rgb.y + 0.09 * rgb.z;

        // Punchy high-contrast photographic S-curve with deep blacks
        let s = if pan_lum <= 0.45 {
            1.8 * pan_lum.powf(1.4)
        } else {
            1.0 - 1.4 * (1.0 - pan_lum).powf(1.5)
        }
        .clamp(0.0, 1.0);

        glam::Vec3::splat(s)
    }

    /// Ilford HP5 Plus emulation:
    /// Smooth extended latitude monochrome, open shadows, gentle highlight shoulder.
    fn apply_ilford_hp5(rgb: glam::Vec3) -> glam::Vec3 {
        // Classic HP5 panchromatic balance
        let pan_lum = 0.28 * rgb.x + 0.62 * rgb.y + 0.10 * rgb.z;

        // Extended latitude: lifted shadows, gentle compression in highlights
        let lift = 0.015;
        let s = lift + (1.0 - lift) * pan_lum.powf(0.95);

        glam::Vec3::splat(s.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clut3d_identity_exactness() -> Result<(), ColorError> {
        let lut = ColorLut3d::identity(17)?;
        assert_eq!(lut.size(), 17);
        assert_eq!(lut.data().len(), 17 * 17 * 17);

        // Test across a grid of points
        for r_step in 0..=10 {
            for g_step in 0..=10 {
                for b_step in 0..=10 {
                    let pt = glam::Vec3::new(
                        r_step as f32 / 10.0,
                        g_step as f32 / 10.0,
                        b_step as f32 / 10.0,
                    );
                    let sampled_tet = lut.sample_tetrahedral(pt);
                    let sampled_tri = lut.sample_trilinear(pt);

                    assert!(
                        (sampled_tet - pt).length() < 1e-5,
                        "Tetrahedral identity failed at {pt:?}, got {sampled_tet:?}"
                    );
                    assert!(
                        (sampled_tri - pt).length() < 1e-5,
                        "Trilinear identity failed at {pt:?}, got {sampled_tri:?}"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_clut3d_neutral_axis_preservation() -> Result<(), ColorError> {
        for profile in FilmSimulationProfile::ALL {
            let lut = ColorLut3d::generate_film_profile(profile, 17)?;

            // For monochrome profiles, R == G == B must hold exactly
            if profile == FilmSimulationProfile::TriX400
                || profile == FilmSimulationProfile::IlfordHp5
            {
                for step in 0..=20 {
                    let gray = glam::Vec3::splat(step as f32 / 20.0);
                    let sampled = lut.sample_tetrahedral(gray);
                    assert!(
                        (sampled.x - sampled.y).abs() < 1e-5
                            && (sampled.y - sampled.z).abs() < 1e-5,
                        "{profile:?} broke neutral axis: {sampled:?}"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_clut3d_cube_parser_roundtrip() -> Result<(), ColorError> {
        let lut = ColorLut3d::generate_film_profile(FilmSimulationProfile::ClassicChrome, 5)?;

        let cube_str = lut.to_cube_str("Classic Chrome Test");
        assert!(cube_str.contains("LUT_3D_SIZE 5"));

        let parsed = ColorLut3d::from_cube_str(&cube_str)?;
        assert_eq!(parsed.size(), 5);
        assert_eq!(parsed.data().len(), 125);

        for (a, b) in lut.data().iter().zip(parsed.data().iter()) {
            assert!((a[0] - b[0]).abs() < 1e-5);
            assert!((a[1] - b[1]).abs() < 1e-5);
            assert!((a[2] - b[2]).abs() < 1e-5);
        }
        Ok(())
    }

    #[test]
    fn test_clut3d_bounds_and_finite() -> Result<(), ColorError> {
        let lut = ColorLut3d::generate_film_profile(FilmSimulationProfile::Portra400, 17)?;

        // Out-of-gamut / out-of-bounds inputs should be safely clamped without panic
        let neg = lut.sample_tetrahedral(glam::Vec3::new(-1.0, -0.5, 0.2));
        assert!(neg.is_finite());
        let pos = lut.sample_tetrahedral(glam::Vec3::new(2.5, 1.2, 3.0));
        assert!(pos.is_finite());
        Ok(())
    }
}
