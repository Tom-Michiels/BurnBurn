//! W2-L1-bewerken: draw order, locks, paste, break apart and auto-group as document commands.
//! Each is one transaction and one history entry; a command that changes nothing is refused
//! without a history entry; undo restores content and identity.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{
    ArrangeCommand, ArrangeError, ArrangeLabel, CommandError, CommandOutcome, CommandPayload, DocumentCommand,
    DocumentSession, ObjectKind, OrderChange, PasteItem, UndoLabel,
};
use burnburn_geometry::{Path, Point, Rect, Subpath, Vector};
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

fn arrange(s: &mut DocumentSession, c: ArrangeCommand) -> Result<CommandOutcome, CommandError> {
    run(s, CommandPayload::Arrange(c))
}

fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: "r".into() }).unwrap().changes.created[0]
}

fn square_subpath(x: f64, y: f64, side: f64) -> Subpath {
    Path::rectangle(Rect::new(Point::new(x, y), mm(side), mm(side)).unwrap()).subpaths()[0].clone()
}

fn order(s: &DocumentSession) -> Vec<GeometryId> {
    s.snapshot().draw_order()
}

fn line_operation(s: &mut DocumentSession, targets: Vec<GeometryId>) -> OperationId {
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

fn item(name: &str, path: Path) -> PasteItem {
    PasteItem {
        name: name.into(),
        kind: ObjectKind::Path,
        path,
        parametric: None,
        group: Vec::new(),
        operations: Vec::new(),
    }
}

#[test]
fn draw_order_defaults_to_creation_order_and_new_objects_land_on_top() {
    let mut s = session();
    let (a, b) = (rect(&mut s, 0.0, 0.0, 5.0, 5.0), rect(&mut s, 10.0, 0.0, 5.0, 5.0));
    assert_eq!(order(&s), vec![a, b]);
    arrange(&mut s, ArrangeCommand::Order { targets: vec![a], change: OrderChange::ToFront }).unwrap();
    let c = rect(&mut s, 20.0, 0.0, 5.0, 5.0);
    assert_eq!(order(&s), vec![b, a, c], "an object created after a reorder is drawn on top");
}

#[test]
fn push_to_front_and_to_back_keep_identity_and_undo_restores_the_order() {
    let mut s = session();
    let ids: Vec<_> = (0..4).map(|i| rect(&mut s, f64::from(i) * 10.0, 0.0, 5.0, 5.0)).collect();
    let revision = s.revision();
    s.mark_saved(revision);
    let out = arrange(&mut s, ArrangeCommand::Order { targets: vec![ids[1]], change: OrderChange::ToFront }).unwrap();
    assert!(s.is_dirty(), "a reorder is a document change");
    assert_eq!(order(&s), vec![ids[0], ids[2], ids[3], ids[1]]);
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::DrawOrder));
    assert_eq!(out.changes.modified, vec![ids[1]], "the moved object is reported, nothing is created or deleted");
    assert!(out.changes.created.is_empty() && out.changes.deleted.is_empty());
    arrange(&mut s, ArrangeCommand::Order { targets: vec![ids[2], ids[3]], change: OrderChange::ToBack }).unwrap();
    assert_eq!(order(&s), vec![ids[2], ids[3], ids[0], ids[1]], "a multi-selection keeps its relative order");
    s.undo(s.revision()).unwrap();
    s.undo(s.revision()).unwrap();
    assert_eq!(order(&s), ids);
    assert_ne!(s.revision(), revision, "undo is a new revision");
    assert!(!s.is_dirty(), "back to the saved content");
}

#[test]
fn push_forward_and_backward_move_one_step_past_the_next_unselected_object() {
    let mut s = session();
    let ids: Vec<_> = (0..5).map(|i| rect(&mut s, f64::from(i) * 10.0, 0.0, 5.0, 5.0)).collect();
    arrange(&mut s, ArrangeCommand::Order { targets: vec![ids[1], ids[2]], change: OrderChange::Forward }).unwrap();
    assert_eq!(order(&s), vec![ids[0], ids[3], ids[1], ids[2], ids[4]]);
    arrange(&mut s, ArrangeCommand::Order { targets: vec![ids[1], ids[2]], change: OrderChange::Backward }).unwrap();
    assert_eq!(order(&s), ids);
    arrange(&mut s, ArrangeCommand::Order { targets: vec![ids[0], ids[2]], change: OrderChange::Backward }).unwrap();
    assert_eq!(order(&s), vec![ids[0], ids[2], ids[1], ids[3], ids[4]], "an object at the back stays; the other moves");
}

