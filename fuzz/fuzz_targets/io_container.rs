//! `.burnburn` container bytes (zip + JSON): typed `LoadError` or a validated project, never a panic.
//! Seed the corpus with `test-data/projects/v1/*.burnburn` (see fuzz/README.md).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = burnburn_io::decode_container(data);
});
