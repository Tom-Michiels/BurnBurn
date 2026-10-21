//! More rig helpers for the acceptance scenarios: open/save through the real tasks, bounds from
//! the scene and a structural fingerprint of a document.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)] // shared test helpers

use std::path::Path as FsPath;

use burnburn_presentation::{HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_types::{DocumentId, GeometryId};

use super::{points_of, view};

/// Opens an existing project file in `h` and returns its document.
pub fn open_file(h: &mut HeadlessEditorHarness, path: &FsPath) -> DocumentId {
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

/// Saves the document to `path` (through the real save task) and returns the bytes written.
pub fn save_bytes(h: &mut HeadlessEditorHarness, doc: DocumentId, path: &FsPath) -> Vec<u8> {
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    h.send(UiIntent::Save { document: doc, path: Some(path.to_path_buf()) });
    h.run_tasks();
    burnburn_io::fs::FileSystem::read(h.fs(), path, u64::MAX).expect("saved")
}

/// Bounds `(min_x, min_y, max_x, max_y)` of one object's outline in the scene, in mm.
pub fn bounds_of(h: &HeadlessEditorHarness, doc: DocumentId, id: GeometryId) -> (f64, f64, f64, f64) {
    let pts = points_of(h, doc, id);
    let xs = pts.iter().map(|p| p.0);
    let ys = pts.iter().map(|p| p.1);
    (
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    )
}

/// Everything a user can see of the document structure: per object its label, group depth, flags
/// and outline (rounded to the micrometre so a save/open roundtrip compares equal).
pub fn fingerprint(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<String> {
    let vm = view(h, doc);
    let scene = h.scene(doc).expect("scene");
    vm.objects
        .iter()
        .map(|o| {
            let outline: Vec<String> = if scene.items.iter().any(|i| i.source == o.id) {
                points_of(h, doc, o.id).iter().map(|p| format!("{:.3},{:.3}", p.0, p.1)).collect()
            } else {
                Vec::new()
            };
            format!("{:?}|{}|g{}|l{}|v{}|{}", o.id, o.label, o.groups.len(), o.locked, o.visible, outline.join(";"))
        })
        .collect()
}
