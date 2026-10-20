//! Art-library container bytes (zip): `Ok` (every item then places and renders a thumbnail) or a
//! typed `LoadError`, never a panic. Stable counterpart: `crates/burnburn-io/tests/robustness_w2.rs`.
#![no_main]

use burnburn_io::art_library::decode_art_library;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(lib) = decode_art_library(data) {
        for item in lib.items() {
            let _ = lib.thumbnail(item.id());
            let _ = lib.place(item.id());
        }
    }
});
