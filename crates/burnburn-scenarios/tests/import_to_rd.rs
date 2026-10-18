//! Import with a unit decision → document → JobPlan → RD (P03 into C-chain). The unit decision
//! must reach the machine coordinates: a wrong scale shows up as a wrong RD extent.
//!
//! Hand-derived from the fixture text (`test-data/import/manifest.json`):
//! - `mm-viewbox.svg`: width 100mm, viewBox 1000 → 0.1 mm/unit; sheet 100 × 50 mm; circle 'hole'
//!   r 50 units under translate(250 250) → centre (25, 50 − 25) = (25, 25) mm y-up, r 5 mm.
//! - `unitless-lines.dxf`: a 4 × 2 L in drawing units; with inch 101.6 × 50.8 mm, with mm 4 × 2 mm.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_cam::preflight;
use burnburn_document::CommandPayload;
use burnburn_geometry::Point;
use burnburn_io::asset_hash;
use burnburn_io::import::{DxfScale, SvgUnitBasis, UnitDecision, parse_dxf, parse_svg};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::Unit;
use common::*;

/// RD cut-motion extent and loops; also returns the RD header bounds for the separate check.
fn rd_of(cmd: burnburn_document::GeometryCommand) -> ((Point, Point), (Point, Point), Vec<Vec<Point>>) {
    let mut s = new_session();
    let created = apply(&mut s, CommandPayload::Edit(cmd), &[]).changes.created;
    add_cut(&mut s, created);
    let snap = s.snapshot();
    let plan = compile_plan(&snap);
    assert!(!preflight(&snap, &plan, &profile()).has_errors());
    let rd = decode(&rd_bytes(&plan));
    let c = cuts(&rd);
    (cut_extent(&c), header_bounds(&rd), loops(&c))
}

fn rd_extent_of(cmd: burnburn_document::GeometryCommand) -> ((Point, Point), Vec<Vec<Point>>) {
    let (extent, header, loops) = rd_of(cmd);
    let _ = header; // checked separately in rd_header_bounds_include_a_cut_that_starts_at_the_job_start
    (extent, loops)
}

#[test]
fn svg_viewbox_units_reach_the_rd_file_in_millimetres() {
    let bytes = read_fixture("import", "mm-viewbox.svg").unwrap();
    let draft = parse_svg(&bytes).unwrap();
    assert_eq!(
        draft.report.units,
        UnitDecision::Svg { mm_per_user_unit: 0.1, basis: SvgUnitBasis::ViewBoxToPhysicalSize }
    );
    let ((lo, hi), loops) = rd_extent_of(draft.into_command(asset_hash(&bytes)).unwrap());
    assert_eq!((lo, hi), (Point::new(0.0, 0.0), Point::new(100.0, 50.0)));
    let centre = Point::new(25.0, 25.0);
    let hole = loops
        .iter()
        .find(|l| l.iter().all(|p| (p.distance(centre) - 5.0).abs() <= 0.002))
        .expect("the Ø10 hole is one loop around (25, 25)");
    assert!(hole.len() > 16);
}

#[test]
fn unitless_dxf_scale_decision_sets_the_machine_extent() {
    let bytes = read_fixture("import", "unitless-lines.dxf").unwrap();
    let dxf = parse_dxf(&bytes).unwrap();
    let asset = asset_hash(&bytes);
    let inch = dxf.import_command(asset, Some(DxfScale::unit(Unit::Inch))).unwrap();
    let ((lo, hi), _) = rd_extent_of(inch);
    assert_eq!((lo, hi), (Point::new(0.0, 0.0), Point::new(101.6, 50.8)), "4 × 2 in");
    let millimetre = dxf.import_command(asset, Some(DxfScale::unit(Unit::Millimeter))).unwrap();
    let ((lo, hi), _) = rd_extent_of(millimetre);
    assert_eq!((lo, hi), (Point::new(0.0, 0.0), Point::new(4.0, 2.0)), "4 × 2 mm");
}

/// Regression (found by this scenario, 18-10-2026): the unitless L starts at (0, 0), which is also
/// the job start (machine origin), so the plan has no travel to it and `JobPlan::bounds` (which
/// excludes the start position) leaves x = 0 out: header `E7 03` min = (101.6, 0) while the
/// first cut runs from x = 0. The RD header must cover every burning position.
#[test]
fn rd_header_bounds_include_a_cut_that_starts_at_the_job_start() {
    let bytes = read_fixture("import", "unitless-lines.dxf").unwrap();
    let dxf = parse_dxf(&bytes).unwrap();
    let inch = dxf.import_command(asset_hash(&bytes), Some(DxfScale::unit(Unit::Inch))).unwrap();
    let (extent, header, _) = rd_of(inch);
    assert_eq!(header, extent, "RD header bounds = cut motion extent");
}

#[test]
fn rd_header_bounds_equal_the_cut_extent_for_the_svg_sheet() {
    let bytes = read_fixture("import", "mm-viewbox.svg").unwrap();
    let (extent, header, _) = rd_of(parse_svg(&bytes).unwrap().into_command(asset_hash(&bytes)).unwrap());
    assert_eq!(header, extent);
}
