//! Bundle container bytes (zip with nested libraries, profiles and templates): `Ok` (then previewed
//! and applied to empty libraries) or a typed `LoadError`, never a panic.
//! Stable counterpart: `crates/burnburn-io/tests/robustness_w2.rs`.
#![no_main]

use burnburn_io::art_library::ArtLibrary;
use burnburn_io::bundle::{ImportChoices, ImportTarget, LocalState, apply_import, decode_bundle, preview_import};
use burnburn_io::material_library::MaterialLibrary;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(bundle) = decode_bundle(data) {
        let _ = preview_import(&bundle, &LocalState::default());
        let mut m = MaterialLibrary::new("m");
        let mut a = ArtLibrary::new("a");
        let target = ImportTarget { materials: &mut m, art: &mut a, profiles: &[], templates: &[] };
        let _ = apply_import(&bundle, target, &ImportChoices::default());
    }
});
