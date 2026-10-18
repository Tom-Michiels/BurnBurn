//! Public contract: recovery copies are separate from Save; power loss keeps the synced project.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::Path;

use burnburn_geometry::{Affine, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, discard_recovery, load_project, read_recovery, save_project, write_recovery};
use common::sample_project;

fn first_min_x(p: &Project) -> f64 {
    p.document.objects().next().unwrap().1.bounds().unwrap().min().x
}

fn shifted(dx: f64) -> Project {
    let p = sample_project();
    let objs = p
        .document
        .objects()
        .map(|(_, o)| {
            let m = Affine::translate(Vector::new(dx, 0.0)).unwrap();
            burnburn_document::GeometryObject::new(
                o.id(),
                o.name().into(),
                o.kind(),
                o.source().clone(),
                o.path().transform(&m),
            )
        })
        .collect();
    Project { document: burnburn_document::DocumentSnapshot::from_parts(objs, p.document.next_id()).unwrap(), ..p }
}

#[test]
fn recovery_is_separate_from_explicit_save() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/work"));
    fs.create_dir_all_for_test(Path::new("/support/Recovery"));
    let saved = Path::new("/work/a.burnburn");
    let recovery_dir = Path::new("/support/Recovery");
    save_project(&fs, saved, &shifted(0.0)).unwrap();

    let edited = shifted(5.0);
    let rpath = write_recovery(&fs, recovery_dir, &edited).unwrap();
    assert!(rpath.starts_with(recovery_dir));
    assert_eq!(first_min_x(&load_project(&fs, saved).unwrap()), 10.0, "the saved file is untouched");

    // Read back as a fresh process would: only the filesystem state is shared.
    let recovered = read_recovery(&fs, recovery_dir, edited.uuid).unwrap().unwrap();
    assert_eq!(first_min_x(&recovered), 15.0);

    discard_recovery(&fs, recovery_dir, edited.uuid).unwrap();
    assert!(read_recovery(&fs, recovery_dir, edited.uuid).unwrap().is_none());
    assert_eq!(first_min_x(&load_project(&fs, saved).unwrap()), 10.0);
}

#[test]
fn power_loss_after_save_keeps_the_synced_project() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/work"));
    let path = Path::new("/work/a.burnburn");
    save_project(&fs, path, &shifted(0.0)).unwrap();
    fs.simulate_power_loss();
    assert_eq!(first_min_x(&load_project(&fs, path).unwrap()), 10.0);
    save_project(&fs, path, &shifted(5.0)).unwrap();
    fs.simulate_power_loss();
    assert_eq!(first_min_x(&load_project(&fs, path).unwrap()), 15.0, "data was synced before the replace");
}
