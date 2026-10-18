//! Public contract: atomic save keeps the last readable project when a step fails.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::path::Path;

use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::fs::FsOp;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, SaveError, load_project, save_project};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn project_with_width(w: f64) -> Project {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::ORIGIN, Length::from_mm(w).unwrap(), Length::from_mm(80.0).unwrap()).unwrap();
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::CreateRectangle { rect, name: "Plaat".into() },
    };
    s.apply(cmd).unwrap();
    Project::new(ProjectUuid::from_bytes([7; 16]), (*s.snapshot()).clone())
}

fn width_on_disk(fs: &MemoryFileSystem, path: &Path) -> f64 {
    let p = load_project(fs, path).unwrap();
    let (_, obj) = p.document.objects().next().unwrap();
    obj.bounds().unwrap().width()
}

#[test]
fn failed_replace_keeps_last_readable_project() {
    let path = Path::new("/work/plaat.burnburn");
    for failing in [FsOp::Write, FsOp::SyncFile, FsOp::Replace] {
        let fs = MemoryFileSystem::new();
        fs.create_dir_all_for_test(Path::new("/work"));
        save_project(&fs, path, &project_with_width(120.0)).unwrap();
        assert_eq!(width_on_disk(&fs, path), 120.0);

        fs.fail_on(failing, 0);
        assert!(save_project(&fs, path, &project_with_width(150.0)).is_err(), "{failing:?} must fail the save");
        assert_eq!(width_on_disk(&fs, path), 120.0, "after a failed {failing:?} the previous project is readable");
        assert_eq!(fs.files(), vec![path.to_path_buf()], "no temp file left behind after {failing:?}");

        save_project(&fs, path, &project_with_width(150.0)).unwrap();
        assert_eq!(width_on_disk(&fs, path), 150.0);
    }
}

#[test]
fn failed_dir_sync_reports_unconfirmed_save_with_new_content_in_place() {
    let path = Path::new("/work/plaat.burnburn");
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/work"));
    save_project(&fs, path, &project_with_width(120.0)).unwrap();
    fs.fail_on(FsOp::SyncDir, 0);
    let err = save_project(&fs, path, &project_with_width(150.0)).unwrap_err();
    assert!(matches!(err, SaveError::Unconfirmed(_)), "{err:?}");
    assert_eq!(width_on_disk(&fs, path), 150.0, "the replace happened; only durability is unconfirmed");
}
