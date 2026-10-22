//! X04 regression guard (G8-trace): the interaction route on the 100k-segment fixtures, measured in
//! deterministic work counts, never in time. A change that makes a click, select all, a scene build
//! or a compile scale worse than linear fails here on every machine and in debug builds; the
//! wall-clock benchmarks (`benchmarks_100k.rs`) stay `#[ignore]`d.
//!
//! The counts come from three places: the editor's per-thread work counters
//! (`burnburn_editor::editor_work`), the spatial index's own node-visit count
//! (`SpatialIndex::query_point_cost`) and the sizes of what the route produces (scene items, plan
//! events). The bounds are a small multiple of what the current code does, per object or per click,
//! so they hold for 100k objects as for 1k; a scan over all objects per click would be 100 000.
//! Provenance and numbers: docs/dev-notes/G8-trace.md. The planner's own cost counters live in
//! `burnburn-cam` (`line::tests::choosing_the_next_contour_examines_a_few_contours_not_all_of_them`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::cmp::Reverse;
use std::f64::consts::PI;

use burnburn_document::{CommandPayload, DocumentSession, DocumentSnapshot, GeometryCommand, ImportItem};
use burnburn_editor::{
    Camera, SceneRequest, Selection, SelectionChange, ToolKind, Viewport, build_scene, editor_work, hit_test,
    pick_index_builds, reset_editor_work,
};
use burnburn_geometry::{Path, Point, SpatialIndex};
use burnburn_types::GeometryId;
use common::*;

/// Names the fixture as the benchmark does: a grid with `round(sqrt(1,6 n))` columns.
struct Grid {
    objects: usize,
    sides: usize,
    spacing: f64,
    radius: f64,
}

/// F100K-C: 100 000 single lines, 3 mm apart (100 000 segments).
const LINES_100K: Grid = Grid { objects: 100_000, sides: 1, spacing: 3.0, radius: 1.0 };

impl Grid {
    fn centre(&self, i: usize) -> Point {
        let cols = ((self.objects as f64 * 1.6).sqrt().round() as usize).max(1);
        Point::new(10.0 + self.spacing * (i % cols) as f64, 10.0 + self.spacing * (i / cols) as f64)
    }

    fn outline_point(&self, i: usize) -> Point {
        let c = self.centre(i);
        Point::new(c.x + self.radius, c.y)
    }

    fn path(&self, i: usize) -> Path {
        let c = self.centre(i);
        if self.sides == 1 {
            return Path::line(Point::new(c.x - self.radius, c.y), Point::new(c.x + self.radius, c.y)).unwrap();
        }
        let pts: Vec<Point> = (0..self.sides)
            .map(|k| 2.0 * PI * k as f64 / self.sides as f64)
            .map(|a| Point::new(c.x + self.radius * a.cos(), c.y + self.radius * a.sin()))
            .collect();
        Path::polyline(&pts, true).unwrap()
    }

    fn session(&self) -> (DocumentSession, Vec<GeometryId>) {
        let mut s = new_session();
        let mut assets = burnburn_io::AssetStore::default();
        let asset = assets.add("image/svg+xml", b"<svg/> perf guard");
        let items: Vec<ImportItem> =
            (0..self.objects).map(|i| ImportItem { name: format!("o{i}"), entity: None, path: self.path(i) }).collect();
        let ids = apply(&mut s, CommandPayload::Edit(GeometryCommand::Import { asset, items }), &[]).changes.created;
        assert_eq!(ids.len(), self.objects);
        (s, ids)
    }
}

