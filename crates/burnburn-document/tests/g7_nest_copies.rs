//! G7-cam, C12: a nest with several copies of a part. One command moves the originals to their
//! places and creates the other copies there: new objects with the groups and the layers of the
//! source, one history entry, nothing changed when a part is not there.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{
    ArrangeCommand, ArrangeError, ArrangeLabel, CommandError, CommandOutcome, CommandPayload, DocumentCommand,
    DocumentSession, NestCopy, ObjectKind, PasteItem, UndoLabel,
};
use burnburn_geometry::{Affine, Bounds, Point, Rect, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, OperationId, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(3), payload };
    s.apply(cmd)
}

fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: "deel".into() }).unwrap().changes.created[0]
}

fn layer(s: &mut DocumentSession, targets: Vec<GeometryId>) -> OperationId {
    let p = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    run(s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets })).unwrap().changes.operations[0]
}

fn shift(dx: f64, dy: f64) -> Affine {
    Affine::translate(Vector::new(dx, dy)).unwrap()
}

fn bounds(s: &DocumentSession, id: GeometryId) -> Bounds {
    s.snapshot().object(id).unwrap().bounds().unwrap()
}

/// The copy of object `id` as a paste item that joins `operations`.
fn item_of(s: &DocumentSession, id: GeometryId, group: Vec<u32>) -> PasteItem {
    let snap = s.snapshot();
    let o = snap.object(id).unwrap();
    PasteItem {
        name: o.name().to_owned(),
        kind: ObjectKind::Rectangle,
        path: o.path().as_ref().clone(),
        parametric: None,
        group,
        operations: snap.process().operations_using(id),
    }
}

fn nest(s: &mut DocumentSession, moves: Vec<(GeometryId, Affine)>, copies: Vec<NestCopy>) -> Result<CommandOutcome, CommandError> {
    run(s, CommandPayload::Arrange(ArrangeCommand::NestCopies { moves, copies }))
}

#[test]
fn the_original_moves_and_every_copy_is_a_new_object_on_the_same_layer_in_one_history_entry() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 4.0);
    let op = layer(&mut s, vec![a]);
    let copies = vec![
        NestCopy { items: vec![item_of(&s, a, vec![])], transform: shift(20.0, 0.0) },
        NestCopy { items: vec![item_of(&s, a, vec![])], transform: shift(40.0, 0.0) },
    ];
    let out = nest(&mut s, vec![(a, shift(100.0, 50.0))], copies).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::NestCopies));
    assert_eq!(out.changes.created.len(), 2);
    assert_eq!(out.changes.modified, vec![a]);
    assert_eq!(bounds(&s, a).min(), Point::new(100.0, 50.0));
    let (c1, c2) = (out.changes.created[0], out.changes.created[1]);
    assert_eq!(bounds(&s, c1).min(), Point::new(20.0, 0.0), "copies are made from the part as it was before the move");
    assert_eq!(bounds(&s, c2).min(), Point::new(40.0, 0.0));
    assert_eq!(s.snapshot().process().operation(op).unwrap().targets(), &[a, c1, c2], "the copies are cut like the original");
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().len(), 1, "one undo removes the copies");
    assert_eq!(bounds(&s, a).min(), Point::new(0.0, 0.0), "and puts the original back");
    assert_eq!(s.snapshot().process().operation(op).unwrap().targets(), &[a]);
}

#[test]
fn a_part_of_several_objects_is_copied_as_a_new_group_per_copy() {
    let mut s = session();
    let (a, b) = (rect(&mut s, 0.0, 0.0, 10.0, 10.0), rect(&mut s, 2.0, 2.0, 4.0, 4.0));
    let copies = vec![
        NestCopy { items: vec![item_of(&s, a, vec![0]), item_of(&s, b, vec![0])], transform: shift(30.0, 0.0) },
        NestCopy { items: vec![item_of(&s, a, vec![0]), item_of(&s, b, vec![0])], transform: shift(60.0, 0.0) },
    ];
    let out = nest(&mut s, vec![(a, shift(0.0, 20.0)), (b, shift(0.0, 20.0))], copies).unwrap();
    let ids = out.changes.created.clone();
    assert_eq!(ids.len(), 4);
    let snap = s.snapshot();
    let group = |i: usize| snap.object(ids[i]).unwrap().groups().to_vec();
    assert_eq!(group(0).len(), 1);
    assert_eq!(group(0), group(1), "the members of one copy share a group");
    assert_eq!(group(2), group(3));
    assert_ne!(group(0), group(2), "each copy has its own group");
    assert_eq!(out.changes.groups.len(), 2);
}

#[test]
fn an_unknown_part_changes_nothing_and_so_does_a_command_without_work() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 4.0);
    let before = s.revision();
    let copy = NestCopy { items: vec![item_of(&s, a, vec![])], transform: shift(20.0, 0.0) };
    let ghost = GeometryId::from_raw(99);
    let err = nest(&mut s, vec![(a, shift(5.0, 0.0)), (ghost, shift(1.0, 1.0))], vec![copy]).unwrap_err();
    assert_eq!(err, CommandError::Arrange(ArrangeError::UnknownGeometry(ghost)));
    assert_eq!(s.revision(), before);
    assert_eq!((s.snapshot().len(), bounds(&s, a).min()), (1, Point::new(0.0, 0.0)));

    assert_eq!(nest(&mut s, vec![], vec![]).unwrap_err(), CommandError::Arrange(ArrangeError::NothingChanged));
    let twice = nest(&mut s, vec![(a, shift(1.0, 0.0)), (a, shift(2.0, 0.0))], vec![]).unwrap_err();
    assert_eq!(twice, CommandError::Arrange(ArrangeError::DuplicateTarget(a)));
}

#[test]
fn copies_alone_are_enough_when_the_original_stays_where_it_is() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 4.0);
    let copy = NestCopy { items: vec![item_of(&s, a, vec![])], transform: shift(0.0, 9.0) };
    let out = nest(&mut s, vec![], vec![copy]).unwrap();
    assert_eq!(out.changes.created.len(), 1);
    assert!(out.changes.modified.is_empty());
    assert_eq!(s.snapshot().len(), 2);
}
