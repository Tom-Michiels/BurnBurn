//! G9-editor S07: fixed geometry with a parametric box survives save and reopen.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::collections::BTreeMap;
use std::path::Path as FsPath;

use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartInstance, PartShape, StructureChange, StructureCommand,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Affine, Path, Point, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, encode_container, load_project, project_hash, save_project};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

#[test]
fn a_fitted_outline_survives_save_and_reopen_and_still_evaluates() {
    let triangle =
        Path::polyline(&[Point::new(2.0, 3.0), Point::new(22.0, 3.0), Point::new(12.0, 13.0)], true).unwrap();
    let shape = PartShape::Fitted {
        path: triangle.clone(),
        x: Dim::Param("px".into()),
        y: Dim::Mm(5.0),
        width: Dim::Expr("max(breedte / 2, 4)".into()),
        height: Dim::Mm(10.0),
    };
    let definition = PartDefinition {
        name: "Driehoek".into(),
        parameters: BTreeMap::from([
            ("breedte".to_owned(), Length::from_mm(20.0).unwrap()),
            ("px".to_owned(), Length::from_mm(10.0).unwrap()),
        ]),
        shapes: BTreeMap::from([(1, NamedShape { name: "vorm".into(), shape })]),
        variants: BTreeMap::new(),
    };
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let run = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let out = run(&mut s, CommandPayload::Structure(StructureCommand::DefinePart { definition }));
    let StructureChange::Definition(d) = out.changes.structure[0] else { panic!() };
    let instance = PartInstance {
        definition: d,
        variant: None,
        overrides: BTreeMap::new(),
        transform: Affine::translate(Vector::new(0.0, 0.0)).unwrap(),
        plate: None,
    };
    run(&mut s, CommandPayload::Structure(StructureCommand::PlaceInstance { instance }));

    let project = Project::new(ProjectUuid::from_bytes(*b"burnburn-fitted!"), (*s.snapshot()).clone());
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(FsPath::new("/p"));
    let path = FsPath::new("/p/passend.burnburn");
    save_project(&fs, path, &project).unwrap();
    let back = load_project(&fs, path).unwrap();
    assert!(back.document.same_content(&s.snapshot()), "the definition and its output are equal");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&project).unwrap());
    assert_eq!(encode_container(&back).unwrap(), encode_container(&project).unwrap());
    let (_, def) = back.document.structure().definitions().next().unwrap();
    let PartShape::Fitted { path: p, width: Dim::Expr(text), x: Dim::Param(px), .. } = &def.shapes[&1].shape else {
        panic!("{:?}", def.shapes[&1].shape)
    };
    assert_eq!((text.as_str(), px.as_str()), ("max(breedte / 2, 4)", "px"));
    assert_eq!(p, &triangle);
}
