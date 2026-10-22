//! Public contract (IM09): new object kinds and nested groups survive save and reopen; group
//! ids are validated like every other id.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::path::Path;

use burnburn_document::{DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::Point;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{
    AssetStore, LoadError, Project, ProjectUuid, decode_container, encode_container, load_project, save_project,
};
use burnburn_types::{GeometryId, GroupId, StableId};

fn project(objects: Vec<GeometryObject>, next_id: u64) -> Project {
    Project {
        uuid: ProjectUuid::parse("0b0b0b0b-0001-4000-8000-000000000999").unwrap(),
        document: DocumentSnapshot::from_parts(objects, next_id).unwrap(),
        assets: AssetStore::default(),
        profiles: Vec::new(),
        revisions: Vec::new(),
        library_links: Vec::new(),
        layer_slots: Vec::new(),
    }
}

#[test]
fn shape_kinds_and_nested_groups_roundtrip() {
    let circle = burnburn_geometry::Path::circle(Point::new(5.0, 5.0), 2.0).unwrap();
    let (g_outer, g_inner) = (GroupId::from_raw(4), GroupId::from_raw(5));
    let objects = vec![
        GeometryObject::new(
            GeometryId::from_raw(1),
            "c".into(),
            ObjectKind::Circle,
            GeometrySource::Drawn,
            circle.clone(),
        )
        .with_groups(vec![g_outer, g_inner]),
        GeometryObject::new(GeometryId::from_raw(2), "s".into(), ObjectKind::Star, GeometrySource::Drawn, circle)
            .with_groups(vec![g_outer]),
    ];
    let original = project(objects, 6);
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    save_project(&fs, Path::new("/p/g.burnburn"), &original).unwrap();
    let back = load_project(&fs, Path::new("/p/g.burnburn")).unwrap();
    let a: Vec<_> = original.document.objects().map(|(_, o)| o.clone()).collect();
    let b: Vec<_> = back.document.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(b, a);
    assert_eq!(back.document.object(GeometryId::from_raw(1)).unwrap().groups(), &[g_outer, g_inner]);
}

#[test]
fn group_ids_must_not_reuse_or_exceed_the_id_space() {
    let line = burnburn_geometry::Path::line(Point::ORIGIN, Point::new(1.0, 0.0)).unwrap();
    for bad in [GroupId::from_raw(9), GroupId::from_raw(1)] {
        let obj = GeometryObject::new(
            GeometryId::from_raw(1),
            "l".into(),
            ObjectKind::Line,
            GeometrySource::Drawn,
            line.clone(),
        )
        .with_groups(vec![bad]);
        let bytes = encode_container(&project(vec![obj], 3)).unwrap();
        let err = decode_container(&bytes).unwrap_err();
        assert!(
            matches!(&err, LoadError::InvalidValue { path, .. } if path.starts_with("objects[0].groups")),
            "{err:?}"
        );
    }
}
