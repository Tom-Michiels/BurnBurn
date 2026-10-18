//! Public contract of the immutable JobPlan (IM13).

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanError, PlanEvent,
    PlanMode, PlanSource, PowerRatio, ProfileRevision, ScanAxis, ScanLine, SourceRef,
};
use burnburn_types::{ContentHash, GeometryId, OperationId, RevisionId, StableId};

pub fn pt(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn source(revision: u64) -> PlanSource {
    PlanSource {
        document_revision: RevisionId::from_raw(revision),
        source_hash: ContentHash::from_bytes([7; 32]),
        machine_profile: ProfileRevision {
            id: "test-bed".into(),
            revision: 1,
            content: ContentHash::from_bytes([3; 32]),
        },
        compiler_version: "test".into(),
    }
}

fn line_settings() -> OperationSettings {
    let p = PowerRatio::new(0.2).unwrap();
    OperationSettings { mode: PlanMode::Line, speed_mm_s: 100.0, min_power: p, max_power: p, air_assist: true }
}

fn scan_settings() -> OperationSettings {
    OperationSettings {
        mode: PlanMode::Scan { interval_mm: 0.1, axis: ScanAxis::X, bidirectional: true },
        ..line_settings()
    }
}

fn origin() -> Option<SourceRef> {
    Some(SourceRef { geometry: GeometryId::from_raw(1), operation: OperationId::from_raw(2) })
}

/// A 30 × 15 mm square cut, as events.
fn square(revision: u64, last_x: f64) -> JobPlan {
    let frame = MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap();
    let mut b = JobPlanBuilder::new(source(revision), frame, pt(0.0, 0.0));
    let s = b.add_settings(line_settings()).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(10.0, 10.0) }).unwrap();
    for (x, y) in [(40.0, 10.0), (40.0, 25.0), (10.0, 25.0), (last_x, 10.0)] {
        b.push(PlanEvent::Cut { to: pt(x, y), origin: origin() }).unwrap();
    }
    b.finish().unwrap()
}

#[test]
fn plan_id_is_a_content_hash_independent_of_session_revision() {
    let a = square(5, 10.0);
    let b = square(5, 10.0);
    assert_eq!(a.id(), b.id(), "same content, same id");
    assert_eq!(square(9, 10.0).id(), a.id(), "a session revision is provenance, not content");
    assert_ne!(square(5, 10.000_001).id(), a.id(), "any event change changes the id");
    assert_eq!(a.source().document_revision, RevisionId::from_raw(5));
    assert_eq!(a.id().hash().to_hex().len(), 64);
}

#[test]
fn builder_rejects_events_that_do_not_match_their_settings() {
    let frame = MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap();
    let mut b = JobPlanBuilder::new(source(1), frame, pt(0.0, 0.0));
    assert_eq!(b.push(PlanEvent::Cut { to: pt(1.0, 1.0), origin: None }), Err(PlanError::MotionOutsideOperation));
    let scan = b.add_settings(scan_settings()).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: scan, pass: 0 }).unwrap();
    assert_eq!(b.push(PlanEvent::Cut { to: pt(1.0, 1.0), origin: None }), Err(PlanError::EventDoesNotMatchMode));
    let run = ScanLine { from: pt(5.0, 5.0), to: pt(9.0, 5.0), overscan_mm: 0.0 };
    assert_eq!(
        b.push(PlanEvent::ScanRun { line: run, origin: None }),
        Err(PlanError::ScanRunNotAtCurrentPosition),
        "a scan run starts where the head is; the compiler travels there first"
    );
    b.push(PlanEvent::Travel { to: pt(5.0, 5.0) }).unwrap();
    b.push(PlanEvent::ScanRun { line: run, origin: None }).unwrap();
    let mut wrong = line_settings();
    wrong.speed_mm_s = 0.0;
    assert_eq!(b.add_settings(wrong), Err(PlanError::OutOfRange("speed")));
    assert_eq!(MachinePoint::new(f64::NAN, 0.0), Err(PlanError::NotFinite));
}

#[test]
fn plan_bounds_include_travel_and_overscan() {
    let frame = MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap();
    let mut b = JobPlanBuilder::new(source(1), frame, pt(0.0, 0.0));
    let scan = b.add_settings(scan_settings()).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: scan, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(10.0, 20.0) }).unwrap();
    let run = ScanLine { from: pt(10.0, 20.0), to: pt(30.0, 20.0), overscan_mm: 2.5 };
    b.push(PlanEvent::ScanRun { line: run, origin: origin() }).unwrap();
    b.push(PlanEvent::Travel { to: pt(30.0, 50.0) }).unwrap();
    let plan = b.finish().unwrap();
    let bounds = plan.bounds().unwrap();
    assert_eq!((bounds.min().x, bounds.min().y), (7.5, 20.0), "run-up 2.5 mm before the run");
    assert_eq!((bounds.max().x, bounds.max().y), (32.5, 50.0), "run-out and the final travel");
}

