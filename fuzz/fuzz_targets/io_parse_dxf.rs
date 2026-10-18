//! DXF import bytes: parse, then convert with the declared unit and with an explicit inch scale
//! (the unitless path). Typed errors only, never a panic.
//! Seed the corpus with `test-data/import/*.dxf` (see fuzz/README.md).
//! Stable counterpart: `crates/burnburn-io/tests/parser_robustness.rs` (Kern, proptest).
#![no_main]

use burnburn_io::import::{DxfScale, parse_dxf};
use burnburn_types::Unit;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(dxf) = parse_dxf(data) {
        let _ = dxf.to_draft(None);
        let _ = dxf.to_draft(Some(DxfScale::unit(Unit::Inch)));
    }
});
