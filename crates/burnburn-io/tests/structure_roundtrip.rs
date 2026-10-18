//! IM21 persistence: modifiers, references, relations, parts, plates and edge keys survive save
//! and reopen with the same meaning; files without them are unchanged (C-3: additive schema 2).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::collections::BTreeMap;
use std::path::Path;

use burnburn_document::structure::{
    AssocReference, Dim, EvalMode, Modifier, ModifierChain, ModifierStep, NamedShape, ObjectRelation, PartDefinition,
    PartInstance, PartShape, Plate, StructureChange, StructureCommand,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, GeometryCommand};
use burnburn_geometry::{Affine, BooleanOp, NodeEdit, OffsetJoin, Point, ProductionTolerance, Rect, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, encode_container, load_project, project_hash, save_project};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn st(cmd: StructureCommand) -> CommandPayload {
    CommandPayload::Structure(cmd)
}

fn session() -> DocumentSession {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = |x, w, h| Rect::new(Point::new(x, 0.0), mm(w), mm(h)).unwrap();
    let plate = run(&mut s, CommandPayload::CreateRectangle { rect: rect(0.0, 100.0, 50.0), name: "Plaat".into() })
        .changes
        .created[0];
    let slot = run(&mut s, CommandPayload::CreateRectangle { rect: rect(80.0, 40.0, 20.0), name: "Sleuf".into() })
        .changes
        .created[0];
    // Edge keys differ from the default numbering after this split.
    let split = NodeEdit::InsertNode { subpath: 0, segment: 0, t: 0.25 };
    run(&mut s, CommandPayload::Edit(GeometryCommand::EditNodes { target: plate, edit: split }));
    let chain = ModifierChain {
        name: "Plaat met sleuf".into(),
        source: plate,
        steps: vec![
            ModifierStep { modifier: Modifier::Offset { distance: mm(2.0), join: OffsetJoin::Round }, enabled: false },
            ModifierStep { modifier: Modifier::Boolean { op: BooleanOp::Difference, with: vec![slot] }, enabled: true },
            ModifierStep { modifier: Modifier::LinearArray { count: 2, step: Vector::new(0.0, 60.0) }, enabled: true },
        ],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    };
    let out = run(&mut s, st(StructureCommand::CreateModifier { chain }));
    let result = out.changes.created[0];
    let snap = s.snapshot();
    let at = snap.object(result).unwrap().path().nearest(Point::new(40.0, 50.0)).unwrap().at;
    let top = snap.edge_ref(result, at).unwrap();
    let at = snap.object(plate).unwrap().path().nearest(Point::new(0.0, 25.0)).unwrap().at;
    let left = snap.edge_ref(plate, at).unwrap();
    let mut refs = Vec::new();
    for (name, target) in [("boven", top), ("links", left)] {
        let o =
            run(&mut s, st(StructureCommand::AddReference { reference: AssocReference { name: name.into(), target } }));
        let StructureChange::Reference(r) = o.changes.structure[0] else { panic!() };
        refs.push(r);
    }
    run(&mut s, st(StructureCommand::AddRelation { relation: ObjectRelation::Tangent { a: refs[0], b: refs[1] } }));
    let o = run(
        &mut s,
        st(StructureCommand::CreatePlate {
            plate: Plate {
                name: "Plaat 1".into(),
                width: mm(600.0),
                height: mm(400.0),
                material: "populier".into(),
                thickness: Some(mm(3.0)),
            },
        }),
    );
    let StructureChange::Plate(p1) = o.changes.structure[0] else { panic!() };
    run(&mut s, st(StructureCommand::AssignToPlate { objects: vec![slot], plate: Some(p1) }));
    let mut shapes = BTreeMap::new();
    shapes.insert(
        3,
        NamedShape {
            name: "omtrek".into(),
            shape: PartShape::Rectangle {
                x: Dim::Mm(0.0),
                y: Dim::Mm(0.0),
                width: Dim::Param("breedte".into()),
                height: Dim::Mm(20.0),
            },
        },
    );
    shapes.insert(
        5,
        NamedShape {
            name: "gat".into(),
            shape: PartShape::Circle { x: Dim::Mm(5.0), y: Dim::Mm(5.0), radius: Dim::Mm(2.0) },
        },
    );
    let definition = PartDefinition {
        name: "Beugel".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0))]),
        shapes,
        variants: BTreeMap::from([("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))]))]),
    };
    let o = run(&mut s, st(StructureCommand::DefinePart { definition }));
    let StructureChange::Definition(d) = o.changes.structure[0] else { panic!() };
    let instance = PartInstance {
        definition: d,
        variant: Some("klein".into()),
        overrides: BTreeMap::from([("breedte".to_owned(), mm(30.0))]),
        transform: Affine::translate(Vector::new(200.0, 0.0)).unwrap(),
        plate: Some(p1),
    };
    run(&mut s, st(StructureCommand::PlaceInstance { instance }));
    s
}

#[test]
fn structure_survives_save_and_reopen_with_identical_meaning() {
    let s = session();
    let snap = s.snapshot();
    let project = Project::new(ProjectUuid::from_bytes(*b"burnburn-struct1"), (*snap).clone());
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/structure.burnburn");
    save_project(&fs, path, &project).unwrap();
    let back = load_project(&fs, path).unwrap();
    assert!(back.document.same_content(&snap), "objects, structure, records and provenance equal");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&project).unwrap());
    for (id, _) in snap.structure().references() {
        assert_eq!(back.document.reference_status(id), snap.reference_status(id));
        assert!(back.document.reference_status(id).unwrap().is_bound());
    }
    for (id, _) in snap.structure().relations() {
        assert_eq!(back.document.relation_state(id), snap.relation_state(id));
    }
    for (id, _) in snap.objects() {
        assert_eq!(back.document.production_blocked(id), snap.production_blocked(id));
        assert_eq!(back.document.object(id).unwrap().edge_keys(), snap.object(id).unwrap().edge_keys());
    }
    // Re-encoding the reopened project gives the same bytes.
    assert_eq!(encode_container(&back).unwrap(), encode_container(&project).unwrap());
    // A reopened session keeps evaluating: editing the source moves the chain output.
    let mut s2 = DocumentSession::from_snapshot(DocumentId::from_raw(2), back.document);
    let plate = snap.objects().find(|(_, o)| o.name() == "Plaat").unwrap().0;
    let mv = Affine::translate(Vector::new(1.0, 0.0)).unwrap();
    let out = run(&mut s2, CommandPayload::Transform { targets: vec![plate], transform: mv });
    assert_eq!(out.changes.modified.len(), 2, "source and chain output");
}

