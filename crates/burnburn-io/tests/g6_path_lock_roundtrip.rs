//! G6-machine, C06: the start and direction locks of shapes persist additively. A project without
//! locks keeps its file form and hash; stored locks come back unchanged or are refused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::process::{PathLock, ProcessCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};
use common::rewrite_json;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn apply(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn session() -> (DocumentSession, Vec<GeometryId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut ids = Vec::new();
    for x in [0.0, 20.0] {
        let rect = Rect::new(Point::new(x, 0.0), mm(10.0), mm(10.0)).unwrap();
        ids.push(apply(&mut s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0]);
    }
    (s, ids)
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([5; 16]), (*s.snapshot()).clone())
}

fn stored(s: &DocumentSession) -> Vec<u8> {
    encode_container(&project(s)).unwrap()
}

#[test]
fn locks_survive_save_and_reopen() {
    let (mut s, ids) = session();
    apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::SetPathLock {
            geometries: vec![ids[0]],
            lock: PathLock { start: true, direction: false },
        }),
    );
    apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::SetPathLock {
            geometries: vec![ids[1]],
            lock: PathLock { start: true, direction: true },
        }),
    );
    let p = project(&s);
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.process().path_lock(ids[0]), PathLock { start: true, direction: false });
    assert_eq!(back.document.process().path_lock(ids[1]), PathLock { start: true, direction: true });
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    assert_eq!(encode_container(&back).unwrap(), encode_container(&p).unwrap());
}

#[test]
fn the_project_hash_sees_each_lock() {
    let (mut s, ids) = session();
    let mut seen = vec![project_hash(&project(&s)).unwrap()];
    for lock in [
        PathLock { start: true, direction: false },
        PathLock { start: false, direction: true },
        PathLock { start: true, direction: true },
    ] {
        apply(&mut s, CommandPayload::Process(ProcessCommand::SetPathLock { geometries: vec![ids[0]], lock }));
        seen.push(project_hash(&project(&s)).unwrap());
    }
    let mut unique = seen.clone();
    unique.sort_by_key(|h| h.to_string());
    unique.dedup();
    assert_eq!(unique.len(), seen.len(), "every state hashes differently");
}

#[test]
fn documents_without_locks_keep_their_file_form() {
    let (s, _) = session();
    let text = String::from_utf8_lossy(&stored(&s)).into_owned();
    assert!(!text.contains("path_locks"), "default form is unchanged");
}

#[test]
fn a_stored_lock_for_a_missing_shape_or_one_that_locks_nothing_is_refused() {
    let (mut s, ids) = session();
    let lock = PathLock { start: true, direction: false };
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetPathLock { geometries: vec![ids[0]], lock }));

    let missing = rewrite_json(&stored(&s), "document.json", |v| v["path_locks"][0]["geometry"] = 99.into());
    assert!(matches!(decode_container(&missing), Err(LoadError::InvalidValue { .. })), "unknown shape");

    let twice = rewrite_json(&stored(&s), "document.json", |v| {
        let first = v["path_locks"][0].clone();
        v["path_locks"].as_array_mut().unwrap().push(first);
    });
    assert!(matches!(decode_container(&twice), Err(LoadError::InvalidValue { .. })), "locked twice");

    let nothing = rewrite_json(&stored(&s), "document.json", |v| {
        v["path_locks"][0]["start"] = false.into();
    });
    assert!(matches!(decode_container(&nothing), Err(LoadError::InvalidValue { .. })), "an entry that locks nothing");
}
