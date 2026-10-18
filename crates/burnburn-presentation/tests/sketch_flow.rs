//! B8 (S01–S04) through the UI: constraints added and removed with intents (one entry each),
//! dimension fields that take expressions, and the solver diagnosis in the view model.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::sketch::{
    Along, ConstraintId, DimensionKind, DimensionRole, EntityId, Relation, Sketch, SketchCommand, SketchConstraint,
    SketchLabel,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::Point;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, SketchStatus, UiIntent};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, RequestId, StableId};

/// A rectangle p0..p3 with fixed p0, edges e0..e3; e0/e2 horizontal and e1 vertical; e3 left
/// free (to be related through the UI). Width driven by the parameter `breedte`, height 40 mm.
struct Fixture {
    sketch: Sketch,
    e: [EntityId; 4],
    width: ConstraintId,
}

fn fixture() -> Fixture {
    let mut s = Sketch::new("Plaat");
    s.set_parameter("breedte", "100 mm").unwrap();
    let p = [
        s.point(0.0, 0.0).unwrap(),
        s.point(98.0, 1.0).unwrap(),
        s.point(99.0, 41.0).unwrap(),
        s.point(1.0, 39.0).unwrap(),
    ];
    let e = [
        s.line(p[0], p[1]).unwrap(),
        s.line(p[1], p[2]).unwrap(),
        s.line(p[2], p[3]).unwrap(),
        s.line(p[3], p[0]).unwrap(),
    ];
    s.relate(Relation::Fix { point: p[0], at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Horizontal(e[0])).unwrap();
    s.relate(Relation::Vertical(e[1])).unwrap();
    s.relate(Relation::Horizontal(e[2])).unwrap();
    let width = s
        .dimension(DimensionKind::Distance { a: p[0], b: p[1], along: Along::X }, "breedte", DimensionRole::Driving)
        .unwrap();
    s.dimension(DimensionKind::Distance { a: p[1], b: p[2], along: Along::Y }, "40 mm", DimensionRole::Driving)
        .unwrap();
    Fixture { sketch: s, e, width }
}

fn opened(sketch: Sketch) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let mut s = DocumentSession::new(DocumentId::from_raw(77));
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch) }),
    };
    s.apply_with(cmd, &[&SketchEvaluator::new(SketchSolverAdapter)]).expect("solves");
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from("/work/plaat.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([5; 16]), (*s.snapshot()).clone())).unwrap();
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    (h, doc)
}

fn outline_size(h: &HeadlessEditorHarness, doc: DocumentId) -> (f64, f64) {
    let b = h.scene(doc).unwrap().items[0].path.bounds().unwrap();
    (b.width(), b.height())
}

/// S01/S03: a relation added through the constraint panel is one undo entry; the view model
/// shows the remaining freedom; undo removes it again.
#[test]
fn constraint_added_through_the_panel_is_one_entry_and_updates_freedom() {
    let f = fixture();
    let (mut h, doc) = opened(f.sketch);
    let row = h.document(doc).unwrap().sketches[0].clone();
    assert_eq!(row.status, SketchStatus::Valid);
    let dof_before = row.dof.expect("solved: freedom known");

    let vertical = SketchConstraint::Relation(Relation::Vertical(f.e[3]));
    h.send(UiIntent::AddSketchConstraint { document: doc, sketch: row.id, constraint: vertical });
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.undo, Some(UndoLabel::Sketch(SketchLabel::AddConstraint)));
    assert_eq!(vm.sketches[0].constraints, row.constraints + 1);
    assert!(vm.sketches[0].dof.unwrap() < dof_before, "the relation removed freedom");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.document(doc).unwrap().sketches[0].constraints, row.constraints);
}

/// S02: a dimension field takes an expression over parameters; an invalid one names the
/// dimension and keeps the last valid geometry.
#[test]
fn dimension_field_takes_expressions_over_parameters() {
    let f = fixture();
    let (mut h, doc) = opened(f.sketch);
    let sketch = h.document(doc).unwrap().sketches[0].id;
    assert!((outline_size(&h, doc).0 - 100.0).abs() < 1e-6);

    h.send(UiIntent::SetSketchDimension { document: doc, sketch, constraint: f.width, text: "breedte + 20 mm".into() });
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.undo, Some(UndoLabel::Sketch(SketchLabel::SetDimensionValue)));
    assert!((outline_size(&h, doc).0 - 120.0).abs() < 1e-6, "{:?}", outline_size(&h, doc));

    h.send(UiIntent::SetSketchDimension { document: doc, sketch, constraint: f.width, text: "breedte + 20".into() });
    let row = h.document(doc).unwrap().sketches[0].clone();
    assert_eq!(row.status, SketchStatus::Failed, "a length plus a bare number is a unit error");
    assert_eq!(row.problem_constraints, vec![f.width], "the field with the error is named");
    assert!((outline_size(&h, doc).0 - 120.0).abs() < 1e-6, "last valid geometry stays");
}

/// S04: a contradicting relation fails the sketch; the diagnosis names the involved constraints
/// (to select them in the panel); removing it through the panel makes the sketch valid again.
#[test]
fn conflict_message_selects_involved_constraints() {
    let f = fixture();
    let (mut h, doc) = opened(f.sketch);
    let sketch = h.document(doc).unwrap().sketches[0].id;
    let size = outline_size(&h, doc);
    h.send(UiIntent::AddSketchConstraint {
        document: doc,
        sketch,
        constraint: SketchConstraint::Relation(Relation::Vertical(f.e[0])),
    });
    let row = h.document(doc).unwrap().sketches[0].clone();
    assert_eq!(row.status, SketchStatus::Failed);
    let added = row.last_constraint.expect("new constraint id");
    assert!(row.problem_constraints.contains(&added), "{:?}", row.problem_constraints);
    assert!(row.problem_constraints.len() >= 2, "the conflict names what it conflicts with");
    assert_eq!(outline_size(&h, doc), size, "last valid geometry");

    h.send(UiIntent::RemoveSketchConstraint { document: doc, sketch, constraint: added });
    let row = h.document(doc).unwrap().sketches[0].clone();
    assert_eq!(row.status, SketchStatus::Valid);
    assert!(row.problem_constraints.is_empty());
}
