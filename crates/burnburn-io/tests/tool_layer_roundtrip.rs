//! Tool layers (T1/T2) survive save and reopen as tool layers: never output, same hash (G5-cam).

#![allow(clippy::unwrap_used)]

use burnburn_document::process::{ProcessCommand, ToolLayer};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

#[test]
fn tool_layers_survive_save_and_reopen() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let run = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let a = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Hulp".into() }).changes.created[0];
    let b = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Merk".into() }).changes.created[0];
    run(&mut s, CommandPayload::Process(ProcessCommand::AddToolLayer { tool: ToolLayer::T1, targets: vec![a] }));
    run(&mut s, CommandPayload::Process(ProcessCommand::AddToolLayer { tool: ToolLayer::T2, targets: vec![b] }));
    let p = Project::new(ProjectUuid::from_bytes([4; 16]), (*s.snapshot()).clone());
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    let ops: Vec<_> = back.document.process().operations().map(|o| (o.tool(), o.output_enabled())).collect();
    assert_eq!(ops, vec![(Some(ToolLayer::T1), false), (Some(ToolLayer::T2), false)]);
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
}
