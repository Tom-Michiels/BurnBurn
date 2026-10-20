//! Fuzz-style properties for the W2-L6 parsers: any input gives `Ok` or a typed error and never
//! panics; accepted geometry is finite. The cargo-fuzz targets in `fuzz/` run the same entry points.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::path::PathBuf;

use burnburn_io::import::{PdfPages, parse_hpgl, parse_pdf};
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

/// PDF-shaped noise: structure keywords, operators and numbers.
fn pdf_noise() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(
        prop_oneof![
            Just("%PDF-1.7\n".to_owned()),
            Just("1 0 obj\n".to_owned()),
            Just("<< /Type /Catalog /Pages 2 0 R >>".to_owned()),
            Just("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned()),
            Just("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>".to_owned()),
            Just("<< /Length 99999 >>\nstream\n".to_owned()),
            Just("\nendstream\nendobj\n".to_owned()),
            Just("xref\n0 1\n0000000000 65535 f \n".to_owned()),
            Just("trailer\n<< /Root 1 0 R /Prev 0 >>\nstartxref\n0\n%%EOF".to_owned()),
            Just("/FlateDecode ".to_owned()),
            Just("/Filter ".to_owned()),
            Just("0 0 m 10 10 l S ".to_owned()),
            Just("q ".to_owned()),
            Just("Q ".to_owned()),
            Just("cm ".to_owned()),
            Just("BI ID EI ".to_owned()),
            Just("[[[".to_owned()),
            Just("((".to_owned()),
            (-500i32..500).prop_map(|v| format!("{v} ")),
            any::<u8>().prop_map(|b| format!("{}", b as char)),
        ],
        0..60,
    )
    .prop_map(|parts| parts.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn pdf_random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let mut with_header = b"%PDF-1.4\n".to_vec();
        with_header.extend_from_slice(&bytes);
        for b in [&bytes, &with_header] {
            if let Ok(doc) = parse_pdf(b) {
                let _ = doc.to_draft(PdfPages::All);
            }
        }
    }

    #[test]
    fn pdf_mutated_fixture_never_panics_and_stays_finite(b in mutate(fixture("plate-layers.pdf"))) {
        if let Ok(doc) = parse_pdf(&b) {
            for i in 0..doc.pages().len().min(3) {
                if let Ok(d) = doc.to_draft(PdfPages::One(i)) {
                    for item in &d.items {
                        prop_assert!(item.path.subpaths().iter().all(|s| s.start().is_finite() && s.end().is_finite()));
                    }
                }
            }
        }
    }

    #[test]
    fn pdf_structure_noise_never_panics(b in pdf_noise()) {
        if let Ok(doc) = parse_pdf(&b) {
            let _ = doc.to_draft(PdfPages::All);
        }
    }
}

fn lbrn2_bitmap_seed() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/fixtures/gray-ramp-grayscale.lbrn2"),
    )
    .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// W2-L6: mutated files with an embedded bitmap (base64 + image decode) and image cut settings.
    #[test]
    fn lbrn2_mutated_bitmap_fixture_never_panics(b in mutate(lbrn2_bitmap_seed())) {
        if let Ok(d) = burnburn_io::import::parse_lbrn2(&b) {
            let source = burnburn_io::asset_hash(b"x");
            for p in &d.cut_settings {
                let _ = d.image_operation(p.index, source);
            }
        }
    }

    #[test]
    fn lbrn2_arbitrary_bitmap_data_and_dither_names_never_panic(data in "[A-Za-z0-9+/= \n!]{0,200}", mode in "[a-z0-9]{0,12}") {
        let xml = format!(
            r#"<LightBurnProject><CutSetting_Img type="Image"><index Value="0"/><maxPower Value="9"/><speed Value="9"/><ditherMode Value="{mode}"/></CutSetting_Img><Shape Type="Bitmap" W="3" H="2" Data="{data}"><XForm>1 0 0 1 0 0</XForm></Shape></LightBurnProject>"#
        );
        let _ = burnburn_io::import::parse_lbrn2(xml.as_bytes());
    }
}
