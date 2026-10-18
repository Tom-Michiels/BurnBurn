//! SVG import bytes: a typed `ParseError` or a draft with finite geometry, never a panic.
//! Seed the corpus with `test-data/import/*.svg` (see fuzz/README.md).
//! Stable counterpart: `crates/burnburn-io/tests/parser_robustness.rs` (Kern, proptest).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(draft) = burnburn_io::import::parse_svg(data) {
        for item in &draft.items {
            assert!(item.path.subpaths().iter().all(|s| s.start().is_finite()), "non-finite import geometry");
        }
    }
});
