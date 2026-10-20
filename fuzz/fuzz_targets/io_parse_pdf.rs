//! PDF / Illustrator import bytes: structure parse (xref tables and streams, object streams,
//! Flate, rebuild scan) and then the content-stream interpreter for every page. `Ok` or a typed
//! `ParseError`, never a panic, never unbounded memory (decoded-size caps).
//! Seed the corpus with `test-data/import/*.pdf` (see fuzz/README.md).
//! Stable counterpart: `crates/burnburn-io/tests/robustness_w2.rs` (proptest).
#![no_main]

use burnburn_io::import::{PdfPages, parse_pdf};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(doc) = parse_pdf(data) {
        let _ = doc.to_draft(PdfPages::All);
    }
});
