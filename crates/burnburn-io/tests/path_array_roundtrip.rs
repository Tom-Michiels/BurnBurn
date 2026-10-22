//! A copy-along-path step survives save and reopen with the path it refers to (G5-cam, AR02).

#![allow(clippy::unwrap_used)]

use burnburn_document::structure::{EvalMode, Modifier, ModifierChain, ModifierStep, StructureCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, GeometryCommand};
use burnburn_geometry::{Path, Point, ProductionTolerance, Rect};
use burnburn_io::{Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

#[test]
fn a_path_array_step_survives_save_and_reopen() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let run = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let src = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).changes.created[0];
    let path = Path::polyline(&[Point::new(0.0, 0.0), Point::new(30.0, 0.0), Point::new(30.0, 40.0)], false).unwrap();
    let pad =
        run(&mut s, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: "Pad".into() })).changes.created[0];
    let chain = ModifierChain {
        name: "Langs pad".into(),
        source: src,
        steps: vec![
            ModifierStep { modifier: Modifier::PathArray { count: 5, along: pad, orient: true }, enabled: true },
            ModifierStep { modifier: Modifier::PathArray { count: 2, along: pad, orient: false }, enabled: false },
        ],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    };
    run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain }));
    let p = Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone());
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.structure(), p.document.structure());
    let a: Vec<_> = p.document.objects().map(|(_, o)| o.clone()).collect();
    let b: Vec<_> = back.document.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(a, b, "the copies are the same after reopening");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
}
