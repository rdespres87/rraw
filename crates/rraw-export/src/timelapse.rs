#![forbid(unsafe_code)]

//! Temporal anti-flicker (deflicker), Holy Grail keyframe ramping, and numbered sequence export
//! for RAW timelapse photography and high-bit-depth video assembly.

use crate::error::ExportError;
use std::path::{Path, PathBuf};

/// Standard cinema, broadcast, and high-frame-rate video cadences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TimelapseFrameRate {
    #[default]
    Fps24,
    Fps25,
    Fps30,
    Fps60,
}

impl TimelapseFrameRate {
    pub const ALL: [Self; 4] = [Self::Fps24, Self::Fps25, Self::Fps30, Self::Fps60];

    /// Returns the integer frames-per-second value.
    #[must_use]
    pub const fn fps(self) -> u32 {
        match self {
            Self::Fps24 => 24,
            Self::Fps25 => 25,
            Self::Fps30 => 30,
            Self::Fps60 => 60,
        }
    }

    /// Human-readable UI label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fps24 => "24 fps (Cinéma)",
            Self::Fps25 => "25 fps (PAL / Broadcast)",
            Self::Fps30 => "30 fps (Web / NTSC)",
            Self::Fps60 => "60 fps (Haute Fréquence)",
        }
    }

    /// Computes total sequence duration in seconds for `frame_count` frames.
    #[must_use]
    pub fn duration_seconds(self, frame_count: usize) -> f32 {
        frame_count as f32 / (self.fps() as f32)
    }
}

/// Keyframe parameters for Holy Grail day-to-night timelapse interpolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelapseKeyframe {
    pub frame_index: usize,
    pub exposure_ev: f32,
    pub kelvin: f32,
    pub tint: f32,
    pub contrast: f32,
    pub saturation: f32,
}

/// Temporal exposure smoothing configuration for eliminating mechanical aperture and shutter flicker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelapseDeflickerConfig {
    pub enabled: bool,
    /// Half-window radius in frames (`1..=15`; radius `3` averages over a 7-frame window).
    pub window_radius: usize,
    /// Blend strength of the temporal EV compensation (`0.0..=1.0`).
    pub strength: f32,
    /// Safety clamp in EV (`0.1..=3.0`) preventing extreme correction on occluded frames.
    pub max_correction_ev: f32,
}

impl Default for TimelapseDeflickerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            window_radius: 3,
            strength: 1.0,
            max_correction_ev: 1.5,
        }
    }
}

/// Computes the robust average log2 (EV) luminance of a linear RGB frame buffer (`[R, G, B, ...]`).
///
/// Pixels in deep noise floor (`< 1e-5`) or sensor saturation (`> 0.99`) are weighted softly
/// to prevent specular sun highlights from skewing the temporal luminance curve.
#[must_use]
pub fn compute_frame_log_luminance(rgb_linear: &[f32]) -> f32 {
    if rgb_linear.len() < 3 {
        return -2.473_931_f32; // log2(0.18) middle gray fallback
    }

    let mut weighted_log_sum = 0.0_f64;
    let mut weight_sum = 0.0_f64;

    for &[r_raw, g_raw, b_raw] in rgb_linear.as_chunks::<3>().0 {
        let r = r_raw.max(0.0);
        let g = g_raw.max(0.0);
        let b = b_raw.max(0.0);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

        if luma > 1e-5 {
            // Midtone-emphasized weight: down-weights clipped highlights (> 0.98) and deep blacks
            let weight = if luma > 0.98 { 0.15_f64 } else { 1.0_f64 };
            let log_y = f64::from(luma.log2());
            weighted_log_sum += log_y * weight;
            weight_sum += weight;
        }
    }

    if weight_sum > 1e-9 {
        (weighted_log_sum / weight_sum) as f32
    } else {
        -2.473_931_f32
    }
}

