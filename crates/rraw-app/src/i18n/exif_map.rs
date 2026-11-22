#![forbid(unsafe_code)]

//! Bidirectional translation mapping for EXIF metadata tags and standard enumerated values.

use crate::i18n::{Key, Language};

/// Maps standard rawler / TIFF / EXIF tag names to an i18n translation key.
#[must_use]
pub fn exif_tag_to_key(tag: &str) -> Option<Key> {
    match tag {
        "Aperture" => Some(Key::ExifTagAperture),
        "Artist" => Some(Key::ExifTagArtist),
        "Camera Make" => Some(Key::ExifTagCameraMake),
        "Camera Model" => Some(Key::ExifTagCameraModel),
        "Camera Owner" => Some(Key::ExifTagCameraOwner),
        "Camera Serial Number" => Some(Key::ExifTagCameraSerialNumber),
        "Color Space" => Some(Key::ExifTagColorSpace),
        "Copyright" => Some(Key::ExifTagCopyright),
        "Date/Time Original" => Some(Key::ExifTagDateTimeOriginal),
        "Digital Zoom Ratio" => Some(Key::ExifTagDigitalZoomRatio),
        "Exposure Bias" => Some(Key::ExifTagExposureBias),
        "Exposure Mode" => Some(Key::ExifTagExposureMode),
        "Exposure Program" => Some(Key::ExifTagExposureProgram),
        "Exposure Time" => Some(Key::ExifTagExposureTime),
        "Flash" => Some(Key::ExifTagFlash),
        "Focal Length" => Some(Key::ExifTagFocalLength),
        "Focal Length (35mm Eq.)" => Some(Key::ExifTagFocalLength35mmEq),
        "GPS Altitude" => Some(Key::ExifTagGpsAltitude),
        "GPS Latitude" => Some(Key::ExifTagGpsLatitude),
        "GPS Longitude" => Some(Key::ExifTagGpsLongitude),
        "ISO Sensitivity" => Some(Key::ExifTagIsoSensitivity),
        "Lens Make" => Some(Key::ExifTagLensMake),
        "Lens Model" => Some(Key::ExifTagLensModel),
        "Lens Serial Number" => Some(Key::ExifTagLensSerialNumber),
        "Light Source" => Some(Key::ExifTagLightSource),
        "Metering Mode" => Some(Key::ExifTagMeteringMode),
        "Orientation" => Some(Key::ExifTagOrientation),
        "Scene Capture Type" => Some(Key::ExifTagSceneCaptureType),
        "Sensing Method" => Some(Key::ExifTagSensingMethod),
        "Shutter Speed" => Some(Key::ExifTagShutterSpeed),
        "Software" => Some(Key::ExifTagSoftware),
        "White Balance Mode" => Some(Key::ExifTagWhiteBalanceMode),
        _ => None,
    }
}

/// Maps standard rawler / TIFF / EXIF string values to an i18n translation key.
#[must_use]
pub fn exif_val_to_key(val: &str) -> Option<Key> {
    match val {
        "Action" => Some(Key::ExifValAction),
        "Aperture priority" | "Aperture Priority" => Some(Key::ExifValAperturePriority),
        "Auto" => Some(Key::ExifValAuto),
        "Auto bracket" | "Auto Bracket" => Some(Key::ExifValAutoBracket),
        "Average" => Some(Key::ExifValAverage),
        "Center-weighted average" => Some(Key::ExifValCenterWeighted),
        "Cloudy Weather" | "Cloudy" => Some(Key::ExifValCloudyWeather),
        "Creative" => Some(Key::ExifValCreative),
        "Daylight" => Some(Key::ExifValDaylight),
        "Fine Weather" => Some(Key::ExifValFineWeather),
        "Flash" => Some(Key::ExifValFlash),
        "Off, Did not fire" | "Auto, Did not fire" | "Did not fire" => {
            Some(Key::ExifValFlashDidNotFire)
        }
        "Fired" | "Auto, Fired" => Some(Key::ExifValFlashFired),
        "Fluorescent" => Some(Key::ExifValFluorescent),
        "Landscape" => Some(Key::ExifValLandscape),
        "Manual" => Some(Key::ExifValManual),
        "Multi-spot" => Some(Key::ExifValMultiSpot),
        "Night Scene" => Some(Key::ExifValNightScene),
        "No Flash" => Some(Key::ExifValNoFlash),
        "Other" => Some(Key::ExifValOther),
        "Partial" => Some(Key::ExifValPartial),
        "Pattern" => Some(Key::ExifValPattern),
        "Portrait" => Some(Key::ExifValPortrait),
        "Program AE" => Some(Key::ExifValProgramAe),
        "Shade" => Some(Key::ExifValShade),
        "Shutter priority" | "Shutter Priority" => Some(Key::ExifValShutterPriority),
        "Spot" => Some(Key::ExifValSpot),
        "Standard" => Some(Key::ExifValStandard),
        "Tungsten (Incandescent)" => Some(Key::ExifValTungsten),
        "Unknown" => Some(Key::ExifValUnknown),
        _ => None,
    }
}

/// Translates an EXIF tag identifier into the active language, falling back to original if unmapped.
#[must_use]
pub fn translate_exif_tag(tag: &str, lang: Language) -> &str {
    if let Some(key) = exif_tag_to_key(tag) {
        key.translate(lang)
    } else {
        tag
    }
}

/// Translates an EXIF value string into the active language, falling back to original if unmapped.
#[must_use]
pub fn translate_exif_val(val: &str, lang: Language) -> &str {
    if let Some(key) = exif_val_to_key(val) {
        key.translate(lang)
    } else {
        val
    }
}
