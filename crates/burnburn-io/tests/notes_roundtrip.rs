//! G5-UI-gaten P04: project notes persist additively in schema 2. A project without notes keeps
//! its file form and hash; a file from before the field loads without notes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::process::ProcessCommand;
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_io::{Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, RequestId, StableId};
use common::rewrite_json;

fn session_with(notes: &str) -> DocumentSession {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    if !notes.is_empty() {
        let cmd = DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(1),
            payload: CommandPayload::Process(ProcessCommand::SetNotes { text: notes.to_owned() }),
        };
        s.apply(cmd).unwrap();
    }
    s
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([5; 16]), (*s.snapshot()).clone())
}

#[test]
fn notes_survive_save_and_reopen_byte_for_byte() {
    let text = "Klant: Jansen\n\tBirch 3 mm — \"proef\" 日本";
    let p = project(&session_with(text));
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.process().notes(), text);
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    assert_eq!(encode_container(&back).unwrap(), encode_container(&p).unwrap());
}

#[test]
fn the_project_hash_sees_the_notes_and_a_project_without_notes_keeps_its_file_form() {
    let plain = project(&session_with(""));
    let noted = project(&session_with("x"));
    assert_ne!(project_hash(&plain).unwrap(), project_hash(&noted).unwrap());
    let text = String::from_utf8_lossy(&encode_container(&plain).unwrap()).into_owned();
    assert!(!text.contains("notes"), "default form is unchanged");
}

#[test]
fn a_file_without_the_field_loads_without_notes_and_oversized_notes_are_refused() {
    let noted = encode_container(&project(&session_with("abc"))).unwrap();
    let stripped = rewrite_json(&noted, "document.json", |v| {
        v.as_object_mut().unwrap().remove("notes");
    });
    assert_eq!(decode_container(&stripped).unwrap().document.process().notes(), "");
    let huge = rewrite_json(&noted, "document.json", |v| v["notes"] = "a".repeat(20_001).into());
    assert!(decode_container(&huge).is_err(), "notes beyond the limit are refused, not cut");
}
