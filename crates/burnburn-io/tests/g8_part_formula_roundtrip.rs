//! G8-editor S07: a formula in a part shape survives save and reopen, and a file without formulas
//! is the file it always was (the project hash does not move when no formula is used).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::collections::BTreeMap;
use std::path::Path;

use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartInstance, PartShape, StructureChange, StructureCommand,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Affine, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, encode_container, load_project, project_hash, save_project};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn session(width: Dim) -> DocumentSession {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut shapes = BTreeMap::new();
    shapes.insert(
        1,
        NamedShape {
            name: "omtrek".into(),
            shape: PartShape::Rectangle {
                x: Dim::Mm(0.0),
                y: Dim::Mm(0.0),
                width,
                height: Dim::Param("hoogte".into()),
            },
        },
    );
    let definition = PartDefinition {
        name: "Plaatje".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0)), ("hoogte".to_owned(), mm(20.0))]),
        shapes,
        variants: BTreeMap::new(),
    };
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
        transform: Affine::translate(Vector::new(10.0, 0.0)).unwrap(),
        plate: None,
    };
    run(&mut s, CommandPayload::Structure(StructureCommand::PlaceInstance { instance }));
    s
}

fn project(s: &DocumentSession) -> Project {
    Project::new(ProjectUuid::from_bytes(*b"burnburn-formula"), (*s.snapshot()).clone())
}

#[test]
fn a_formula_survives_save_and_reopen_and_still_evaluates() {
    let s = session(Dim::Expr("breedte / 2 + 3".into()));
    let project = project(&s);
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/formule.burnburn");
    save_project(&fs, path, &project).unwrap();
    let back = load_project(&fs, path).unwrap();
    assert!(back.document.same_content(&s.snapshot()), "the definition and its outputs are equal");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&project).unwrap());
    assert_eq!(encode_container(&back).unwrap(), encode_container(&project).unwrap());
    let (_, def) = back.document.structure().definitions().next().unwrap();
    assert!(matches!(
        &def.shapes[&1].shape,
        PartShape::Rectangle { width: Dim::Expr(text), .. } if text == "breedte / 2 + 3"
    ));
    // The reopened document keeps evaluating through the formula: the outline is 23 mm wide.
    let (_, outline) =
        back.document.objects().find(|(_, o)| o.name() == "Plaatje/omtrek").expect("the instance's outline");
    assert!((outline.bounds().unwrap().width() - 23.0).abs() < 1e-9);
}

#[test]
fn a_part_without_a_formula_has_the_same_file_as_before() {
    // The same part with the width as a plain number or a parameter: no `expr` anywhere, and the
    // hash differs from the one with a formula only because of the formula.
    let plain = project(&session(Dim::Param("breedte".into())));
    let formula = project(&session(Dim::Expr("breedte".into())));
    assert_ne!(project_hash(&plain).unwrap(), project_hash(&formula).unwrap());
    let bytes = String::from_utf8_lossy(&encode_container(&plain).unwrap()).into_owned();
    assert!(!bytes.contains("\"expr\""), "no trace of formulas in a file that uses none");
}
