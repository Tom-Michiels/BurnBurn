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

fn material_seed() -> Vec<u8> {
    use burnburn_document::process::{
        Kerf, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, RecipeDraft, Speed,
    };
    let mut lib = burnburn_io::material_library::MaterialLibrary::new("seed");
    let p = PowerPercent::new(50.0).unwrap();
    lib.add(RecipeDraft {
        name: "Snijden".into(),
        material: "Acryl".into(),
        thickness: burnburn_types::Length::from_mm(3.0).unwrap(),
        machine: "m".into(),
        notes: String::new(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(10.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: false,
        },
    })
    .unwrap();
    burnburn_io::material_library::encode_material_library(&lib).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn material_library_random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = burnburn_io::material_library::decode_material_library(&bytes);
    }

    #[test]
    fn material_library_mutated_file_never_panics_and_valid_ones_reencode(b in mutate(material_seed())) {
        if let Ok(lib) = burnburn_io::material_library::decode_material_library(&b) {
            let again = burnburn_io::material_library::encode_material_library(&lib).unwrap();
            prop_assert_eq!(burnburn_io::material_library::decode_material_library(&again).unwrap(), lib);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Whatever the option combination, what we write we can read back: item count and (for SVG
    /// with exact decimals) closedness survive.
    #[test]
    fn export_options_always_produce_files_our_importers_read(
        unit in 0u8..3, decimals in proptest::option::of(0u8..8), group in any::<bool>(), colours in any::<bool>(),
        dxf_unit in 0u8..2, layers in 0u8..3, flatten in proptest::option::of(0.001f64..1.0),
    ) {
        use burnburn_geometry::{PathBuilder, Point};
        use burnburn_io::export::{DxfCurves, DxfLayers, DxfOptions, DxfUnit, ExportItem, SvgOptions, SvgUnit, export_dxf_with, export_svg_with};
        let blob = PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .cubic_to(Point::new(0.0, 30.0), Point::new(40.0, 30.0), Point::new(40.0, 0.0))
            .close()
            .build()
            .unwrap();
        let line = burnburn_geometry::Path::line(Point::new(5.0, -5.0), Point::new(35.0, -5.0)).unwrap();
        let items = [
            ExportItem::new("blob", &blob).with_layer("A").with_color([200, 10, 10]),
            ExportItem::new("line", &line).with_layer("B"),
        ];
        let svg = SvgOptions {
            unit: [SvgUnit::Mm, SvgUnit::Inch, SvgUnit::Pixel][usize::from(unit)],
            decimals,
            group_layers: group,
            use_item_colors: colours,
            ..SvgOptions::default()
        };
        let (text, _) = export_svg_with(&items, &svg).unwrap();
        let back = burnburn_io::import::parse_svg(text.as_bytes()).unwrap();
        prop_assert_eq!(back.items.len(), 2);
        prop_assert!(back.items[0].path.subpaths()[0].is_closed());
        let dxf = DxfOptions {
            unit: [DxfUnit::Mm, DxfUnit::Inch][usize::from(dxf_unit)],
            layers: [DxfLayers::ItemName, DxfLayers::ItemLayerOrName, DxfLayers::Single][usize::from(layers)],
            curves: flatten.map_or(DxfCurves::Splines, |t| DxfCurves::Polylines { tolerance_mm: t }),
            use_item_colors: colours,
            ..DxfOptions::default()
        };
        let (text, _) = export_dxf_with(&items, &dxf).unwrap();
        let back = burnburn_io::import::parse_dxf(text.as_bytes()).unwrap().to_draft(None).unwrap();
        prop_assert_eq!(back.items.len(), 2);
    }
}

fn art_seed() -> Vec<u8> {
    use burnburn_io::art_library::{ArtFormat, ArtLibrary, NewArt, encode_art_library};
    let mut lib = ArtLibrary::new("seed");
    lib.add_group("G").unwrap();
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10mm" height="10mm"><rect x="1" y="1" width="5" height="5"/></svg>"#;
    lib.add_item(NewArt {
        name: "A",
        group: "G",
        tags: vec!["t".into()],
        format: ArtFormat::Svg,
        bytes: svg,
        dxf_scale: None,
    })
    .unwrap();
    encode_art_library(&lib).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn art_library_random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = burnburn_io::art_library::decode_art_library(&bytes);
    }

    #[test]
    fn art_library_mutated_file_never_panics_and_valid_ones_work(b in mutate(art_seed())) {
        if let Ok(lib) = burnburn_io::art_library::decode_art_library(&b) {
            for item in lib.items() {
                let _ = lib.thumbnail(item.id());
                let _ = lib.place(item.id());
            }
        }
    }
}
