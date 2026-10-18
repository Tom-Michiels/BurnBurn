//! Public contract: a saved project reopens with the same meaning.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::Path;

use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{load_project, save_project};
use common::sample_project;

#[test]
fn project_roundtrip_preserves_geometry_and_operations() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/sample.burnburn");
    let original = sample_project();
    save_project(&fs, path, &original).unwrap();
    let back = load_project(&fs, path).unwrap();

    assert_eq!(back.uuid, original.uuid);
    assert_eq!(back.document.next_id(), original.document.next_id());
    let a: Vec<_> = original.document.objects().map(|(_, o)| o.clone()).collect();
    let b: Vec<_> = back.document.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(b, a, "ids, names, kinds, sources and bit-identical geometry");
    assert_eq!(back.document.process(), original.document.process(), "one geometry, two operations, no copy");
    assert_eq!(back.assets, original.assets);
}
