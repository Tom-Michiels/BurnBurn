//! Whole scrambled `.rd` file: the first byte selects the magic so other keys are explored too.
#![no_main]

use burnburn_ruida::{Magic, decode_rd_file};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = decode_rd_file(data, Magic::FIXTURE_0X88);
    if let [magic, rest @ ..] = data {
        let _ = decode_rd_file(rest, Magic(*magic));
    }
});
