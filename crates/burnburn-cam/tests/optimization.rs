//! W2-L3: optimization settings — the ordered `Order by` hierarchy, inner-first, start and
//! direction choice, travel reduction, backlash and overlap removal on the planned output.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::compile;
use burnburn_document::process::{CutPriority, OptimizationSettings, OrderKey, ProcessCommand};
use burnburn_document::{CommandPayload, DocumentSession, GeometryCommand};
use burnburn_geometry::{PathBuilder, Point};
use burnburn_job::{JobPlan, PlanEvent};
use burnburn_types::{CancellationToken, GeometryId};
use common::*;

fn set_priority(s: &mut DocumentSession, g: GeometryId, p: i32) {
    let priority = CutPriority::new(p).unwrap();
    run(s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority }));
}

fn set_optimization(s: &mut DocumentSession, o: OptimizationSettings) {
    run(s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: o }));
}

fn compiled(s: &DocumentSession) -> JobPlan {
    compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap()
}

/// The geometry of every burning event, consecutive repeats collapsed.
fn cut_order(plan: &JobPlan) -> Vec<GeometryId> {
    let mut out: Vec<GeometryId> = Vec::new();
    for e in plan.events() {
        let g = match *e {
            PlanEvent::Cut { origin: Some(o), .. } | PlanEvent::ScanRun { origin: Some(o), .. } => o.geometry,
            _ => continue,
        };
        if out.last() != Some(&g) {
            out.push(g);
        }
    }
    out
}

/// Two layers of two well-separated squares each; per layer the second square has priority 0 and
/// the first priority 1.
fn two_layers() -> (DocumentSession, [GeometryId; 4]) {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rectangle(&mut s, 100.0, 0.0, 10.0, 10.0);
    let c = rectangle(&mut s, 200.0, 0.0, 10.0, 10.0);
    let d = rectangle(&mut s, 300.0, 0.0, 10.0, 10.0);
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![a, b]);
    add_op(&mut s, line_op("L2", 20.0, 60.0), vec![c, d]);
    for (g, p) in [(a, 1), (b, 0), (c, 1), (d, 0)] {
        set_priority(&mut s, g, p);
    }
    (s, [a, b, c, d])
}

#[test]
fn layer_then_priority_is_a_different_plan_than_priority_then_layer() {
    let (mut s, [a, b, c, d]) = two_layers();

    let layer_first = OptimizationSettings::default().with_order(vec![OrderKey::Layer, OrderKey::Priority]).unwrap();
    set_optimization(&mut s, layer_first);
    let by_layer = compiled(&s);
    assert_eq!(cut_order(&by_layer), vec![b, a, d, c], "every layer completely, low priority first inside it");

    let priority_first = OptimizationSettings::default().with_order(vec![OrderKey::Priority, OrderKey::Layer]).unwrap();
    set_optimization(&mut s, priority_first);
    let by_priority = compiled(&s);
    assert_eq!(cut_order(&by_priority), vec![b, d, a, c], "priority 0 of every layer before any priority 1");

    assert_ne!(by_layer.id(), by_priority.id());
    // The settings blocks follow the layers: the speed of L1 (10) and L2 (20) alternate.
    let speeds: Vec<f64> = by_priority
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::OperationBoundary { settings, .. } => Some(by_priority.settings()[settings.get()].speed_mm_s),
            _ => None,
        })
        .collect();
    assert_eq!(speeds, vec![10.0, 20.0, 10.0, 20.0]);
}

#[test]
fn default_settings_keep_the_layer_order_and_ignore_priorities() {
    let (s, [a, b, c, d]) = two_layers();
    // Without an Order by on priority, the nearest-neighbour planner decides inside a layer.
    assert_eq!(cut_order(&compiled(&s)), vec![a, b, c, d]);
}

#[test]
fn levels_that_are_not_listed_only_break_ties() {
    let (mut s, [a, b, c, d]) = two_layers();
    // Priority alone: priority 0 of both layers first; the layer breaks the tie.
    set_optimization(&mut s, OptimizationSettings::default().with_order(vec![OrderKey::Priority]).unwrap());
    assert_eq!(cut_order(&compiled(&s)), vec![b, d, a, c]);
}

