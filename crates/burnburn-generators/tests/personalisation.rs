//! Frozen data rows and Code 128 barcodes (E11).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_generators::{
    Code128Params, CsvError, DataTable, FrozenRows, GenError, RowBinding, RowError, code128, code128_patterns,
    code128_symbols, resolve, resolve_bound,
};

const CSV: &str = "naam,nummer\r\n\"Jansen, P.\",7\n\"Zeg \"\"hoi\"\"\",\n";

#[test]
fn csv_parses_quotes_escapes_crlf_and_empty_cells() {
    let t = DataTable::parse_csv(CSV, ',').unwrap();
    assert_eq!(t.headers, ["naam", "nummer"]);
    assert_eq!(t.rows, [vec!["Jansen, P.".to_string(), "7".into()], vec!["Zeg \"hoi\"".into(), "".into()]]);
    let semi = DataTable::parse_csv("a;b\n1;2", ';').unwrap();
    assert_eq!(semi.rows, [vec!["1".to_string(), "2".into()]]);
}

#[test]
fn csv_errors_are_located() {
    assert_eq!(DataTable::parse_csv("", ','), Err(CsvError::Empty));
    assert_eq!(DataTable::parse_csv("a,b\n1,2,3\n", ','), Err(CsvError::Ragged { line: 2, expected: 2, found: 3 }));
    assert_eq!(DataTable::parse_csv("a,b\n\"open,2\n", ','), Err(CsvError::UnterminatedQuote { line: 2 }));
    assert_eq!(DataTable::parse_csv("a,a\n", ','), Err(CsvError::DuplicateHeader { name: "a".into() }));
    assert_eq!(DataTable::parse_csv("a,\n", ','), Err(CsvError::EmptyHeader { column: 1 }));
}

#[test]
fn row_preview_shows_text_sequence_and_missing_fields() {
    let rows = FrozenRows::freeze(DataTable::parse_csv(CSV, ',').unwrap(), "2026-10-18");
    let r = resolve("{naam} #{#} – {datum} {{x}}", &rows, 0, 100).unwrap();
    assert_eq!(r.text, "Jansen, P. #100 – 2026-10-18 {x}");
    assert_eq!(r.sequence, 100);
    assert!(r.missing.is_empty());
    let r = resolve("{naam}: {nummer}{onbekend}", &rows, 1, 100).unwrap();
    assert_eq!(r.text, "Zeg \"hoi\": ");
    assert_eq!(r.sequence, 101);
    assert_eq!(r.missing, ["nummer", "onbekend"]);
    assert_eq!(resolve("{naam", &rows, 0, 1), Err(RowError::BadTemplate { at: 0 }));
    assert_eq!(resolve("x}", &rows, 0, 1), Err(RowError::BadTemplate { at: 1 }));
    assert_eq!(resolve("{naam}", &rows, 2, 1), Err(RowError::RowOutOfRange { row: 2, rows: 2 }));
}

#[test]
fn frozen_rows_are_reproducible_and_bound_by_hash() {
    let table = DataTable::parse_csv(CSV, ',').unwrap();
    let a = FrozenRows::freeze(table.clone(), "2026-10-18");
    let b = FrozenRows::freeze(table.clone(), "2026-10-18");
    assert_eq!(a.hash, b.hash, "reopening the same frozen data gives the same personalisation");
    assert_ne!(a.hash, FrozenRows::freeze(table.clone(), "2026-10-19").hash, "time value is part of the frozen set");
    let mut changed = table;
    changed.rows[0][1] = "8".into();
    let c = FrozenRows::freeze(changed, "2026-10-18");
    assert_ne!(a.hash, c.hash);

    let binding = RowBinding { rows: a.hash, row: 0 };
    assert_eq!(resolve_bound("{nummer}", &a, binding, 1).unwrap().text, "7");
    assert_eq!(resolve_bound("{nummer}", &c, binding, 1), Err(RowError::StaleRows));
}

#[test]
fn code128_table_satisfies_the_symbology_invariants() {
    let t = code128_patterns();
    assert_eq!(t.len(), 107);
    for (v, p) in t.iter().enumerate() {
        let expected = if v == 106 { (7, 13) } else { (6, 11) };
        assert_eq!((p.len(), p.iter().map(|&w| u32::from(w)).sum::<u32>()), expected, "symbol {v}");
        assert!(p.iter().all(|&w| (1..=4).contains(&w)), "symbol {v}");
        // Bars (even positions) of a data/start symbol have an even module sum.
        let bars: u32 = p.iter().step_by(2).map(|&w| u32::from(w)).sum();
        assert_eq!(bars % 2, 0, "bar parity of symbol {v}");
    }
    let mut unique: Vec<_> = t[..106].to_vec();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 106, "patterns are distinct");
}

/// Check symbol = (start + Σ position·value) mod 103, computed by hand:
/// "AB": 104 + 33·1 + 34·2 = 205 → 102. "1234" (subset C): 105 + 12·1 + 34·2 = 185 → 82.
/// "Wiki": 104 + 55·1 + 73·2 + 75·3 + 73·4 = 822 → 101.
#[test]
fn code128_symbols_match_hand_computed_vectors() {
    assert_eq!(code128_symbols("AB").unwrap(), [104, 33, 34, 102, 106]);
    assert_eq!(code128_symbols("1234").unwrap(), [105, 12, 34, 82, 106]);
    assert_eq!(code128_symbols("Wiki").unwrap(), [104, 55, 73, 75, 73, 101, 106]);
    assert_eq!(code128_symbols("123").unwrap()[0], 104, "odd digit count uses subset B");
    assert_eq!(code128_symbols("Aé").unwrap_err(), GenError::UnsupportedChar { ch: 'é', index: 1 });
    assert!(code128_symbols("").is_err());
}

fn params() -> Code128Params {
    Code128Params { module: 0.3, height: 10.0, quiet_modules: 10, min_module: 0.2 }
}

#[test]
fn code128_geometry_has_spec_widths_and_physical_module_size() {
    let b = code128("AB", &params()).unwrap();
    // Start B 211214, A 111323, B 131123, check 102 411131, stop 2331112 (symbology table).
    let expected: Vec<u8> = "211214111323131123411131".bytes().chain("2331112".bytes()).map(|c| c - b'0').collect();
    assert_eq!(b.widths, expected);
    let modules: u32 = expected.iter().map(|&w| u32::from(w)).sum();
    assert_eq!(modules, 57);
    assert!((b.width - (57.0 + 20.0) * 0.3).abs() < 1e-9);
    // One closed rectangle per bar; first bar starts after the quiet zone and is 2 modules wide.
    let bars = b.path.subpaths().len();
    assert_eq!(bars, expected.iter().step_by(2).count());
    let first = b.path.subpaths()[0].start();
    let bb = b.path.bounds().unwrap();
    assert!((bb.min().x - 3.0).abs() < 1e-9 && (bb.height() - 10.0).abs() < 1e-9, "{first:?}");
    let area: f64 = expected.iter().step_by(2).map(|&w| f64::from(w) * 0.3 * 10.0).sum();
    assert!((b.path.signed_area().abs() - area).abs() < 1e-9);
}

#[test]
fn code128_refuses_modules_below_the_read_test_minimum() {
    let mut p = params();
    p.module = 0.15;
    assert_eq!(code128("AB", &p).unwrap_err(), GenError::TooSmall { param: "module", min_mm: 0.2, value_mm: 0.15 });
    p = params();
    p.quiet_modules = 5;
    assert_eq!(code128("AB", &p).unwrap_err(), GenError::Count { param: "quiet_modules", min: 10, max: 100, value: 5 });
}