#[test]
fn structure_changes_the_project_hash_and_is_absent_from_plain_files() {
    let s = session();
    let with = Project::new(ProjectUuid::from_bytes(*b"burnburn-struct1"), (*s.snapshot()).clone());
    let mut plain = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    run(&mut plain, CommandPayload::CreateRectangle { rect, name: "A".into() });
    let plain = Project::new(ProjectUuid::from_bytes(*b"burnburn-struct1"), (*plain.snapshot()).clone());
    let doc = |p: &Project| {
        let bytes = encode_container(p).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("document.json").unwrap(), &mut out).unwrap();
        out
    };
    let plain_json = doc(&plain);
    assert!(!plain_json.contains("structure") && !plain_json.contains("edge_keys"), "{plain_json}");
    let with_json = doc(&with);
    assert!(with_json.contains("\"structure\"") && with_json.contains("\"edge_keys\""));

    // Renaming a plate is a meaning change.
    let mut s2 = session();
    let (p, plate) = s2.snapshot().structure().plates().map(|(i, p)| (i, p.clone())).next().unwrap();
    run(&mut s2, st(StructureCommand::SetPlate { plate: p, value: Plate { name: "Anders".into(), ..plate } }));
    let renamed = Project::new(with.uuid, (*s2.snapshot()).clone());
    assert_ne!(project_hash(&renamed).unwrap(), project_hash(&with).unwrap());
}
