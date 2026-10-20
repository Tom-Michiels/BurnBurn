//! HP-GL / PLT import bytes: `Ok` with finite geometry or a typed `ParseError`, never a panic.
//! Seed the corpus with `test-data/import/*.plt` (see fuzz/README.md).
//! Stable counterpart: `crates/burnburn-io/tests/robustness_w2.rs` (proptest).
#![no_main]

use burnburn_io::import::parse_hpgl;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = parse_hpgl(data);
});
