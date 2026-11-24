#![forbid(unsafe_code)]

//! Pure-Rust EXIF tag formatting and DNG opcode extraction for decoded RAW files.

use std::path::Path;

use rraw_core::PhotoMetadata;

use crate::{makernotes, opcodes};

/// Extracts DNG opcodes and EXIF metadata from `path`, enriching `metadata`, `orientation`, and `exif_tags`.
pub fn extract_dng_and_exif_metadata(
    path: &Path,
    metadata: &mut PhotoMetadata,
    orientation: &mut u16,
    exif_tags: &mut Vec<(String, String)>,
) {
    let Ok(rawsource) = rawler::rawsource::RawSource::new(path) else {
        return;
    };

    apply_dng_opcodes(&rawsource, metadata, exif_tags);

    let Ok(decoder) = rawler::get_decoder(&rawsource) else {
        return;
    };
    let Ok(rmeta) = decoder.raw_metadata(&rawsource, &rawler::decoders::RawDecodeParams::default())
    else {
        return;
    };

    let exif = &rmeta.exif;
    if let Some(eo) = exif.orientation {
        if (1..=8).contains(&eo) && *orientation == 1 {
            *orientation = eo;
        }
    }

    populate_exposure_and_lens_tags(&rawsource, &rmeta, metadata, exif_tags);
    populate_camera_and_scene_tags(&rmeta, exif_tags);
    populate_gps_tags(&rmeta, exif_tags);
}

fn apply_dng_opcodes(
    rawsource: &rawler::rawsource::RawSource,
    metadata: &mut PhotoMetadata,
    exif_tags: &mut Vec<(String, String)>,
) {
    for op in &opcodes::extract_dng_opcodes(rawsource) {
        match op {
            opcodes::DngOpcode::WarpRectilinear {
                planes,
                coefficients,
                ..
            } => {
                metadata.dng_warp_rectilinear = Some(rraw_core::DngWarpRectilinearData {
                    planes: *planes,
                    coefficients: coefficients.clone(),
                });
                exif_tags.push((
                    "DNG Lens Opcode".to_owned(),
                    format!(
                        "WarpRectilinear ({planes} planes, {} coeffs)",
                        coefficients.len()
                    ),
                ));
            }
            opcodes::DngOpcode::GainMap { .. } | opcodes::DngOpcode::FixVignetteRadial { .. } => {
                metadata.vignette_handled_by_gainmap = true;
            }
            _ => {}
        }
    }
}

fn populate_exposure_and_lens_tags(
    rawsource: &rawler::rawsource::RawSource,
    rmeta: &rawler::decoders::RawMetadata,
    metadata: &mut PhotoMetadata,
    exif_tags: &mut Vec<(String, String)>,
) {
    let exif = &rmeta.exif;

    if let Some(focal) = &exif.focal_length {
        if focal.d != 0 {
            let fl = focal.n as f32 / focal.d as f32;
            metadata.focal_length_mm = Some(fl);
            exif_tags.push(("Focal Length".to_owned(), format!("{fl:.1} mm")));
        }
    }

    if let Some(fnum) = &exif.fnumber {
        if fnum.d != 0 {
            let fn_val = fnum.n as f32 / fnum.d as f32;
            metadata.aperture_f_number = Some(fn_val);
            exif_tags.push(("Aperture".to_owned(), format!("f/{fn_val:.1}")));
        }
    }

    if let Some(exp_time) = &exif.exposure_time {
        if exp_time.n != 0 && exp_time.d != 0 {
            let formatted = if exp_time.n >= exp_time.d {
                format!("{:.1} s", exp_time.n as f32 / exp_time.d as f32)
            } else {
                format!("{}/{} s", exp_time.n, exp_time.d)
            };
            exif_tags.push(("Shutter Speed".to_owned(), formatted));
        }
    }

    if let Some(iso) = exif.iso_speed_ratings.or(exif.iso_speed.map(|v| v as u16)) {
        metadata.iso = Some(u32::from(iso));
        exif_tags.push(("ISO Sensitivity".to_owned(), format!("{iso}")));
    }

    if let Some(lens) = &exif.lens_model {
        let clean = makernotes::sanitize_lens_name(lens);
        metadata.lens_model = Some(clean.clone());
        exif_tags.push(("Lens Model".to_owned(), clean));
    } else if let Some(lens_desc) = &rmeta.lens {
        let clean = makernotes::sanitize_lens_name(&lens_desc.lens_name);
        metadata.lens_model = Some(clean.clone());
        exif_tags.push(("Lens Model".to_owned(), clean));
    } else if let Some(lens_name) = makernotes::resolve_universal_lens(
        rawsource,
        &metadata.camera_make,
        &metadata.camera_model,
        metadata.focal_length_mm,
        metadata.aperture_f_number,
    ) {
        metadata.lens_model = Some(lens_name.clone());
        exif_tags.push(("Lens Model".to_owned(), lens_name));
    }

    if let Some(make) = &exif.lens_make {
        exif_tags.push(("Lens Make".to_owned(), make.clone()));
    }

    if let Some(bias) = &exif.exposure_bias {
        if bias.d != 0 {
            let b_val = bias.n as f32 / bias.d as f32;
            exif_tags.push(("Exposure Bias".to_owned(), format!("{b_val:+.1} EV")));
        }
    }

    if let Some(ma) = &exif.max_aperture_value {
        if ma.d != 0 {
            let ma_val = 2.0_f32.powf(ma.n as f32 / ma.d as f32 / 2.0);
            exif_tags.push(("Max Aperture".to_owned(), format!("f/{ma_val:.1}")));
        }
    }
}

