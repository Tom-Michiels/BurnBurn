//! G7-werk, W09: a frame run for a red-dot pointer. The dot sits `offset` away from the beam; for the
//! dot to run along the job's outline the head travels the outline shifted by minus the offset.
//! Still laser-off travel only, and a shifted outline that would leave the bed is a typed error.
#![allow(clippy::unwrap_used)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanError, PlanEvent,
    PlanMode, PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::{FrameError, FrameOptions, FrameShape, frame_plan_aimed, frame_plan_shaped};
use burnburn_types::{ContentHash, HashDomain, OperationId, RevisionId, StableId};

fn p(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

/// A 40 x 20 mm rectangle cut at (10,20)-(50,40) on a 300 x 200 bed.
fn rectangle_plan() -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"frame-aimed"),
        machine_profile: ProfileRevision {
            id: "bench".into(),
            revision: 1,
            content: ContentHash::of(HashDomain::Profile, b"bench"),
        },
        compiler_version: "test".into(),
    };
    let mut b =
        JobPlanBuilder::new(source, MachineFrame::new(300.0, 200.0, OriginCorner::BottomLeft).unwrap(), p(0.0, 0.0));
    let cut = OperationSettings {
        mode: PlanMode::Line,
        speed_mm_s: 20.0,
        min_power: PowerRatio::new(0.5).unwrap(),
        max_power: PowerRatio::new(0.6).unwrap(),
        air_assist: true,
    };
    let c = b.add_settings(cut).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: c, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(10.0, 20.0) }).unwrap();
    for to in [p(50.0, 20.0), p(50.0, 40.0), p(10.0, 40.0), p(10.0, 20.0)] {
        b.push(PlanEvent::Cut { to, origin: None }).unwrap();
    }
    b.finish().unwrap()
}

const OPTIONS: FrameOptions = FrameOptions { speed_mm_s: 100.0 };

fn corners(plan: &JobPlan, shape: FrameShape, offset: Option<(f64, f64)>) -> Vec<(f64, f64)> {
    let frame = frame_plan_aimed(plan, &OPTIONS, shape, offset).unwrap();
    frame
        .plan()
        .events()
        .iter()
        .filter_map(|e| if let PlanEvent::Travel { to } = e { Some((to.x_mm(), to.y_mm())) } else { None })
        .collect()
}

#[test]
fn without_an_offset_the_aimed_frame_is_the_plain_frame() {
    let plan = rectangle_plan();
    let plain = frame_plan_shaped(&plan, &OPTIONS, FrameShape::BoundingRectangle).unwrap();
    let aimed = frame_plan_aimed(&plan, &OPTIONS, FrameShape::BoundingRectangle, None).unwrap();
    assert_eq!(aimed.plan().id(), plain.plan().id());
}

#[test]
fn the_head_travels_the_outline_shifted_by_minus_the_offset_so_the_dot_follows_the_job() {
    let plan = rectangle_plan();
    // The dot is 2 mm right of and 1 mm below the beam: the beam must stand 2 left and 1 above the outline.
    let got = corners(&plan, FrameShape::BoundingRectangle, Some((2.0, -1.0)));
    assert_eq!(got, vec![(8.0, 21.0), (48.0, 21.0), (48.0, 41.0), (8.0, 41.0), (8.0, 21.0)]);
    // Adding the offset back gives the job's own outline: where the dot is.
    let dot: Vec<(f64, f64)> = got.iter().map(|(x, y)| (x + 2.0, y - 1.0)).collect();
    assert_eq!(dot, vec![(10.0, 20.0), (50.0, 20.0), (50.0, 40.0), (10.0, 40.0), (10.0, 20.0)]);
}

#[test]
fn the_rubber_band_is_shifted_too_and_the_frame_stays_laser_off() {
    let plan = rectangle_plan();
    let plain = corners(&plan, FrameShape::RubberBand, None);
    let aimed = corners(&plan, FrameShape::RubberBand, Some((2.0, -1.0)));
    assert_eq!(plain.len(), aimed.len());
    for (a, b) in plain.iter().zip(&aimed) {
        assert_eq!((b.0, b.1), (a.0 - 2.0, a.1 + 1.0));
    }
    let frame = frame_plan_aimed(&plan, &OPTIONS, FrameShape::RubberBand, Some((2.0, -1.0))).unwrap();
    assert!(frame.plan().settings().iter().all(|s| (s.min_power.get(), s.max_power.get()) == (0.0, 0.0)));
    assert!(
        frame
            .plan()
            .events()
            .iter()
            .all(|e| matches!(e, PlanEvent::Travel { .. } | PlanEvent::OperationBoundary { .. }))
    );
}

#[test]
fn an_outline_the_head_cannot_reach_is_a_typed_error_not_a_clamped_frame() {
    let plan = rectangle_plan();
    // Shifting by minus 15 mm in x puts the beam at x = -5.
    let err = frame_plan_aimed(&plan, &OPTIONS, FrameShape::BoundingRectangle, Some((15.0, 0.0))).unwrap_err();
    assert!(matches!(err, FrameError::Plan(PlanError::OutOfRange(_))) || matches!(err, FrameError::Plan(_)), "{err:?}");
    let nan = frame_plan_aimed(&plan, &OPTIONS, FrameShape::BoundingRectangle, Some((f64::NAN, 0.0)));
    assert!(nan.is_err(), "a offset that is not a number is refused");
}