#[test]
fn an_order_change_that_moves_nothing_is_refused_without_history() {
    let mut s = session();
    let (a, b) = (rect(&mut s, 0.0, 0.0, 5.0, 5.0), rect(&mut s, 10.0, 0.0, 5.0, 5.0));
    let revision = s.revision();
    for (target, change) in
        [(b, OrderChange::ToFront), (b, OrderChange::Forward), (a, OrderChange::ToBack), (a, OrderChange::Backward)]
    {
        let err = arrange(&mut s, ArrangeCommand::Order { targets: vec![target], change }).unwrap_err();
        assert_eq!(err, CommandError::Arrange(ArrangeError::NothingChanged));
    }
    assert_eq!(s.revision(), revision);
    assert_eq!(s.undo_label(), Some(UndoLabel::CreateRectangle));
}

#[test]
fn deleting_an_object_removes_it_from_the_draw_order_and_undo_brings_it_back_in_place() {
    let mut s = session();
    let ids: Vec<_> = (0..3).map(|i| rect(&mut s, f64::from(i) * 10.0, 0.0, 5.0, 5.0)).collect();
    arrange(&mut s, ArrangeCommand::Order { targets: vec![ids[0]], change: OrderChange::ToFront }).unwrap();
    run(&mut s, CommandPayload::Delete { targets: vec![ids[2]] }).unwrap();
    assert_eq!(order(&s), vec![ids[1], ids[0]]);
    s.undo(s.revision()).unwrap();
    assert_eq!(order(&s), vec![ids[1], ids[2], ids[0]]);
}

#[test]
fn lock_marks_objects_and_is_one_undoable_step() {
    let mut s = session();
    let (a, b) = (rect(&mut s, 0.0, 0.0, 5.0, 5.0), rect(&mut s, 10.0, 0.0, 5.0, 5.0));
    let out = arrange(&mut s, ArrangeCommand::SetLocked { targets: vec![a], locked: true }).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::Lock));
    assert!(s.snapshot().is_locked(a) && !s.snapshot().is_locked(b));
    assert_eq!(
        arrange(&mut s, ArrangeCommand::SetLocked { targets: vec![a], locked: true }).unwrap_err(),
        CommandError::Arrange(ArrangeError::NothingChanged)
    );
    let out = arrange(&mut s, ArrangeCommand::SetLocked { targets: vec![a], locked: false }).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::Unlock));
    assert!(!s.snapshot().is_locked(a));
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().is_locked(a), "undo restores the lock");
}

#[test]
fn hide_is_a_display_flag_in_the_document_with_its_own_undo_step() {
    let mut s = session();
    let (a, b) = (rect(&mut s, 0.0, 0.0, 5.0, 5.0), rect(&mut s, 10.0, 0.0, 5.0, 5.0));
    let out = arrange(&mut s, ArrangeCommand::SetHidden { targets: vec![a, b], hidden: true }).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::Hide));
    assert!(s.snapshot().is_hidden(a) && s.snapshot().is_hidden(b));
    assert_eq!(s.snapshot().len(), 2, "hidden objects stay in the document");
    assert_eq!(
        arrange(&mut s, ArrangeCommand::SetHidden { targets: vec![a], hidden: true }).unwrap_err(),
        CommandError::Arrange(ArrangeError::NothingChanged)
    );
    let out = arrange(&mut s, ArrangeCommand::SetHidden { targets: vec![a], hidden: false }).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::Show));
    assert!(!s.snapshot().is_hidden(a) && s.snapshot().is_hidden(b));
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().is_hidden(a));
}

#[test]
fn only_visible_unlocked_objects_are_selectable() {
    let mut s = session();
    let (a, b, c) =
        (rect(&mut s, 0.0, 0.0, 5.0, 5.0), rect(&mut s, 10.0, 0.0, 5.0, 5.0), rect(&mut s, 20.0, 0.0, 5.0, 5.0));
    arrange(&mut s, ArrangeCommand::SetLocked { targets: vec![a], locked: true }).unwrap();
    arrange(&mut s, ArrangeCommand::SetHidden { targets: vec![b], hidden: true }).unwrap();
    let snap = s.snapshot();
    assert!(!snap.is_selectable(a) && !snap.is_selectable(b) && snap.is_selectable(c));
}