fn populate_camera_and_scene_tags(
    rmeta: &rawler::decoders::RawMetadata,
    exif_tags: &mut Vec<(String, String)>,
) {
    let exif = &rmeta.exif;
    push_non_empty(
        exif_tags,
        "Date/Time Original",
        exif.date_time_original.as_deref(),
    );

    if let Some(prog) = exif.exposure_program {
        exif_tags.push((
            "Exposure Program".to_owned(),
            exposure_program_label(prog).to_owned(),
        ));
    }
    if let Some(meter) = exif.metering_mode {
        exif_tags.push((
            "Metering Mode".to_owned(),
            metering_mode_label(meter).to_owned(),
        ));
    }
    if let Some(wb_mode) = exif.white_balance {
        let wb_str = if wb_mode == 0 { "Auto" } else { "Manual" };
        exif_tags.push(("White Balance Mode".to_owned(), wb_str.to_owned()));
    }
    if let Some(flash_val) = exif.flash {
        let flash_str = if flash_val & 1 == 0 {
            "Did not fire"
        } else {
            "Fired"
        };
        exif_tags.push(("Flash".to_owned(), flash_str.to_owned()));
    }

    push_non_empty(exif_tags, "Artist", exif.artist.as_deref());
    push_non_empty(exif_tags, "Copyright", exif.copyright.as_deref());
    push_non_empty(
        exif_tags,
        "Camera Serial Number",
        exif.serial_number.as_deref(),
    );
    push_non_empty(
        exif_tags,
        "Lens Serial Number",
        exif.lens_serial_number.as_deref(),
    );
    push_non_empty(exif_tags, "Camera Owner", exif.owner_name.as_deref());

    if let Some(cs) = exif.color_space {
        exif_tags.push(("Color Space".to_owned(), color_space_label(cs).to_owned()));
    }
    if let Some(em) = exif.exposure_mode {
        exif_tags.push((
            "Exposure Mode".to_owned(),
            exposure_mode_label(em).to_owned(),
        ));
    }
    if let Some(sct) = exif.scene_capture_type {
        exif_tags.push((
            "Scene Capture Type".to_owned(),
            scene_capture_label(sct).to_owned(),
        ));
    }
    if let Some(ls) = exif.light_source {
        exif_tags.push(("Light Source".to_owned(), light_source_label(ls).to_owned()));
    }

    push_non_empty(exif_tags, "Create Date", exif.create_date.as_deref());
    push_non_empty(exif_tags, "Modify Date", exif.modify_date.as_deref());
    push_non_empty(
        exif_tags,
        "Timezone Offset",
        exif.offset_time_original
            .as_deref()
            .or(exif.offset_time.as_deref()),
    );
    if let Some(num) = exif.image_number {
        exif_tags.push(("Image Number".to_owned(), num.to_string()));
    }
    push_non_empty(exif_tags, "User Comment", exif.user_comment.as_deref());
}

