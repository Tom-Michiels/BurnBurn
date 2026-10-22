//! G5-UI-gaten, onderdelen, varianten, platen en referenties (S06, S07, P06) in de UI: het
//! structuurpaneel toont wat het document weet, en varianten/lokale overrides worden via de
//! publieke route gewijzigd (één undo-stap, het onderdeel zelf blijft ongemoeid).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers

use std::collections::BTreeMap;
use std::path::PathBuf;

use burnburn_document::structure::{
    AssocReference, Dim, GeometryRef, NamedShape, PartDefinition, PartInstance, PartShape, Plate, StructureChange,
    StructureCommand,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{Affine, Rect, Vector};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, InstanceStatus, MessageKey, ReferenceStatus,
    StructureEdit, UiIntent,
};
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

/// "Beugel": `breedte x 20`, variants "klein" (25) and "kapot" (-5, invalid on purpose).
fn bracket() -> PartDefinition {
    let mut shapes = BTreeMap::new();
    shapes.insert(
        1,
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
    let mut variants = BTreeMap::new();
    variants.insert("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))]));
    variants.insert("kapot".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(-5.0))]));
    PartDefinition {
        name: "Beugel".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0))]),
        shapes,
        variants,
    }
}

/// A saved project with the part, three instances (klein on plate A, kapot on plate A, plain),
/// a plate and a reference to an object that was deleted afterwards.
fn opened() -> (HeadlessEditorHarness, DocumentId, [u64; 3]) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let out = run(&mut s, st(StructureCommand::DefinePart { definition: bracket() }));
    let StructureChange::Definition(def) = out.changes.structure[0] else { panic!() };
    let out = run(
        &mut s,
        st(StructureCommand::CreatePlate {
            plate: Plate {
                name: "Plaat A".into(),
                width: mm(600.0),
                height: mm(400.0),
                material: "Berken".into(),
                thickness: Some(mm(3.0)),
            },
        }),
    );
    let StructureChange::Plate(plate) = out.changes.structure[0] else { panic!() };
    let place = |s: &mut DocumentSession, x: f64, variant: Option<&str>, plate| {
        let instance = PartInstance {
            definition: def,
            variant: variant.map(str::to_owned),
            overrides: BTreeMap::new(),
            transform: Affine::translate(Vector::new(x, 0.0)).unwrap(),
            plate,
        };
        let out = run(s, st(StructureCommand::PlaceInstance { instance }));
        let StructureChange::Instance(i) = out.changes.structure[0] else { panic!() };
        burnburn_types::StableId::raw(i)
    };
    let klein = place(&mut s, 0.0, Some("klein"), Some(plate));
    let kapot = place(&mut s, 100.0, Some("kapot"), Some(plate));
    let plain = place(&mut s, 200.0, None, None);

    let rect = Rect::new(burnburn_geometry::Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let object = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).changes.created[0];
    run(
        &mut s,
        st(StructureCommand::AddReference {
            reference: AssocReference { name: "Rand".into(), target: GeometryRef { object, sub: None } },
        }),
    );
    run(&mut s, CommandPayload::Delete { targets: vec![object] });

    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/onderdelen.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())).unwrap();
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    (h, doc, [klein, kapot, plain])
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn row(h: &HeadlessEditorHarness, doc: DocumentId, instance: u64) -> burnburn_presentation::InstanceRow {
    vm(h, doc).structure.parts[0].instances.iter().find(|i| i.id == instance).cloned().expect("instance row")
}

/// S07: the panel lists the part with its parameters and variants, and every instance with its
/// variant, its effective values and its own status: the invalid variant fails alone.
#[test]
fn the_panel_lists_parts_with_their_variants_and_each_instance_fails_alone() {
    let (h, doc, [klein, kapot, plain]) = opened();
    let parts = vm(&h, doc).structure.parts;
    assert_eq!(parts.len(), 1);
    let part = &parts[0];
    assert_eq!(part.name, "Beugel");
    assert_eq!(part.parameters.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect::<Vec<_>>(), [("breedte", "40")]);
    assert_eq!(part.variants, ["kapot", "klein"]);
    assert_eq!(part.instances.len(), 3);

    let klein = row(&h, doc, klein);
    assert_eq!((klein.variant.as_deref(), klein.status.clone()), (Some("klein"), InstanceStatus::Valid));
    assert_eq!(klein.effective[0].value, "25", "the variant decides the effective value");
    let kapot = row(&h, doc, kapot);
    assert_eq!(kapot.status, InstanceStatus::Failed { parameter: Some("breedte".into()) }, "fails alone");
    assert_eq!(row(&h, doc, plain).status, InstanceStatus::Valid);
}