/// Computes per-frame exposure compensation offsets (`ΔEV`) to suppress temporal flicker
/// while preserving smooth low-frequency day-to-night luminance transitions.
#[must_use]
pub fn compute_deflicker_ev_offsets(
    frame_log_luminances: &[f32],
    config: &TimelapseDeflickerConfig,
) -> Vec<f32> {
    let n = frame_log_luminances.len();
    if n <= 1 || !config.enabled || config.strength <= 1e-5 {
        return vec![0.0; n];
    }

    let radius = config.window_radius.clamp(1, 30);
    let strength = config.strength.clamp(0.0, 1.0);
    let max_ev = config.max_correction_ev.clamp(0.05, 4.0);

    // Local linear regression (Savitzky-Golay degree-1 / triangular moving trend)
    // preserves linear exposure ramps without boundary lag while cancelling high-frequency jitter.
    let mut offsets = Vec::with_capacity(n);

    for i in 0..n {
        let start = i.saturating_sub(radius);
        let end = (i + radius).min(n - 1);

        let mut sum_w = 0.0_f64;
        let mut sum_wx = 0.0_f64;
        let mut sum_wy = 0.0_f64;
        let mut sum_wxx = 0.0_f64;
        let mut sum_wxy = 0.0_f64;

        for (j, &val) in frame_log_luminances
            .iter()
            .enumerate()
            .take(end + 1)
            .skip(start)
        {
            let dist = (j as isize - i as isize).unsigned_abs();
            // Triangular kernel weight: higher weight near frame i
            let w = (radius + 1 - dist.min(radius)) as f64;
            let x = (j as isize - i as isize) as f64;
            let y = f64::from(val);

            sum_w += w;
            sum_wx += w * x;
            sum_wy += w * y;
            sum_wxx += w * x * x;
            sum_wxy += w * x * y;
        }

        let denom = sum_w * sum_wxx - sum_wx * sum_wx;
        let target_log_luma = if denom.abs() > 1e-9 {
            // Local weighted linear regression evaluated at x = 0 (intercept a = (sum_wy * sum_wxx - sum_wx * sum_wxy) / denom)
            ((sum_wy * sum_wxx - sum_wx * sum_wxy) / denom) as f32
        } else if sum_w > 1e-9 {
            (sum_wy / sum_w) as f32
        } else {
            frame_log_luminances[i]
        };

        let raw_delta = (target_log_luma - frame_log_luminances[i]) * strength;
        offsets.push(raw_delta.clamp(-max_ev, max_ev));
    }

    offsets
}

/// Interpolates development parameters across `total_frames` from sparse user keyframes.
#[must_use]
pub fn interpolate_keyframes(
    total_frames: usize,
    keyframes: &[TimelapseKeyframe],
) -> Vec<TimelapseKeyframe> {
    if total_frames == 0 {
        return Vec::new();
    }
    if keyframes.is_empty() {
        return (0..total_frames)
            .map(|frame_index| TimelapseKeyframe {
                frame_index,
                exposure_ev: 0.0,
                kelvin: 5500.0,
                tint: 0.0,
                contrast: 0.0,
                saturation: 0.0,
            })
            .collect();
    }

    let mut sorted = keyframes.to_vec();
    sorted.sort_by_key(|k| k.frame_index);
    sorted.dedup_by_key(|k| k.frame_index);

    let first = sorted[0];
    let last = sorted[sorted.len() - 1];

    (0..total_frames)
        .map(|idx| {
            if idx <= first.frame_index {
                let mut k = first;
                k.frame_index = idx;
                return k;
            }
            if idx >= last.frame_index {
                let mut k = last;
                k.frame_index = idx;
                return k;
            }

            // Find bounding segment
            let mut seg_start = first;
            let mut seg_end = last;
            for pair in sorted.windows(2) {
                if idx >= pair[0].frame_index && idx <= pair[1].frame_index {
                    seg_start = pair[0];
                    seg_end = pair[1];
                    break;
                }
            }

            let span = (seg_end.frame_index.saturating_sub(seg_start.frame_index)).max(1) as f32;
            let t = ((idx.saturating_sub(seg_start.frame_index)) as f32 / span).clamp(0.0, 1.0);

            TimelapseKeyframe {
                frame_index: idx,
                exposure_ev: seg_start.exposure_ev
                    + (seg_end.exposure_ev - seg_start.exposure_ev) * t,
                kelvin: seg_start.kelvin + (seg_end.kelvin - seg_start.kelvin) * t,
                tint: seg_start.tint + (seg_end.tint - seg_start.tint) * t,
                contrast: seg_start.contrast + (seg_end.contrast - seg_start.contrast) * t,
                saturation: seg_start.saturation + (seg_end.saturation - seg_start.saturation) * t,
            }
        })
        .collect()
}

