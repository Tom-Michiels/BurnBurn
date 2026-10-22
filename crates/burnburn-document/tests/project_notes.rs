//! G5-UI-gaten P04: the project notes (File > Show Notes) are document data: one undo step, a
//! bounded size, and canonical only when present so existing projects keep their hash.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::process::{ProcessCommand, ProcessError, ProcessLabel};
use burnburn_document::{CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_types::{CanonicalEncode, CanonicalWriter, DocumentId, RequestId, StableId};

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd)
}

fn set_notes(s: &mut DocumentSession, text: &str) -> Result<CommandOutcome, CommandError> {
    run(s, CommandPayload::Process(ProcessCommand::SetNotes { text: text.to_owned() }))
}

fn process_bytes(s: &DocumentSession) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    s.snapshot().process().encode(&mut w);
    w.finish().unwrap()
}

#[test]
fn notes_are_one_undoable_step_with_their_own_label() {
    let mut s = session();
    assert_eq!(s.snapshot().process().notes(), "");
    let out = set_notes(&mut s, "Klant: Jansen\nBirch 3 mm").unwrap();
    assert_eq!(out.undo_label, UndoLabel::Process(ProcessLabel::SetNotes));
    assert_eq!(s.snapshot().process().notes(), "Klant: Jansen\nBirch 3 mm");
    assert!(!s.snapshot().process().is_empty(), "a layer with notes is not empty");
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().notes(), "");
    s.redo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().notes(), "Klant: Jansen\nBirch 3 mm");
}

#[test]
fn empty_notes_keep_the_canonical_form_and_filled_notes_change_it() {
    let mut s = session();
    let before = process_bytes(&s);
    set_notes(&mut s, "").unwrap();
    assert_eq!(process_bytes(&s), before, "projects without notes keep their hash");
    set_notes(&mut s, "x").unwrap();
    assert_ne!(process_bytes(&s), before);
}

#[test]
fn notes_beyond_the_limit_are_refused_and_the_document_stays() {
    let mut s = session();
    let revision = s.revision();
    let err = set_notes(&mut s, &"a".repeat(ProcessCommand::MAX_NOTES_CHARS + 1)).unwrap_err();
    assert_eq!(err, CommandError::Process(ProcessError::OutOfRange { field: "notes" }));
    assert_eq!(s.revision(), revision);
    assert!(set_notes(&mut s, &"a".repeat(ProcessCommand::MAX_NOTES_CHARS)).is_ok(), "the limit itself is allowed");
}
