//! Feeder events in the plan (FE01, G5-cam): a feed moves the material, a pause waits; neither
//! moves the head, both are part of the plan's content and of its time estimate.

#![allow(clippy::unwrap_used)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, MachineProfile, OperationSettings, OriginCorner, PlanError,
    PlanEvent, PlanMode, PlanSource, PowerRatio, ProfileRevision, preview,
};
use burnburn_types::{ContentHash, GeometryId, OperationId, RevisionId, StableId};

fn pt(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
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
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

/// A 30 mm cut at 100 mm/s after a 10 mm travel at 400 mm/s, with `extra` events after the cut.
fn plan_with(extra: &[PlanEvent]) -> JobPlan {
    let mut b = builder();
    let p = PowerRatio::new(0.2).unwrap();
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 100.0,
            min_power: p,
            max_power: p,
            air_assist: true,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(10.0, 0.0) }).unwrap();
    let origin =
        Some(burnburn_job::SourceRef { geometry: GeometryId::from_raw(1), operation: OperationId::from_raw(2) });
    b.push(PlanEvent::Cut { to: pt(40.0, 0.0), origin }).unwrap();
    for e in extra {
        b.push(*e).unwrap();
    }
    b.finish().unwrap()
}

#[test]
fn feeds_and_pauses_are_checked_and_never_move_the_head() {
    let mut b = builder();
    for bad in [
        PlanEvent::Feed { mm: -1.0, speed_mm_s: 50.0 },
        PlanEvent::Feed { mm: f64::NAN, speed_mm_s: 50.0 },
        PlanEvent::Feed { mm: 10.0, speed_mm_s: 0.0 },
        PlanEvent::Pause { seconds: -0.5 },
        PlanEvent::Pause { seconds: f64::INFINITY },
    ] {
        assert!(matches!(b.push(bad), Err(PlanError::OutOfRange(_))), "{bad:?}");
    }
    let before = b.position();
    b.push(PlanEvent::Feed { mm: 120.0, speed_mm_s: 50.0 }).unwrap();
    b.push(PlanEvent::Pause { seconds: 3.0 }).unwrap();
    assert_eq!(b.position(), before, "the head stays where it is while the material moves");
}

#[test]
fn feeder_events_are_plan_content_and_not_burning_bounds() {
    let plain = plan_with(&[]);
    let fed = plan_with(&[PlanEvent::Feed { mm: 100.0, speed_mm_s: 50.0 }]);
    let paused = plan_with(&[PlanEvent::Pause { seconds: 5.0 }]);
    assert_ne!(plain.id(), fed.id());
    assert_ne!(plain.id(), paused.id());
    assert_ne!(fed.id(), paused.id());
    assert_eq!(plain.bounds(), fed.bounds());
    assert_eq!(plain.bounds(), paused.bounds());
    // The length matters, not only the kind.
    assert_ne!(fed.id(), plan_with(&[PlanEvent::Feed { mm: 100.5, speed_mm_s: 50.0 }]).id());
}

#[test]
fn the_estimate_counts_the_feed_at_its_own_speed_and_the_pause_in_full() {
    let base = preview(&plan_with(&[]), &profile());
    let fed = preview(
        &plan_with(&[
            PlanEvent::Feed { mm: 100.0, speed_mm_s: 50.0 },
            PlanEvent::Pause { seconds: 5.0 },
            PlanEvent::Feed { mm: 50.0, speed_mm_s: 25.0 },
        ]),
        &profile(),
    );
    // 100/50 + 5 + 50/25 = 9 s on top of 10/400 + 30/100.
    assert!((fed.estimated_seconds - base.estimated_seconds - 9.0).abs() < 1e-9, "{fed:?}");
    assert_eq!(fed.feed_length_mm, 150.0);
    assert_eq!(fed.pause_seconds, 5.0);
    assert_eq!(fed.moves.len(), base.moves.len(), "the feeder draws no head move");
    assert_eq!((base.feed_length_mm, base.pause_seconds), (0.0, 0.0));
}
