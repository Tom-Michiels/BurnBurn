//! The v1 schema DTOs straight from JSON bytes (`document.json` and `manifest.json`).
#![no_main]

use burnburn_io::dto::v1::{Document, Manifest};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Document>(data);
    let _ = serde_json::from_slice::<Manifest>(data);
});
