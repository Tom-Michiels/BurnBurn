//! Material-library file bytes: `Ok` (and then re-encodable) or a typed `LoadError`, never a panic.
//! Stable counterpart: `crates/burnburn-io/tests/robustness_w2.rs` (proptest).
#![no_main]

use burnburn_io::material_library::{decode_material_library, encode_material_library};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(lib) = decode_material_library(data) {
        let _ = encode_material_library(&lib);
    }
});
