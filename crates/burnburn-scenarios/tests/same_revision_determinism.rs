//! Same document revision compiled twice → identical plan id and identical RD bytes; and the
//! same content after save/reopen (a new session revision) → still the same plan id and bytes,
//! because the plan id is content addressed (preview, export and upload name one plan). A real
//! content change must change both.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;

use burnburn_document::CommandPayload;
use burnburn_document::sketch::SketchCommand;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, save_project};
use burnburn_job::preview;
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use common::*;

#[test]
fn same_revision_compiles_to_identical_plan_hash_and_rd_bytes() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = new_session();
    let out = apply(&mut s, CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(plate()) }), &[&ev]);
    let sketch = out.changes.sketches[0];
    let targets = derived_objects(&s.snapshot());
    add_cut(&mut s, targets);
    let snap = s.snapshot();

    let (a, b) = (compile_plan(&snap), compile_plan(&snap));
    assert_eq!(a.id(), b.id(), "plan id");
    assert_eq!(a, b, "whole plan, not only its id");
    let (rd_a, rd_b) = (rd_bytes(&a), rd_bytes(&b));
    assert_eq!(rd_a, rd_b, "RD bytes");
    assert_eq!(preview(&a, &profile()).plan, a.id(), "preview names the same plan");

    // Reopened: other session revision, same content → same plan and bytes.
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/w"));
    let path = Path::new("/w/p.burnburn");
    save_project(&fs, path, &Project::new(ProjectUuid::from_bytes([9; 16]), (*snap).clone())).unwrap();
    let reopened = load_project(&fs, path).unwrap().document;
    let c = compile_plan(&reopened);
    assert_eq!(c.id(), a.id(), "content-addressed: revision is not part of the id");
    assert_eq!(rd_bytes(&c), rd_a);

    // A content change (width 121 mm) changes the plan id and the RD bytes.
    let set = SketchCommand::SetParameter { sketch, name: "width".into(), text: "121 mm".into() };
    apply(&mut s, CommandPayload::Sketch(set), &[&ev]);
    let d = compile_plan(&s.snapshot());
    assert_ne!(d.id(), a.id());
    assert_ne!(rd_bytes(&d), rd_a);
}
