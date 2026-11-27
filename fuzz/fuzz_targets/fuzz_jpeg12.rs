#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = rraw_decode::jpeg12::is_jpeg12_stream(data);
    let _ = rraw_decode::jpeg12::decode_jpeg_gray(data);
});
