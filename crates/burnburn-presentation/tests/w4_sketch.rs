//! W4-UI-gaten, schetsfamilie (S01–S08): gaten uit de storydekking die in de UI bedienbaar zijn.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::sketch::{
    Along, DimensionKind, DimensionRole, EntityId, Relation, Sketch, SketchCommand, SketchConstraint,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_editor::Modifiers;
use burnburn_geometry::Point;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, MessageKey, SelectMode, SketchConstraintKind,
    SketchStatus, SketchViewModel, ToolRejection, UiIntent,
};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, RequestId, StableId};

type H = HeadlessEditorHarness;

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn open_window(h: &mut H) -> DocumentId {
    h.run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened")
}

/// Opens `sketch` in sketch mode; `then` are further sketch commands applied (and saved) before opening.
fn in_sketch_after(sketch: Sketch, then: Vec<SketchCommandFn>) -> (H, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let mut session = DocumentSession::new(DocumentId::from_raw(77));
    let evaluator = SketchEvaluator::new(SketchSolverAdapter);
    let create = DocumentCommand {
        document: session.id(),
        expected: session.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch) }),
    };
    session.apply_with(create, &[&evaluator]).expect("solves");
    for (i, make) in then.into_iter().enumerate() {
        let cmd = DocumentCommand {
            document: session.id(),
            expected: session.revision(),
            request: RequestId::from_raw(2 + i as u64),
            payload: CommandPayload::Sketch(make(&session)),
        };
        session.apply_with(cmd, &[&evaluator]).expect("committed");
    }
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let file = PathBuf::from("/work/schets.burnburn");
    save_project(h.fs(), &file, &Project::new(ProjectUuid::from_bytes([5; 16]), (*session.snapshot()).clone()))
        .unwrap();
    h.send(UiIntent::Open { path: file });
    let doc = open_window(&mut h);
    h.send(UiIntent::Select { document: doc, ids: vec![vm(&h, doc).objects[0].id], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::SketchEdit);
    assert!(vm(&h, doc).sketch_mode.is_some(), "sketch mode is on");
    (h, doc)
}

type SketchCommandFn = Box<dyn FnOnce(&DocumentSession) -> SketchCommand>;

fn in_sketch(sketch: Sketch) -> (H, DocumentId) {
    in_sketch_after(sketch, Vec::new())
}

fn kinds(h: &H, doc: DocumentId) -> Vec<SketchConstraintKind> {
    vm(h, doc).sketch_mode.expect("sketch mode").rows.iter().map(|r| r.kind).collect()
}

fn sketch_row(h: &H, doc: DocumentId) -> SketchViewModel {
    vm(h, doc).sketches[0].clone()
}

fn outline(h: &H, doc: DocumentId) -> (f64, f64) {
    let b = h.scene(doc).unwrap().items[0].path.bounds().unwrap();
    (b.width(), b.height())
}

// ---------------------------------------------------------------------------------------------
// S08
// ---------------------------------------------------------------------------------------------

/// A line 2 degrees off horizontal is neither clearly horizontal nor clearly sloped: auto-constrain
/// applies nothing at all (not even the clear relation of the other line) and says why.
#[test]
fn ambiguous_autoconstrain_is_not_applied() {
    let mut s = Sketch::new("S");
    let a = s.point(0.0, 0.0).unwrap();
    let b = s.point(40.0, 1.4).unwrap(); // atan(1.4 / 40) = 2.0 degrees
    let c = s.point(0.0, 20.0).unwrap();
    let d = s.point(40.0, 20.0).unwrap(); // clearly horizontal
    s.line(a, b).unwrap();
    s.line(c, d).unwrap();
    let (mut h, doc) = in_sketch(s);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.auto_constrain".to_owned() });
    h.click(doc, (20.0, 20.0), Modifiers::default());
    let after = vm(&h, doc);
    assert_eq!(after.revision, revision, "the document is unchanged");
    assert!(kinds(&h, doc).is_empty(), "no relation was applied");
    assert!(after.messages.iter().any(|m| m.key == MessageKey::ToolRejected(ToolRejection::AmbiguousRelations)));
}

// ---------------------------------------------------------------------------------------------
// S04: "Herstel laatste geldige"
// ---------------------------------------------------------------------------------------------

/// A rectangle p0..p3 with edges e0..e3, p0 fixed, e0/e2 horizontal and e1 vertical, width = `breedte`.
fn rectangle() -> (Sketch, [EntityId; 4]) {
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
    s.dimension(DimensionKind::Distance { a: p[0], b: p[1], along: Along::X }, "breedte", DimensionRole::Driving)
        .unwrap();
    s.dimension(DimensionKind::Distance { a: p[1], b: p[2], along: Along::Y }, "40 mm", DimensionRole::Driving)
        .unwrap();
    (s, e)
}

