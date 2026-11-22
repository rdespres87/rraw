#![forbid(unsafe_code)]

//! Pure-Rust RAW decoding contracts, signed normalization, and DNG metadata extraction.

use std::path::Path;

use rraw_core::{CfaPattern, ColorLabel, CoreError, PhotoMetadata, PickStatus, SignedCfaBuffer};
use thiserror::Error;

pub mod cfa;
pub mod makernotes;
pub mod noise_profile;
pub mod opcodes;
pub mod synthetic;

pub use cfa::*;
use noise_profile::estimate_noise_profile_wavelet_mad;

/// Typed errors produced by `rraw-decode`.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DecodeError {
    #[error("RAW I/O error: {0}")]
    Io(String),
    #[error("unsupported RAW format: {0}")]
    UnsupportedFormat(String),
    #[error("malformed RAW metadata: {0}")]
    MalformedMetadata(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}

/// Output produced by a `RawDecoder` implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedRaw {
    pub metadata: PhotoMetadata,
    pub cfa_signed: SignedCfaBuffer,
    pub wb_coeffs: [f32; 4],
    pub color_matrix: Option<[f32; 9]>,
    pub exif_tags: Vec<(String, String)>,
    /// Standard EXIF TIFF Orientation code (`1..=8`, where `6` = 90° CW Portrait, `8` = 270° CW Portrait).
    pub orientation: u16,
}

/// Pure-Rust contract for read-only RAW file decoding.
pub trait RawDecoder: Send + Sync {
    /// Decodes a RAW file at `path` into canonical metadata and signed normalized CFA data.
    fn decode_file(&self, path: &Path) -> Result<DecodedRaw, DecodeError>;
}

/// Concrete pure-safe-Rust RAW decoder wrapping `rawler` and DNG parsers.
#[derive(Debug, Default, Clone, Copy)]
pub struct PureRustRawDecoder;

