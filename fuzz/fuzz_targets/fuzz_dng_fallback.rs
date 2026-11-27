#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let _ = rraw_decode::dng_fallback::try_decode_fallback_bytes(Path::new("fuzz.dng"), data);
});