fn populate_gps_tags(rmeta: &rawler::decoders::RawMetadata, exif_tags: &mut Vec<(String, String)>) {
    let Some(gps) = &rmeta.exif.gps else {
        return;
    };
    if let (Some(lat), Some(lat_ref)) = (&gps.gps_latitude, &gps.gps_latitude_ref) {
        if lat[0].d != 0 && lat[1].d != 0 && lat[2].d != 0 {
            let deg = f64::from(lat[0].n) / f64::from(lat[0].d);
            let min = f64::from(lat[1].n) / f64::from(lat[1].d);
            let sec = f64::from(lat[2].n) / f64::from(lat[2].d);
            exif_tags.push((
                "GPS Latitude".to_owned(),
                format!("{deg:.0}° {min:.0}' {sec:.2}\" {lat_ref}"),
            ));
        }
    }
    if let (Some(lon), Some(lon_ref)) = (&gps.gps_longitude, &gps.gps_longitude_ref) {
        if lon[0].d != 0 && lon[1].d != 0 && lon[2].d != 0 {
            let deg = f64::from(lon[0].n) / f64::from(lon[0].d);
            let min = f64::from(lon[1].n) / f64::from(lon[1].d);
            let sec = f64::from(lon[2].n) / f64::from(lon[2].d);
            exif_tags.push((
                "GPS Longitude".to_owned(),
                format!("{deg:.0}° {min:.0}' {sec:.2}\" {lon_ref}"),
            ));
        }
    }
    if let Some(alt) = &gps.gps_altitude {
        if alt.d != 0 {
            let alt_m = f64::from(alt.n) / f64::from(alt.d);
            exif_tags.push(("GPS Altitude".to_owned(), format!("{alt_m:.1} m")));
        }
    }
}

fn push_non_empty(exif_tags: &mut Vec<(String, String)>, label: &str, value: Option<&str>) {
    if let Some(v) = value {
        if !v.is_empty() {
            exif_tags.push((label.to_owned(), v.to_owned()));
        }
    }
}

/// Returns a human-readable EXIF TIFF orientation description.
#[must_use]
pub const fn orientation_label(orientation: u16) -> &'static str {
    match orientation {
        1 => "1 - Horizontal (Normal)",
        2 => "2 - Mirror Horizontal",
        3 => "3 - Rotate 180°",
        4 => "4 - Mirror Vertical",
        5 => "5 - Mirror Horizontal & Rotate 270° CW (Portrait)",
        6 => "6 - Rotate 90° CW (Portrait)",
        7 => "7 - Mirror Horizontal & Rotate 90° CW (Portrait)",
        8 => "8 - Rotate 270° CW / 90° CCW (Portrait)",
        _ => "1 - Normal",
    }
}

const fn exposure_program_label(prog: u16) -> &'static str {
    match prog {
        1 => "Manual",
        2 => "Program AE",
        3 => "Aperture Priority",
        4 => "Shutter Priority",
        5 => "Creative",
        6 => "Action",
        7 => "Portrait",
        8 => "Landscape",
        _ => "Normal",
    }
}

const fn metering_mode_label(meter: u16) -> &'static str {
    match meter {
        1 => "Average",
        2 => "Center-Weighted Average",
        3 => "Spot",
        4 => "Multi-Spot",
        5 => "Multi-Segment",
        6 => "Partial",
        _ => "Other",
    }
}

const fn color_space_label(cs: u16) -> &'static str {
    match cs {
        1 => "sRGB",
        2 => "Adobe RGB",
        65535 => "Uncalibrated",
        _ => "Custom / Other",
    }
}

const fn exposure_mode_label(em: u16) -> &'static str {
    match em {
        0 => "Auto",
        1 => "Manual",
        2 => "Auto Bracket",
        _ => "Other",
    }
}

const fn scene_capture_label(sct: u16) -> &'static str {
    match sct {
        0 => "Standard",
        1 => "Landscape",
        2 => "Portrait",
        3 => "Night Scene",
        _ => "Other",
    }
}

const fn light_source_label(ls: u16) -> &'static str {
    match ls {
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
    }
}

#[cfg(test)]
mod tests {
    use super::{exposure_program_label, light_source_label, orientation_label};

    #[test]
    fn formats_known_and_unknown_exif_enums() {
        assert_eq!(orientation_label(6), "6 - Rotate 90° CW (Portrait)");
        assert_eq!(orientation_label(99), "1 - Normal");
        assert_eq!(exposure_program_label(3), "Aperture Priority");
        assert_eq!(light_source_label(21), "D65");
        assert_eq!(light_source_label(255), "Unknown");
    }
}
