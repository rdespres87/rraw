#![forbid(unsafe_code)]

//! Photo decoding from disk, thumbnail generation, and timelapse grading preparation.

use std::path::{Path, PathBuf};

use rraw_core::{ExecutionBackend, PipelineStageId, PipelineTiming, SignedCfaBuffer, StageTimer};
use rraw_decode::{PureRustRawDecoder, RawDecoder};
use rraw_storage::{DiskThumbnailCache, ThumbnailSize};
use sha2::{Digest, Sha256};

use super::orientation::{
    apply_exif_orientation_dyn_image, oriented_dimensions, read_exif_orientation,
};
use super::pipeline::render_preview_from_base;
use super::types::{BackgroundDecodePayload, BaseRgbPreview, LoadedPhotoView};
use crate::state::{load_photo_sidecars, DarkroomGradingState};

/// Returns the on-disk thumbnail cache directory.
#[must_use]
pub fn thumbnail_cache_dir() -> PathBuf {
    std::env::temp_dir().join("rraw_thumbnails")
}

/// Computes a fast, content-addressed 64-character hex hash for thumbnail disk caching.
#[must_use]
pub fn compute_thumbnail_content_hash(path: &Path) -> Option<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_str = canonical.to_string_lossy();
    let meta = std::fs::metadata(&canonical).ok()?;
    let len = meta.len();
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();

    let mut hasher = Sha256::new();
    hasher.update(b"RRAW_THUMB_V1:");
    hasher.update(path_str.as_bytes());
    hasher.update(b":");
    hasher.update(len.to_le_bytes());
    hasher.update(b":");
    hasher.update(mtime.to_le_bytes());

    let sidecar_xmp = canonical.with_extension("xmp");
    if let Ok(xmp_meta) = std::fs::metadata(&sidecar_xmp) {
        let xmp_mtime = xmp_meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        hasher.update(b":xmp:");
        hasher.update(xmp_mtime.to_le_bytes());
    }

    let result = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in result {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Some(hex)
}

/// Serializes an `egui::ColorImage` into binary bytes (`[w: u32][h: u32][pixels: [u8; 4]]`).
#[must_use]
pub fn serialize_color_image(img: &egui::ColorImage) -> Vec<u8> {
    let w = img.size[0] as u32;
    let h = img.size[1] as u32;
    let pixel_count = (w as usize).saturating_mul(h as usize);
    let mut buf = Vec::with_capacity(8 + pixel_count * 4);
    buf.extend_from_slice(&w.to_le_bytes());
    buf.extend_from_slice(&h.to_le_bytes());
    for px in &img.pixels {
        buf.extend_from_slice(&px.to_array());
    }
    buf
}

/// Deserializes binary bytes back into an `egui::ColorImage`.
#[must_use]
pub fn deserialize_color_image(data: &[u8]) -> Option<egui::ColorImage> {
    if data.len() < 8 {
        return None;
    }
    let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let expected_pixels = w.saturating_mul(h);
    let expected_bytes = 8 + expected_pixels.saturating_mul(4);
    if w == 0 || h == 0 || data.len() < expected_bytes {
        return None;
    }
    let mut pixels = Vec::with_capacity(expected_pixels);
    for chunk in data[8..expected_bytes].as_chunks::<4>().0 {
        pixels.push(egui::Color32::from_rgba_unmultiplied(
            chunk[0], chunk[1], chunk[2], chunk[3],
        ));
    }
    Some(egui::ColorImage::new([w, h], pixels))
}

