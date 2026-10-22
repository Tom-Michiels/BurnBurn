//! G5-UI-gaten E11: serial counter, advance policy and chosen data row travel with the project
//! (`production.json`). An optional entry: a project without it has the same bytes as before and
//! the same project hash; a damaged one is refused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_io::{
    AdvanceChoice, LoadError, ProductionState, Project, RowsSource, SerialState, decode_container, encode_container,
    project_hash,
};
use common::sample_project;

fn state(project: &mut Project) -> ProductionState {
    let csv = project.assets.add("text/csv", b"naam,nr\nAnna,1\nBert,2\nCleo,3\n");
    ProductionState {
        serial: Some(SerialState { start: 100, end: 199, advance_by: 3, width: 5, wrap: false, current: 112 }),
        row: 2,
        policy: AdvanceChoice::OnUploadConfirmed,
        rows: Some(RowsSource { name: "klanten.csv".into(), asset: csv }),
    }
}

#[test]
fn the_counters_survive_save_and_reopen() {
    let mut p = sample_project();
    let production = state(&mut p);
    p.production = Some(production.clone());
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.production, Some(production));
    assert_eq!(encode_container(&back).unwrap(), encode_container(&p).unwrap(), "deterministic bytes");
}

#[test]
fn a_project_without_counters_gains_no_entry_and_the_hash_ignores_them() {
    let mut p = sample_project();
    let plain = encode_container(&p).unwrap();
    assert!(!String::from_utf8_lossy(&plain).contains("production.json"));
    assert_eq!(decode_container(&plain).unwrap().production, None);
    let hash = project_hash(&p).unwrap();
    p.production = Some(state(&mut p));
    // The CSV is an asset, so the assets part of the hash differs; the counters themselves must not.
    let with_asset = project_hash(&p).unwrap();
    p.production.as_mut().unwrap().row = 9;
    p.production.as_mut().unwrap().serial.as_mut().unwrap().current = 100;
    assert_eq!(project_hash(&p).unwrap(), with_asset, "counters are bookkeeping, not design");
    assert_ne!(hash, with_asset);
}

#[test]
fn inconsistent_counters_and_a_missing_table_are_refused() {
    let mut p = sample_project();
    let good = state(&mut p);
    let refused = |change: &dyn Fn(&mut ProductionState)| {
        let mut p = p.clone();
        let mut s = good.clone();
        change(&mut s);
        p.production = Some(s);
        matches!(decode_container(&encode_container(&p).unwrap()), Err(LoadError::InvalidValue { .. }))
    };
    assert!(refused(&|s| s.serial.as_mut().unwrap().advance_by = 0), "advance by zero");
    assert!(refused(&|s| s.serial.as_mut().unwrap().width = 21), "width above 20");
    assert!(refused(&|s| s.serial.as_mut().unwrap().end = 50), "end before start");
    assert!(refused(&|s| s.serial.as_mut().unwrap().current = 113), "current is not a value of the series");
    assert!(refused(&|s| s.rows.as_mut().unwrap().asset = burnburn_types::ContentHash::from_bytes([7; 32])), "no such asset");
    assert!(refused(&|s| s.rows.as_mut().unwrap().name = String::new()), "a table has a name");
    assert!(!refused(&|_| {}), "the good state is accepted");
}