#[test]
fn deleting_a_locked_object_forgets_the_lock() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 5.0, 5.0);
    arrange(&mut s, ArrangeCommand::SetLocked { targets: vec![a], locked: true }).unwrap();
    run(&mut s, CommandPayload::Delete { targets: vec![a] }).unwrap();
    assert!(!s.snapshot().is_locked(a));
    assert!(s.snapshot().arrange().is_empty());
    let b = rect(&mut s, 0.0, 0.0, 5.0, 5.0);
    arrange(&mut s, ArrangeCommand::SetHidden { targets: vec![b], hidden: true }).unwrap();
    run(&mut s, CommandPayload::Delete { targets: vec![b] }).unwrap();
    assert!(s.snapshot().arrange().is_empty(), "hide flags of deleted objects are forgotten too");
}

#[test]
fn paste_creates_all_objects_in_one_step_shifted_by_the_offset_and_on_top() {
    let mut s = session();
    let existing = rect(&mut s, 0.0, 0.0, 5.0, 5.0);
    let items = vec![
        item("a", Path::rectangle(Rect::new(Point::new(0.0, 0.0), mm(2.0), mm(2.0)).unwrap())),
        item("b", Path::rectangle(Rect::new(Point::new(10.0, 0.0), mm(2.0), mm(2.0)).unwrap())),
    ];
    let out = arrange(&mut s, ArrangeCommand::Paste { items, offset: Vector::new(5.0, 7.0) }).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::Paste));
    let created = out.changes.created.clone();
    assert_eq!(created.len(), 2);
    let snap = s.snapshot();
    let first = snap.object(created[0]).unwrap().bounds().unwrap();
    assert_eq!((first.min().x, first.min().y), (5.0, 7.0));
    assert_eq!(snap.object(created[1]).unwrap().name(), "b");
    assert_eq!(order(&s), vec![existing, created[0], created[1]]);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().len(), 1, "one undo removes the whole paste");
}

#[test]
fn paste_keeps_group_membership_as_new_groups_and_layer_assignment() {
    let mut s = session();
    let source = rect(&mut s, 0.0, 0.0, 5.0, 5.0);
    let operation = line_operation(&mut s, vec![source]);
    let mk = |name: &str, group: Vec<u32>, ops: Vec<OperationId>| PasteItem {
        group,
        operations: ops,
        ..item(name, Path::rectangle(Rect::new(Point::new(0.0, 0.0), mm(1.0), mm(1.0)).unwrap()))
    };
    let items = vec![mk("a", vec![0], vec![operation]), mk("b", vec![0], vec![]), mk("c", vec![], vec![operation])];
    let out = arrange(&mut s, ArrangeCommand::Paste { items, offset: Vector::new(1.0, 1.0) }).unwrap();
    let ids = out.changes.created.clone();
    let snap = s.snapshot();
    let (ga, gb, gc) = (
        snap.object(ids[0]).unwrap().groups(),
        snap.object(ids[1]).unwrap().groups(),
        snap.object(ids[2]).unwrap().groups(),
    );
    assert_eq!(ga.len(), 1);
    assert_eq!(ga, gb, "members of one source group share a new group");
    assert!(gc.is_empty());
    assert_eq!(out.changes.groups, ga.to_vec());
    let op = snap.process().operation(operation).unwrap();
    assert_eq!(op.targets(), &[source, ids[0], ids[2]]);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().operation(operation).unwrap().targets(), &[source]);
}

#[test]
fn paste_of_nothing_or_an_empty_path_is_refused() {
    let mut s = session();
    assert_eq!(
        arrange(&mut s, ArrangeCommand::Paste { items: vec![], offset: Vector::ZERO }).unwrap_err(),
        CommandError::Arrange(ArrangeError::NothingChanged)
    );
    assert_eq!(
        arrange(&mut s, ArrangeCommand::Paste { items: vec![item("e", Path::default())], offset: Vector::ZERO })
            .unwrap_err(),
        CommandError::Arrange(ArrangeError::EmptyPath)
    );
    assert!(s.snapshot().is_empty() && !s.can_undo());
}

