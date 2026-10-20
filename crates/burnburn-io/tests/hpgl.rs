//! Public contract (W2-L6, P03): HP-GL/PLT import. Plotter units are 0.025 mm (40 per mm); every
//! expectation is derived by hand from the command text below, never from a previous run.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use std::f64::consts::PI;

use burnburn_io::import::{
    Analysis, HpglUnitBasis, ImportFormat, ParseError, ParseLimits, SkipReason, UnitDecision, analyze, parse_hpgl,
    parse_hpgl_with,
};

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

#[test]
fn square_in_plotter_units_becomes_a_closed_ten_millimetre_path() {
    // 400 plotter units = 10 mm.
    let draft = parse_hpgl(b"IN;SP1;PU0,0;PD400,0,400,400,0,400,0,0;PU;").unwrap();
    assert_eq!(draft.items.len(), 1);
    let item = &draft.items[0];
    assert_eq!(item.name, "Pen 1");
    let sp = &item.path.subpaths()[0];
    assert!(sp.is_closed(), "a run that ends where it started is closed");
    let b = item.path.bounds().unwrap();
    assert!(close(b.min().x, 0.0, 1e-12) && close(b.max().x, 10.0, 1e-12) && close(b.max().y, 10.0, 1e-12), "{b:?}");
    assert!(close(item.path.signed_area().abs(), 100.0, 1e-9));
    assert_eq!(draft.report.units, UnitDecision::Hpgl { mm_per_unit: 0.025, basis: HpglUnitBasis::PlotterUnits });
    assert!(draft.report.open_contours.is_empty() && draft.report.skipped.is_empty());
}

#[test]
fn pen_up_splits_runs_and_pen_numbers_name_the_items() {
    let draft = parse_hpgl(b"IN;SP1;PU0,0;PD40,0;PU80,0;PD120,0;SP2;PU0,40;PD40,40;").unwrap();
    let names: Vec<&str> = draft.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Pen 1", "Pen 1", "Pen 2"]);
    // Two open lines of 1 mm and one more: all three are reported as open contours.
    assert_eq!(draft.report.open_contours.len(), 3);
    let second = draft.items[1].path.bounds().unwrap();
    assert!(close(second.min().x, 2.0, 1e-12) && close(second.max().x, 3.0, 1e-12));
}

#[test]
fn relative_mode_and_missing_semicolons_follow_the_plotter_state() {
    // PR moves relative to the current point; mnemonics may follow numbers without ';'.
    let draft = parse_hpgl(b"IN;PA400,400;PR;PD400,0,0,400,-400,0,0,-400PU").unwrap();
    assert_eq!(draft.items.len(), 1);
    let b = draft.items[0].path.bounds().unwrap();
    assert!(close(b.min().x, 10.0, 1e-12) && close(b.min().y, 10.0, 1e-12));
    assert!(close(b.max().x, 20.0, 1e-12) && close(b.max().y, 20.0, 1e-12));
    assert!(draft.items[0].path.subpaths()[0].is_closed());
}

#[test]
fn circle_command_draws_a_full_circle_of_the_given_radius() {
    // CI200 = radius 5 mm around the current point (400, 800) = (10, 20) mm; pen position unchanged.
    let draft = parse_hpgl(b"IN;PU400,800;CI200;").unwrap();
    assert_eq!(draft.items.len(), 1);
    let path = &draft.items[0].path;
    assert!(close(path.signed_area().abs(), PI * 25.0, 1e-9), "{}", path.signed_area());
    let b = path.bounds().unwrap();
    assert!(close(b.min().x, 5.0, 1e-9) && close(b.max().y, 25.0, 1e-9), "{b:?}");
}

#[test]
fn absolute_arc_sweeps_counter_clockwise_around_the_centre() {
    // Start (8, 0) mm = (320, 0) units, centre (0, 0), +90°: ends at (0, 8) mm after a quarter turn.
    let draft = parse_hpgl(b"IN;PU320,0;PD;AA0,0,90;").unwrap();
    let sp = &draft.items[0].path.subpaths()[0];
    assert!(!sp.is_closed());
    let (s, e) = (sp.start(), sp.end());
    assert!(close(s.x, 8.0, 1e-12) && close(s.y, 0.0, 1e-12), "{s:?}");
    assert!(close(e.x, 0.0, 1e-9) && close(e.y, 8.0, 1e-9), "{e:?}");
    // A negative sweep goes clockwise, to (0, -8).
    let cw = parse_hpgl(b"IN;PU320,0;PD;AA0,0,-90;").unwrap();
    let e = cw.items[0].path.subpaths()[0].end();
    assert!(close(e.x, 0.0, 1e-9) && close(e.y, -8.0, 1e-9), "{e:?}");
}

#[test]
fn ip_and_sc_scale_user_units_onto_plotter_units() {
    // P1 = (0, 0), P2 = (4000, 4000) units = 100 mm; user space 0..100 in both axes: 1 user unit = 1 mm.
    let draft = parse_hpgl(b"IN;IP0,0,4000,4000;SC0,100,0,100;PU10,20;PD60,20;").unwrap();
    let b = draft.items[0].path.bounds().unwrap();
    assert!(close(b.min().x, 10.0, 1e-9) && close(b.max().x, 60.0, 1e-9) && close(b.min().y, 20.0, 1e-9), "{b:?}");
    assert_eq!(draft.report.units, UnitDecision::Hpgl { mm_per_unit: 0.025, basis: HpglUnitBasis::ScaledByIpSc });
}