/// Extracts a cropped, downscaled thumbnail `egui::ColorImage` from an existing rendered preview image.
#[must_use]
pub fn create_thumbnail_from_preview(
    preview: &egui::ColorImage,
    grading: &DarkroomGradingState,
    max_w: usize,
    max_h: usize,
) -> egui::ColorImage {
    let src_w = preview.size[0];
    let src_h = preview.size[1];
    if src_w == 0 || src_h == 0 {
        return egui::ColorImage::example();
    }

    let (u_min, u_max, v_min, v_max) = if grading.geometry.crop_enabled {
        (
            grading.geometry.crop_left.clamp(0.0, 0.95),
            grading.geometry.crop_right.clamp(0.05, 1.0),
            grading.geometry.crop_top.clamp(0.0, 0.95),
            grading.geometry.crop_bottom.clamp(0.05, 1.0),
        )
    } else {
        (0.0, 1.0, 0.0, 1.0)
    };

    let crop_w = (u_max - u_min).max(0.01);
    let crop_h = (v_max - v_min).max(0.01);
    let aspect = (crop_w * src_w as f32) / (crop_h * src_h as f32).max(1.0);

    let (dst_w, dst_h) = if aspect >= (max_w as f32 / max_h as f32) {
        let w = max_w.max(10);
        let h = ((w as f32 / aspect).round() as usize).clamp(10, max_h);
        (w, h)
    } else {
        let h = max_h.max(10);
        let w = ((h as f32 * aspect).round() as usize).clamp(10, max_w);
        (w, h)
    };

    let mut pixels = Vec::with_capacity(dst_w * dst_h);
    for dy in 0..dst_h {
        let frac_y = (dy as f32 + 0.5) / dst_h as f32;
        let v = (v_min + frac_y * crop_h).clamp(0.0, 1.0);
        let sy = ((v * src_h as f32).floor() as usize).min(src_h - 1);
        let row_start = sy * src_w;
        for dx in 0..dst_w {
            let frac_x = (dx as f32 + 0.5) / dst_w as f32;
            let u = (u_min + frac_x * crop_w).clamp(0.0, 1.0);
            let sx = ((u * src_w as f32).floor() as usize).min(src_w - 1);
            pixels.push(preview.pixels[row_start + sx]);
        }
    }

    egui::ColorImage::new([dst_w, dst_h], pixels)
}

/// Generates a fast ~140px RGB thumbnail image from a RAW or raster file applying its sidecar grading if available.
#[must_use]
pub fn generate_fast_thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let cache_dir = thumbnail_cache_dir();
    let cache_opt = DiskThumbnailCache::new(&cache_dir).ok();
    let hash_opt = compute_thumbnail_content_hash(path);

    if let (Some(cache), Some(hash)) = (&cache_opt, &hash_opt) {
        if let Some(bytes) = cache.get_thumbnail(hash, ThumbnailSize::Size160) {
            if let Some(img) = deserialize_color_image(&bytes) {
                return Some(img);
            }
        }
    }

    let generated = generate_fast_thumbnail_uncached(path)?;

    if let (Some(cache), Some(hash)) = (&cache_opt, &hash_opt) {
        let serialized = serialize_color_image(&generated);
        let _ = cache.store_thumbnail(hash, ThumbnailSize::Size160, &serialized);
    }

    Some(generated)
}

/// Computes the un-cached ~140px RGB thumbnail image from a RAW or raster file.
#[must_use]
pub fn generate_fast_thumbnail_uncached(path: &Path) -> Option<egui::ColorImage> {
    let ext_lower = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let header_orient = read_exif_orientation(path);

    if matches!(ext_lower.as_str(), "jpg" | "jpeg" | "png" | "tif" | "tiff") {
        if let Ok(dyn_img) = image::open(path) {
            let oriented = apply_exif_orientation_dyn_image(dyn_img, header_orient);
            let thumb = oriented.thumbnail(140, 100).to_rgb8();
            let (w, h) = thumb.dimensions();
            let raw_bytes = thumb.into_raw();
            let (grading_opt, _) = load_photo_sidecars(path);
            if let Some(grading) = grading_opt {
                let samples_rgb: Vec<[f32; 3]> = raw_bytes
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| {
                        [
                            (c[0] as f32 / 255.0).powf(2.2),
                            (c[1] as f32 / 255.0).powf(2.2),
                            (c[2] as f32 / 255.0).powf(2.2),
                        ]
                    })
                    .collect();
                let base = BaseRgbPreview {
                    width: w as usize,
                    height: h as usize,
                    samples_rgb: std::sync::Arc::from(samples_rgb),
                    as_shot_wb: [1.0, 1.0, 1.0],
                    cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                    semantic_masks: std::sync::Arc::new(rraw_ai::SemanticMaskSet {
                        width: w as usize,
                        height: h as usize,
                        subject: vec![0.0; w as usize * h as usize],
                        sky: vec![0.0; w as usize * h as usize],
                        background: vec![0.0; w as usize * h as usize],
                        portrait: vec![0.0; w as usize * h as usize],
                    }),
                    exif_orientation: 1,
                    matched_lens: None,
                    focal_length_mm: None,
                    aperture_f_number: None,
                };
                let (preview, _) = render_preview_from_base(&base, &grading);
                return Some(create_thumbnail_from_preview(&preview, &grading, 140, 100));
            }
            return Some(egui::ColorImage::from_rgb(
                [w as usize, h as usize],
                &raw_bytes,
            ));
        }
    }

    let decoder = PureRustRawDecoder::new();
    let decoded = decoder.decode_file(path).ok()?;
    let orient = if decoded.orientation > 1 {
        decoded.orientation
    } else {
        header_orient
    };
    let (grading_opt, _) = load_photo_sidecars(path);
    let grading = grading_opt.unwrap_or_default();

    let base = BaseRgbPreview::from_cfa_with_color_and_target_width(
        &decoded.cfa_signed,
        [
            decoded.wb_coeffs[0],
            decoded.wb_coeffs[1],
            decoded.wb_coeffs[2],
        ],
        decoded.color_matrix.as_ref(),
        140,
    )
    .with_exif_orientation(orient);

    let (preview, _) = render_preview_from_base(&base, &grading);
    Some(create_thumbnail_from_preview(&preview, &grading, 140, 100))
}

