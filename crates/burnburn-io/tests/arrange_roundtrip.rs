//! W2-L1-bewerken: draw order and locks survive save and reopen, count in the project hash and
//! are validated like every other id. Projects without arrangement stay byte-compatible.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::Path;

use burnburn_document::{ArrangeCommand, CommandPayload, DocumentCommand, DocumentSession, OrderChange};
use burnburn_geometry::{Point, Rect};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{
    LoadError, Project, ProjectUuid, decode_container, encode_container, load_project, project_hash, save_project,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};
use common::rewrite_json;

fn arranged_session() -> (DocumentSession, Vec<GeometryId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut run = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let ids: Vec<GeometryId> = (0..3)
        .map(|i| {
            let rect = Rect::new(
                Point::new(f64::from(i) * 10.0, 0.0),
                Length::from_mm(5.0).unwrap(),
                Length::from_mm(5.0).unwrap(),
            )
            .unwrap();
            run(&mut s, CommandPayload::CreateRectangle { rect, name: format!("r{i}") }).changes.created[0]
        })
        .collect();
    run(&mut s, CommandPayload::Arrange(ArrangeCommand::Order { targets: vec![ids[0]], change: OrderChange::ToFront }));
    run(&mut s, CommandPayload::Arrange(ArrangeCommand::SetLocked { targets: vec![ids[1]], locked: true }));
    (s, ids)
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes([7; 16]), (*s.snapshot()).clone())
}

#[test]
fn draw_order_and_locks_roundtrip_through_save_and_load() {
    let (s, ids) = arranged_session();
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    save_project(&fs, Path::new("/p/a.burnburn"), &project(&s)).unwrap();
    let back = load_project(&fs, Path::new("/p/a.burnburn")).unwrap();
    assert_eq!(back.document.draw_order(), vec![ids[1], ids[2], ids[0]]);
    assert!(back.document.is_locked(ids[1]) && !back.document.is_locked(ids[0]));
}

#[test]
fn the_project_hash_covers_draw_order_and_locks_only_when_present() {
    let (s, ids) = arranged_session();
    let plain = Project::new(
        ProjectUuid::from_bytes([7; 16]),
        (*s.snapshot()).clone().with_arrange(burnburn_document::ArrangeLayer::default()).unwrap(),
    );
    assert_ne!(project_hash(&project(&s)).unwrap(), project_hash(&plain).unwrap());
    let reordered = (*s.snapshot())
        .clone()
        .with_arrange(burnburn_document::ArrangeLayer::from_parts(vec![ids[2], ids[1], ids[0]], vec![ids[1]]).unwrap())
        .unwrap();
    assert_ne!(
        project_hash(&project(&s)).unwrap(),
        project_hash(&Project::new(ProjectUuid::from_bytes([7; 16]), reordered)).unwrap()
    );
}

#[test]
fn a_file_without_arrangement_has_no_arrange_block() {
    let project = common::sample_project();
    let bytes = encode_container(&project).unwrap();
    let doc = rewrite_json(&bytes, "document.json", |v| {
        assert!(v.get("arrange").is_none(), "no arrangement, no block");
    });
    assert!(decode_container(&doc).is_ok());
}

#[test]
fn a_stored_arrangement_that_names_unknown_objects_or_repeats_one_is_refused_with_its_path() {
    let (s, ids) = arranged_session();
    let bytes = encode_container(&project(&s)).unwrap();
    for (what, edit) in [
        ("unknown order", serde_json::json!({ "order": [ids[0].raw(), 999], "locked": [] })),
        ("duplicate order", serde_json::json!({ "order": [ids[0].raw(), ids[0].raw()], "locked": [] })),
        ("unknown lock", serde_json::json!({ "order": [], "locked": [999] })),
    ] {
        let broken = rewrite_json(&bytes, "document.json", |v| v["arrange"] = edit.clone());
        let err = decode_container(&broken).unwrap_err();
        assert!(matches!(&err, LoadError::InvalidValue { path, .. } if path.starts_with("arrange")), "{what}: {err:?}");
    }
}
