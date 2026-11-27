#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = rraw_decode::opcodes::parse_opcode_list(data);
    let _ = rraw_decode::opcodes::extract_dng_opcodes(data);
    let _ = rraw_decode::opcodes::extract_dng_warp_rectilinear(data);
});
