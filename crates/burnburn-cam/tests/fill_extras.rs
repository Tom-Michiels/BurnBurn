//! W1-L3b fill extras: cross-hatch, flood fill, fill grouping and ramp length. Oracles are
//! analytic: rows at half-interval centres, scanned lengths per even-odd span, region order.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{CompileError, required_features};
use burnburn_document::process::{
    FillGrouping, FillSettings, OperationDraft, OperationMode, ScanAxis as DocAxis, ScanDirection,
};
use burnburn_document::{CommandPayload, DocumentSession, GeometryCommand};
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_job::{
    Feature, JobPlan, MachineCapabilities, MachinePoint, PlanEvent, PlanMode, ScanAxis, Support, preview,
};
use burnburn_types::{GeometryId, OperationId, StableId};
use common::*;

fn fill_settings(interval: f64) -> FillSettings {
    FillSettings::new(mm(interval), DocAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap()
}

fn fill_draft(f: FillSettings) -> OperationDraft {
    OperationDraft {
        name: "Vullen".into(),
        mode: OperationMode::Fill(f),
        settings: settings(100.0, 20.0),
        output_enabled: true,
    }
}

fn path_geometry(s: &mut DocumentSession, path: Path) -> GeometryId {
    run(s, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: String::new() })).changes.created[0]
}

/// Two disjoint 10 × 10 squares at x = 0 and x = 20 as one geometry.
fn two_squares() -> Path {
    let square = |b: PathBuilder, x: f64| {
        b.move_to(Point::new(x, 0.0))
            .line_to(Point::new(x + 10.0, 0.0))
            .line_to(Point::new(x + 10.0, 10.0))
            .line_to(Point::new(x, 10.0))
            .close()
    };
    square(square(PathBuilder::new(), 0.0), 20.0).build().unwrap()
}

/// `(from, to)` of every scan run (plain or ramped).
fn runs(plan: &JobPlan) -> Vec<(MachinePoint, MachinePoint)> {
    plan.events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::ScanRun { line, .. } | PlanEvent::RampRun { line, .. } => Some((line.from, line.to)),
            _ => None,
        })
        .collect()
}

fn one_op(f: FillSettings, w: f64, h: f64) -> DocumentSession {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, w, h);
    add_op(&mut s, fill_draft(f), vec![g]);
    s
}

fn unsupported(feature: Feature) -> burnburn_job::MachineProfile {
    profile().with_capabilities(MachineCapabilities::conservative().with(feature, Support::Unsupported)).unwrap()
}

#[test]
fn cross_hatch_adds_a_perpendicular_pass_at_the_same_interval() {
    let s = one_op(fill_settings(1.0).with_cross_hatch(true), 10.0, 10.0);
    let plan = plan_from_outside(&s, &profile()).unwrap();
    assert_eq!(plan.settings().len(), 2, "one settings block per scan direction");
    assert_eq!(plan.settings()[0].mode, PlanMode::Scan { interval_mm: 1.0, axis: ScanAxis::X, bidirectional: true });
    assert_eq!(plan.settings()[1].mode, PlanMode::Scan { interval_mm: 1.0, axis: ScanAxis::Y, bidirectional: true });
    let boundaries: Vec<(usize, u32)> = plan
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::OperationBoundary { settings, pass, .. } => Some((settings.get(), pass)),
            _ => None,
        })
        .collect();
    assert_eq!(boundaries, vec![(0, 0), (1, 0)], "both directions belong to pass 0");
    let r = runs(&plan);
    assert_eq!(r.len(), 20);
    let (horizontal, vertical) = r.split_at(10);
    assert!(horizontal.iter().all(|(a, b)| a.y_mm() == b.y_mm() && (a.x_mm() - b.x_mm()).abs() == 10.0));
    assert!(vertical.iter().all(|(a, b)| a.x_mm() == b.x_mm() && (a.y_mm() - b.y_mm()).abs() == 10.0));
    let mut rows: Vec<f64> = horizontal.iter().map(|(a, _)| a.y_mm()).collect();
    let mut cols: Vec<f64> = vertical.iter().map(|(a, _)| a.x_mm()).collect();
    rows.sort_by(f64::total_cmp);
    cols.sort_by(f64::total_cmp);
    let centres: Vec<f64> = (0..10).map(|k| 0.5 + f64::from(k)).collect();
    assert_eq!((rows, cols), (centres.clone(), centres), "rows at half-interval centres in both directions");
    assert!((preview(&plan, &profile()).scan_length_mm - 200.0).abs() < 1e-9);
}

