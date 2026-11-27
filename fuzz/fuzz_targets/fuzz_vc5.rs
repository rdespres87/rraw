#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let width: usize = if !data.is_empty() {
        (data[0] as usize % 255) + 2
    } else {
        64
    };

    let height: usize = if data.len() >= 2 {
        (data[1] as usize % 255) + 2
    } else {
        64
    };

    let _ =
        rraw_decode::vc5::decode_vc5_bayer(data, width, height, rraw_core::BayerOrigin::Rggb, 12);
});