#[test]
fn scaling_without_reference_points_is_reported_and_not_applied() {
    let draft = parse_hpgl(b"IN;SC0,100,0,100;PU40,0;PD80,0;").unwrap();
    let b = draft.items[0].path.bounds().unwrap();
    assert!(close(b.min().x, 1.0, 1e-12) && close(b.max().x, 2.0, 1e-12), "plotter units stay in force: {b:?}");
    assert!(
        draft.report.skipped.iter().any(|s| s.kind == "SC" && matches!(s.reason, SkipReason::UnsupportedEffect(_)))
    );
}

#[test]
fn labels_encoded_polylines_and_unknown_commands_are_reported_not_dropped() {
    let draft = parse_hpgl(b"IN;PU0,0;PD40,0;LBhello\x03;PE>B<;ZZ1,2;LT2;SP1;").unwrap();
    assert_eq!(draft.items.len(), 1, "the plain line still imports");
    let kinds: Vec<&str> = draft.report.skipped.iter().map(|s| s.kind.as_str()).collect();
    for expected in ["LB", "PE", "ZZ", "LT"] {
        assert!(kinds.contains(&expected), "{expected} not reported in {kinds:?}");
    }
}

#[test]
fn pjl_and_escape_wrappers_around_hpgl2_are_skipped() {
    let mut file = b"\x1b%-12345X@PJL JOB NAME=\"x\"\r\n@PJL ENTER LANGUAGE=HPGL2\r\n\x1b%0B".to_vec();
    file.extend_from_slice(b"IN;SP1;PU0,0;PD40,40;\x1b%0A\x1b%-12345X");
    let draft = parse_hpgl(&file).unwrap();
    assert_eq!(draft.items.len(), 1);
    assert!(close(draft.items[0].path.bounds().unwrap().max().x, 1.0, 1e-12));
}

#[test]
fn analyze_routes_plt_and_hpgl_extensions_and_empty_files_are_an_error() {
    assert_eq!(ImportFormat::from_extension("PLT"), Some(ImportFormat::Hpgl));
    assert_eq!(ImportFormat::from_extension("hpgl"), Some(ImportFormat::Hpgl));
    assert!(matches!(analyze(ImportFormat::Hpgl, b"IN;PU0,0;PD40,0;"), Ok(Analysis::Vector(_))));
    let empty = parse_hpgl(b"IN;SP1;").unwrap();
    assert!(empty.items.is_empty(), "nothing drawn gives an empty draft that cannot be committed");
    assert_eq!(empty.into_command(burnburn_io::asset_hash(b"x")).unwrap_err(), burnburn_io::import::ImportError::Empty);
}

#[test]
fn malformed_numbers_and_odd_coordinate_lists_are_reported_per_command() {
    let draft = parse_hpgl(b"IN;PU0,0;PD40,0,80;PD99999999999,5;PD80,0;").unwrap();
    // The odd coordinate list and the non-finite number are named; the valid points still count.
    let invalid = draft.report.skipped.iter().filter(|s| matches!(s.reason, SkipReason::Invalid(_))).count();
    assert_eq!(invalid, 2, "{:?}", draft.report.skipped);
    assert!(!draft.items.is_empty());
}

#[test]
fn limits_are_typed_errors() {
    let tiny = ParseLimits { max_bytes: 8, ..ParseLimits::default() };
    assert_eq!(parse_hpgl_with(b"IN;PU0,0;PD1,1;", &tiny).unwrap_err(), ParseError::TooLarge { limit: 8, actual: 15 });
    let few = ParseLimits { max_entities: 3, ..ParseLimits::default() };
    assert_eq!(
        parse_hpgl_with(b"IN;PU0,0;PD1,1;PD2,2;PD3,3;", &few).unwrap_err(),
        ParseError::TooManyEntities { limit: 3 }
    );
    let pts = ParseLimits { max_path_commands: 4, ..ParseLimits::default() };
    assert_eq!(parse_hpgl_with(b"PD1,1,2,2,3,3,4,4,5,5;", &pts).unwrap_err(), ParseError::TooManyCommands { limit: 4 });
}

#[test]
fn fixture_square_circle_and_arc_run_have_the_hand_derived_geometry() {
    let bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import/square-circle.plt"),
    )
    .unwrap();
    let draft = parse_hpgl(&bytes).unwrap();
    assert_eq!(draft.items.len(), 3);
    assert!(close(draft.items[0].path.signed_area().abs(), 100.0, 1e-9));
    assert!(close(draft.items[1].path.signed_area().abs(), PI * 2.5 * 2.5, 1e-9));
    let run = &draft.items[2].path.subpaths()[0];
    assert!(close(run.start().x, 15.0, 1e-9) && close(run.end().x, 15.0, 1e-9) && close(run.end().y, 5.0, 1e-9));
    assert_eq!(draft.items[2].name, "Pen 2");
    assert_eq!(draft.report.skipped.len(), 1);
    assert_eq!(draft.report.skipped[0].kind, "LB");
}
