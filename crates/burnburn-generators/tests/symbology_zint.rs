//! W3-robuust: "2 of 5" and Han Xin against expected symbols taken unchanged from the zint test-suite
//! (`fixtures/zint-vectors.txt`, with its provenance). The vectors are the independent oracle: BurnBurn
//! computed none of them. Han Xin case 1 is the ISO/IEC 20830 Figure 1 example.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_generators::{SymbolError, SymbolLayout, Symbology, SymbologyParams, encode_symbol};

struct Vector {
    kind: String,
    id: u32,
    opts: Vec<(String, String)>,
    data: String,
    bits: String,
}

fn vectors(kind: &str) -> Vec<Vector> {
    let mut out = Vec::new();
    for line in include_str!("fixtures/zint-vectors.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split(' ');
        let k = parts.next().unwrap();
        if k != kind {
            continue;
        }
        let id = parts.next().unwrap().parse().unwrap();
        let (mut opts, mut data, mut bits) = (Vec::new(), String::new(), String::new());
        for p in parts {
            let (key, value) = p.split_once('=').unwrap();
            match key {
                "data" => {
                    data = (0..value.len() / 2)
                        .map(|i| u8::from_str_radix(&value[2 * i..2 * i + 2], 16).unwrap() as char)
                        .collect()
                }
                "bits" => bits = value.to_string(),
                _ => opts.push((key.to_string(), value.to_string())),
            }
        }
        out.push(Vector { kind: k.to_string(), id, opts, data, bits });
    }
    out
}

fn opt<'a>(v: &'a Vector, key: &str) -> &'a str {
    &v.opts.iter().find(|(k, _)| k == key).unwrap().1
}

fn params() -> SymbologyParams {
    SymbologyParams::new(0.4, 0.1)
}

/// Dark/light modules of a linear symbol, bar first, as a 0/1 string.
fn linear_bits(widths: &[u8]) -> String {
    widths
        .iter()
        .enumerate()
        .flat_map(|(i, &w)| std::iter::repeat_n(if i % 2 == 0 { '1' } else { '0' }, usize::from(w)))
        .collect()
}

#[test]
fn the_zint_fixture_holds_the_expected_cases() {
    assert_eq!(vectors("c25standard").len(), 3);
    assert_eq!(vectors("hanxin").len(), 3);
    for v in vectors("hanxin") {
        let w: usize = opt(&v, "width").parse().unwrap();
        assert_eq!(v.bits.len(), w * w, "hanxin case {}", v.id);
    }
}

#[test]
fn two_of_five_matches_the_zint_standard_code_2_of_5_modules() {
    for v in vectors("c25standard") {
        // zint `check=1` appends a mod-10 check digit; the payload here is the digits to draw. The digit
        // itself is part of the expectation: the vector for 87654321 + check ends in the pattern of "2".
        let payload = if opt(&v, "check") == "1" { format!("{}2", v.data) } else { v.data.clone() };
        let sym = encode_symbol(Symbology::TwoOfFive, &payload, &params()).unwrap();
        let SymbolLayout::Linear { widths } = &sym.layout else { panic!("not linear") };
        assert_eq!(linear_bits(widths), v.bits, "{} case {}", v.kind, v.id);
    }
}

#[test]
fn two_of_five_differs_from_industrial_and_iata_so_it_is_a_third_symbology() {
    let modules = |s| match encode_symbol(s, "87654321", &params()).unwrap().layout {
        SymbolLayout::Linear { widths } => linear_bits(&widths),
        other => panic!("{other:?}"),
    };
    let plain = modules(Symbology::TwoOfFive);
    assert_ne!(plain, modules(Symbology::TwoOfFiveIndustrial));
    assert_ne!(plain, modules(Symbology::TwoOfFiveIata));
}

#[test]
fn two_of_five_refuses_non_digits_with_a_typed_error() {
    assert!(matches!(
        encode_symbol(Symbology::TwoOfFive, "12A4", &params()),
        Err(SymbolError::InvalidChar { ch: 'A', index: 2, .. })
    ));
}

#[test]
fn han_xin_matches_the_zint_modules_including_the_iso_figure_1() {
    for v in vectors("hanxin") {
        let mut p = params();
        // zint's option_1 is the level 1..=4. Its automatic choice (-1) produced level 4 in the vector for
        // "1234" (found by trying all four levels: only L4 matches), so the automatic case asks for step 3.
        let ecc: i32 = opt(&v, "ecc").parse().unwrap();
        p.ecc_step = Some(if ecc > 0 { u8::try_from(ecc - 1).unwrap() } else { 3 });
        let sym = encode_symbol(Symbology::HanXin, &v.data, &p).unwrap();
        let SymbolLayout::Matrix { width, modules, .. } = &sym.layout else { panic!("not a matrix") };
        let want: usize = opt(&v, "width").parse().unwrap();
        assert_eq!(*width, want, "hanxin case {}", v.id);
        let ours: String = modules.iter().map(|&m| if m { '1' } else { '0' }).collect();
        assert_eq!(ours, v.bits, "hanxin case {}", v.id);
    }
}

#[test]
fn han_xin_reports_the_capacity_of_the_versions_it_has() {
    // Versions 1-3 only: a long text is refused with a typed error, never a different symbol.
    let long = "0123456789".repeat(20);
    assert!(matches!(encode_symbol(Symbology::HanXin, &long, &params()), Err(SymbolError::DataTooLong { .. })));
}
