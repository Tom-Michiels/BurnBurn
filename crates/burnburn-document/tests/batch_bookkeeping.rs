//! X04: batch commands keep their observable bookkeeping (duplicate-name warnings, the modified
//! list) exactly while being linear in the number of objects.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::{
    CommandOutcome, CommandPayload, CommandWarning, DocumentCommand, DocumentSession, GeometryCommand, ImportItem,
};
use burnburn_geometry::{Affine, Path, Point, Rect, Vector};
use burnburn_types::{ContentHash, DocumentId, GeometryId, Length, RequestId, StableId};

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn item(name: &str, x: f64) -> ImportItem {
    ImportItem { name: name.into(), entity: None, path: Path::line(Point::new(x, 0.0), Point::new(x + 1.0, 0.0)).unwrap() }
}

fn import(s: &mut DocumentSession, names: &[&str]) -> CommandOutcome {
    let items = names.iter().enumerate().map(|(i, n)| item(n, 10.0 * i as f64)).collect();
    let asset = ContentHash::from_bytes([1; 32]);
    run(s, CommandPayload::Edit(GeometryCommand::Import { asset, items }))
}

fn duplicates(out: &CommandOutcome) -> Vec<(String, GeometryId)> {
    out.warnings
        .iter()
        .filter_map(|w| match w {
            CommandWarning::DuplicateName { name, other } => Some((name.clone(), *other)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_import_warns_once_per_repeated_name_naming_the_lowest_existing_object() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let first = import(&mut s, &["a", "x", "b"]);
    assert!(duplicates(&first).is_empty());
    let ids = first.changes.created.clone();

    // "x" exists already (lowest id = ids[1]); "a" repeats inside the batch; "" never warns.
    let second = import(&mut s, &["x", "c", "c", "", "", "a"]);
    let created = &second.changes.created;
    assert_eq!(
        duplicates(&second),
        vec![
            ("x".to_owned(), ids[1]),
            ("c".to_owned(), created[1]), // the second "c" names the first "c" of the batch
            ("a".to_owned(), ids[0]),
        ]
    );
}

#[test]
fn a_name_freed_by_rename_is_not_reported_and_the_lowest_current_holder_wins() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let first = import(&mut s, &["p", "q", "p"]);
    let ids = first.changes.created.clone();
    // Rename the lowest "p" away: the remaining "p" (ids[2]) is now the one a new "p" duplicates.
    run(&mut s, CommandPayload::Rename { target: ids[0], name: "renamed".into() });
    let out = import(&mut s, &["p"]);
    assert_eq!(duplicates(&out), vec![("p".to_owned(), ids[2])]);
    run(&mut s, CommandPayload::Delete { targets: vec![ids[2]] });
    let out = import(&mut s, &["p"]);
    assert_eq!(duplicates(&out), vec![("p".to_owned(), out_first_created_p(&s, ids[2]))]);
}

/// The only object still named "p" besides the new one is the one created by the previous import.
fn out_first_created_p(s: &DocumentSession, deleted: GeometryId) -> GeometryId {
    let snap = s.snapshot();
    snap.objects().find(|(id, o)| o.name() == "p" && *id != deleted).map(|(id, _)| id).unwrap()
}

#[test]
fn transform_lists_each_target_once_in_target_order() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut ids = Vec::new();
    for i in 0..5 {
        let rect = Rect::new(Point::new(10.0 * i as f64, 0.0), Length::from_mm(2.0).unwrap(), Length::from_mm(2.0).unwrap())
            .unwrap();
        ids.push(run(&mut s, CommandPayload::CreateRectangle { rect, name: format!("r{i}") }).changes.created[0]);
    }
    let m = Affine::translate(Vector::new(1.0, 1.0)).unwrap();
    let targets = vec![ids[3], ids[1], ids[3], ids[0], ids[1]];
    let out = run(&mut s, CommandPayload::Transform { targets, transform: m });
    assert_eq!(out.changes.modified, vec![ids[3], ids[1], ids[0]]);
    let snap = s.snapshot();
    // A duplicated target is moved once per listing (the document applies the matrix per entry).
    let x = |id: GeometryId| snap.object(id).unwrap().bounds().unwrap().min().x;
    assert_eq!((x(ids[0]), x(ids[2])), (1.0, 20.0));
}
