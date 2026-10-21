//! X04 / W4-prestaties: the whole interaction route on named, deterministic 100k-segment fixtures.
//! Ignored benchmarks that print p50/p95/max wall-clock times and check correctness invariants;
//! they never assert a time limit (that is a release-build measurement on one machine).
//! Provenance of the fixtures and the measured numbers: docs/dev-notes/W4-prestaties.md.
//!
//! `cargo test -p burnburn-scenarios --release --test benchmarks_100k -- --ignored --nocapture --test-threads=1`
//! `BB_FIXTURE=F100K-B` (name prefix) limits the run to one fixture.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::f64::consts::PI;
use std::time::{Duration, Instant};

use burnburn_document::{CommandPayload, DocumentSession, GeometryCommand, ImportItem};
use burnburn_editor::{
    Camera, CanvasInput, Editor, PointerButton, PointerEvent, PointerPhase, SceneRequest, Selection, SelectionChange,
    SnapSettings, ToolContext, ToolKind, ToolStep, Viewport, build_scene, hit_test, objects_inside,
};
use burnburn_geometry::{Affine, Bounds, Path, Point, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, save_project};
use burnburn_types::{ContentHash, GeometryId, MonotonicTime};
use common::*;

/// A named, generated fixture: `objects` shapes of `sides` segments each on a grid. Every shape is
/// a closed `sides`-gon of radius `radius`, or one horizontal line for `sides == 1`.
pub struct Fixture100k {
    pub name: &'static str,
    pub objects: usize,
    pub sides: usize,
    pub spacing: f64,
    pub radius: f64,
}

/// F100K-A: 1 000 closed 100-gons (as the compile benchmark); F100K-B: 10 000 closed 10-gons;
/// F100K-C: 100 000 single lines (hatching-like). Each holds exactly 100 000 segments.
pub const FIXTURES_100K: [Fixture100k; 3] = [
    Fixture100k { name: "F100K-A 1000x100-gon", objects: 1_000, sides: 100, spacing: 30.0, radius: 12.0 },
    Fixture100k { name: "F100K-B 10000x10-gon", objects: 10_000, sides: 10, spacing: 9.0, radius: 3.5 },
    Fixture100k { name: "F100K-C 100000x1-line", objects: 100_000, sides: 1, spacing: 3.0, radius: 1.0 },
];

/// The (tiny) imported-file asset every fixture object refers to, and its content hash.
fn fixture_asset() -> (burnburn_io::AssetStore, ContentHash) {
    let mut store = burnburn_io::AssetStore::default();
    let hash = store.add("image/svg+xml", b"<svg/> F100K");
    (store, hash)
}

impl Fixture100k {
    fn cols(&self) -> usize {
        ((self.objects as f64 * 1.6).sqrt().round() as usize).max(1)
    }

    pub fn centre(&self, i: usize) -> Point {
        let cols = self.cols();
        Point::new(10.0 + self.spacing * (i % cols) as f64, 10.0 + self.spacing * (i / cols) as f64)
    }

    /// A point on the outline of object `i` (right-most vertex / right line end).
    pub fn outline_point(&self, i: usize) -> Point {
        let c = self.centre(i);
        Point::new(c.x + self.radius, c.y)
    }

