//! Fuzz-style properties: the import entry points return `Ok` or a typed error for any input
//! and never panic (IM10). Seeds are the hand-written fixtures plus random mutations.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::path::PathBuf;

use burnburn_io::import::{RasterLimits, inspect_raster, parse_dxf, parse_svg};
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

/// Syntax-shaped noise for SVG path data.
fn path_data() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("M".to_owned()),
            Just("m".to_owned()),
            Just("L".to_owned()),
            Just("C".to_owned()),
            Just("S".to_owned()),
            Just("Q".to_owned()),
            Just("T".to_owned()),
            Just("A".to_owned()),
            Just("a".to_owned()),
            Just("Z".to_owned()),
            Just("H".to_owned()),
            Just("v".to_owned()),
            Just(",".to_owned()),
            Just(" ".to_owned()),
            Just("1e308".to_owned()),
            Just("-.5.5".to_owned()),
            (-1.0e6f64..1.0e6).prop_map(|v| v.to_string()),
            Just("0".to_owned()),
            Just("1".to_owned()),
        ],
        0..60,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = parse_svg(&bytes);
        let _ = parse_dxf(&bytes);
        let _ = inspect_raster(&bytes, &RasterLimits::default());
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&bytes);
        let _ = inspect_raster(&png, &RasterLimits::default());
        let mut jpg = vec![0xff, 0xd8, 0xff];
        jpg.extend_from_slice(&bytes);
        let _ = inspect_raster(&jpg, &RasterLimits::default());
    }

    #[test]
    fn mutated_svg_fixtures_never_panic(b in mutate(fixture("mm-viewbox.svg"))) {
        let _ = parse_svg(&b);
    }

    #[test]
    fn mutated_transform_svg_never_panics(b in mutate(fixture("px-transforms.svg"))) {
        let _ = parse_svg(&b);
    }

    #[test]
    fn mutated_dxf_fixtures_never_panic(b in mutate(fixture("mm-plate.dxf")), c in mutate(fixture("inch-spline.dxf"))) {
        if let Ok(d) = parse_dxf(&b) { let _ = d.to_draft(None); }
        if let Ok(d) = parse_dxf(&c) { let _ = d.to_draft(None); }
    }

    #[test]
    fn arbitrary_path_data_never_panics(d in path_data()) {
        let svg = format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'><path d='{d}'/></svg>");
        if let Ok(draft) = parse_svg(svg.as_bytes()) {
            for item in &draft.items {
                prop_assert!(item.path.subpaths().iter().all(|s| s.start().is_finite()));
            }
        }
    }
}

fn lbrn2_seed() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/fixtures/rectangle-fill.lbrn2"),
    )
    .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// IM28: arbitrary bytes never panic the LBRN2 importer.
    #[test]
    fn lbrn2_random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = burnburn_io::import::parse_lbrn2(&bytes);
    }

    /// IM28: mutations of a real LightBurn file never panic and imports stay finite.
    #[test]
    fn lbrn2_mutated_fixture_never_panics(bytes in mutate(lbrn2_seed())) {
        if let Ok(d) = burnburn_io::import::parse_lbrn2(&bytes) {
            for it in &d.items {
                if let Some(b) = it.path.bounds() {
                    prop_assert!(b.min().x.is_finite() && b.max().y.is_finite());
                }
            }
        }
    }

    /// IM28: syntax-shaped vertex and primitive lists never panic.
    #[test]
    fn lbrn2_vertex_noise_never_panics(v in "[Vc01xy .,eE+-]{0,80}", p in "[LB0-9 ]{0,40}") {
        let doc = format!(
            r#"<LightBurnProject><Shape Type="Path"><VertList>{v}</VertList><PrimList>{p}</PrimList></Shape></LightBurnProject>"#
        );
        let _ = burnburn_io::import::parse_lbrn2(doc.as_bytes());
    }
}
