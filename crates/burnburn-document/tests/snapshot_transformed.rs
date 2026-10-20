//! W3-UI-schikken (Print and Cut): the document as it is cut when a registration is active — every
//! outline moved by one transform, nothing else changed. The document itself is never touched.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::process::ProcessCommand;
use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, GeometryCommand};
use burnburn_geometry::{Affine, Point, Rect, Shape, Vector};
use burnburn_types::{Angle, DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn apply(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    s.apply(DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(3), payload })
        .unwrap()
}

fn draft() -> OperationDraft {
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: "Snijden".into(),
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

fn session_with_one_cut() -> (DocumentSession, GeometryId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let shape = Shape::Rectangle(Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(4.0)).unwrap());
    let id = apply(&mut s, CommandPayload::Edit(GeometryCommand::CreateShape { shape, name: "r".into() }))
        .changes
        .created[0];
    apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft: draft(), targets: vec![id] }));
    (s, id)
}

#[test]
fn every_outline_moves_by_the_transform_and_the_rest_stays() {
    let (s, id) = session_with_one_cut();
    let snapshot = s.snapshot();
    let m =
        Affine::rotate(Angle::from_degrees(90.0).unwrap()).then(&Affine::translate(Vector::new(50.0, 20.0)).unwrap());
    let moved = snapshot.transformed(&m);
    let b = moved.object(id).unwrap().bounds().unwrap();
    // (x, y) → (−y, x) + (50, 20): the 10 × 4 box at the origin becomes 4 × 10 at (46, 20).
    assert!((b.min().x - 46.0).abs() < 1e-9 && (b.min().y - 20.0).abs() < 1e-9, "{b:?}");
    assert!((b.width() - 4.0).abs() < 1e-9 && (b.height() - 10.0).abs() < 1e-9);
    assert_eq!(moved.revision(), snapshot.revision(), "a plan made from it is not stale for the document");
    assert_eq!(moved.process().operations().count(), 1, "the operations keep their targets");
    assert_eq!(
        moved.process().operations().next().unwrap().targets(),
        snapshot.process().operations().next().unwrap().targets()
    );
    // The document is untouched.
    assert_eq!(s.snapshot().object(id).unwrap().bounds().unwrap().min(), Point::new(0.0, 0.0));
}

#[test]
fn the_identity_changes_nothing() {
    let (s, id) = session_with_one_cut();
    let same = s.snapshot().transformed(&Affine::IDENTITY);
    assert_eq!(same.object(id).unwrap().path(), s.snapshot().object(id).unwrap().path());
}