/// A conflicting relation fails the sketch (the last valid geometry stays); the restore button
/// goes back to the last state that solved. The failed edit stays available as a redo.
#[test]
fn restore_last_valid_sketch_undoes_back_to_the_last_solving_state() {
    let (s, e) = rectangle();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    let size = outline(&h, doc);
    let constraints = sketch_row(&h, doc).constraints;
    assert!(!vm(&h, doc).sketch_mode.unwrap().can_restore, "a valid sketch has nothing to restore");

    h.send(UiIntent::AddSketchConstraint {
        document: doc,
        sketch,
        constraint: SketchConstraint::Relation(Relation::Vertical(e[0])),
    });
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Failed);
    assert!(vm(&h, doc).sketch_mode.unwrap().can_restore, "a failed sketch with an earlier valid state");

    h.send(UiIntent::RestoreLastValidSketch { document: doc, sketch });
    let row = sketch_row(&h, doc);
    assert_eq!(row.status, SketchStatus::Valid);
    assert_eq!(row.constraints, constraints, "the conflicting relation is gone");
    assert_eq!(outline(&h, doc), size);
    assert!(!vm(&h, doc).sketch_mode.unwrap().can_restore);
    h.invoke(doc, CommandKey::Redo);
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Failed, "the failed edit can still be redone");
}

/// Several failing edits in a row are all undone: the result is the last *valid* state.
#[test]
fn restore_last_valid_sketch_skips_every_failed_step() {
    let (s, e) = rectangle();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    let constraints = sketch_row(&h, doc).constraints;
    for edge in [e[0], e[2]] {
        h.send(UiIntent::AddSketchConstraint {
            document: doc,
            sketch,
            constraint: SketchConstraint::Relation(Relation::Vertical(edge)),
        });
    }
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Failed);
    h.send(UiIntent::RestoreLastValidSketch { document: doc, sketch });
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Valid);
    assert_eq!(sketch_row(&h, doc).constraints, constraints);
}

/// A valid sketch is left alone: no history entry, no change.
#[test]
fn restore_last_valid_sketch_on_a_valid_sketch_changes_nothing() {
    let (s, _) = rectangle();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::RestoreLastValidSketch { document: doc, sketch });
    assert_eq!(vm(&h, doc).revision, revision);
}

/// A project opened with an already failed sketch has no earlier valid state in its history:
/// nothing is undone and the user is told why.
#[test]
fn restore_without_an_earlier_valid_state_is_refused_and_keeps_the_history() {
    let (s, e) = rectangle();
    let fail: SketchCommandFn = Box::new(move |_| SketchCommand::AddConstraint {
        sketch: burnburn_document::sketch::SketchId::from_raw(1),
        constraint: SketchConstraint::Relation(Relation::Vertical(e[0])),
    });
    let (mut h, doc) = in_sketch_after(s, vec![fail]);
    let sketch = sketch_row(&h, doc).id;
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Failed);
    assert!(!vm(&h, doc).sketch_mode.unwrap().can_restore, "no earlier valid state to go back to");
    let (revision, undo) = (vm(&h, doc).revision, vm(&h, doc).undo);
    h.send(UiIntent::RestoreLastValidSketch { document: doc, sketch });
    assert_eq!(vm(&h, doc).revision, revision);
    assert_eq!(vm(&h, doc).undo, undo);
    assert_eq!(vm(&h, doc).sketch_mode.unwrap().refused, Some(MessageKey::SketchNothingToRestore));
}

// ---------------------------------------------------------------------------------------------
// S02: decimale komma, parameter verwijderen
// ---------------------------------------------------------------------------------------------

fn parameter(h: &H, doc: DocumentId, name: &str) -> Option<String> {
    sketch_row(h, doc).parameters.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone())
}

/// A Dutch user types `100,5 mm`: outside a function call a comma can only be a decimal comma.
#[test]
fn parameter_with_a_decimal_comma_is_read_as_a_decimal() {
    let (s, _) = rectangle();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    h.send(UiIntent::SetSketchParameter { document: doc, sketch, name: "breedte".into(), text: "100,5 mm".into() });
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Valid);
    assert!((outline(&h, doc).0 - 100.5).abs() < 1e-6, "{:?}", outline(&h, doc));
    assert_eq!(parameter(&h, doc, "breedte").as_deref(), Some("100.5 mm"), "stored unambiguously");
}

/// A dimension field takes the same comma, also inside grouping parentheses.
#[test]
fn dimension_field_with_a_decimal_comma_is_read_as_a_decimal() {
    let (s, _) = rectangle();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    let width = vm(&h, doc).sketch_mode.unwrap().rows.iter().find(|r| r.text.as_deref() == Some("breedte")).unwrap().id;
    h.send(UiIntent::SetSketchDimension {
        document: doc,
        sketch,
        constraint: width,
        text: "(breedte + 2,5 mm)".into(),
    });
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Valid);
    assert!((outline(&h, doc).0 - 102.5).abs() < 1e-6, "{:?}", outline(&h, doc));
}

