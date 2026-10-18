//! C-7 / DR-24: `FileSystem::append` for the history journal, and `list_dir` for project search.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::path::Path;

use burnburn_io::fs::{FileSystem, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::std_fs::StdFileSystem;

fn append_contract(fs: &dyn FileSystem, dir: &Path) {
    let path = dir.join("journal.log");
    assert_eq!(fs.append(&path, b"abc").unwrap(), 3, "creates the file");
    assert_eq!(fs.append(&path, b"de").unwrap(), 5, "returns the new length");
    fs.sync_file(&path).unwrap();
    assert_eq!(fs.read(&path, 100).unwrap(), b"abcde");
}

#[test]
fn append_extends_file_and_sync_makes_it_durable() {
    let mem = MemoryFileSystem::new();
    mem.create_dir_all_for_test(Path::new("/h"));
    append_contract(&mem, Path::new("/h"));
    mem.append(Path::new("/h/journal.log"), b"fg").unwrap();
    mem.simulate_power_loss();
    assert_eq!(mem.read(Path::new("/h/journal.log"), 100).unwrap(), b"abcde", "unsynced tail is lost");

    let tmp = tempfile::tempdir().unwrap();
    append_contract(&StdFileSystem, tmp.path());
}

#[test]
fn torn_append_keeps_prefix_only() {
    let mem = MemoryFileSystem::new();
    mem.create_dir_all_for_test(Path::new("/h"));
    let path = Path::new("/h/journal.log");
    mem.append(path, b"rec1;").unwrap();
    mem.tear_next_append(2);
    let err = mem.append(path, b"rec2;").unwrap_err();
    assert_eq!(err.op, FsOp::Append);
    assert_eq!(mem.read(path, 100).unwrap(), b"rec1;re", "k of n bytes reached the file");
}

#[test]
fn list_dir_lists_files_directly_inside_sorted() {
    let mem = MemoryFileSystem::new();
    mem.insert(Path::new("/p/b.burnburn"), b"x".to_vec());
    mem.insert(Path::new("/p/a.burnburn"), b"x".to_vec());
    mem.insert(Path::new("/p/sub/c.burnburn"), b"x".to_vec());
    assert_eq!(
        mem.list_dir(Path::new("/p")).unwrap(),
        vec![Path::new("/p/a.burnburn").to_path_buf(), Path::new("/p/b.burnburn").to_path_buf()]
    );
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("z.burnburn"), b"x").unwrap();
    std::fs::write(tmp.path().join("y.txt"), b"x").unwrap();
    std::fs::create_dir(tmp.path().join("d")).unwrap();
    assert_eq!(
        StdFileSystem.list_dir(tmp.path()).unwrap(),
        vec![tmp.path().join("y.txt"), tmp.path().join("z.burnburn")]
    );
}