/// Formats a 5-digit zero-padded sequence filename (`<prefix>_00001.<ext>`) compatible with
/// DaVinci Resolve, Adobe Premiere, Final Cut Pro, and FFmpeg image2 demuxers.
#[must_use]
pub fn format_sequence_filename(prefix: &str, frame_index: usize, ext: &str) -> String {
    let clean_prefix = if prefix.trim().is_empty() {
        "timelapse"
    } else {
        prefix.trim().trim_end_matches('_')
    };
    let clean_ext = ext.trim().trim_start_matches('.');
    format!("{clean_prefix}_{frame_index:05}.{clean_ext}")
}

/// Writes a JSON sequence manifest (`timelapse_sequence.json`) containing frame rate, duration,
/// per-frame deflicker `ΔEV` curve, and ready-to-run FFmpeg ProRes / H.265 assembly commands.
pub fn write_timelapse_manifest(
    output_dir: &Path,
    prefix: &str,
    ext: &str,
    fps: TimelapseFrameRate,
    frame_count: usize,
    deflicker_offsets_ev: &[f32],
) -> Result<PathBuf, ExportError> {
    std::fs::create_dir_all(output_dir)?;

    let clean_prefix = if prefix.trim().is_empty() {
        "timelapse"
    } else {
        prefix.trim().trim_end_matches('_')
    };
    let clean_ext = ext.trim().trim_start_matches('.');
    let fps_val = fps.fps();
    let duration_sec = fps.duration_seconds(frame_count);
    let pattern = format!("{clean_prefix}_%05d.{clean_ext}");

    let max_abs_offset = deflicker_offsets_ev
        .iter()
        .fold(0.0_f32, |acc, &v| acc.max(v.abs()));

    let offsets_formatted: Vec<String> = deflicker_offsets_ev
        .iter()
        .map(|v| format!("{v:.4}"))
        .collect();

    let json = format!(
        r#"{{
  "generator": "Rraw Timelapse & Temporal Deflicker Engine",
  "sequence_pattern": "{pattern}",
  "fps": {fps_val},
  "frame_count": {frame_count},
  "duration_seconds": {duration_sec:.2},
  "max_deflicker_compensation_ev": {max_abs_offset:.4},
  "deflicker_offsets_ev": [{offsets_list}],
  "ffmpeg_prores422hq_cmd": "ffmpeg -framerate {fps_val} -start_number 1 -i {pattern} -c:v prores_ks -profile:v 3 -pix_fmt yuv422p10le {clean_prefix}_prores422hq.mov",
  "ffmpeg_hevc10bit_cmd": "ffmpeg -framerate {fps_val} -start_number 1 -i {pattern} -c:v libx265 -crf 16 -pix_fmt yuv420p10le {clean_prefix}_hevc10bit.mp4"
}}
"#,
        offsets_list = offsets_formatted.join(", ")
    );

    let manifest_path = output_dir.join("timelapse_sequence.json");
    std::fs::write(&manifest_path, json)?;
    Ok(manifest_path)
}