/// Choosing another variant is one undo step; an invalid one fails that instance only and keeps
/// the others; an unknown variant is refused.
#[test]
fn choosing_a_variant_is_one_undo_step_and_an_unknown_one_is_refused() {
    let (mut h, doc, [klein, _, plain]) = opened();
    let before = vm(&h, doc).revision;
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetVariant { instance: plain, variant: Some("klein".into()) },
    });
    let now = vm(&h, doc);
    assert_eq!(now.undo, Some(UndoLabel::Structure(burnburn_document::structure::StructureLabel::EditInstance)));
    assert_eq!(row(&h, doc, plain).effective[0].value, "25");
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetVariant { instance: klein, variant: Some("kapot".into()) },
    });
    assert!(matches!(row(&h, doc, klein).status, InstanceStatus::Failed { .. }));
    assert_eq!(row(&h, doc, plain).status, InstanceStatus::Valid, "the others are unaffected");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(row(&h, doc, klein).status, InstanceStatus::Valid);

    let revision = vm(&h, doc).revision;
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetVariant { instance: plain, variant: Some("bestaat niet".into()) },
    });
    assert_eq!(vm(&h, doc).revision, revision, "an unknown variant changes nothing");
    assert_eq!(vm(&h, doc).problem.map(|p| p.message), Some(MessageKey::CommandRejected));
    h.send(UiIntent::Structure { document: doc, edit: StructureEdit::SetVariant { instance: plain, variant: None } });
    assert_eq!(row(&h, doc, plain).variant, None, "back to the default values");
    let _ = before;
}

/// A local override is visible on its instance, wins over the variant and is never written into
/// the definition; an empty text removes it; bad input is a field error without a command.
#[test]
fn a_local_override_is_visible_and_never_written_into_the_definition() {
    let (mut h, doc, [klein, _, plain]) = opened();
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetOverride { instance: klein, parameter: "breedte".into(), text: "80 mm".into() },
    });
    let r = row(&h, doc, klein);
    assert_eq!(r.overrides.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect::<Vec<_>>(), [("breedte", "80")]);
    assert_eq!(r.effective[0].value, "80", "the override wins over the variant (25)");
    assert_eq!(vm(&h, doc).structure.parts[0].parameters[0].value, "40", "the definition keeps its value");
    assert_eq!(row(&h, doc, plain).overrides.len(), 0, "other instances are untouched");

    let revision = vm(&h, doc).revision;
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetOverride { instance: klein, parameter: "breedte".into(), text: "veel".into() },
    });
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetOverride { instance: klein, parameter: "hoogte".into(), text: "5".into() },
    });
    assert_eq!(vm(&h, doc).revision, revision, "bad input and unknown parameters change nothing");

    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetOverride { instance: klein, parameter: "breedte".into(), text: String::new() },
    });
    let r = row(&h, doc, klein);
    assert!(r.overrides.is_empty());
    assert_eq!(r.effective[0].value, "25", "the variant applies again");
}

/// P06: a plate shows its own size and material and the instances on it.
#[test]
fn a_plate_shows_its_size_material_and_instances() {
    let (h, doc, _) = opened();
    let plates = vm(&h, doc).structure.plates;
    assert_eq!(plates.len(), 1);
    let p = &plates[0];
    assert_eq!((p.name.as_str(), p.width.as_str(), p.height.as_str(), p.material.as_str()), ("Plaat A", "600", "400", "Berken"));
    assert_eq!(p.thickness.as_deref(), Some("3"));
    assert_eq!(p.instances, 2, "klein and kapot are on this plate; the third is not");
}

/// S06: a reference whose object was deleted shows a visible broken link; nothing rebinds.
#[test]
fn a_reference_to_a_deleted_object_shows_a_broken_link() {
    let (h, doc, _) = opened();
    let refs = vm(&h, doc).structure.references;
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].name, "Rand");
    assert_eq!(refs[0].status, ReferenceStatus::Broken { reason: burnburn_presentation::BrokenReason::ObjectMissing });
}