#[test]
fn order_by_group_cuts_ungrouped_shapes_first_then_one_group_at_a_time() {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rectangle(&mut s, 100.0, 0.0, 10.0, 10.0);
    let c = rectangle(&mut s, 200.0, 0.0, 10.0, 10.0);
    let d = rectangle(&mut s, 300.0, 0.0, 10.0, 10.0);
    run(&mut s, CommandPayload::Edit(GeometryCommand::Group { targets: vec![a, b] }));
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![a, b, c, d]);
    // Nearest neighbour only: A B C D.
    assert_eq!(cut_order(&compiled(&s)), vec![a, b, c, d]);
    set_optimization(&mut s, OptimizationSettings::default().with_order(vec![OrderKey::Group]).unwrap());
    // Ungrouped C, D first (nearest neighbour from the origin), then the group: B is nearer to
    // D's entry (300, 0) than A is.
    assert_eq!(cut_order(&compiled(&s)), vec![c, d, b, a]);
}

#[test]
fn inner_shapes_are_cut_first_unless_inner_first_is_off() {
    let mut s = session();
    let outer = rectangle(&mut s, 0.0, 0.0, 100.0, 100.0);
    let inner = rectangle(&mut s, 40.0, 40.0, 20.0, 20.0);
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![outer, inner]);
    assert_eq!(cut_order(&compiled(&s)), vec![inner, outer]);
    set_optimization(&mut s, OptimizationSettings::default().with_inner_first(false));
    // The head starts on the outer corner, so the outer contour is the nearest.
    assert_eq!(cut_order(&compiled(&s)), vec![outer, inner]);
}

#[test]
fn travel_reduction_off_keeps_the_given_order() {
    let mut s = session();
    let far = rectangle(&mut s, 200.0, 0.0, 10.0, 10.0);
    let origin = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let middle = rectangle(&mut s, 100.0, 0.0, 10.0, 10.0);
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![far, origin, middle]);
    assert_eq!(cut_order(&compiled(&s)), vec![origin, middle, far]);
    set_optimization(&mut s, OptimizationSettings::default().with_reduce_travel(false));
    assert_eq!(cut_order(&compiled(&s)), vec![far, origin, middle]);
}

fn path_geometry(s: &mut DocumentSession, path: burnburn_geometry::Path) -> GeometryId {
    run(s, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: String::new() })).changes.created[0]
}

#[test]
fn the_shapes_own_start_point_is_used_when_the_best_start_is_off() {
    let mut s = session();
    // Drawn from (20, 20); the vertex nearest to the head at (−1, −1) is (10, 10).
    let g = path_geometry(&mut s, path_of(&[(20.0, 20.0), (10.0, 20.0), (10.0, 10.0), (20.0, 10.0)]));
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![g]);
    let first = |s: &DocumentSession| motion(plan_from_outside(s, &profile()).unwrap().events())[0];
    assert!(near((first(&s).1, first(&s).2), (10.0, 10.0)), "{:?}", first(&s));
    set_optimization(&mut s, OptimizationSettings::default().with_best_start(false));
    assert!(near((first(&s).1, first(&s).2), (20.0, 20.0)), "{:?}", first(&s));
}

#[test]
fn an_open_path_keeps_its_drawn_direction_when_the_best_direction_is_off() {
    let mut s = session();
    let open = PathBuilder::new().move_to(Point::new(100.0, 0.0)).line_to(Point::new(0.0, 0.0)).build().unwrap();
    let g = path_geometry(&mut s, open);
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![g]);
    let first = |s: &DocumentSession| motion(plan_from_outside(s, &profile()).unwrap().events())[0];
    assert!(near((first(&s).1, first(&s).2), (0.0, 0.0)), "starts at the near end: {:?}", first(&s));
    set_optimization(&mut s, OptimizationSettings::default().with_best_direction(false));
    assert!(near((first(&s).1, first(&s).2), (100.0, 0.0)), "drawn direction: {:?}", first(&s));
}

