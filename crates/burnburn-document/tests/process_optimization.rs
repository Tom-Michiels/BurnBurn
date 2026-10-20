//! Optimization settings and cut priorities (W2-L3) as document data: validated, one undoable
//! transaction each, canonical only when they differ from the defaults.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::process::{
    CutPriority, OptimizationSettings, OrderKey, ProcessCommand, ProcessError, ProcessLabel,
};
use burnburn_document::{CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{CanonicalEncode, CanonicalWriter, DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd)
}

fn rectangle(s: &mut DocumentSession) -> GeometryId {
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: String::new() }).unwrap().changes.created[0]
}

fn process_bytes(s: &DocumentSession) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    s.snapshot().process().encode(&mut w);
    w.finish().unwrap()
}

fn priority(p: i32) -> CutPriority {
    CutPriority::new(p).unwrap()
}

#[test]
fn default_settings_and_priorities_do_not_change_the_canonical_process_layer() {
    let mut s = session();
    let g = rectangle(&mut s);
    let before = process_bytes(&s);
    run(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: OptimizationSettings::default() }))
        .unwrap();
    run(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority: priority(0) })).unwrap();
    assert_eq!(process_bytes(&s), before, "projects without these settings keep their hash");
    assert!(s.snapshot().process().is_empty());
}

#[test]
fn non_default_settings_are_part_of_the_canonical_content() {
    let mut s = session();
    let g = rectangle(&mut s);
    let before = process_bytes(&s);
    let order = OptimizationSettings::default().with_order(vec![OrderKey::Priority, OrderKey::Layer]).unwrap();
    run(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: order })).unwrap();
    let with_order = process_bytes(&s);
    assert_ne!(with_order, before);
    run(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority: priority(3) })).unwrap();
    assert_ne!(process_bytes(&s), with_order);
    // The order of the hierarchy is content: Layer→Priority differs from Priority→Layer.
    let other = OptimizationSettings::default().with_order(vec![OrderKey::Layer, OrderKey::Priority]).unwrap();
    let mut t = session();
    rectangle(&mut t);
    run(&mut t, CommandPayload::Process(ProcessCommand::SetOptimization { settings: other })).unwrap();
    assert_ne!(process_bytes(&t), with_order);
}

#[test]
fn settings_and_priorities_are_undoable_single_transactions() {
    let mut s = session();
    let g = rectangle(&mut s);
    let settings = OptimizationSettings::default().with_inner_first(false);
    run(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: settings.clone() })).unwrap();
    assert_eq!(s.undo_label(), Some(UndoLabel::Process(ProcessLabel::SetOptimization)));
    run(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority: priority(-2) })).unwrap();
    assert_eq!(s.undo_label(), Some(UndoLabel::Process(ProcessLabel::SetPriority)));
    assert_eq!(s.snapshot().process().priority(g), priority(-2));

    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().priority(g), priority(0));
    assert_eq!(s.snapshot().process().optimization(), &settings);
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().process().optimization().is_default());
    s.redo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().optimization(), &settings);
}

#[test]
fn invalid_values_are_rejected_with_typed_errors() {
    assert_eq!(CutPriority::new(CutPriority::MAX + 1), Err(ProcessError::OutOfRange { field: "priority" }));
    assert!(CutPriority::new(-CutPriority::MAX).is_ok());
    let dup = OptimizationSettings::default().with_order(vec![OrderKey::Layer, OrderKey::Layer]);
    assert_eq!(dup.unwrap_err(), ProcessError::OutOfRange { field: "order by" });
    let mut s = session();
    let err = run(
        &mut s,
        CommandPayload::Process(ProcessCommand::SetPriority {
            geometry: GeometryId::from_raw(99),
            priority: priority(1),
        }),
    )
    .unwrap_err();
    assert_eq!(err, CommandError::Process(ProcessError::UnknownGeometry(GeometryId::from_raw(99))));
}

#[test]
fn deleting_a_shape_drops_its_priority_and_undo_brings_it_back() {
    let mut s = session();
    let g = rectangle(&mut s);
    run(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority: priority(4) })).unwrap();
    run(&mut s, CommandPayload::Delete { targets: vec![g] }).unwrap();
    assert_eq!(s.snapshot().process().priorities().count(), 0);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().priority(g), priority(4));
}
