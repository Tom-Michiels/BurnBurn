//! The palette button of each layer travels with the project (C01, G5-cam), outside the project
//! hash, and a damaged entry is refused instead of guessed.

#![allow(clippy::unwrap_used)]

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn project() -> (Project, u64) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let run = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    let g = run(&mut s, CommandPayload::CreateRectangle { rect, name: "R".into() }).changes.created[0];
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
    let op = run(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }))
        .changes
        .operations[0];
    (Project::new(ProjectUuid::from_bytes([5; 16]), (*s.snapshot()).clone()), op.raw())
}

#[test]
fn the_layer_colours_survive_save_and_reopen_and_are_not_in_the_project_hash() {
    let (mut p, op) = project();
    let plain_hash = project_hash(&p).unwrap();
    p.layer_slots = vec![(op, 7)];
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.layer_slots, vec![(op, 7)]);
    assert_eq!(project_hash(&p).unwrap(), plain_hash, "a colour is not production data");
    // Without colours nothing extra is written and the file reads as before.
    let (plain, _) = project();
    assert!(decode_container(&encode_container(&plain).unwrap()).unwrap().layer_slots.is_empty());
}

#[test]
fn a_colour_for_a_layer_that_does_not_exist_or_beyond_the_palette_is_refused() {
    let (mut p, op) = project();
    p.layer_slots = vec![(op + 100, 3)];
    let err = decode_container(&encode_container(&p).unwrap()).unwrap_err();
    assert!(matches!(err, LoadError::InvalidValue { .. }), "{err:?}");
    p.layer_slots = vec![(op, 32)];
    let err = decode_container(&encode_container(&p).unwrap()).unwrap_err();
    assert!(matches!(err, LoadError::InvalidValue { .. }), "{err:?}");
    p.layer_slots = vec![(op, 1), (op, 2)];
    let err = decode_container(&encode_container(&p).unwrap()).unwrap_err();
    assert!(matches!(err, LoadError::InvalidValue { .. }), "a layer has one colour: {err:?}");
}