/// The comma of a function call stays an argument separator.
#[test]
fn a_comma_inside_a_function_call_stays_an_argument_separator() {
    let (s, _) = rectangle();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    h.send(UiIntent::SetSketchParameter {
        document: doc,
        sketch,
        name: "breedte".into(),
        text: "max(80 mm, 90 mm)".into(),
    });
    assert_eq!(parameter(&h, doc, "breedte").as_deref(), Some("max(80 mm, 90 mm)"));
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Valid);
    assert!((outline(&h, doc).0 - 90.0).abs() < 1e-6, "{:?}", outline(&h, doc));
}

/// An unused parameter is removed in one undo entry.
#[test]
fn an_unused_parameter_can_be_removed_and_the_removal_is_undoable() {
    let (mut s, _) = rectangle();
    s.set_parameter("speling", "0.2 mm").unwrap();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    h.send(UiIntent::RemoveSketchParameter { document: doc, sketch, name: "speling".into() });
    assert_eq!(parameter(&h, doc, "speling"), None);
    assert_eq!(
        vm(&h, doc).undo,
        Some(burnburn_document::UndoLabel::Sketch(burnburn_document::sketch::SketchLabel::RemoveParameter))
    );
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(parameter(&h, doc, "speling").as_deref(), Some("0.2 mm"));
}

/// A parameter a dimension (or another parameter) still uses is not removed: the user is told.
#[test]
fn a_parameter_that_is_in_use_is_not_removed() {
    let (mut s, _) = rectangle();
    s.set_parameter("dubbel", "breedte * 2").unwrap();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    let revision = vm(&h, doc).revision;
    for name in ["breedte", "nergens"] {
        h.send(UiIntent::RemoveSketchParameter { document: doc, sketch, name: name.into() });
        assert_eq!(vm(&h, doc).revision, revision, "{name}");
    }
    assert_eq!(vm(&h, doc).sketch_mode.unwrap().refused, Some(MessageKey::SketchParameterInUse));
    assert!(parameter(&h, doc, "breedte").is_some());
    // `dubbel` itself is used by nothing.
    h.send(UiIntent::RemoveSketchParameter { document: doc, sketch, name: "dubbel".into() });
    assert_eq!(parameter(&h, doc, "dubbel"), None);
}

/// Return and end-of-editing both deliver the typed text: the second one is no new history entry.
#[test]
fn setting_a_parameter_to_the_value_it_already_has_is_not_a_new_entry() {
    let (s, _) = rectangle();
    let (mut h, doc) = in_sketch(s);
    let sketch = sketch_row(&h, doc).id;
    for _ in 0..2 {
        h.send(UiIntent::SetSketchParameter { document: doc, sketch, name: "breedte".into(), text: "100,5 mm".into() });
    }
    assert_eq!(parameter(&h, doc, "breedte").as_deref(), Some("100.5 mm"));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(parameter(&h, doc, "breedte").as_deref(), Some("100 mm"), "one undo reverts the only change");
}

// ---------------------------------------------------------------------------------------------
// S01: bogen in een schets
// ---------------------------------------------------------------------------------------------

/// The three-point arc tool inside sketch mode draws an arc entity: one history entry, the
/// sketch solves, the output follows the drawn arc; dimensioning its radius drives the arc.
#[test]
fn an_arc_drawn_in_sketch_mode_becomes_an_arc_entity_that_dimensions_can_drive() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::SketchNew);
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.arc.three_point".to_owned() });
    for at in [(10.0, 0.0), (0.0, 10.0), (-10.0, 0.0)] {
        h.click(doc, at, Modifiers::default());
    }
    let after = vm(&h, doc);
    assert_eq!(
        after.undo.map(|u| matches!(u, burnburn_document::UndoLabel::Sketch(_))),
        Some(true),
        "one sketch entry"
    );
    assert_eq!(after.sketch_mode.as_ref().unwrap().refused, None, "the arc was accepted");
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Valid);
    let b = h.scene(doc).unwrap().items[0].path.bounds().unwrap();
    assert!((b.max().y - 10.0).abs() < 1e-3 && (b.width() - 20.0).abs() < 1e-3, "{b:?}");

    // A radius dimension on the arc, through the panel.
    let arc = burnburn_document::sketch::EntityId::from_raw(3);
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: vec![arc],
        constraints: vec![],
        mode: SelectMode::Replace,
    });
    h.send(UiIntent::AddSketchDimension {
        document: doc,
        kind: burnburn_presentation::SketchDimensionKind::Radius,
        text: "10 mm".into(),
    });
    assert_eq!(sketch_row(&h, doc).status, SketchStatus::Valid);
    assert_eq!(sketch_row(&h, doc).constraints, 1, "the radius dimension");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(sketch_row(&h, doc).constraints, 0, "the dimension is one entry");
    assert_eq!(vm(&h, doc).objects.len(), 1, "the arc stays");
}
