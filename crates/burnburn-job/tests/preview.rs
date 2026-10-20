//! Preview data (IM15, C07): derived from one frozen plan, traceable, with a labelled estimate.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_job::{
    JobPlanBuilder, MachineFrame, MachinePoint, MachineProfile, MoveKind, OperationSettings, OriginCorner, PlanEvent,
    PlanMode, PlanSource, PowerRatio, ScanAxis, ScanLine, SourceRef, preview,
};
use burnburn_types::{ContentHash, GeometryId, OperationId, RevisionId, StableId};

fn pt(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn profile() -> MachineProfile {
    MachineProfile::new(
        "test-bed",
        1,
        MachineFrame::new(300.0, 200.0, OriginCorner::BottomLeft).unwrap(),
        500.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn builder() -> JobPlanBuilder {
    let source = PlanSource {
        document_revision: RevisionId::from_raw(3),
        source_hash: ContentHash::from_bytes([1; 32]),
        machine_profile: profile().revision().clone(),
        compiler_version: "test".into(),
    };
    JobPlanBuilder::new(source, *profile().frame(), pt(0.0, 0.0))
}

fn src() -> SourceRef {
    SourceRef { geometry: GeometryId::from_raw(1), operation: OperationId::from_raw(2) }
}

#[test]
fn preview_moves_trace_to_source_and_estimate_time() {
    let mut b = builder();
    let p = PowerRatio::new(0.5).unwrap();
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 100.0,
            min_power: p,
            max_power: p,
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(30.0, 40.0) }).unwrap(); // 50 mm
    for (x, y) in [(60.0, 40.0), (60.0, 55.0), (30.0, 55.0), (30.0, 40.0)] {
        b.push(PlanEvent::Cut { to: pt(x, y), origin: Some(src()) }).unwrap();
    }
    let plan = b.finish().unwrap();
    let pv = preview(&plan, &profile());

    assert_eq!(pv.plan, plan.id(), "preview names the plan it was derived from");
    let kinds: Vec<MoveKind> = pv.moves.iter().map(|m| m.kind).collect();
    assert_eq!(kinds, vec![MoveKind::Travel, MoveKind::Cut, MoveKind::Cut, MoveKind::Cut, MoveKind::Cut]);
    assert_eq!((pv.moves[0].from, pv.moves[0].to), (pt(0.0, 0.0), pt(30.0, 40.0)), "travel from the start point");
    assert!(pv.moves[1..].iter().all(|m| m.source == Some(src()) && m.operation == Some(OperationId::from_raw(2))));
    assert_eq!(pv.moves[2].event, 3, "each segment points at its plan event");
    assert_eq!((pv.travel_length_mm, pv.cut_length_mm), (50.0, 90.0));
    // 90 mm at 100 mm/s + 50 mm at 400 mm/s travel speed.
    assert!((pv.estimated_seconds - (0.9 + 0.125)).abs() < 1e-12, "{}", pv.estimated_seconds);
}

#[test]
fn scan_preview_shows_overscan_as_firmware_dependent_laser_off_motion() {
    let mut b = builder();
    let p = PowerRatio::new(0.2).unwrap();
    let mode = PlanMode::Scan { interval_mm: 0.5, axis: ScanAxis::X, bidirectional: true };
    let s = b
        .add_settings(OperationSettings { mode, speed_mm_s: 200.0, min_power: p, max_power: p, air_assist: true })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(10.0, 10.0) }).unwrap();
    let run = ScanLine { from: pt(10.0, 10.0), to: pt(30.0, 10.0), overscan_mm: 1.0 };
    b.push(PlanEvent::ScanRun { line: run, origin: Some(src()) }).unwrap();
    let plan = b.finish().unwrap();
    let pv = preview(&plan, &profile());

    let kinds: Vec<MoveKind> = pv.moves.iter().map(|m| m.kind).collect();
    assert_eq!(kinds, vec![MoveKind::Travel, MoveKind::Overscan, MoveKind::Scan, MoveKind::Overscan]);
    assert_eq!((pv.moves[1].from, pv.moves[1].to), (pt(9.0, 10.0), pt(10.0, 10.0)), "run-up before the run");
    assert_eq!((pv.moves[3].from, pv.moves[3].to), (pt(30.0, 10.0), pt(31.0, 10.0)), "run-out after the run");
    assert_eq!((pv.scan_length_mm, pv.overscan_length_mm), (20.0, 2.0));
    assert!(pv.moves[1].source.is_none(), "overscan burns nothing");
}

#[test]
fn move_durations_add_up_to_the_estimate_and_follow_each_moves_own_speed() {
    let mut b = builder();
    let p = PowerRatio::new(0.5).unwrap();
    let cut = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 100.0,
            min_power: p,
            max_power: p,
            air_assist: false,
        })
        .unwrap();
    let dots = b
        .add_settings(OperationSettings {
            mode: PlanMode::Dots { dwell_ms: 250.0 },
            speed_mm_s: 100.0,
            min_power: p,
            max_power: p,
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: cut, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(40.0, 0.0) }).unwrap(); // 40 mm at the 400 mm/s travel speed
    b.push(PlanEvent::Cut { to: pt(40.0, 50.0), origin: Some(src()) }).unwrap(); // 50 mm at 100 mm/s
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: dots, pass: 0 }).unwrap();
    b.push(PlanEvent::Dot { origin: Some(src()) }).unwrap(); // one 250 ms pulse
    let plan = b.finish().unwrap();
    let pv = preview(&plan, &profile());

    let seconds: Vec<f64> = pv.moves.iter().map(|m| m.seconds).collect();
    let expected = [0.1, 0.5, 0.25];
    assert_eq!(seconds.len(), 3, "{:?}", pv.moves.iter().map(|m| m.kind).collect::<Vec<_>>());
    for (got, want) in seconds.iter().zip(expected) {
        assert!((got - want).abs() < 1e-12, "{seconds:?} vs {expected:?}");
    }
    assert!((seconds.iter().sum::<f64>() - pv.estimated_seconds).abs() < 1e-12, "the clock has one source");
}
