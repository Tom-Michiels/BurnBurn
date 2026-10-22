//! Power per movement (Tab Cut Power, C04, G7-cam): a cut can carry its own power. It is a Line
//! mode event, moves the head like a cut, is part of the plan's content, and the preview shows
//! the power of the segment.

#![allow(clippy::unwrap_used)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, MachineProfile, MoveKind, OperationSettings, OriginCorner,
    PlanError, PlanEvent, PlanMode, PlanSource, PowerRatio, ProfileRevision, ScanAxis, preview,
};
use burnburn_types::{ContentHash, GeometryId, OperationId, RevisionId, StableId};

fn pt(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn ratio(r: f64) -> PowerRatio {
    PowerRatio::new(r).unwrap()
}

fn builder() -> JobPlanBuilder {
    let source = PlanSource {
        document_revision: RevisionId::from_raw(1),
        source_hash: ContentHash::from_bytes([7; 32]),
        machine_profile: ProfileRevision { id: "bed".into(), revision: 1, content: ContentHash::from_bytes([3; 32]) },
        compiler_version: "test".into(),
    };
    JobPlanBuilder::new(source, MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(), pt(0.0, 0.0))
}

fn profile() -> MachineProfile {
    MachineProfile::new(
        "bed",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        ratio(0.0),
        ratio(1.0),
    )
    .unwrap()
}

fn origin() -> Option<burnburn_job::SourceRef> {
    Some(burnburn_job::SourceRef { geometry: GeometryId::from_raw(1), operation: OperationId::from_raw(2) })
}

fn settings(mode: PlanMode) -> OperationSettings {
    OperationSettings { mode, speed_mm_s: 100.0, min_power: ratio(0.5), max_power: ratio(0.5), air_assist: false }
}

/// 30 mm cut, then `middle`, then a 10 mm cut: all at 100 mm/s.
fn plan_with(middle: PlanEvent) -> JobPlan {
    let mut b = builder();
    let s = b.add_settings(settings(PlanMode::Line)).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(10.0, 0.0) }).unwrap();
    b.push(PlanEvent::Cut { to: pt(40.0, 0.0), origin: origin() }).unwrap();
    b.push(middle).unwrap();
    b.push(PlanEvent::Cut { to: pt(60.0, 5.0), origin: origin() }).unwrap();
    b.finish().unwrap()
}

#[test]
fn a_cut_at_power_moves_the_head_like_a_cut_but_only_in_line_mode() {
    let mut b = builder();
    let s = b.add_settings(settings(PlanMode::Line)).unwrap();
    assert_eq!(
        b.push(PlanEvent::CutPower { to: pt(1.0, 0.0), power: ratio(0.1), origin: None }),
        Err(PlanError::MotionOutsideOperation)
    );
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::CutPower { to: pt(3.0, 4.0), power: ratio(0.1), origin: None }).unwrap();
    assert_eq!(b.position(), pt(3.0, 4.0));

    let mut scan = builder();
    let s = scan
        .add_settings(settings(PlanMode::Scan { interval_mm: 0.1, axis: ScanAxis::X, bidirectional: false }))
        .unwrap();
    scan.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    assert_eq!(
        scan.push(PlanEvent::CutPower { to: pt(1.0, 0.0), power: ratio(0.1), origin: None }),
        Err(PlanError::EventDoesNotMatchMode)
    );
}

#[test]
fn the_power_of_a_movement_is_plan_content_and_its_ends_are_in_the_bounds() {
    let tab = |p: f64| plan_with(PlanEvent::CutPower { to: pt(45.0, 0.0), power: ratio(p), origin: origin() });
    let plain = plan_with(PlanEvent::Cut { to: pt(45.0, 0.0), origin: origin() });
    assert_ne!(tab(0.1).id(), plain.id(), "the same path at another power is another plan");
    assert_ne!(tab(0.1).id(), tab(0.2).id(), "so is the value");
    assert_eq!(tab(0.1).id(), tab(0.1).id());
    assert_eq!(tab(0.1).bounds(), plain.bounds());

    // A cut-at-power that extends the reach is counted like a cut.
    let far = plan_with(PlanEvent::CutPower { to: pt(80.0, -7.0), power: ratio(0.1), origin: origin() });
    let b = far.bounds().unwrap();
    assert_eq!((b.max().x, b.min().y), (80.0, -7.0));
}

#[test]
fn the_preview_marks_the_power_of_a_segment_and_counts_it_as_cutting() {
    let pv = preview(&plan_with(PlanEvent::CutPower { to: pt(45.0, 0.0), power: ratio(0.1), origin: origin() }), &profile());
    let cuts: Vec<_> = pv.moves.iter().filter(|m| m.kind == MoveKind::Cut).collect();
    assert_eq!(cuts.len(), 3);
    assert_eq!(cuts.iter().map(|m| m.power).collect::<Vec<_>>(), vec![None, Some(ratio(0.1)), None]);
    // 30 + 5 + sqrt(15^2 + 5^2) mm of cutting at 100 mm/s; the 10 mm travel at 400 mm/s.
    let last = (15.0_f64 * 15.0 + 5.0 * 5.0).sqrt();
    assert!((pv.cut_length_mm - (35.0 + last)).abs() < 1e-9, "{}", pv.cut_length_mm);
    assert!((cuts[1].seconds - 0.05).abs() < 1e-12, "5 mm at 100 mm/s");
    assert_eq!(cuts[1].source, origin(), "a click on the segment still leads back to its shape");
}