#[test]
fn hiding_backlash_prefers_the_approach_that_does_not_reverse_the_head() {
    let mut s = session();
    // From the head at the origin, Q's nearest vertex (0, 10) is entered by moving +Y but the
    // contour then leaves towards (20, 5), i.e. against that move; P's nearest vertex (10, 0)
    // is entered along +X and cut on along +X. Both are 10 mm away; Q is given first.
    let q = path_geometry(&mut s, path_of(&[(0.0, 10.0), (20.0, 5.0), (20.0, 25.0), (0.0, 25.0)]));
    let p = path_geometry(&mut s, path_of(&[(10.0, 0.0), (30.0, 0.0), (30.0, 10.0), (10.0, 10.0)]));
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![q, p]);
    assert_eq!(cut_order(&compiled(&s)), vec![q, p], "ties keep the given order");
    set_optimization(&mut s, OptimizationSettings::default().with_hide_backlash(true));
    assert_eq!(cut_order(&compiled(&s)), vec![p, q]);
}

/// Two squares of 10 mm side next to each other; B's left edge runs `gap` mm right of A's right
/// edge. The plan's cut length shows which edge was cut.
fn neighbours(gap: f64, passes: u32, same_layer: bool) -> f64 {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rectangle(&mut s, 10.0 + gap, 0.0, 10.0, 10.0);
    let mut draft = line_op("L1", 10.0, 50.0);
    draft.settings.passes = burnburn_document::process::Passes::new(passes).unwrap();
    if same_layer {
        add_op(&mut s, draft, vec![a, b]);
    } else {
        add_op(&mut s, draft.clone(), vec![a]);
        add_op(&mut s, draft, vec![b]);
    }
    set_optimization(&mut s, OptimizationSettings::default().with_remove_overlapping(Some(mm(0.01))).unwrap());
    cut_length(&compiled(&s))
}

#[test]
fn overlapping_edges_within_the_tolerance_are_cut_once() {
    // A: 40, B: 40 minus its left edge (10) which lies 0.005 mm from A's right edge.
    assert!((neighbours(0.005, 1, true) - 70.0).abs() < 1e-9);
}

#[test]
fn edges_further_apart_than_the_tolerance_are_both_cut() {
    assert!((neighbours(0.02, 1, true) - 80.0).abs() < 1e-9);
}

#[test]
fn every_pass_cuts_the_same_line_again() {
    assert!((neighbours(0.005, 2, true) - 140.0).abs() < 1e-9);
}

#[test]
fn lines_of_different_layers_are_not_merged() {
    assert!((neighbours(0.005, 1, false) - 80.0).abs() < 1e-9);
}

#[test]
fn a_partial_overlap_removes_only_the_covered_stretch() {
    let mut s = session();
    let one = PathBuilder::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(20.0, 0.0)).build().unwrap();
    // 10 mm of this line run 0.005 mm beside the first one, in the opposite direction.
    let two = PathBuilder::new().move_to(Point::new(30.0, 0.005)).line_to(Point::new(10.0, 0.005)).build().unwrap();
    let (a, b) = (path_geometry(&mut s, one), path_geometry(&mut s, two));
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![a, b]);
    set_optimization(&mut s, OptimizationSettings::default().with_remove_overlapping(Some(mm(0.01))).unwrap());
    let plan = compiled(&s);
    // 20 (first) + the part of the second beyond x = 20 (10) minus the sliver of the tolerance
    // stadium that still covers it past the end of the first line.
    let sliver = (0.01f64.powi(2) - 0.005f64.powi(2)).sqrt();
    assert!((cut_length(&plan) - (30.0 - sliver)).abs() < 1e-6, "{}", cut_length(&plan));
    // The source geometry is untouched.
    assert!(s.snapshot().object(b).is_some());
}

#[test]
fn overlap_tolerance_must_be_positive_and_bounded() {
    let o = OptimizationSettings::default();
    assert!(o.clone().with_remove_overlapping(Some(mm(0.0))).is_err());
    assert!(o.clone().with_remove_overlapping(Some(mm(5.1))).is_err());
    assert!(o.with_remove_overlapping(Some(mm(5.0))).is_ok());
}
#[test]
fn joints_of_one_contour_and_a_shape_over_itself_are_never_removed() {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    add_op(&mut s, line_op("L1", 10.0, 50.0), vec![a]);
    set_optimization(&mut s, OptimizationSettings::default().with_remove_overlapping(Some(mm(0.5))).unwrap());
    assert!((cut_length(&compiled(&s)) - 40.0).abs() < 1e-9);
}