/// Decodes a RAW image file from disk and precomputes its base RGB preview.
#[must_use]
pub fn decode_photo_file_payload(
    target_path: &Path,
    is_prefetch: bool,
    epoch: u64,
) -> Option<BackgroundDecodePayload> {
    decode_photo_file_payload_profiled(target_path, is_prefetch, epoch).0
}

/// Decodes a RAW image file from disk and precomputes its base RGB preview, recording stage-by-stage timing.
pub fn decode_photo_file_payload_profiled(
    target_path: &Path,
    is_prefetch: bool,
    epoch: u64,
) -> (Option<BackgroundDecodePayload>, PipelineTiming) {
    let mut timing = PipelineTiming::new();
    let ext_lower = target_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let header_orient = read_exif_orientation(target_path);

    if matches!(ext_lower.as_str(), "jpg" | "jpeg" | "png" | "tif" | "tiff") {
        let t_decode = StageTimer::start(
            PipelineStageId::Stage1Decode,
            ExecutionBackend::CpuScalar,
            0,
        );
        if let Ok(dyn_img) = image::open(target_path) {
            t_decode.finish_and_record(&mut timing);
            let rgb = dyn_img.to_rgb8();
            let (raw_w, raw_h) = rgb.dimensions();
            let (orig_w, orig_h) = oriented_dimensions(raw_w, raw_h, header_orient);
            let num_px = (raw_w as usize).saturating_mul(raw_h as usize);

            let t_demosaic = StageTimer::start(
                PipelineStageId::Stage4Demosaic,
                ExecutionBackend::CpuScalar,
                num_px,
            );
            let base =
                BaseRgbPreview::from_rgb_image(&rgb, 1600).with_exif_orientation(header_orient);
            t_demosaic.finish_and_record(&mut timing);
            let dummy_cfa = SignedCfaBuffer::default();

            let file_name = target_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image")
                .to_string();

            let photo_view = LoadedPhotoView {
                path: target_path.to_path_buf(),
                camera_make: "Raster Image".to_string(),
                camera_model: format!("{} ({})", ext_lower.to_ascii_uppercase(), file_name),
                lens_model: String::new(),
                dimensions: (orig_w, orig_h),
                iso: 100,
                shutter_str: "-".to_string(),
                aperture_str: "-".to_string(),
                focal_length_str: String::new(),
                date_time: String::new(),
                exposure_bias: String::new(),
                rating: 0,
                exif_tags: vec![
                    ("Format".to_string(), ext_lower.to_ascii_uppercase()),
                    ("Dimensions".to_string(), format!("{orig_w}x{orig_h}")),
                    ("Orientation".to_string(), format!("{header_orient}")),
                ],
            };

            return (
                Some(BackgroundDecodePayload {
                    epoch,
                    path: target_path.to_path_buf(),
                    photo_view,
                    base_preview: base,
                    cfa: dummy_cfa,
                    is_prefetch,
                }),
                timing,
            );
        }
        return (None, timing);
    }

    let t_decode = StageTimer::start(PipelineStageId::Stage1Decode, ExecutionBackend::CpuRayon, 0);
    let decoder = PureRustRawDecoder::new();
    let decoded = match decoder.decode_file(target_path) {
        Ok(d) => {
            t_decode.finish_and_record(&mut timing);
            d
        }
        Err(_) => return (None, timing),
    };

    let orient = if decoded.orientation > 1 {
        decoded.orientation
    } else {
        header_orient
    };
    let meta = decoded.metadata;
    let num_pixels = meta.width.saturating_mul(meta.height);

    let t_calib = StageTimer::start(
        PipelineStageId::Stage2Calibration,
        ExecutionBackend::CpuScalar,
        num_pixels,
    );
    let make = meta.camera_make.clone();
    let model = meta.camera_model.clone();
    let make_model = format!("{make} {model}");
    let camera_str = make_model.trim().to_string();
    let camera = if camera_str.is_empty() {
        "Digital RAW Camera".to_string()
    } else {
        camera_str
    };

    let iso = meta.iso.unwrap_or(100);
    let aperture_str = meta
        .aperture_f_number
        .map(|f| format!("f/{f:.1}"))
        .unwrap_or_else(|| "f/2.8".to_string());
    let focal_str = meta
        .focal_length_mm
        .map(|f| format!("{f:.1} mm"))
        .unwrap_or_default();
    let lens_str = meta.lens_model.clone().unwrap_or_default();

    let shutter_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Shutter Speed")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "1/125 s".to_string());

    let date_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Date/Time Original")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let bias_str = decoded
        .exif_tags
        .iter()
        .find(|(k, _)| k == "Exposure Bias")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let upright_dims = oriented_dimensions(meta.width as u32, meta.height as u32, orient);

    let photo_view = LoadedPhotoView {
        path: target_path.to_path_buf(),
        camera_make: make,
        camera_model: camera,
        lens_model: lens_str,
        dimensions: upright_dims,
        iso,
        shutter_str,
        aperture_str,
        focal_length_str: focal_str,
        date_time: date_str,
        exposure_bias: bias_str,
        rating: meta.rating,
        exif_tags: decoded.exif_tags,
    };
    t_calib.finish_and_record(&mut timing);

    let t_color = StageTimer::start(
        PipelineStageId::Stage3ColorWb,
        ExecutionBackend::CpuScalar,
        num_pixels,
    );
    let matched_lens = rraw_optics::LensfunDatabase::global().match_metadata(&meta);
    t_color.finish_and_record(&mut timing);

    let t_demosaic = StageTimer::start(
        PipelineStageId::Stage4Demosaic,
        ExecutionBackend::CpuRayon,
        num_pixels,
    );
    let mut base = BaseRgbPreview::from_cfa_with_color(
        &decoded.cfa_signed,
        [
            decoded.wb_coeffs[0],
            decoded.wb_coeffs[1],
            decoded.wb_coeffs[2],
        ],
        decoded.color_matrix.as_ref(),
    )
    .with_exif_orientation(orient);
    base.matched_lens = Some(matched_lens);
    base.focal_length_mm = meta.focal_length_mm;
    base.aperture_f_number = meta.aperture_f_number;
    t_demosaic.finish_and_record(&mut timing);

    (
        Some(BackgroundDecodePayload {
            epoch,
            path: target_path.to_path_buf(),
            photo_view,
            base_preview: base,
            cfa: decoded.cfa_signed,
            is_prefetch,
        }),
        timing,
    )
}

