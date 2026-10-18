//! Sketch end to end with the real evaluator (request Sketch, docs/coordination/sketch.md):
//! sketch → solve → save → reopen → re-evaluate → edit again. Joins the two halves that cannot
//! live in one crate (`io/tests/sketch_roundtrip.rs` uses a fake evaluator; the sketch crate
//! cannot reach io).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;

use burnburn_document::sketch::SketchCommand;
use burnburn_document::{CommandPayload, DerivedOwner, DocumentSession, Evaluator, GeometrySource};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, save_project};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, StableId};
use common::*;

fn hole_centres(snap: &burnburn_document::DocumentSnapshot) -> Vec<(f64, f64)> {
    let mut c: Vec<_> = derived_objects(snap)
        .into_iter()
        .map(|id| snap.object(id).unwrap().bounds().unwrap())
        .filter(|b| b.width() < 10.0)
        .map(|b| (b.center().x, b.center().y))
        .collect();
    c.sort_by(|a, b| a.partial_cmp(b).unwrap());
    c
}

fn close(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| (p.0 - q.0).abs() <= 1e-9 && (p.1 - q.1).abs() <= 1e-9)
}

#[test]
fn sketch_survives_save_and_reopen_and_stays_editable_with_the_real_solver() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = new_session();
    let out = apply(&mut s, CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(plate()) }), &[&ev]);
    let sketch = out.changes.sketches[0];
    let set = |w: &str| SketchCommand::SetParameter { sketch, name: "width".into(), text: w.into() };
    apply(&mut s, CommandPayload::Sketch(set("150 mm")), &[&ev]);
    let before = s.snapshot();
    assert!(close(&hole_centres(&before), &[(10.0, 10.0), (10.0, 70.0), (140.0, 10.0), (140.0, 70.0)]));

    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/work"));
    let path = Path::new("/work/plaat.burnburn");
    save_project(&fs, path, &Project::new(ProjectUuid::from_bytes([5; 16]), (*before).clone())).unwrap();
    let reopened = load_project(&fs, path).unwrap().document;

    // Definition with expression texts and derived outputs come back unchanged.
    assert_eq!(
        reopened.sketch(sketch).unwrap().parameters().get("width").map(|t| t.to_string()),
        Some("150 mm".to_owned())
    );
    let ids = derived_objects(&reopened);
    assert_eq!(ids, derived_objects(&before), "same GeometryIds for derived outputs");
    for id in &ids {
        assert_eq!(reopened.object(*id).unwrap().path(), before.object(*id).unwrap().path(), "bit-identical path");
    }

    // The real evaluator, run on the reopened document, reproduces the stored outputs bit for bit
    // and accepts the stored input hash as current.
    let patch = ev.evaluate(&reopened, DerivedOwner::Sketch(sketch));
    assert_eq!(reopened.derived_record(DerivedOwner::Sketch(sketch)).unwrap().inputs, Some(patch.inputs));
    assert_eq!(patch.outputs.len(), 5);
    for o in &patch.outputs {
        let stored = reopened
            .objects()
            .find(|(_, x)| matches!(x.source(), GeometrySource::Derived(k) if *k == o.key))
            .map(|(_, x)| x.path().clone())
            .unwrap();
        assert_eq!(*stored, o.path, "re-evaluation after reopen is bit-identical");
    }

    // And the reopened sketch is still a live, editable definition: width 90 moves the right holes.
    let mut s2 = DocumentSession::from_snapshot(DocumentId::from_raw(2), reopened);
    apply(&mut s2, CommandPayload::Sketch(set("90 mm")), &[&ev]);
    assert!(close(&hole_centres(&s2.snapshot()), &[(10.0, 10.0), (10.0, 70.0), (80.0, 10.0), (80.0, 70.0)]));
}