impl PureRustRawDecoder {
    /// Creates a new pure-Rust decoder instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RawDecoder for PureRustRawDecoder {
    fn decode_file(&self, path: &Path) -> Result<DecodedRaw, DecodeError> {
        let raw_image = rawler::decode_file(path).map_err(|e| {
            DecodeError::Io(format!("failed to decode RAW file {}: {e}", path.display()))
        })?;

        let raw_samples = match &raw_image.data {
            rawler::RawImageData::Integer(samples) => samples.as_slice(),
            rawler::RawImageData::Float(_) => {
                return Err(DecodeError::UnsupportedFormat(
                    "floating point raw image format not yet supported".to_string(),
                ));
            }
        };

        let cfa_pattern = resolve_cfa_pattern_from_rawler(&raw_image.camera.cfa);

        // Crop optical black / sensor margins preserving CFA pattern phase
        let (width, height, cropped_samples_storage) =
            if let Some(crop) = raw_image.crop_area.or(raw_image.active_area) {
                let (crop_x, crop_y, crop_w, crop_h) = align_crop_to_cfa_phase(
                    &cfa_pattern,
                    crop.x(),
                    crop.y(),
                    crop.width(),
                    crop.height(),
                    raw_image.width,
                    raw_image.height,
                );
                if crop_x + crop_w <= raw_image.width
                    && crop_y + crop_h <= raw_image.height
                    && crop_w > 0
                    && crop_h > 0
                {
                    let mut cropped = Vec::with_capacity(crop_w * crop_h);
                    for row in crop_y..(crop_y + crop_h) {
                        let start = row * raw_image.width + crop_x;
                        let end = start + crop_w;
                        cropped.extend_from_slice(&raw_samples[start..end]);
                    }
                    (crop_w, crop_h, Some(cropped))
                } else {
                    (raw_image.width, raw_image.height, None)
                }
            } else {
                (raw_image.width, raw_image.height, None)
            };
        let raw_samples = cropped_samples_storage.as_deref().unwrap_or(raw_samples);

        let black_levels = raw_image.blacklevel.as_bayer_array();
        let white_levels = raw_image.whitelevel.as_bayer_array();

        let mut dummy_meta = PhotoMetadata {
            camera_make: raw_image.clean_make.clone(),
            camera_model: raw_image.clean_model.clone(),
            lens_model: None,
            focal_length_mm: None,
            aperture_f_number: None,
            iso: None,
            width,
            height,
            black_levels,
            white_levels,
            cfa_pattern,
            noise_profile: rraw_core::NoiseProfile::new(
                [0.001; 3],
                [0.0001; 3],
                rraw_core::NoiseProfileSource::WaveletMadFallback,
            )?,
            vignette_handled_by_gainmap: false,
            rating: 0,
            pick: PickStatus::Unflagged,
            label: ColorLabel::None,
            keywords: Vec::new(),
        };

        // Extract color matrix and white balance coefficients
        let mut wb_coeffs = raw_image.wb_coeffs;
        if !wb_coeffs[0].is_finite() || wb_coeffs[0] <= 0.0 {
            wb_coeffs[0] = 2.0;
        }
        if !wb_coeffs[1].is_finite() || wb_coeffs[1] <= 0.0 {
            wb_coeffs[1] = 1.0;
        }
        if !wb_coeffs[2].is_finite() || wb_coeffs[2] <= 0.0 {
            wb_coeffs[2] = 1.5;
        }

        let color_matrix = raw_image
            .color_matrix_find_first([
                rawler::imgop::xyz::Illuminant::D65,
                rawler::imgop::xyz::Illuminant::D50,
                rawler::imgop::xyz::Illuminant::A,
            ])
            .and_then(|(_, mat)| {
                if mat.len() >= 9 {
                    let mut arr = [0.0_f32; 9];
                    arr.copy_from_slice(&mat[0..9]);
                    Some(arr)
                } else {
                    None
                }
            });

        let mut exif_tags = Vec::with_capacity(64);
        exif_tags.push(("Camera Make".to_string(), dummy_meta.camera_make.clone()));
        exif_tags.push(("Camera Model".to_string(), dummy_meta.camera_model.clone()));
        exif_tags.push((
            "Dimensions".to_string(),
            format!(
                "{width} x {height} ({:.1} MP)",
                (width * height) as f32 / 1_000_000.0
            ),
        ));
        exif_tags.push((
            "As-Shot WB Multipliers".to_string(),
            format!(
                "R={:.3}, G={:.3}, B={:.3}",
                wb_coeffs[0], wb_coeffs[1], wb_coeffs[2]
            ),
        ));

        let mut orientation = match raw_image.orientation.to_u16() {
            code @ 1..=8 => code,
            _ => 1,
        };

        // Read complete EXIF tags using rawler's IFD parser
        if let Ok(rawsource) = rawler::rawsource::RawSource::new(path) {
            if let Ok(decoder) = rawler::get_decoder(&rawsource) {
                if let Ok(rmeta) =
                    decoder.raw_metadata(&rawsource, &rawler::decoders::RawDecodeParams::default())
                {
                    let exif = &rmeta.exif;

                    if let Some(eo) = exif.orientation {
                        if (1..=8).contains(&eo) && orientation == 1 {
                            orientation = eo;
                        }
                    }

                    if let Some(focal) = &exif.focal_length {
                        if focal.d != 0 {
                            let fl = focal.n as f32 / focal.d as f32;
                            dummy_meta.focal_length_mm = Some(fl);
                            exif_tags.push(("Focal Length".to_string(), format!("{fl:.1} mm")));
                        }
                    }

                    if let Some(fnum) = &exif.fnumber {
                        if fnum.d != 0 {
                            let fn_val = fnum.n as f32 / fnum.d as f32;
                            dummy_meta.aperture_f_number = Some(fn_val);
                            exif_tags.push(("Aperture".to_string(), format!("f/{fn_val:.1}")));
                        }
                    }

                    if let Some(exp_time) = &exif.exposure_time {
                        if exp_time.n != 0 && exp_time.d != 0 {
                            if exp_time.n >= exp_time.d {
                                exif_tags.push((
                                    "Shutter Speed".to_string(),
                                    format!("{:.1} s", exp_time.n as f32 / exp_time.d as f32),
                                ));
                            } else {
                                exif_tags.push((
                                    "Shutter Speed".to_string(),
                                    format!("{}/{} s", exp_time.n, exp_time.d),
                                ));
                            }
                        }
                    }

                    if let Some(iso) = exif.iso_speed_ratings.or(exif.iso_speed.map(|v| v as u16)) {
                        dummy_meta.iso = Some(iso as u32);
                        exif_tags.push(("ISO Sensitivity".to_string(), format!("{iso}")));
                    }

                    if let Some(lens) = &exif.lens_model {
                        let clean = makernotes::sanitize_lens_name(lens);
                        dummy_meta.lens_model = Some(clean.clone());
                        exif_tags.push(("Lens Model".to_string(), clean));
                    } else if let Some(lens_desc) = &rmeta.lens {
                        let clean = makernotes::sanitize_lens_name(&lens_desc.lens_name);
                        dummy_meta.lens_model = Some(clean.clone());
                        exif_tags.push(("Lens Model".to_string(), clean));
                    } else if let Some(lens_name) = makernotes::resolve_universal_lens(
                        &rawsource,
                        &dummy_meta.camera_make,
                        &dummy_meta.camera_model,
                        dummy_meta.focal_length_mm,
                        dummy_meta.aperture_f_number,
                    ) {
                        dummy_meta.lens_model = Some(lens_name.clone());
                        exif_tags.push(("Lens Model".to_string(), lens_name));
                    }

                    if let Some(make) = &exif.lens_make {
                        exif_tags.push(("Lens Make".to_string(), make.clone()));
                    }

                    if let Some(bias) = &exif.exposure_bias {
                        if bias.d != 0 {
                            let b_val = bias.n as f32 / bias.d as f32;
                            exif_tags
                                .push(("Exposure Bias".to_string(), format!("{b_val:+.1} EV")));
                        }
                    }

                    if let Some(dt) = &exif.date_time_original {
                        exif_tags.push(("Date/Time Original".to_string(), dt.clone()));
                    }

                    if let Some(prog) = exif.exposure_program {
                        let prog_str = match prog {
                            1 => "Manual",
                            2 => "Program AE",
                            3 => "Aperture Priority",
                            4 => "Shutter Priority",
                            5 => "Creative",
                            6 => "Action",
                            7 => "Portrait",
                            8 => "Landscape",
                            _ => "Normal",
                        };
                        exif_tags.push(("Exposure Program".to_string(), prog_str.to_string()));
                    }

                    if let Some(meter) = exif.metering_mode {
                        let meter_str = match meter {
                            1 => "Average",
                            2 => "Center-Weighted Average",
                            3 => "Spot",
                            4 => "Multi-Spot",
                            5 => "Multi-Segment",
                            6 => "Partial",
                            _ => "Other",
                        };
                        exif_tags.push(("Metering Mode".to_string(), meter_str.to_string()));
                    }

                    if let Some(wb_mode) = exif.white_balance {
                        let wb_str = if wb_mode == 0 { "Auto" } else { "Manual" };
                        exif_tags.push(("White Balance Mode".to_string(), wb_str.to_string()));
                    }

                    if let Some(flash_val) = exif.flash {
                        let flash_str = if flash_val & 1 == 0 {
                            "Did not fire"
                        } else {
                            "Fired"
                        };
                        exif_tags.push(("Flash".to_string(), flash_str.to_string()));
                    }

                    if let Some(artist) = &exif.artist {
                        if !artist.is_empty() {
                            exif_tags.push(("Artist".to_string(), artist.clone()));
                        }
                    }

                    if let Some(copy) = &exif.copyright {
                        if !copy.is_empty() {
                            exif_tags.push(("Copyright".to_string(), copy.clone()));
                        }
                    }

                    if let Some(sn) = &exif.serial_number {
                        if !sn.is_empty() {
                            exif_tags.push(("Camera Serial Number".to_string(), sn.clone()));
                        }
                    }

                    if let Some(lsn) = &exif.lens_serial_number {
                        if !lsn.is_empty() {
                            exif_tags.push(("Lens Serial Number".to_string(), lsn.clone()));
                        }
                    }

                    if let Some(owner) = &exif.owner_name {
                        if !owner.is_empty() {
                            exif_tags.push(("Camera Owner".to_string(), owner.clone()));
                        }
                    }

                    if let Some(cs) = exif.color_space {
                        let cs_str = match cs {
                            1 => "sRGB",
                            2 => "Adobe RGB",
                            65535 => "Uncalibrated",
                            _ => "Custom / Other",
                        };
                        exif_tags.push(("Color Space".to_string(), cs_str.to_string()));
                    }

                    if let Some(em) = exif.exposure_mode {
                        let em_str = match em {
                            0 => "Auto",
                            1 => "Manual",
                            2 => "Auto Bracket",
                            _ => "Other",
                        };
                        exif_tags.push(("Exposure Mode".to_string(), em_str.to_string()));
                    }

                    if let Some(sct) = exif.scene_capture_type {
                        let sct_str = match sct {
                            0 => "Standard",
                            1 => "Landscape",
                            2 => "Portrait",
                            3 => "Night Scene",
                            _ => "Other",
                        };
                        exif_tags.push(("Scene Capture Type".to_string(), sct_str.to_string()));
                    }

                    if let Some(ls) = exif.light_source {
                        let ls_str = match ls {
                            1 => "Daylight",
                            2 => "Fluorescent",
                            3 => "Tungsten (Incandescent)",
                            4 => "Flash",
                            9 => "Fine Weather",
                            10 => "Cloudy Weather",
                            11 => "Shade",
                            17 => "Standard Light A",
                            18 => "Standard Light B",
                            19 => "Standard Light C",
                            20 => "D55",
                            21 => "D65",
                            22 => "D75",
                            23 => "D50",
                            _ => "Unknown",
                        };
                        exif_tags.push(("Light Source".to_string(), ls_str.to_string()));
                    }

                    if let Some(cd) = &exif.create_date {
                        if !cd.is_empty() {
                            exif_tags.push(("Create Date".to_string(), cd.clone()));
                        }
                    }

                    if let Some(md) = &exif.modify_date {
                        if !md.is_empty() {
                            exif_tags.push(("Modify Date".to_string(), md.clone()));
                        }
                    }

                    if let Some(off) = exif
                        .offset_time_original
                        .as_ref()
                        .or(exif.offset_time.as_ref())
                    {
                        if !off.is_empty() {
                            exif_tags.push(("Timezone Offset".to_string(), off.clone()));
                        }
                    }

                    if let Some(num) = exif.image_number {
                        exif_tags.push(("Image Number".to_string(), num.to_string()));
                    }

                    if let Some(cmt) = &exif.user_comment {
                        if !cmt.is_empty() {
                            exif_tags.push(("User Comment".to_string(), cmt.clone()));
                        }
                    }

                    if let Some(ma) = &exif.max_aperture_value {
                        if ma.d != 0 {
                            let ma_val = 2.0f32.powf(ma.n as f32 / ma.d as f32 / 2.0);
                            exif_tags.push(("Max Aperture".to_string(), format!("f/{ma_val:.1}")));
                        }
                    }

                    if let Some(gps) = &exif.gps {
                        if let (Some(lat), Some(lat_ref)) =
                            (&gps.gps_latitude, &gps.gps_latitude_ref)
                        {
                            if lat[0].d != 0 && lat[1].d != 0 && lat[2].d != 0 {
                                let deg = lat[0].n as f64 / lat[0].d as f64;
                                let min = lat[1].n as f64 / lat[1].d as f64;
                                let sec = lat[2].n as f64 / lat[2].d as f64;
                                exif_tags.push((
                                    "GPS Latitude".to_string(),
                                    format!("{deg:.0}° {min:.0}' {sec:.2}\" {lat_ref}"),
                                ));
                            }
                        }
                        if let (Some(lon), Some(lon_ref)) =
                            (&gps.gps_longitude, &gps.gps_longitude_ref)
                        {
                            if lon[0].d != 0 && lon[1].d != 0 && lon[2].d != 0 {
                                let deg = lon[0].n as f64 / lon[0].d as f64;
                                let min = lon[1].n as f64 / lon[1].d as f64;
                                let sec = lon[2].n as f64 / lon[2].d as f64;
                                exif_tags.push((
                                    "GPS Longitude".to_string(),
                                    format!("{deg:.0}° {min:.0}' {sec:.2}\" {lon_ref}"),
                                ));
                            }
                        }
                        if let Some(alt) = &gps.gps_altitude {
                            if alt.d != 0 {
                                let alt_m = alt.n as f64 / alt.d as f64;
                                exif_tags
                                    .push(("GPS Altitude".to_string(), format!("{alt_m:.1} m")));
                            }
                        }
                    }
                }
            }
        }

        exif_tags.push((
            "CFA Pattern".to_string(),
            format!("{:?}", dummy_meta.cfa_pattern),
        ));
        exif_tags.push((
            "Sensor Black Level".to_string(),
            format!(
                "{:.1}, {:.1}, {:.1}, {:.1}",
                black_levels[0], black_levels[1], black_levels[2], black_levels[3]
            ),
        ));
        exif_tags.push((
            "Sensor White Level".to_string(),
            format!(
                "{:.1}, {:.1}, {:.1}, {:.1}",
                white_levels[0], white_levels[1], white_levels[2], white_levels[3]
            ),
        ));

        if black_levels[0] >= white_levels[0] {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid black/white level range: black={}, white={}",
                black_levels[0], white_levels[0]
            )));
        }

        let signed_cfa = normalize_cfa_signed(raw_samples, width, height, &dummy_meta)?;

        // If no noise profile was loaded, compute wavelet MAD fallback
        let estimated_noise = estimate_noise_profile_wavelet_mad(&signed_cfa)?;
        dummy_meta.noise_profile = estimated_noise;

        let orient_str = match orientation {
            1 => "1 - Horizontal (Normal)",
            2 => "2 - Mirror Horizontal",
            3 => "3 - Rotate 180°",
            4 => "4 - Mirror Vertical",
            5 => "5 - Mirror Horizontal & Rotate 270° CW (Portrait)",
            6 => "6 - Rotate 90° CW (Portrait)",
            7 => "7 - Mirror Horizontal & Rotate 90° CW (Portrait)",
            8 => "8 - Rotate 270° CW / 90° CCW (Portrait)",
            _ => "1 - Normal",
        };
        exif_tags.push(("Orientation".to_string(), orient_str.to_string()));

        Ok(DecodedRaw {
            metadata: dummy_meta,
            cfa_signed: signed_cfa,
            wb_coeffs,
            color_matrix,
            exif_tags,
            orientation,
        })
    }
}

