//! X04 / W4-prestaties: hit test and snapping reuse one spatial index per document state, and give
//! exactly the results of the straightforward implementations (the oracles below: a full scan over
//! all objects, rebuilt from scratch on every call).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers

use std::cmp::Reverse;

use burnburn_document::{
    ArrangeCommand, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, DocumentSnapshot, OrderChange,
};
use burnburn_editor::{SnapSettings, hit_test, pick_index_builds, snap_candidates};
use burnburn_geometry::{Affine, HitTestTolerance, Path, Point, Rect, SnapFilter, SpatialIndex, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn rect_cmd(x: f64, y: f64, w: f64, h: f64, name: String) -> CommandPayload {
    let rect = Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap();
    CommandPayload::CreateRectangle { rect, name }
}

/// Deterministic pseudo-random numbers in 0..1.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// 300 overlapping rectangles in a 200 × 200 mm area.
fn crowded() -> (DocumentSession, Vec<GeometryId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut r = Lcg(42);
    let mut ids = Vec::new();
    for i in 0..300 {
        let (x, y, w, h) = (r.next() * 190.0, r.next() * 190.0, 2.0 + r.next() * 25.0, 2.0 + r.next() * 25.0);
        ids.push(run(&mut s, rect_cmd(x, y, w, h, format!("r{i}"))).changes.created[0]);
    }
    (s, ids)
}

/// Oracle: the pre-X04 implementation, an index rebuilt from the whole document per call.
fn reference_hit_test(snapshot: &DocumentSnapshot, at: Point, tolerance_mm: f64) -> Vec<GeometryId> {
    let tol = HitTestTolerance::new(Length::from_mm(tolerance_mm).unwrap()).unwrap();
    let index = SpatialIndex::build(
        snapshot
            .draw_order()
            .iter()
            .enumerate()
            .filter(|(_, id)| snapshot.is_selectable(**id))
            .filter_map(|(rank, id)| snapshot.object(*id)?.bounds().map(|b| ((Reverse(rank), *id), b))),
    );
    burnburn_geometry::hit_test(&index, |(_, id)| snapshot.object(id).map(|o| o.path().as_ref()), at, &tol)
        .into_iter()
        .map(|c| c.key.1)
        .collect()
}

/// Oracle: the pre-X04 snap, every object not excluded handed to the domain.
fn reference_snap(
    snapshot: &DocumentSnapshot,
    settings: &SnapSettings,
    mm_per_point: f64,
    at: Point,
    exclude: &[GeometryId],
) -> Vec<(Point, GeometryId)> {
    let tol = HitTestTolerance::new(Length::from_mm(settings.radius_pt * mm_per_point).unwrap()).unwrap();
    let shapes: Vec<(GeometryId, &Path)> =
        snapshot.objects().filter(|(id, _)| !exclude.contains(id)).map(|(id, o)| (id, o.path().as_ref())).collect();
    burnburn_geometry::snap_candidates(&shapes, at, &tol, &SnapFilter::all())
        .into_iter()
        .map(|c| (c.point, c.source))
        .collect()
}

fn queries(n: usize) -> Vec<Point> {
    let mut r = Lcg(7);
    (0..n).map(|_| Point::new(r.next() * 220.0 - 10.0, r.next() * 220.0 - 10.0)).collect()
}

#[test]
fn hit_test_equals_the_reference_also_with_hidden_locked_and_reordered_objects() {
    let (mut s, ids) = crowded();
    run(&mut s, CommandPayload::Arrange(ArrangeCommand::SetHidden { targets: ids[10..40].to_vec(), hidden: true }));
    run(&mut s, CommandPayload::Arrange(ArrangeCommand::SetLocked { targets: ids[40..60].to_vec(), locked: true }));
    run(
        &mut s,
        CommandPayload::Arrange(ArrangeCommand::Order { targets: ids[100..110].to_vec(), change: OrderChange::ToBack }),
    );
    let snap = s.snapshot();
    let mut hits = 0;
    for at in queries(400) {
        for tolerance in [0.5, 3.0] {
            let got = hit_test(&snap, at, tolerance);
            assert_eq!(got, reference_hit_test(&snap, at, tolerance), "at {at:?} tolerance {tolerance}");
            hits += usize::from(!got.is_empty());
        }
    }
    assert!(hits > 200, "the fixture must actually produce hits ({hits})");
}

#[test]
fn the_index_is_built_once_per_document_state_and_shared_by_clones() {
    let (s, _) = crowded();
    let snap = s.snapshot();
    let before = pick_index_builds();
    for at in queries(50) {
        hit_test(&snap, at, 1.0);
    }
    hit_test(&s.snapshot(), Point::new(5.0, 5.0), 1.0);
    hit_test(&(*snap).clone(), Point::new(5.0, 5.0), 1.0);
    assert_eq!(pick_index_builds() - before, 1, "52 clicks on one state: one index");
}

#[test]
fn every_edit_gets_a_fresh_index_so_a_stale_one_is_never_used() {
    let (mut s, ids) = crowded();
    // A rectangle far away from the crowd, then move it: the old place must no longer hit.
    let lone = run(&mut s, rect_cmd(500.0, 500.0, 10.0, 10.0, "lone".into())).changes.created[0];
    let probe_old = Point::new(505.0, 505.0);
    assert_eq!(hit_test(&s.snapshot(), probe_old, 1.0), vec![lone]);
    let builds = pick_index_builds();

    let m = Affine::translate(Vector::new(100.0, 0.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![lone], transform: m });
    assert!(hit_test(&s.snapshot(), probe_old, 1.0).is_empty(), "moved away");
    assert_eq!(hit_test(&s.snapshot(), Point::new(605.0, 505.0), 1.0), vec![lone], "found at its new place");
    assert_eq!(pick_index_builds() - builds, 1, "one rebuild for the new state, reused by the second query");

    run(&mut s, CommandPayload::Arrange(ArrangeCommand::SetLocked { targets: vec![lone], locked: true }));
    assert!(hit_test(&s.snapshot(), Point::new(605.0, 505.0), 1.0).is_empty(), "locked objects are not picked");
    let rev = s.revision();
    s.undo(rev).unwrap(); // unlock
    assert_eq!(hit_test(&s.snapshot(), Point::new(605.0, 505.0), 1.0), vec![lone]);
    let _ = ids;
}

#[test]
fn snap_candidates_equal_the_reference_with_and_without_exclusions() {
    let (s, ids) = crowded();
    let snap = s.snapshot();
    let settings = SnapSettings::default();
    let excludes: [&[GeometryId]; 4] = [&[], &ids[..1], &ids[..150], &ids[..299]];
    let mut found = 0;
    for exclude in excludes {
        for at in queries(150) {
            let got: Vec<(Point, GeometryId)> = snap_candidates(&snap, &settings, 0.5, at, exclude)
                .into_iter()
                .map(|c| (c.at, c.source.unwrap()))
                .collect();
            let want = reference_snap(&snap, &settings, 0.5, at, exclude);
            assert_eq!(got.len(), want.len(), "at {at:?}, {} excluded", exclude.len());
            for (g, w) in got.iter().zip(&want) {
                assert!((g.0.x - w.0.x).abs() < 1e-12 && (g.0.y - w.0.y).abs() < 1e-12 && g.1 == w.1, "{g:?} vs {w:?}");
            }
            found += got.len();
        }
    }
    assert!(found > 100, "the fixture must produce snap candidates ({found})");
}

#[test]
fn snapping_and_hit_testing_share_one_index() {
    let (s, _) = crowded();
    let snap = s.snapshot();
    let before = pick_index_builds();
    let settings = SnapSettings::default();
    for at in queries(30) {
        snap_candidates(&snap, &settings, 0.5, at, &[]);
        hit_test(&snap, at, 1.0);
    }
    assert_eq!(pick_index_builds() - before, 1);
}
