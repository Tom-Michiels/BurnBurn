//! W2-L1-laser: "Cut Selected Graphics" compiles a view of the document that holds only the chosen
//! objects. The view keeps the revision of the document it was taken from (so a compile result
//! is not rejected as stale) and leaves the document itself untouched.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn apply(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn draft(name: &str) -> OperationDraft {
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: name.into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

fn square(s: &mut DocumentSession, x: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, 0.0), mm(10.0), mm(10.0)).unwrap();
    apply(s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0]
}

fn add_operation(s: &mut DocumentSession, name: &str, targets: Vec<GeometryId>) {
    apply(s, CommandPayload::Process(ProcessCommand::AddOperation { draft: draft(name), targets }));
}

fn session() -> (DocumentSession, GeometryId, GeometryId, GeometryId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let (a, b, c) = (square(&mut s, 0.0), square(&mut s, 20.0), square(&mut s, 40.0));
    add_operation(&mut s, "Alles", vec![a, b, c]);
    add_operation(&mut s, "Alleen c", vec![c]);
    (s, a, b, c)
}

#[test]
fn a_restricted_view_holds_only_the_chosen_objects_and_keeps_the_revision() {
    let (s, a, b, c) = session();
    let snapshot = s.snapshot();
    let view = snapshot.restricted_to(&[a, c]);

    let ids: Vec<GeometryId> = view.objects().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![a, c]);
    assert_eq!(view.revision(), snapshot.revision(), "same revision: a compile of the view is not stale");
    assert!(view.object(b).is_none());
    // The document is unchanged.
    assert_eq!(snapshot.len(), 3);
}

#[test]
fn operations_are_restricted_to_the_chosen_objects_and_empty_ones_disappear() {
    let (s, a, _b, c) = session();
    let snapshot = s.snapshot();
    let view = snapshot.restricted_to(&[a]);
    let targets: Vec<Vec<GeometryId>> = view.process().operations().map(|o| o.targets().to_vec()).collect();
    assert_eq!(targets, vec![vec![a]], "'Alles' keeps only a; 'Alleen c' has nothing left and is dropped");

    let view = snapshot.restricted_to(&[c]);
    let targets: Vec<Vec<GeometryId>> = view.process().operations().map(|o| o.targets().to_vec()).collect();
    assert_eq!(targets, vec![vec![c], vec![c]]);
}

#[test]
fn restricting_to_nothing_leaves_an_empty_view() {
    let (s, ..) = session();
    let view = s.snapshot().restricted_to(&[]);
    assert!(view.is_empty());
    assert_eq!(view.process().operations().count(), 0);
}