/// Performs Stage 1 signed black/white normalization `(raw - B_c) / (W_c - B_c)`
/// without clamping sub-black read-noise values to `0.0`.
///
/// Validates buffer vs `metadata` dimensions, rejects zero/overflow dimensions and
/// sample length mismatches, and verifies all active CFA channel spans upfront
/// (`0..4` for Bayer, `0..3` for X-Trans) before entering the pixel loop.
pub fn normalize_cfa_signed(
    raw_samples: &[u16],
    width: usize,
    height: usize,
    metadata: &PhotoMetadata,
) -> Result<SignedCfaBuffer, DecodeError> {
    if width != metadata.width || height != metadata.height {
        return Err(DecodeError::MalformedMetadata(format!(
            "dimension mismatch: buffer {width}x{height} != metadata {}x{}",
            metadata.width, metadata.height
        )));
    }
    let Some(expected) = width.checked_mul(height) else {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    };
    if expected == 0 {
        return Err(CoreError::InvalidDimensions { width, height }.into());
    }
    if raw_samples.len() != expected {
        return Err(CoreError::BufferLengthMismatch {
            expected,
            actual: raw_samples.len(),
        }
        .into());
    }

    let active_channels = match metadata.cfa_pattern {
        CfaPattern::Bayer(_) => 4,
        CfaPattern::XTrans(_) => 3,
    };
    let mut inv_spans = [0.0_f32; 4];
    for (idx, inv_span) in inv_spans.iter_mut().enumerate().take(active_channels) {
        let black = metadata.black_levels[idx];
        let white = metadata.white_levels[idx];
        let span = white - black;
        if !black.is_finite() || !white.is_finite() || !span.is_finite() || span <= 0.0 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid white-black levels at channel index {idx}: black={black}, white={white}"
            )));
        }
        *inv_span = 1.0 / span;
    }

    let mut out = Vec::with_capacity(expected);
    match metadata.cfa_pattern {
        CfaPattern::Bayer(_) => {
            let b0 = metadata.black_levels[0];
            let b1 = metadata.black_levels[1];
            let b2 = metadata.black_levels[2];
            let b3 = metadata.black_levels[3];
            let inv0 = inv_spans[0];
            let inv1 = inv_spans[1];
            let inv2 = inv_spans[2];
            let inv3 = inv_spans[3];

            for y in 0..height {
                let (black_even, inv_even, black_odd, inv_odd) = if y.is_multiple_of(2) {
                    (b0, inv0, b1, inv1)
                } else {
                    (b2, inv2, b3, inv3)
                };

                let row_start = y * width;
                let row_end = row_start + width;
                let row_samples = &raw_samples[row_start..row_end];

                let (chunks, remainder) = row_samples.as_chunks::<2>();
                for chunk in chunks {
                    out.push((f32::from(chunk[0]) - black_even) * inv_even);
                    out.push((f32::from(chunk[1]) - black_odd) * inv_odd);
                }
                if let Some(&last) = remainder.first() {
                    out.push((f32::from(last) - black_even) * inv_even);
                }
            }
        }
        CfaPattern::XTrans(_) => {
            for y in 0..height {
                let row_start = y * width;
                for x in 0..width {
                    let level_idx = metadata.cfa_pattern.color_at(x, y).channel_index();
                    let black = metadata.black_levels[level_idx];
                    let inv = inv_spans[level_idx];
                    out.push((f32::from(raw_samples[row_start + x]) - black) * inv);
                }
            }
        }
    }

    Ok(SignedCfaBuffer::new(
        width,
        height,
        metadata.cfa_pattern,
        out,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_core::{
        BayerOrigin, CfaPattern, ColorLabel, NoiseProfile, NoiseProfileSource, PickStatus,
    };

    fn synthetic_metadata(width: usize, height: usize) -> Result<PhotoMetadata, CoreError> {
        Ok(PhotoMetadata {
            camera_make: "Synthetic".to_owned(),
            camera_model: "TestCam".to_owned(),
            lens_model: None,
            focal_length_mm: None,
            aperture_f_number: None,
            iso: Some(100),
            width,
            height,
            black_levels: [512.0; 4],
            white_levels: [16383.0; 4],
            cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
            noise_profile: NoiseProfile::new(
                [0.001; 3],
                [0.0001; 3],
                NoiseProfileSource::DngTag0xc761,
            )?,
            vignette_handled_by_gainmap: false,
            rating: 0,
            pick: PickStatus::Unflagged,
            label: ColorLabel::None,
            keywords: Vec::new(),
        })
    }

    #[test]
    fn signed_normalization_preserves_sub_black_samples() -> Result<(), DecodeError> {
        let meta = synthetic_metadata(2, 2)?;
        // Value 500 is below black level 512 and must remain strictly negative.
        let raw = [500_u16, 512_u16, 524_u16, 16383_u16];
        let signed = normalize_cfa_signed(&raw, 2, 2, &meta)?;
        assert!(signed.samples()[0] < 0.0);
        assert!((signed.samples()[1] - 0.0).abs() < 1e-7);
        assert!(signed.samples()[2] > 0.0);
        assert!((signed.samples()[3] - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn signed_normalization_handles_xtrans_and_rejects_degenerate_span() -> Result<(), DecodeError>
    {
        let mut meta = synthetic_metadata(6, 1)?;
        meta.cfa_pattern = CfaPattern::fujifilm_xtrans();
        meta.black_levels = [100.0, 200.0, 300.0, 0.0];
        meta.white_levels = [1100.0, 1200.0, 1300.0, 0.0];
        // Row 0 of Fujifilm X-Trans: [G, B, G, G, R, G]
        let raw = [200_u16, 250_u16, 300_u16, 200_u16, 100_u16, 200_u16];
        let signed = normalize_cfa_signed(&raw, 6, 1, &meta)?;
        assert!((signed.samples()[0] - 0.0).abs() < 1e-6); // G at black=200
        assert!(signed.samples()[1] < 0.0); // B=250 < black=300
        assert!((signed.samples()[4] - 0.0).abs() < 1e-6); // R at black=100

        meta.white_levels[0] = meta.black_levels[0];
        assert!(matches!(
            normalize_cfa_signed(&raw, 6, 1, &meta),
            Err(DecodeError::MalformedMetadata(_))
        ));
        Ok(())
    }

    #[test]
    fn signed_normalization_validates_dimensions_length_and_unvisited_channels(
    ) -> Result<(), DecodeError> {
        let meta_2x2 = synthetic_metadata(2, 2)?;

        // Buffer length mismatch
        assert_eq!(
            normalize_cfa_signed(&[512_u16; 3], 2, 2, &meta_2x2),
            Err(DecodeError::Core(CoreError::BufferLengthMismatch {
                expected: 4,
                actual: 3
            }))
        );

        // Dimension mismatch against metadata
        assert!(matches!(
            normalize_cfa_signed(&[512_u16; 4], 4, 1, &meta_2x2),
            Err(DecodeError::MalformedMetadata(_))
        ));

        // Zero dimensions
        let meta_zero = synthetic_metadata(0, 2)?;
        assert_eq!(
            normalize_cfa_signed(&[], 0, 2, &meta_zero),
            Err(DecodeError::Core(CoreError::InvalidDimensions {
                width: 0,
                height: 2
            }))
        );

        // Multiplication overflow
        let meta_overflow = synthetic_metadata(usize::MAX, 2)?;
        assert_eq!(
            normalize_cfa_signed(&[], usize::MAX, 2, &meta_overflow),
            Err(DecodeError::Core(CoreError::InvalidDimensions {
                width: usize::MAX,
                height: 2
            }))
        );

        // 1x1 Bayer image only visits channel 0, yet invalid channel 3 must be rejected upfront
        let mut meta_1x1 = synthetic_metadata(1, 1)?;
        meta_1x1.white_levels[3] = meta_1x1.black_levels[3];
        assert!(matches!(
            normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1),
            Err(DecodeError::MalformedMetadata(_))
        ));

        // 1x1 Bayer image with NaN in unvisited channel 2 must also be rejected upfront
        let mut meta_1x1_nan = synthetic_metadata(1, 1)?;
        meta_1x1_nan.black_levels[2] = f32::NAN;
        assert!(matches!(
            normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1_nan),
            Err(DecodeError::MalformedMetadata(_))
        ));

        // 1x1 X-Trans image visits only Green, yet invalid channel 2 (Blue) must be rejected
        let mut meta_1x1_xtrans = synthetic_metadata(1, 1)?;
        meta_1x1_xtrans.cfa_pattern = CfaPattern::fujifilm_xtrans();
        meta_1x1_xtrans.white_levels[2] = meta_1x1_xtrans.black_levels[2];
        assert!(matches!(
            normalize_cfa_signed(&[512_u16], 1, 1, &meta_1x1_xtrans),
            Err(DecodeError::MalformedMetadata(_))
        ));

        Ok(())
    }
}
