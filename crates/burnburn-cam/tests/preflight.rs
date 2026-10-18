//! Preflight (IM15, C09): findings linked to source geometry, capability data from the profile,
//! exact bounds checks and nominal size.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use burnburn_cam::{Finding, Severity, compile, preflight};
use burnburn_document::{
    CommandPayload, DocumentSession, DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind,
};
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_job::{JobPlan, SourceRef};
use burnburn_types::{CancellationToken, DocumentId, GeometryId, StableId};
use common::*;

fn compiled(s: &DocumentSession) -> JobPlan {
    compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap()
}

#[test]
fn outside_bed_operation_links_error_to_source_geometry() {
    let mut s = session();
    let inside = rectangle(&mut s, 100.0, 100.0, 50.0, 50.0);
    let outside = rectangle(&mut s, 1480.0, 100.0, 50.0, 50.0); // right edge at 1530 > 1500
    let op = add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![inside, outside]);
    let plan = compiled(&s);
    let report = preflight(&s.snapshot(), &plan, &profile());

    let errors: Vec<&Finding> = report.errors().collect();
    assert_eq!(errors, vec![&Finding::OutsideBed { source: SourceRef { geometry: outside, operation: op } }]);
    assert!(report.has_errors());
}

#[test]
fn bounds_check_is_exact_and_never_rounds_inward() {
    // 0.4 µm beyond the bed edge: a µm-quantising backend would round it onto the edge.
    let mut s = session();
    let g = rectangle(&mut s, 1400.0, 100.0, 100.000_4, 10.0);
    let op = add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![g]);
    let report = preflight(&s.snapshot(), &compiled(&s), &profile());
    assert_eq!(
        report.errors().collect::<Vec<_>>(),
        vec![&Finding::OutsideBed { source: SourceRef { geometry: g, operation: op } }]
    );

    // Exactly on the edge is inside.
    let mut t = session();
    let g = rectangle(&mut t, 1400.0, 800.0, 100.0, 100.0);
    add_op(&mut t, line_op("Snijden", 100.0, 20.0), vec![g]);
    let report = preflight(&t.snapshot(), &compiled(&t), &profile());
    assert!(!report.has_errors(), "{:?}", report.findings);
}

#[test]
fn overscan_outside_bed_is_a_laser_off_error_without_blaming_the_run() {
    let mut s = session();
    let g = rectangle(&mut s, 0.5, 100.0, 10.0, 1.0); // 2 mm run-up starts at x = −1.5
    let op = add_op(&mut s, fill_op("Vullen", 0.5, 2.0), vec![g]);
    let report = preflight(&s.snapshot(), &compiled(&s), &profile());
    assert_eq!(report.errors().collect::<Vec<_>>(), vec![&Finding::TravelOutsideBed { operation: Some(op) }]);
}

#[test]
fn capability_and_input_problems_are_classified_and_linked() {
    let open = PathBuilder::new().move_to(Point::new(10.0, 10.0)).line_to(Point::new(50.0, 10.0)).build().unwrap();
    let obj = |id: u64, path| {
        GeometryObject::new(GeometryId::from_raw(id), String::new(), ObjectKind::Path, GeometrySource::Drawn, path)
    };
    let square = path_of(&[(100.0, 100.0), (120.0, 100.0), (120.0, 120.0), (100.0, 120.0)]);
    let spare = path_of(&[(300.0, 100.0), (320.0, 100.0), (320.0, 120.0), (300.0, 120.0)]);
    let snap = DocumentSnapshot::from_parts(vec![obj(1, open), obj(2, square), obj(3, spare)], 4).unwrap();
    let mut s = DocumentSession::from_snapshot(DocumentId::from_raw(1), snap);
    let (open, square, spare) = (GeometryId::from_raw(1), GeometryId::from_raw(2), GeometryId::from_raw(3));

    let fill = add_op(&mut s, fill_op("Vullen", 1.0, 0.0), vec![open, square]);
    let fast = add_op(&mut s, line_op("Te snel", 2000.0, 20.0), vec![square]); // profile max 1000 mm/s
    let score = add_op(&mut s, line_op("Rillen", 100.0, 5.0), vec![square]);
    let empty = add_op(&mut s, line_op("Leeg", 100.0, 5.0), vec![]);

    let report = preflight(&s.snapshot(), &compiled(&s), &profile());
    let mut errors: Vec<Finding> = report.errors().cloned().collect();
    errors.sort_by_key(|f| format!("{f:?}"));
    assert_eq!(
        errors,
        vec![
            Finding::OpenFillContour { operation: fill, geometry: open },
            Finding::SpeedAboveMachineMax { operation: fast, speed_mm_s: 2000.0, max_mm_s: 1000.0 },
        ]
    );
    let warnings: Vec<Finding> = report.warnings().cloned().collect();
    assert!(warnings.contains(&Finding::DuplicateCut { geometry: square, operations: vec![fast, score] }));
    assert!(warnings.contains(&Finding::EmptyOperation { operation: empty }));
    assert!(warnings.contains(&Finding::UnassignedGeometry { geometry: spare }));
    assert!(report.findings.iter().all(|f| matches!(f.severity(), Severity::Error | Severity::Warning)));
}

