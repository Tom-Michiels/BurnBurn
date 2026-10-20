//! Fuzz-style properties for the W2-L6 parsers: any input gives `Ok` or a typed error and never
//! panics; accepted geometry is finite. The cargo-fuzz targets in `fuzz/` run the same entry points.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::path::PathBuf;

use burnburn_io::import::parse_hpgl;
use proptest::prelude::*;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import").join(name)).unwrap()
}

/// Random byte edits (replace, insert, delete) and truncation of a seed.
fn mutate(seed: Vec<u8>) -> impl Strategy<Value = Vec<u8>> {
    let len = seed.len().max(1);
    (proptest::collection::vec((0..len, any::<u8>(), 0u8..3), 0..24), 0..=len).prop_map(move |(edits, cut)| {
        let mut b = seed.clone();
        for (i, byte, op) in edits {
            let i = i.min(b.len());
            match op {
                0 if i < b.len() => b[i] = byte,
                1 => b.insert(i, byte),
                _ if i < b.len() => {
                    b.remove(i);
                }
                _ => {}
            }
        }
        b.truncate(cut.max(b.len() / 2));
        b
    })
}

/// Command-shaped HP-GL noise.
fn hpgl_noise() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("IN;".to_owned()),
            Just("PU".to_owned()),
            Just("PD".to_owned()),
            Just("PA".to_owned()),
            Just("PR".to_owned()),
            Just("CI".to_owned()),
            Just("AA".to_owned()),
            Just("AR".to_owned()),
            Just("IP".to_owned()),
            Just("SC".to_owned()),
            Just("SP".to_owned()),
            Just("LB".to_owned()),
            Just("PE".to_owned()),
            Just(";".to_owned()),
            Just(",".to_owned()),
            Just("\u{1b}%".to_owned()),
            Just("-.".to_owned()),
            Just("99999999999".to_owned()),
            (-5000i32..5000).prop_map(|v| v.to_string()),
        ],
        0..80,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn hpgl_random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let _ = parse_hpgl(&bytes);
    }

    #[test]
    fn hpgl_mutated_fixture_never_panics_and_stays_finite(b in mutate(fixture("square-circle.plt"))) {
        if let Ok(d) = parse_hpgl(&b) {
            for item in &d.items {
                prop_assert!(item.path.subpaths().iter().all(|s| s.start().is_finite() && s.end().is_finite()));
            }
        }
    }

    #[test]
    fn hpgl_command_noise_never_panics(s in hpgl_noise()) {
        let _ = parse_hpgl(s.as_bytes());
    }
}