#[test]
fn cross_hatch_of_a_rotated_fill_adds_the_perpendicular_angle() {
    let f = fill_settings(1.0).with_angle(45.0).unwrap().with_cross_hatch(true);
    let s = one_op(f, 10.0, 10.0);
    let plan = plan_from_outside(&s, &supporting(&[Feature::ScanAngle])).unwrap();
    assert_eq!(
        plan.settings()[0].mode,
        PlanMode::AngledScan { interval_mm: 1.0, angle_deg: 45.0, bidirectional: true }
    );
    assert_eq!(
        plan.settings()[1].mode,
        PlanMode::AngledScan { interval_mm: 1.0, angle_deg: 135.0, bidirectional: true }
    );
    // A fill along Y gets its perpendicular partner along X.
    let g =
        FillSettings::new(mm(1.0), DocAxis::Y, ScanDirection::Bidirectional, mm(0.0)).unwrap().with_cross_hatch(true);
    let plan = plan_from_outside(&one_op(g, 10.0, 10.0), &profile()).unwrap();
    assert!(matches!(plan.settings()[0].mode, PlanMode::Scan { axis: ScanAxis::Y, .. }));
    assert!(matches!(plan.settings()[1].mode, PlanMode::Scan { axis: ScanAxis::X, .. }));
}

#[test]
fn fill_extras_are_gated_by_the_capability_matrix_with_typed_errors() {
    let f = fill_settings(1.0);
    for (feature, settings) in [
        (Feature::CrossHatch, f.with_cross_hatch(true)),
        (Feature::FloodFill, f.with_flood_fill(true)),
        (Feature::FillGrouping, f.with_grouping(FillGrouping::Individual)),
    ] {
        let s = one_op(settings, 10.0, 10.0);
        let err = plan_from_outside(&s, &unsupported(feature)).unwrap_err();
        assert_eq!(
            err,
            CompileError::UnsupportedFeature {
                operation: OperationId::from_raw(2),
                feature,
                support: Support::Unsupported
            }
        );
        assert!(plan_from_outside(&s, &profile()).is_ok(), "{feature:?} is supported by default");
    }
    // Without a proof for ramps (RD form open) the default matrix refuses them.
    let s = one_op(f.with_ramp(mm(1.0)).unwrap(), 10.0, 10.0);
    let err = plan_from_outside(&s, &profile()).unwrap_err();
    assert!(
        matches!(
            err,
            CompileError::UnsupportedFeature { feature: Feature::RampLength, support: Support::Unverified { .. }, .. }
        ),
        "{err:?}"
    );
    // The default fill needs none of them.
    assert!(plan_from_outside(&one_op(f, 10.0, 10.0), &unsupported(Feature::FloodFill)).is_ok());
}

#[test]
fn required_features_lists_the_fill_extras_in_a_fixed_order() {
    let f = fill_settings(1.0)
        .with_angle(30.0)
        .unwrap()
        .with_ramp(mm(1.0))
        .unwrap()
        .with_grouping(FillGrouping::Groups)
        .with_flood_fill(true)
        .with_cross_hatch(true);
    let s = one_op(f, 10.0, 10.0);
    let op = s.snapshot().process().operation(OperationId::from_raw(2)).unwrap().clone();
    assert_eq!(
        required_features(&op),
        vec![Feature::ScanAngle, Feature::CrossHatch, Feature::FloodFill, Feature::FillGrouping, Feature::RampLength]
    );
}

#[test]
fn flood_fill_scans_each_connected_region_completely_before_the_next() {
    // Two squares, 4 rows each at interval 2.5 (rows at y = 1.25, 3.75, 6.25, 8.75).
    let build = |flood: bool| {
        let mut s = session();
        let g = path_geometry(&mut s, two_squares());
        add_op(&mut s, fill_draft(fill_settings(2.5).with_flood_fill(flood)), vec![g]);
        plan_from_outside(&s, &profile()).unwrap()
    };
    let (normal, flood) = (build(false), build(true));
    let in_square = |r: &(MachinePoint, MachinePoint), lo: f64| {
        r.0.x_mm() >= lo && r.0.x_mm() <= lo + 10.0 && r.1.x_mm() >= lo && r.1.x_mm() <= lo + 10.0
    };
    let n = runs(&normal);
    assert_eq!(n.len(), 8);
    assert!(in_square(&n[0], 0.0) && in_square(&n[1], 20.0), "row by row the head jumps between the squares");
    let f = runs(&flood);
    assert_eq!(f.len(), 8);
    assert!(f[..4].iter().all(|r| in_square(r, 0.0)), "left square first: {f:?}");
    assert!(f[4..].iter().all(|r| in_square(r, 20.0)), "then the right one: {f:?}");
    let ys: Vec<f64> = f[..4].iter().map(|r| r.0.y_mm()).collect();
    assert_eq!(ys, vec![1.25, 3.75, 6.25, 8.75], "within a region the rows still run in order");
    let (pn, pf) = (preview(&normal, &profile()), preview(&flood, &profile()));
    assert!((pn.scan_length_mm - pf.scan_length_mm).abs() < 1e-9, "the same area is burned");
    // Travel by hand. Entry from (−1, −1) to (0, 1.25): √(1 + 2.25²). Row by row: every row
    // crosses the 10 mm gap once (4 × 10) and three row steps of 2.5 mm. Flood: two squares with
    // three 2.5 mm row steps each, and one jump from (0, 8.75) to the nearest corner row of the
    // other square, (20, 8.75), i.e. 20 mm.
    let entry = (1.0f64 + 2.25 * 2.25).sqrt();
    assert!((pn.travel_length_mm - (entry + 40.0 + 7.5)).abs() < 1e-9, "{}", pn.travel_length_mm);
    assert!((pf.travel_length_mm - (entry + 7.5 + 20.0 + 7.5)).abs() < 1e-9, "{}", pf.travel_length_mm);
}

