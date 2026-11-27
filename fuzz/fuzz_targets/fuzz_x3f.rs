#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = rraw_decode::x3f::try_decode_x3f_bytes(data);
});
