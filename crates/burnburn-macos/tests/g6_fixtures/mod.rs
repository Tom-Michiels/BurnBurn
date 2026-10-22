//! Shared fixtures of the golf-6 editor panel tests (macOS and Windows hosts include this file): a
//! folder of projects for the search panel, built through the public route only.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use burnburn_document::structure::{Plate, StructureCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn edit(s: &mut DocumentSession, payload: CommandPayload) {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap();
}

fn rect(s: &mut DocumentSession, name: &str, x: f64) {
    let r = Rect::new(Point::new(x, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    edit(s, CommandPayload::CreateRectangle { rect: r, name: name.into() });
}

/// /werk holds: deurbord (two objects, a plate of birch plywood, frozen revision 1 "Order 4711" of
/// the design with one object), onderzetters, and a broken file. Returns the first two paths.
pub fn werk(h: &HeadlessEditorHarness) -> (PathBuf, PathBuf) {
    h.fs().create_dir_all_for_test(Path::new("/werk"));
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    rect(&mut s, "Deurbord", 0.0);
    let plate = Plate {
        name: "Plaat 1".into(),
        width: Length::from_mm(600.0).unwrap(),
        height: Length::from_mm(400.0).unwrap(),
        material: "Berken multiplex".into(),
        thickness: None,
    };
    edit(&mut s, CommandPayload::Structure(StructureCommand::CreatePlate { plate }));
    let mut deurbord = Project::new(ProjectUuid::from_bytes(*b"burnburn-search1"), (*s.snapshot()).clone());
    deurbord.freeze_revision("Order 4711").unwrap();
    rect(&mut s, "Extra paneel", 20.0);
    deurbord.document = (*s.snapshot()).clone();
    let deurbord_path = PathBuf::from("/werk/deurbord.burnburn");
    save_project(h.fs(), &deurbord_path, &deurbord).unwrap();
    let mut o = DocumentSession::new(DocumentId::from_raw(2));
    rect(&mut o, "Onderzetter", 0.0);
    let onderzetters = PathBuf::from("/werk/onderzetters.burnburn");
    save_project(
        h.fs(),
        &onderzetters,
        &Project::new(ProjectUuid::from_bytes(*b"burnburn-search2"), (*o.snapshot()).clone()),
    )
    .unwrap();
    h.fs().insert(Path::new("/werk/kapot.burnburn"), b"not a zip".to_vec());
    // 18 June 2026, 14:30:45 UTC.
    h.fs().set_modified_for_test(&deurbord_path, 1_781_793_045);
    (deurbord_path, onderzetters)
}

pub fn open_window(h: &mut HeadlessEditorHarness, path: &Path) -> DocumentId {
    h.send(UiIntent::Open { path: path.to_path_buf() });
    h.run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened")
}

/// A window on deurbord with the search panel open and the folder read.
pub fn searching() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let (deurbord, _) = werk(&h);
    let doc = open_window(&mut h, &deurbord);
    h.invoke(doc, CommandKey::SearchProjects);
    h.run_tasks();
    (h, doc)
}
