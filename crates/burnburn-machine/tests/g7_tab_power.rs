//! A plan with tab cut power (C04, G7-cam) goes through placement and framing like any other: the
//! lowered power moves with the job, and the frame follows every place the head reaches.
#![allow(clippy::unwrap_used)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::{
    FrameOptions, FrameShape, JobAnchor, PlacementInput, PlacementSpec, Rect, StartFrom, frame_plan_shaped, place_plan,
};
use burnburn_types::{ContentHash, HashDomain, OperationId, RevisionId, StableId};

fn p(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn frame() -> MachineFrame {
    MachineFrame::new(300.0, 200.0, OriginCorner::BottomLeft).unwrap()
}

/// A triangle (10,10) - (50,10) - (30,40) whose last side is a cut at 12.5 % power.
fn plan() -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"tab-power-test"),
        machine_profile: ProfileRevision {
            id: "bench".into(),
            revision: 1,
            content: ContentHash::of(HashDomain::Profile, b"bench"),
        },
        compiler_version: "test".into(),
    };
    let mut b = JobPlanBuilder::new(source, frame(), p(0.0, 0.0));
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 20.0,
            min_power: PowerRatio::new(0.5).unwrap(),
            max_power: PowerRatio::new(0.5).unwrap(),
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(10.0, 10.0) }).unwrap();
    b.push(PlanEvent::Cut { to: p(50.0, 10.0), origin: None }).unwrap();
    b.push(PlanEvent::CutPower { to: p(30.0, 40.0), power: PowerRatio::new(0.125).unwrap(), origin: None }).unwrap();
    b.push(PlanEvent::Cut { to: p(10.0, 10.0), origin: None }).unwrap();
    b.finish().unwrap()
}

#[test]
fn placing_a_job_moves_a_lowered_cut_with_it_and_keeps_its_power() {
    let f = frame();
    let spec = PlacementSpec { start_from: StartFrom::UserOrigin, anchor: JobAnchor::BottomLeft };
    let input = PlacementInput { frame: &f, current_position: None, user_origin: Some(p(100.0, 100.0)) };
    let placed = place_plan(&plan(), &spec, Rect::new(10.0, 10.0, 50.0, 40.0).unwrap(), &input).unwrap();
    assert_eq!(placed.delta_mm, (90.0, 90.0));
    let moved: Vec<_> = placed
        .plan
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::CutPower { to, power, .. } => Some((to.x_mm(), to.y_mm(), power.get())),
            _ => None,
        })
        .collect();
    assert_eq!(moved, vec![(120.0, 130.0, 0.125)]);
}

#[test]
fn the_rubber_band_includes_the_apex_that_only_a_lowered_cut_reaches() {
    let band = frame_plan_shaped(&plan(), &FrameOptions { speed_mm_s: 100.0 }, FrameShape::RubberBand).unwrap();
    let travels: Vec<(f64, f64)> = band
        .plan()
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::Travel { to } => Some((to.x_mm(), to.y_mm())),
            _ => None,
        })
        .collect();
    assert_eq!(travels, vec![(10.0, 10.0), (50.0, 10.0), (30.0, 40.0), (10.0, 10.0)]);
}
