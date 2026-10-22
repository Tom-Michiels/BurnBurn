//! G6-machine, C06: a locked start point or direction is respected by the cut-path optimization.
//! The planner still picks the best start and direction of every other shape.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use burnburn_cam::{StartPoint, compile};
use burnburn_document::process::{OptimizationSettings, PathLock, ProcessCommand};
use burnburn_document::{CommandPayload, DocumentSession, GeometryCommand};
use burnburn_geometry::{PathBuilder, Point};
use burnburn_job::{JobPlan, PlanEvent};
use burnburn_types::{CancellationToken, GeometryId};
use common::*;

const START: PathLock = PathLock { start: true, direction: false };
const DIRECTION: PathLock = PathLock { start: false, direction: true };

fn set_lock(s: &mut DocumentSession, geometries: Vec<GeometryId>, lock: PathLock) {
    run(s, CommandPayload::Process(ProcessCommand::SetPathLock { geometries, lock }));
}

fn plan_with_head_at(s: &DocumentSession, head: (f64, f64)) -> JobPlan {
    let mut o = options();
    o.start = StartPoint::Bed(Point::new(head.0, head.1));
    compile(&s.snapshot(), &profile(), &o, &CancellationToken::never()).unwrap()
}

/// The points the head travels to or cuts to, in order, with `(kind, x, y)`.
fn path(plan: &JobPlan) -> Vec<(&'static str, f64, f64)> {
    motion(plan.events())
}

/// A closed 100 mm square from (10, 10): its own start point is the bottom-left corner.
fn square() -> (DocumentSession, GeometryId) {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 10.0, 100.0, 100.0);
    add_op(&mut s, line_op("Snijden", 20.0, 60.0), vec![g]);
    (s, g)
}

/// An open line from (0, 0) to (50, 0), drawn left to right.
fn open_line() -> (DocumentSession, GeometryId) {
    let mut s = session();
    let line = PathBuilder::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(50.0, 0.0)).build().unwrap();
    let g = run(&mut s, CommandPayload::Edit(GeometryCommand::CreatePath { path: line, name: String::new() }))
        .changes
        .created[0];
    add_op(&mut s, line_op("Snijden", 20.0, 60.0), vec![g]);
    (s, g)
}

#[test]
fn without_a_lock_a_closed_shape_is_entered_at_the_corner_nearest_the_head() {
    let (s, _) = square();
    let plan = path(&plan_with_head_at(&s, (120.0, 120.0)));
    assert_eq!(plan[0], ("travel", 110.0, 110.0), "the nearest corner, not the drawn start");
}

#[test]
fn a_locked_start_keeps_the_drawn_start_point_of_a_closed_shape() {
    let (mut s, g) = square();
    set_lock(&mut s, vec![g], START);
    let plan = path(&plan_with_head_at(&s, (120.0, 120.0)));
    assert_eq!(plan[0], ("travel", 10.0, 10.0), "entered where the shape was drawn to start");
    // Still a closed square, cut once, ending where it started.
    let cuts: Vec<_> = plan.iter().filter(|m| m.0 == "cut").collect();
    assert_eq!(cuts.len(), 4);
    assert_eq!((cuts[3].1, cuts[3].2), (10.0, 10.0));
}

#[test]
fn without_a_lock_an_open_shape_is_entered_at_the_end_nearest_the_head() {
    let (s, _) = open_line();
    let plan = path(&plan_with_head_at(&s, (60.0, 0.0)));
    assert_eq!(plan[0], ("travel", 50.0, 0.0));
    assert_eq!((plan[1].1, plan[1].2), (0.0, 0.0), "cut towards the other end");
}

#[test]
fn a_locked_direction_or_start_keeps_an_open_shape_running_the_way_it_was_drawn() {
    for lock in [DIRECTION, START, PathLock { start: true, direction: true }] {
        let (mut s, g) = open_line();
        set_lock(&mut s, vec![g], lock);
        let plan = path(&plan_with_head_at(&s, (60.0, 0.0)));
        assert_eq!(plan[0], ("travel", 0.0, 0.0), "{lock:?}: entered at the drawn start");
        assert_eq!((plan[1].1, plan[1].2), (50.0, 0.0), "{lock:?}: and cut in the drawn direction");
    }
}

#[test]
fn only_the_locked_shape_loses_its_freedom() {
    // Two squares side by side; the head is at the right of the right one.
    let mut s = session();
    let left = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let right = rectangle(&mut s, 30.0, 0.0, 10.0, 10.0);
    add_op(&mut s, line_op("Snijden", 20.0, 60.0), vec![left, right]);
    set_lock(&mut s, vec![left], START);
    let plan = path(&plan_with_head_at(&s, (50.0, 0.0)));
    let travels: Vec<_> = plan.iter().filter(|m| m.0 == "travel").map(|m| (m.1, m.2)).collect();
    assert_eq!(travels[0], (40.0, 0.0), "the free shape is entered at its nearest corner");
    assert_eq!(travels[1], (0.0, 0.0), "the locked one at its drawn start, although (10, 0) is nearer the head");
}

#[test]
fn a_lock_only_restricts_it_never_turns_the_optimization_on() {
    let (mut s, g) = square();
    let off = OptimizationSettings::default().with_best_start(false).with_best_direction(false);
    run(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: off }));
    let unlocked = path(&plan_with_head_at(&s, (120.0, 120.0)));
    set_lock(&mut s, vec![g], PathLock { start: true, direction: true });
    assert_eq!(path(&plan_with_head_at(&s, (120.0, 120.0))), unlocked);
}

#[test]
fn a_locked_shape_is_another_plan() {
    let (mut s, g) = square();
    let free = plan_with_head_at(&s, (120.0, 120.0));
    set_lock(&mut s, vec![g], START);
    assert_ne!(plan_with_head_at(&s, (120.0, 120.0)).id(), free.id());
    let _ = PlanEvent::Pause { seconds: 0.0 };
}