    pub fn path(&self, i: usize) -> Path {
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

    pub fn session(&self) -> (DocumentSession, Vec<GeometryId>) {
        let mut s = new_session();
        let items: Vec<ImportItem> =
            (0..self.objects).map(|i| ImportItem { name: format!("o{i}"), entity: None, path: self.path(i) }).collect();
        let asset = fixture_asset().1;
        let ids = apply(&mut s, CommandPayload::Edit(GeometryCommand::Import { asset, items }), &[]).changes.created;
        assert_eq!(ids.len(), self.objects);
        (s, ids)
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    println!("bench {label}: {:.1} ms", ms(t));
    out
}

/// Runs `f` `runs` times and prints p50/p95/max in ms.
fn percentiles<T>(label: &str, runs: usize, mut f: impl FnMut(usize) -> T) {
    let mut v: Vec<f64> = (0..runs)
        .map(|k| {
            let t = Instant::now();
            std::hint::black_box(f(k));
            ms(t)
        })
        .collect();
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[(((v.len() - 1) as f64) * q).ceil() as usize];
    println!("bench {label}: runs {runs} p50 {:.3} ms p95 {:.3} ms max {:.3} ms", at(0.5), at(0.95), v[v.len() - 1]);
}

/// Deterministic pseudo-random sequence (LCG); no rand dependency.
fn lcg(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (*seed >> 11) as f64 / (1u64 << 53) as f64
}

fn pointer(phase: PointerPhase, p: Point) -> CanvasInput {
    let mut e = PointerEvent::mouse(phase, p, MonotonicTime::from_offset(Duration::from_millis(1)));
    if phase == PointerPhase::Move {
        e.button = Some(PointerButton::Primary);
    }
    CanvasInput::Pointer(e)
}

fn scene_request<'a>(
    document: burnburn_types::DocumentId,
    snapshot: &'a burnburn_document::DocumentSnapshot,
    selection: &'a Selection,
    camera: Camera,
    viewport: Viewport,
) -> SceneRequest<'a> {
    SceneRequest {
        document,
        snapshot,
        selection,
        tool: ToolKind::Select,
        preview: None,
        snap: None,
        issues: &[],
        extra: &[],
        result_preview: None,
        selected_nodes: &[],
        camera,
        viewport,
    }
}