/// Computes per-frame grading states for a timelapse sequence, applying optional Holy Grail
/// keyframe ramping (between first and last frame) and temporal exposure deflicker (`ΔEV`).
#[must_use]
pub fn prepare_timelapse_frame_gradings(
    frames: &[(&BaseRgbPreview, DarkroomGradingState)],
    keyframe_ramping_enabled: bool,
    deflicker_config: &rraw_export::TimelapseDeflickerConfig,
) -> (Vec<DarkroomGradingState>, Vec<f32>) {
    let n = frames.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }

    let mut gradings: Vec<DarkroomGradingState> = frames.iter().map(|(_, g)| g.clone()).collect();

    if keyframe_ramping_enabled && n >= 2 {
        let g_first = &gradings[0];
        let g_last = &gradings[n - 1];
        let kfs = [
            rraw_export::TimelapseKeyframe {
                frame_index: 0,
                exposure_ev: g_first.tone.exposure_ev,
                kelvin: g_first.color.kelvin,
                tint: g_first.color.tint,
                contrast: g_first.tone.contrast,
                saturation: g_first.tone.saturation,
            },
            rraw_export::TimelapseKeyframe {
                frame_index: n - 1,
                exposure_ev: g_last.tone.exposure_ev,
                kelvin: g_last.color.kelvin,
                tint: g_last.color.tint,
                contrast: g_last.tone.contrast,
                saturation: g_last.tone.saturation,
            },
        ];

        let interpolated = rraw_export::interpolate_keyframes(n, &kfs);
        for (g, kf) in gradings.iter_mut().zip(interpolated.iter()) {
            g.tone.exposure_ev = kf.exposure_ev;
            g.color.kelvin = kf.kelvin;
            g.color.tint = kf.tint;
            g.tone.contrast = kf.contrast;
            g.tone.saturation = kf.saturation;
        }
    }

    let mut measured_log_lumas = Vec::with_capacity(n);
    for (base, _) in frames {
        let flat_rgb: Vec<f32> = base
            .samples_rgb
            .iter()
            .flat_map(|px| [px[0], px[1], px[2]])
            .collect();
        let sensor_log_y = rraw_export::compute_frame_log_luminance(&flat_rgb);
        measured_log_lumas.push(sensor_log_y);
    }

    prepare_timelapse_gradings_from_lumas(
        &gradings,
        &measured_log_lumas,
        keyframe_ramping_enabled,
        deflicker_config,
    )
}