#[test]
fn flood_fill_keeps_a_connected_shape_in_ordinary_row_order() {
    // A U: the arms are joined by the two lower rows, so there is one region.
    let u = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(30.0, 0.0))
        .line_to(Point::new(30.0, 10.0))
        .line_to(Point::new(20.0, 10.0))
        .line_to(Point::new(20.0, 4.0))
        .line_to(Point::new(10.0, 4.0))
        .line_to(Point::new(10.0, 10.0))
        .line_to(Point::new(0.0, 10.0))
        .close()
        .build()
        .unwrap();
    let build = |flood: bool| {
        let mut s = session();
        let g = path_geometry(&mut s, u.clone());
        add_op(&mut s, fill_draft(fill_settings(2.5).with_flood_fill(flood)), vec![g]);
        plan_from_outside(&s, &profile()).unwrap()
    };
    assert_eq!(build(true).events(), build(false).events());
}

/// A, B overlapping squares in one group; C an ungrouped copy of A. 10 mm high, interval 2.5.
fn grouping_doc(grouping: FillGrouping) -> JobPlan {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rectangle(&mut s, 5.0, 0.0, 10.0, 10.0);
    let c = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    run(&mut s, CommandPayload::Edit(GeometryCommand::Group { targets: vec![a, b] }));
    add_op(&mut s, fill_draft(fill_settings(2.5).with_grouping(grouping)), vec![a, b, c]);
    plan_from_outside(&s, &profile()).unwrap()
}

#[test]
fn fill_grouping_decides_which_shapes_share_the_even_odd_rule() {
    let scanned = |p: &JobPlan| preview(p, &profile()).scan_length_mm;
    // All at once: A and C cancel, B overlaps nothing else → [5,10] and [10,15] per row.
    let all = grouping_doc(FillGrouping::AllShapes);
    assert!((scanned(&all) - 4.0 * 10.0).abs() < 1e-9, "{}", scanned(&all));
    assert_eq!(runs(&all).len(), 8);
    // Groups: {A, B} together leave [0,5] and [10,15] (10 per row), C alone adds 10 per row.
    let groups = grouping_doc(FillGrouping::Groups);
    assert!((scanned(&groups) - 4.0 * 20.0).abs() < 1e-9, "{}", scanned(&groups));
    // Individually: three shapes of 10 per row.
    let individual = grouping_doc(FillGrouping::Individual);
    assert!((scanned(&individual) - 4.0 * 30.0).abs() < 1e-9, "{}", scanned(&individual));
    // Shapes are finished one after the other in target order: A's four rows, B's, then C's.
    let r = runs(&individual);
    assert_eq!(r.len(), 12);
    let within = |rs: &[(MachinePoint, MachinePoint)], lo: f64, hi: f64| {
        rs.iter().all(|(a, b)| a.x_mm().min(b.x_mm()) >= lo && a.x_mm().max(b.x_mm()) <= hi)
    };
    assert!(within(&r[..4], 0.0, 10.0) && within(&r[4..8], 5.0, 15.0) && within(&r[8..], 0.0, 10.0), "{r:?}");
}

#[test]
fn ramp_length_turns_every_run_into_a_ramped_run() {
    let s = one_op(fill_settings(1.0).with_ramp(mm(1.0)).unwrap(), 10.0, 10.0);
    let plan = plan_from_outside(&s, &supporting(&[Feature::RampLength])).unwrap();
    let ramped: Vec<f64> = plan
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::RampRun { ramp_mm, .. } => Some(ramp_mm),
            _ => None,
        })
        .collect();
    assert_eq!(ramped, vec![1.0; 10]);
    assert!(!plan.events().iter().any(|e| matches!(e, PlanEvent::ScanRun { .. })), "no plain runs mixed in");
    // No ramp: plain runs, and the default profile accepts them.
    let plain = plan_from_outside(&one_op(fill_settings(1.0), 10.0, 10.0), &profile()).unwrap();
    assert!(plain.events().iter().all(|e| !matches!(e, PlanEvent::RampRun { .. })));
}