#[test]
fn break_apart_splits_subpaths_into_objects_that_keep_group_layer_and_place() {
    let mut s = session();
    let bottom = rect(&mut s, 50.0, 0.0, 5.0, 5.0);
    let compound = Path::from_subpaths(vec![
        square_subpath(0.0, 0.0, 4.0),
        square_subpath(10.0, 0.0, 4.0),
        square_subpath(20.0, 0.0, 4.0),
    ]);
    let id = arrange(&mut s, ArrangeCommand::Paste { items: vec![item("letters", compound)], offset: Vector::ZERO })
        .unwrap()
        .changes
        .created[0];
    let top = rect(&mut s, 60.0, 0.0, 5.0, 5.0);
    let operation = line_operation(&mut s, vec![id]);
    let out = arrange(&mut s, ArrangeCommand::BreakApart { targets: vec![id] }).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::BreakApart));
    assert_eq!(out.changes.deleted, vec![id]);
    assert_eq!(out.changes.created.len(), 3);
    let snap = s.snapshot();
    assert!(snap.object(id).is_none());
    for piece in &out.changes.created {
        assert_eq!(snap.object(*piece).unwrap().path().subpaths().len(), 1);
        assert_eq!(snap.object(*piece).unwrap().name(), "letters");
    }
    assert_eq!(snap.process().operation(operation).unwrap().targets(), out.changes.created.as_slice());
    let mut expected = vec![bottom];
    expected.extend(out.changes.created.iter().copied());
    expected.push(top);
    assert_eq!(order(&s), expected, "the pieces take the place of the original");
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().object(id).is_some() && s.snapshot().len() == 3);
    assert_eq!(s.snapshot().process().operation(operation).unwrap().targets(), &[id]);
}

#[test]
fn break_apart_of_single_subpath_objects_is_refused() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 5.0, 5.0);
    assert_eq!(
        arrange(&mut s, ArrangeCommand::BreakApart { targets: vec![a] }).unwrap_err(),
        CommandError::Arrange(ArrangeError::NothingChanged)
    );
}

#[test]
fn auto_group_groups_shapes_that_lie_inside_each_other_and_leaves_the_rest() {
    let mut s = session();
    let outer = rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let hole = rect(&mut s, 5.0, 5.0, 4.0, 4.0);
    let lone = rect(&mut s, 50.0, 0.0, 5.0, 5.0);
    let other_outer = rect(&mut s, 100.0, 0.0, 20.0, 20.0);
    let other_hole = rect(&mut s, 105.0, 5.0, 4.0, 4.0);
    let out = arrange(&mut s, ArrangeCommand::AutoGroup { targets: vec![outer, hole, lone, other_outer, other_hole] })
        .unwrap();
    assert_eq!(out.undo_label, UndoLabel::Arrange(ArrangeLabel::AutoGroup));
    assert_eq!(out.changes.groups.len(), 2);
    let snap = s.snapshot();
    let g = |id| snap.object(id).unwrap().groups().to_vec();
    assert_eq!(g(outer).len(), 1);
    assert_eq!(g(outer), g(hole));
    assert_eq!(g(other_outer), g(other_hole));
    assert_ne!(g(outer), g(other_outer));
    assert!(g(lone).is_empty());
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().object(outer).unwrap().groups().is_empty());
}

#[test]
fn auto_group_with_nothing_nested_is_refused() {
    let mut s = session();
    let (a, b) = (rect(&mut s, 0.0, 0.0, 5.0, 5.0), rect(&mut s, 10.0, 0.0, 5.0, 5.0));
    assert_eq!(
        arrange(&mut s, ArrangeCommand::AutoGroup { targets: vec![a, b] }).unwrap_err(),
        CommandError::Arrange(ArrangeError::NothingChanged)
    );
}

#[test]
fn unknown_targets_are_rejected_and_change_nothing() {
    let mut s = session();
    rect(&mut s, 0.0, 0.0, 5.0, 5.0);
    let ghost = GeometryId::from_raw(999);
    let revision = s.revision();
    let err =
        arrange(&mut s, ArrangeCommand::Order { targets: vec![ghost], change: OrderChange::ToFront }).unwrap_err();
    assert_eq!(err, CommandError::Arrange(ArrangeError::UnknownGeometry(ghost)));
    assert!(arrange(&mut s, ArrangeCommand::SetLocked { targets: vec![ghost], locked: true }).is_err());
    assert_eq!(s.revision(), revision);
}
