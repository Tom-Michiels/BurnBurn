//! W1-L3: dot and ramp plan vocabulary and the capability matrix entries of the line and fill
//! extras. Plan ids and profile hashes of plans/profiles without the new vocabulary stay valid.

#![allow(clippy::unwrap_used)]

use burnburn_job::{
    Feature, JobPlanBuilder, MachineCapabilities, MachineFrame, MachinePoint, MachineProfile, MoveKind,
    OperationSettings, OriginCorner, PlanError, PlanEvent, PlanMode, PlanSource, PowerRatio, ProfileRevision, ScanAxis,
    ScanLine, Support, preview,
};
use burnburn_types::{ContentHash, OperationId, RevisionId, StableId};

fn pt(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn source() -> PlanSource {
    PlanSource {
        document_revision: RevisionId::from_raw(1),
        source_hash: ContentHash::from_bytes([7; 32]),
        machine_profile: ProfileRevision { id: "t".into(), revision: 1, content: ContentHash::from_bytes([3; 32]) },
        compiler_version: "test".into(),
    }
}

fn settings(mode: PlanMode) -> OperationSettings {
    let (lo, hi) = (PowerRatio::new(0.1).unwrap(), PowerRatio::new(0.5).unwrap());
    OperationSettings { mode, speed_mm_s: 100.0, min_power: lo, max_power: hi, air_assist: false }
}

fn builder() -> JobPlanBuilder {
    JobPlanBuilder::new(source(), MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap(), pt(0.0, 0.0))
}

fn begin(b: &mut JobPlanBuilder, mode: PlanMode) {
    let s = b.add_settings(settings(mode)).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
}

fn profile() -> MachineProfile {
    MachineProfile::new(
        "pin",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

#[test]
fn dot_events_belong_to_dot_settings_only() {
    let mut b = builder();
    assert_eq!(b.add_settings(settings(PlanMode::Dots { dwell_ms: 0.0 })), Err(PlanError::OutOfRange("dwell")));
    assert_eq!(b.add_settings(settings(PlanMode::Dots { dwell_ms: f64::NAN })), Err(PlanError::OutOfRange("dwell")));
    begin(&mut b, PlanMode::Line);
    assert_eq!(b.push(PlanEvent::Dot { origin: None }), Err(PlanError::EventDoesNotMatchMode));
    begin(&mut b, PlanMode::Dots { dwell_ms: 2.0 });
    b.push(PlanEvent::Travel { to: pt(5.0, 5.0) }).unwrap();
    b.push(PlanEvent::Dot { origin: None }).unwrap();
    assert_eq!(
        b.push(PlanEvent::Cut { to: pt(6.0, 5.0), origin: None }),
        Err(PlanError::EventDoesNotMatchMode),
        "a dot block never carries a continuous cut"
    );
}

#[test]
fn ramp_runs_belong_to_binary_scans_and_are_validated() {
    let run = ScanLine { from: pt(0.0, 0.0), to: pt(10.0, 0.0), overscan_mm: 0.0 };
    let ramp = |ramp_mm| PlanEvent::RampRun { line: run, ramp_mm, origin: None };
    let mut b = builder();
    begin(&mut b, PlanMode::Line);
    assert_eq!(b.push(ramp(1.0)), Err(PlanError::EventDoesNotMatchMode));
    begin(&mut b, PlanMode::GrayscaleScan { interval_mm: 0.1, bidirectional: true });
    assert_eq!(b.push(ramp(1.0)), Err(PlanError::EventDoesNotMatchMode), "grayscale runs have their own power");
    begin(&mut b, PlanMode::Scan { interval_mm: 0.1, axis: ScanAxis::X, bidirectional: true });
    assert_eq!(b.push(ramp(-1.0)), Err(PlanError::OutOfRange("ramp")));
    assert_eq!(b.push(ramp(f64::INFINITY)), Err(PlanError::OutOfRange("ramp")));
    b.push(ramp(1.0)).unwrap();
    assert_eq!(b.position(), pt(10.0, 0.0), "a ramped run moves the head like a plain run");
    // A run that does not start at the head is refused like a plain scan run.
    assert_eq!(b.push(ramp(1.0)), Err(PlanError::ScanRunNotAtCurrentPosition));
}

#[test]
fn dots_and_ramped_runs_count_in_bounds_preview_and_plan_id() {
    let dot_plan = |dwell_ms: f64, at: f64| {
        let mut b = builder();
        begin(&mut b, PlanMode::Dots { dwell_ms });
        b.push(PlanEvent::Travel { to: pt(at, 5.0) }).unwrap();
        b.push(PlanEvent::Dot { origin: None }).unwrap();
        b.finish().unwrap()
    };
    let plan = dot_plan(2.0, 5.0);
    assert_ne!(plan.id(), dot_plan(3.0, 5.0).id(), "dwell is part of the plan content");
    assert_ne!(plan.id(), dot_plan(2.0, 6.0).id());
    let b = plan.bounds().unwrap();
    assert_eq!((b.min().x, b.max().y), (5.0, 5.0));
    let pv = preview(&plan, &profile());
    // Travel from the start to the dot, then the pulse itself: a zero-length move that costs
    // exactly its dwell time.
    let kinds: Vec<MoveKind> = pv.moves.iter().map(|m| m.kind).collect();
    assert_eq!(kinds, vec![MoveKind::Travel, MoveKind::Dot]);
    let travel_s = 50f64.sqrt() / 400.0;
    assert!((pv.estimated_seconds - (travel_s + 0.002)).abs() < 1e-12, "{}", pv.estimated_seconds);

    let ramped = |ramp_mm: f64| {
        let mut b = builder();
        begin(&mut b, PlanMode::Scan { interval_mm: 0.1, axis: ScanAxis::X, bidirectional: true });
        b.push(PlanEvent::Travel { to: pt(2.0, 3.0) }).unwrap();
        let line = ScanLine { from: pt(2.0, 3.0), to: pt(12.0, 3.0), overscan_mm: 1.0 };
        b.push(PlanEvent::RampRun { line, ramp_mm, origin: None }).unwrap();
        b.finish().unwrap()
    };
    let (p, q) = (ramped(1.0), ramped(2.0));
    assert_ne!(p.id(), q.id());
    let b = p.bounds().unwrap();
    assert_eq!((b.min().x, b.max().x), (1.0, 13.0), "overscan of a ramped run counts in the bounds");
    let pv = preview(&p, &profile());
    let kinds: Vec<MoveKind> = pv.moves.iter().map(|m| m.kind).collect();
    assert_eq!(kinds, vec![MoveKind::Travel, MoveKind::Overscan, MoveKind::Scan, MoveKind::Overscan]);
    assert_eq!(pv.scan_length_mm, 10.0);
}

#[test]
fn new_features_have_conservative_defaults_and_do_not_move_existing_profile_hashes() {
    let caps = MachineCapabilities::conservative();
    for f in [Feature::CrossHatch, Feature::FloodFill, Feature::FillGrouping] {
        assert_eq!(caps.support(f), Support::Supported, "{f:?} only reorders or repeats ordinary scan events");
    }
    for f in [Feature::DotMode, Feature::RampLength] {
        assert!(matches!(caps.support(f), Support::Unverified { .. }), "{f:?} has no proven RD form");
    }
    // Values computed with the matrix of IM25 (12 features) before the new entries existed.
    assert_eq!(
        profile().revision().content.to_hex(),
        "e8b841b4a5d492f15a4bd99e7896c5f7205169d3b2e6f0e9dff451fae4ac0461"
    );
    let edited = profile()
        .with_capabilities(
            MachineCapabilities::conservative()
                .with(Feature::Tabs, Support::Unsupported)
                .with(Feature::ScanAngle, Support::Supported),
        )
        .unwrap();
    assert_eq!(edited.revision().content.to_hex(), "f0fb5603b0e52f3248cbceca502863687595078bbd64444b6d899b97e270408e");
    // Overriding a new feature does change the profile content.
    let dots = profile().with_capabilities(caps.with(Feature::DotMode, Support::Supported)).unwrap();
    assert_ne!(dots.revision().content, profile().revision().content);
    assert_eq!(Feature::DotMode.tag(), "dot-mode");
}