#[test]
fn plan_for_another_revision_or_profile_is_stale() {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 10.0, 10.0, 10.0);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![g]);
    let plan = compiled(&s);
    assert!(!preflight(&s.snapshot(), &plan, &profile()).has_errors());

    run(&mut s, CommandPayload::Rename { target: g, name: "nieuw".into() });
    let report = preflight(&s.snapshot(), &plan, &profile());
    assert!(report.findings.contains(&Finding::StalePlan), "an edit after compile makes the plan stale");

    let other = profile_with(burnburn_job::OriginCorner::TopRight);
    let fresh = compiled(&s);
    assert!(preflight(&s.snapshot(), &fresh, &other).findings.contains(&Finding::StalePlan));
}

#[test]
fn planned_size_equals_nominal_size_without_kerf() {
    let mut s = session();
    let rect = rectangle(&mut s, 10.0, 10.0, 120.0, 80.0);
    let circle = Path::circle(Point::new(300.0, 300.0), 25.0).unwrap();
    let snap = s.snapshot();
    let mut objects: Vec<GeometryObject> = snap.objects().map(|(_, o)| o.clone()).collect();
    objects.push(GeometryObject::new(
        GeometryId::from_raw(9),
        String::new(),
        ObjectKind::Path,
        GeometrySource::Drawn,
        circle,
    ));
    let mut s =
        DocumentSession::from_snapshot(DocumentId::from_raw(1), DocumentSnapshot::from_parts(objects, 10).unwrap());
    let circle = GeometryId::from_raw(9);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![rect, circle]);
    let report = preflight(&s.snapshot(), &compiled(&s), &profile());

    let size = |g| report.sizes.iter().find(|c| c.source.geometry == g).unwrap();
    let r = size(rect);
    assert_eq!((r.nominal_mm, r.planned_mm), ((120.0, 80.0), (120.0, 80.0)), "no kerf: nominal size is cut");
    // A flattened circle never grows beyond its nominal size and loses at most the tolerance per side.
    let c = size(circle);
    assert_eq!(c.nominal_mm, (50.0, 50.0));
    for (planned, nominal) in [(c.planned_mm.0, c.nominal_mm.0), (c.planned_mm.1, c.nominal_mm.1)] {
        assert!(planned <= nominal + 1e-9 && planned >= nominal - 2.0 * 0.01, "{planned} vs {nominal}");
    }
}

#[test]
fn plan_checked_against_a_profile_without_a_used_feature_is_blocked() {
    use burnburn_document::process::{AdvancedOptions, LineOptions, ProcessCommand, TabSpec};
    use burnburn_job::{Feature, MachineCapabilities, Support};
    let mut s = session();
    let g = rectangle(&mut s, 100.0, 100.0, 50.0, 50.0);
    let op = add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![g]);
    let line = LineOptions { tabs: Some(TabSpec::new(4, mm(1.0)).unwrap()), ..Default::default() };
    let advanced = AdvancedOptions { line, ..Default::default() };
    run(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }));
    let plan = compiled(&s);
    // The machine was switched to one that cannot leave tabs.
    let other = profile()
        .with_capabilities(MachineCapabilities::conservative().with(Feature::Tabs, Support::Unsupported))
        .unwrap();
    let report = preflight(&s.snapshot(), &plan, &other);
    let errors: Vec<&Finding> = report.errors().collect();
    assert_eq!(
        errors,
        vec![&Finding::StalePlan, &Finding::UnsupportedFeature { operation: op, feature: Feature::Tabs }]
    );
}
