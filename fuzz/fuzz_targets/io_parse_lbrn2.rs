//! LightBurn project (.lbrn2) bytes: XML tree build, shape/transform/vertex parsing, embedded
//! bitmaps (base64 + image decode) and the image-cut proposals. `Ok` with finite geometry or a
//! typed `ParseError`, never a panic. Seed with `research/lightburn/fixtures/*.lbrn2`.
//! Stable counterpart: `crates/burnburn-io/tests/robustness_w2.rs` (proptest).
#![no_main]

use burnburn_io::asset_hash;
use burnburn_io::import::parse_lbrn2;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(draft) = parse_lbrn2(data) {
        let source = asset_hash(data);
        for proposal in &draft.cut_settings {
            let _ = draft.image_operation(proposal.index, source);
        }
        let _ = draft.geometry_command(source);
    }
});
