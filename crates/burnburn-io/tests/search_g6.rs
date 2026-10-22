//! P08 (G6-editor): the project index carries what a search panel shows next to a hit (the time
//! the file was last saved and a bounded outline for the preview) and indexing only ever reads.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::fs::FsOp;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::search::{MAX_OUTLINE_OBJECTS, index_projects};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn project(objects: usize) -> Project {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    for i in 0..objects {
        let rect =
            Rect::new(Point::new(i as f64 * 2.0, 0.0), Length::from_mm(1.0).unwrap(), Length::from_mm(1.0).unwrap())
                .unwrap();
        let cmd = DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(i as u64 + 1),
            payload: CommandPayload::CreateRectangle { rect, name: format!("Blokje {i}") },
        };
        s.apply(cmd).unwrap();
    }
    Project::new(ProjectUuid::from_bytes(*b"burnburn-search3"), (*s.snapshot()).clone())
}

#[test]
fn an_entry_knows_when_its_file_was_last_saved() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/werk"));
    save_project(&fs, Path::new("/werk/a.burnburn"), &project(1)).unwrap();
    save_project(&fs, Path::new("/werk/b.burnburn"), &project(1)).unwrap();
    fs.set_modified_for_test(Path::new("/werk/a.burnburn"), 1_781_751_845);
    let index = index_projects(&fs, Path::new("/werk")).unwrap();
    let time = |name: &str| index.entries().iter().find(|e| e.file_name == name).unwrap().modified;
    assert_eq!(time("a.burnburn"), Some(1_781_751_845));
    assert_eq!(time("b.burnburn"), None, "a file system that does not say stays unknown, never invented");
}

#[test]
fn the_outline_for_the_preview_is_bounded() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/werk"));
    save_project(&fs, Path::new("/werk/klein.burnburn"), &project(3)).unwrap();
    save_project(&fs, Path::new("/werk/groot.burnburn"), &project(MAX_OUTLINE_OBJECTS + 20)).unwrap();
    let index = index_projects(&fs, Path::new("/werk")).unwrap();
    let outline = |name: &str| index.entries().iter().find(|e| e.file_name == name).unwrap().outline.len();
    assert_eq!(outline("klein.burnburn"), 3);
    assert_eq!(outline("groot.burnburn"), MAX_OUTLINE_OBJECTS, "a big project is previewed by its first objects");
}

#[test]
fn indexing_only_reads() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/werk"));
    save_project(&fs, Path::new("/werk/a.burnburn"), &project(2)).unwrap();
    let before = fs.log().len();
    let _ = index_projects(&fs, Path::new("/werk")).unwrap();
    let ops: Vec<FsOp> = fs.log()[before..].iter().map(|(op, _)| *op).collect();
    assert!(!ops.is_empty());
    assert!(ops.iter().all(|op| matches!(op, FsOp::Read | FsOp::List | FsOp::Exists)), "{ops:?}");
}