#[test]
fn machine_frame_maps_each_origin_corner_and_back() {
    use burnburn_geometry::Point;
    // Bed view point 10 mm right of and 20 mm above the bottom-left corner of a 300 × 200 bed.
    let p = Point::new(10.0, 20.0);
    let expected = [
        (OriginCorner::BottomLeft, (10.0, 20.0)),
        (OriginCorner::TopLeft, (10.0, 180.0)),
        (OriginCorner::TopRight, (290.0, 180.0)),
        (OriginCorner::BottomRight, (290.0, 20.0)),
    ];
    for (corner, (x, y)) in expected {
        let frame = MachineFrame::new(300.0, 200.0, corner).unwrap();
        let m = frame.to_machine(p).unwrap();
        assert_eq!((m.x_mm(), m.y_mm()), (x, y), "{corner:?}");
        assert_eq!(frame.to_bed(m), p, "{corner:?}: inverse");
        // The origin corner itself is machine (0, 0).
        let at_origin = frame.to_bed(pt(0.0, 0.0));
        assert!(at_origin.x == 0.0 || at_origin.x == 300.0);
    }
    let frame = MachineFrame::new(300.0, 200.0, OriginCorner::BottomLeft).unwrap();
    assert!(frame.contains(pt(300.0, 200.0)), "bed edge is inside");
    assert!(!frame.contains(pt(300.000_000_1, 0.0)), "no rounding inward at the edge");
    assert!(!frame.contains(pt(-1e-12, 0.0)));
}

#[test]
fn power_runs_belong_to_grayscale_settings_only_and_angles_are_validated() {
    let frame = MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap();
    let mut b = JobPlanBuilder::new(source(1), frame, pt(0.0, 0.0));
    let binary = b.add_settings(scan_settings()).unwrap();
    let gray = b
        .add_settings(OperationSettings {
            mode: PlanMode::GrayscaleScan { interval_mm: 0.1, bidirectional: true },
            ..line_settings()
        })
        .unwrap();
    let run = |p: f64| PlanEvent::PowerRun {
        line: ScanLine { from: pt(0.0, 0.0), to: pt(1.0, 0.0), overscan_mm: 0.0 },
        power: PowerRatio::new(p).unwrap(),
        origin: origin(),
    };
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: binary, pass: 0 }).unwrap();
    assert_eq!(b.push(run(0.5)), Err(PlanError::EventDoesNotMatchMode), "no modulated run under on/off scan settings");
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: gray, pass: 0 }).unwrap();
    assert_eq!(b.push(run(0.5)), Ok(()));
    let angled = |a: f64| OperationSettings {
        mode: PlanMode::AngledScan { interval_mm: 0.1, angle_deg: a, bidirectional: true },
        ..line_settings()
    };
    assert_eq!(b.add_settings(angled(90.0)), Err(PlanError::OutOfRange("angle")), "90° is axis Y, not an angle");
    assert!(b.add_settings(angled(30.0)).is_ok());
}

#[test]
fn capability_matrix_is_profile_content_with_conservative_defaults() {
    use burnburn_job::{Feature, MachineCapabilities, MachineProfile, Support};
    let caps = MachineCapabilities::conservative();
    assert!(matches!(caps.support(Feature::GrayscaleModulation), Support::Unverified { .. }));
    assert!(matches!(caps.support(Feature::ScanAngle), Support::Unverified { .. }));
    assert!(matches!(caps.support(Feature::CommonLine), Support::Unverified { .. }));
    assert_eq!(caps.support(Feature::ZStep), Support::Unsupported);
    assert_eq!(caps.support(Feature::Tabs), Support::Supported);
    let frame = MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap();
    let base =
        MachineProfile::new("m", 1, frame, 1000.0, 400.0, PowerRatio::new(0.0).unwrap(), PowerRatio::new(1.0).unwrap())
            .unwrap();
    let claimed = base.clone().with_capabilities(caps.clone().with(Feature::ZStep, Support::Supported)).unwrap();
    assert_ne!(base.revision().content, claimed.revision().content, "another matrix is another profile revision");
    let same = base.clone().with_capabilities(caps.with(Feature::Tabs, Support::Supported)).unwrap();
    assert_eq!(base.revision().content, same.revision().content, "explicitly setting a default changes nothing");
}

/// Regression (Testing, scenario import_to_rd): a cut that starts at the job start position has
/// no travel before it; its start point is burned and must be in the bounds.
#[test]
fn plan_bounds_include_the_start_of_a_cut_that_begins_at_the_job_start() {
    let frame = MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap();
    let mut b = JobPlanBuilder::new(source(1), frame, pt(0.0, 0.0));
    let s = b.add_settings(line_settings()).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Cut { to: pt(10.0, 5.0), origin: origin() }).unwrap();
    let bounds = b.finish().unwrap().bounds().unwrap();
    assert_eq!((bounds.min().x, bounds.min().y, bounds.max().x, bounds.max().y), (0.0, 0.0, 10.0, 5.0));
}
