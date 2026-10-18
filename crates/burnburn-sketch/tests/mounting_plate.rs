//! IM11 first failing test of the plan: a width change keeps the holes symmetric, with their
//! GeometryIds (DR-12: OperationRefs must stay bound).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use burnburn_document::sketch::SketchCommand;
use burnburn_document::{DerivedOwner, EvalStatus, GeometrySource};
use burnburn_geometry::Point;
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::GeometryId;
use common::{plate, session, set_param, sketch_cmd};

fn centre(snap: &burnburn_document::DocumentSnapshot, id: GeometryId) -> Point {
    let b = snap.object(id).unwrap().bounds().unwrap();
    Point::new(0.5 * (b.min().x + b.max().x), 0.5 * (b.min().y + b.max().y))
}

#[test]
fn changing_plate_width_preserves_symmetric_hole_offsets() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = session();
    let p = plate();
    let out =
        s.apply_with(sketch_cmd(&s, SketchCommand::Create { sketch: Box::new(p.sketch.clone()) }), &[&ev]).unwrap();
    let id = out.changes.sketches[0];
    let owner = DerivedOwner::Sketch(id);
    assert_eq!(s.snapshot().derived_record(owner).unwrap().status, EvalStatus::Valid);

    // Output per hole, keyed by the circle entity.
    let hole_ids: Vec<GeometryId> = p
        .holes
        .iter()
        .map(|h| {
            s.snapshot()
                .objects()
                .find(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.local == u64::from(h.raw())))
                .map(|(gid, _)| gid)
                .unwrap()
        })
        .collect();
    assert_eq!(s.snapshot().len(), 5, "outline + 4 holes; construction lines and points are not output");

    let rev = s.revision();
    s.apply_with(sketch_cmd(&s, set_param("width", "150 mm", id)), &[&ev]).unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.revision(), rev.next().unwrap(), "one transaction for definition + geometry");
    assert_eq!(snap.derived_record(owner).unwrap().status, EvalStatus::Valid);
    assert_eq!(snap.len(), 5);

    let expected = [(10.0, 10.0), (140.0, 10.0), (140.0, 70.0), (10.0, 70.0)];
    for (gid, (x, y)) in hole_ids.iter().zip(expected) {
        assert!(snap.object(*gid).is_some(), "hole keeps its GeometryId {gid}");
        let c = centre(&snap, *gid);
        assert!((c.x - x).abs() <= 1e-9 && (c.y - y).abs() <= 1e-9, "hole {gid} at {c:?}, expected ({x}, {y})");
        assert!(!snap.production_blocked(*gid));
    }
    // Symmetric: equal distances to the left and right edges, exactly.
    let (l, r) = (centre(&snap, hole_ids[0]), centre(&snap, hole_ids[1]));
    assert!(((l.x - 0.0) - (150.0 - r.x)).abs() <= 1e-9);
    let outline = snap.objects().find(|(gid, _)| !hole_ids.contains(gid)).map(|(_, o)| o.bounds().unwrap()).unwrap();
    assert!((outline.width() - 150.0).abs() <= 1e-9 && (outline.height() - 80.0).abs() <= 1e-9);
}