/// Computes per-frame timelapse grading states from pre-measured frame sensor log-luminances without
/// retaining decoded full-resolution RGB buffers in memory ($O(1)$ frame memory footprint).
#[must_use]
pub fn prepare_timelapse_gradings_from_lumas(
    initial_gradings: &[DarkroomGradingState],
    sensor_log_lumas: &[f32],
    keyframe_ramping_enabled: bool,
    deflicker_config: &rraw_export::TimelapseDeflickerConfig,
) -> (Vec<DarkroomGradingState>, Vec<f32>) {
    let n = initial_gradings.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }

    let mut gradings = initial_gradings.to_vec();

    if keyframe_ramping_enabled && n >= 2 {
        let g_first = &gradings[0];
        let g_last = &gradings[n - 1];
        let kfs = [
            rraw_export::TimelapseKeyframe {
                frame_index: 0,
                exposure_ev: g_first.tone.exposure_ev,
                kelvin: g_first.color.kelvin,
                tint: g_first.color.tint,
                contrast: g_first.tone.contrast,
                saturation: g_first.tone.saturation,
            },
            rraw_export::TimelapseKeyframe {
                frame_index: n - 1,
                exposure_ev: g_last.tone.exposure_ev,
                kelvin: g_last.color.kelvin,
                tint: g_last.color.tint,
                contrast: g_last.tone.contrast,
                saturation: g_last.tone.saturation,
            },
        ];

        let interpolated = rraw_export::interpolate_keyframes(n, &kfs);
        for (g, kf) in gradings.iter_mut().zip(interpolated.iter()) {
            g.tone.exposure_ev = kf.exposure_ev;
            g.color.kelvin = kf.kelvin;
            g.color.tint = kf.tint;
            g.tone.contrast = kf.contrast;
            g.tone.saturation = kf.saturation;
        }
    }

    let mut measured_log_lumas = Vec::with_capacity(n);
    for (idx, g) in gradings.iter().enumerate() {
        let sensor_log_y = sensor_log_lumas.get(idx).copied().unwrap_or(0.0);
        measured_log_lumas.push(sensor_log_y + g.tone.exposure_ev);
    }

    let offsets_ev =
        rraw_export::compute_deflicker_ev_offsets(&measured_log_lumas, deflicker_config);
    for (g, &delta_ev) in gradings.iter_mut().zip(offsets_ev.iter()) {
        g.tone.exposure_ev = (g.tone.exposure_ev + delta_ev).clamp(-5.0, 5.0);
    }

    (gradings, offsets_ev)
}
