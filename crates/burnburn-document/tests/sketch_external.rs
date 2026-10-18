//! Sketch relations against other document objects (IM21 tangent/symmetric between objects,
//! request in sketch.md) need an external sketch entity. Not supported yet: the command exists
//! so presentation can offer the route, and it is refused explicitly, changing nothing.

#![allow(clippy::unwrap_used)]

use burnburn_document::sketch::{Sketch, SketchCommand, SketchError, SketchId};
use burnburn_document::structure::GeometryRef;
use burnburn_document::{CommandError, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn cmd(s: &DocumentSession, payload: CommandPayload) -> DocumentCommand {
    DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload }
}

#[test]
fn adding_an_external_entity_to_a_sketch_is_refused_as_not_supported_yet() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(5.0).unwrap()).unwrap();
    let other =
        s.apply(cmd(&s, CommandPayload::CreateRectangle { rect, name: "Ander".into() })).unwrap().changes.created[0];
    let out = s
        .apply(cmd(&s, CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(Sketch::new("Schets")) })))
        .unwrap();
    let sketch: SketchId = out.changes.sketches[0];
    let before = s.snapshot();
    let c = SketchCommand::AddExternal { sketch, target: GeometryRef { object: other, sub: None } };
    let err = s.apply(cmd(&s, CommandPayload::Sketch(c))).unwrap_err();
    assert_eq!(err, CommandError::Sketch(SketchError::ExternalNotSupported));
    assert_eq!(s.revision(), before.revision(), "nothing committed");
    assert!(s.snapshot().same_content(&before));
}