/// Deterministic pseudo-random sequence (LCG); no rand dependency.
fn lcg(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (*seed >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
fn perf_guard_a_click_uses_one_index_and_looks_at_a_handful_of_outlines() {
    let (s, _) = LINES_100K.session();
    let snap = s.snapshot();
    let n = LINES_100K.objects;

    // The index itself: how many nodes and boxes a point query examines on this very layout.
    let order = snap.draw_order();
    let index = SpatialIndex::build(
        order.iter().enumerate().filter_map(|(rank, id)| snap.object(*id)?.bounds().map(|b| ((Reverse(rank), *id), b))),
    );
    let mut seed = 7u64;
    let targets: Vec<Point> =
        (0..200).map(|_| LINES_100K.outline_point((lcg(&mut seed) * n as f64) as usize % n)).collect();
    let worst = targets.iter().map(|t| index.query_point_cost(*t, 4.0).1).max().unwrap();
    assert!(worst < 400, "one point query examined {worst} nodes and boxes of {n} objects");

    // The editor's hit test: one index for the whole run, a few candidates per click.
    reset_editor_work();
    let builds = pick_index_builds();
    for t in &targets {
        assert!(!hit_test(&snap, *t, 4.0).is_empty(), "a click on an outline must hit");
    }
    assert_eq!(pick_index_builds() - builds, 1, "200 clicks on one document state share one index");
    let per_click = editor_work().pick_candidates / 200;
    println!("guard click: worst index query {worst} nodes+boxes, {per_click} outlines per click");
    assert!((1..=64).contains(&per_click), "{per_click} outlines examined per click (of {n})");

    // A click on empty paper examines nothing.
    reset_editor_work();
    assert!(hit_test(&snap, Point::new(-500.0, -500.0), 4.0).is_empty());
    assert_eq!(editor_work().pick_candidates, 0);
}

/// The selectable objects in draw order, as presentation does for select all.
fn all_selectable(snap: &DocumentSnapshot) -> Vec<GeometryId> {
    snap.draw_order().into_iter().filter(|id| snap.is_selectable(*id)).collect()
}

#[test]
fn perf_guard_select_all_costs_a_few_steps_per_object() {
    let (s, _) = LINES_100K.session();
    let snap = s.snapshot();
    let n = LINES_100K.objects as u64;
    let mut sel = Selection::default();
    reset_editor_work();
    sel.apply(&SelectionChange::Replace(all_selectable(&snap)));
    assert_eq!(sel.len() as u64, n);
    let steps = editor_work().selection_steps;
    println!("guard select all: {steps} steps for {n} objects");
    assert!((n..=3 * n).contains(&steps), "{steps} selection steps for {n} objects");

    // Lookups in a full selection are constant time: 1 000 of them add about 1 000 steps.
    reset_editor_work();
    let mut seed = 3u64;
    let ids = sel.ids().to_vec();
    for _ in 0..1_000 {
        assert!(sel.contains(ids[(lcg(&mut seed) * n as f64) as usize % ids.len()]));
    }
    assert!(editor_work().selection_steps <= 2_000, "{} steps for 1000 lookups", editor_work().selection_steps);
}

#[test]
fn perf_guard_the_scene_of_a_fully_selected_document_is_linear() {
    let (s, _) = LINES_100K.session();
    let snap = s.snapshot();
    let n = LINES_100K.objects;
    let mut sel = Selection::default();
    sel.apply(&SelectionChange::Replace(all_selectable(&snap)));
    let camera = Camera::new(Point::new(600.0, 375.0), 1.0).unwrap();
    let viewport = Viewport::new(1300.0, 800.0, 2.0).unwrap();
    reset_editor_work();
    let scene = build_scene(&SceneRequest {
        document: s.id(),
        snapshot: &snap,
        selection: &sel,
        tool: ToolKind::Select,
        preview: None,
        snap: None,
        issues: &[],
        extra: &[],
        result_preview: None,
        selected_nodes: &[],
        camera,
        viewport,
    });
    assert_eq!(scene.items.len(), n, "one scene item per object");
    let w = editor_work();
    println!("guard scene: {} items, {} selection steps for {n} objects", w.scene_items, w.selection_steps);
    assert_eq!(w.scene_items as usize, n);
    assert!(w.selection_steps <= 3 * n as u64, "{} selection steps for a scene of {n} objects", w.selection_steps);
}

#[test]
fn perf_guard_the_plan_grows_linearly_with_the_number_of_contours() {
    // F100K-A shape (closed 100-gons, 30 mm apart) at 120 and 240 objects: doubling the contours
    // must double the plan, give or take the fixed events; a pairwise step would quadruple it.
    let events = |objects: usize| {
        let grid = Grid { objects, sides: 100, spacing: 30.0, radius: 12.0 };
        let (mut s, ids) = grid.session();
        add_cut(&mut s, ids);
        compile_plan(&s.snapshot()).events().len()
    };
    let (small, big) = (events(120), events(240));
    let ratio = big as f64 / small as f64;
    println!("guard plan: {small} -> {big} events (x{ratio:.3})");
    assert!((1.8..=2.2).contains(&ratio), "plan events {small} -> {big} (x{ratio:.2}) for twice the contours");
    assert!(small >= 120 * 100, "every segment of every contour is cut: {small} events");
}