#[test]
#[ignore = "benchmark; run in release with --ignored --nocapture"]
fn bench_interaction_100k() {
    let which = std::env::var("BB_FIXTURE").ok();
    for fx in FIXTURES_100K.iter().filter(|f| which.as_deref().is_none_or(|w| f.name.starts_with(w))) {
        println!("== {} ==", fx.name);
        let (mut s, ids) = time("fixture import", || fx.session());
        let snap = s.snapshot();
        let mut seed = 7u64;
        let snap_off = SnapSettings { objects: false, ..SnapSettings::default() };
        let snap_on = SnapSettings::default();
        let empty = Selection::default();

        // 1. click selection through the select tool (press + release on an object outline).
        let targets: Vec<Point> =
            (0..200).map(|_| fx.outline_point((lcg(&mut seed) * fx.objects as f64) as usize % fx.objects)).collect();
        percentiles("click select (press via SelectTool)", 200, |k| {
            let mut editor = Editor::new();
            let tc = ToolContext { document: &snap, selection: &empty, mm_per_point: 1.0, snap: &snap_off };
            let down = editor.input(&pointer(PointerPhase::Down, targets[k]), &tc);
            let up = editor.input(&pointer(PointerPhase::Up, targets[k]), &tc);
            assert!(matches!(down, ToolStep::Select(SelectionChange::Replace(ref v)) if !v.is_empty()), "{down:?}");
            up
        });
        percentiles("click on empty space (miss)", 100, |_| hit_test(&snap, Point::new(-500.0, -500.0), 4.0));

        // 2. band selection: a window with about a tenth of the objects, and the whole document.
        let all_b = Bounds::from_corners(Point::new(-1.0, -1.0), Point::new(5000.0, 5000.0));
        let tenth = Bounds::from_corners(Point::new(-1.0, -1.0), Point::new(380.0, 240.0));
        println!("   band window holds {} objects", objects_inside(&snap, tenth).len());
        percentiles("band select ~10% window (objects_inside)", 50, |_| objects_inside(&snap, tenth));
        percentiles("band select whole document", 20, |_| objects_inside(&snap, all_b).len());

        // 3. select all, the way presentation does it, plus the selection update.
        let mut sel = Selection::default();
        percentiles("select all (draw_order + selectable + Replace)", 20, |_| {
            let all: Vec<_> = snap.draw_order().into_iter().filter(|id| snap.is_selectable(*id)).collect();
            sel.apply(&SelectionChange::Replace(all));
            sel.len()
        });
        assert_eq!(sel.len(), fx.objects);

        // 4. drag preview (move): press on a selected object, then moves.
        for (label, snap_settings) in [("snap off", &snap_off), ("snap on", &snap_on)] {
            let mut editor = Editor::new();
            let tc = ToolContext { document: &snap, selection: &sel, mm_per_point: 1.0, snap: snap_settings };
            let p0 = targets[0];
            editor.input(&pointer(PointerPhase::Down, p0), &tc);
            percentiles(&format!("drag move of ALL objects, one pointer move ({label})"), 20, |k| {
                let step = editor.input(&pointer(PointerPhase::Move, Point::new(p0.x + 5.0 + k as f64, p0.y + 3.0)), &tc);
                let _ = editor.preview();
                step
            });
            editor.cancel();
        }
        {
            let mut one = Selection::default();
            one.apply(&SelectionChange::Replace(vec![ids[0]]));
            let mut editor = Editor::new();
            let tc = ToolContext { document: &snap, selection: &one, mm_per_point: 1.0, snap: &snap_on };
            let p0 = fx.outline_point(0);
            editor.input(&pointer(PointerPhase::Down, p0), &tc);
            percentiles("drag move of ONE object, one pointer move (snap on)", 30, |k| {
                editor.input(&pointer(PointerPhase::Move, Point::new(p0.x + 5.0 + k as f64, p0.y + 3.0)), &tc)
            });
            editor.cancel();
        }

        // 5. commit + undo + redo of a move.
        let m = Affine::translate(Vector::new(5.0, 3.0)).unwrap();
        time("commit move of all", || apply(&mut s, CommandPayload::Transform { targets: ids.clone(), transform: m }, &[]));
        let rev = s.revision();
        time("undo of move-all", || s.undo(rev).unwrap());
        let rev = s.revision();
        time("redo of move-all", || s.redo(rev).unwrap());
        let rev = s.revision();
        s.undo(rev).unwrap();
        time("commit move of one", || apply(&mut s, CommandPayload::Transform { targets: vec![ids[0]], transform: m }, &[]));
        let rev = s.revision();
        time("undo of move-one", || s.undo(rev).unwrap());

        // 6. scene build for the renderer (editor side; the render side is in burnburn-render's perf test).
        let snap = s.snapshot();
        let camera = Camera::new(Point::new(600.0, 375.0), 1.0).unwrap();
        let viewport = Viewport::new(1300.0, 800.0, 2.0).unwrap();
        let (doc_id, snap_ref) = (s.id(), &*snap);
        percentiles("build_scene (nothing selected)", 20, |_| build_scene(&scene_request(doc_id, snap_ref, &empty, camera, viewport)).items.len());
        percentiles("build_scene (all selected)", 20, |_| build_scene(&scene_request(doc_id, snap_ref, &sel, camera, viewport)).items.len());

        // 7. compile and preflight.
        add_cut(&mut s, ids.clone());
        let snap2 = s.snapshot();
        let plan = time("compile to JobPlan", || compile_plan(&snap2));
        let _ = time("preflight", || burnburn_cam::preflight(&snap2, &plan, &profile()));

        // 8. save and open (in-memory file system: serialisation cost, not disk).
        let fs = MemoryFileSystem::new();
        let path = std::path::Path::new("/bench/project.burnburn");
        let mut project = Project::new(ProjectUuid::from_bytes([9; 16]), (*snap2).clone());
        project.assets = fixture_asset().0;
        time("save project (memory fs)", || save_project(&fs, path, &project).unwrap());
        let loaded = time("open project (memory fs)", || load_project(&fs, path).unwrap());
        assert_eq!(loaded.document.len(), fx.objects);
    }
}
