//! Shared rig for the Edit/Arrange stories (W2-L1-bewerken): a document opened from an in-memory
//! project with a few drawn paths, driven only through `UiIntent`s.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)] // shared test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::{Path, Point};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId, StableId};

pub fn view(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

pub fn opened_with(paths: Vec<Path>) -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    opened_with_kinds(paths.into_iter().map(|p| (p, ObjectKind::Path)).collect())
}

pub fn opened_with_kinds(paths: Vec<(Path, ObjectKind)>) -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    let mut h = HeadlessEditorHarness::new();
    let (doc, ids) = open_kinds_in(&mut h, paths);
    (h, doc, ids)
}

/// Opens a fixture project with these paths in an existing harness (e.g. one with a machine profile).
pub fn open_in(h: &mut HeadlessEditorHarness, paths: Vec<Path>) -> DocumentId {
    open_kinds_in(h, paths.into_iter().map(|p| (p, ObjectKind::Path)).collect()).0
}

fn open_kinds_in(h: &mut HeadlessEditorHarness, paths: Vec<(Path, ObjectKind)>) -> (DocumentId, Vec<GeometryId>) {
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let objects: Vec<_> = paths
        .into_iter()
        .enumerate()
        .map(|(i, (p, kind))| {
            let id = GeometryId::from_raw(i as u64 + 1);
            GeometryObject::new(id, format!("Pad {}", i + 1), kind, GeometrySource::Drawn, p)
        })
        .collect();
    let next = objects.len() as u64 + 1;
    let snapshot = DocumentSnapshot::from_parts(objects, next).expect("snapshot");
    let path = PathBuf::from("/work/fixture.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([7; 16]), snapshot)).expect("fixture");
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    let ids = view(h, doc).objects.iter().map(|o| o.id).collect();
    (doc, ids)
}

pub fn square(x: f64, y: f64, size: f64) -> Path {
    Path::polyline(
        &[Point::new(x, y), Point::new(x + size, y), Point::new(x + size, y + size), Point::new(x, y + size)],
        true,
    )
    .unwrap()
}

pub fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

/// Ids of the objects, in the order the document draws them (back to front).
pub fn draw_order(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    h.scene(doc).expect("scene").items.iter().map(|i| i.source).collect()
}

pub fn invoke(h: &mut HeadlessEditorHarness, doc: DocumentId, key: CommandKey) -> burnburn_presentation::AppUpdate {
    h.invoke(doc, key)
}

/// Corner points of an object's outline in the scene (start and every segment end).
pub fn points_of(h: &HeadlessEditorHarness, doc: DocumentId, id: GeometryId) -> Vec<(f64, f64)> {
    let scene = h.scene(doc).expect("scene");
    let item = scene.items.iter().find(|i| i.source == id).expect("object in scene");
    let mut out = Vec::new();
    for sp in item.path.subpaths() {
        out.push((sp.start().x, sp.start().y));
        out.extend(sp.segments().iter().map(|seg| (seg.end().x, seg.end().y)));
    }
    out
}

pub fn selected(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    let mut ids = view(h, doc).selection;
    ids.sort();
    ids
}

pub fn enabled(h: &HeadlessEditorHarness, key: CommandKey) -> bool {
    h.view().commands.get(&key).is_some_and(|c| c.enabled)
}
