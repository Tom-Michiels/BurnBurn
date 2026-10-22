//! G6-werk, S06 and S07: defining parts from drawn objects, placing instances, parameters and
//! variants, and making associative references to an edge of the selected object. Every edit goes
//! through the public intents and is one undo step; typed values that do not fit change nothing.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use std::collections::BTreeMap;
use std::path::PathBuf;

use burnburn_document::structure::{
    AssocReference, GeometryRef, ModifierChain, StructureChange, StructureCommand, SubRef,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, ObjectKind};
use burnburn_geometry::{Path, Point, Rect, Shape};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    BrokenReason, CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, InstanceStatus, ReferenceStatus,
    StructureEdit, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};
use edit_rig::{opened_with_kinds, select, square};

type Harness = HeadlessEditorHarness;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn circle_path(cx: f64, cy: f64, r: f64) -> Path {
    Shape::Circle { center: Point::new(cx, cy), radius: mm(r) }.to_path().unwrap()
}

/// A 30 x 20 plate at (100, 50) with a hole of radius 5 at (125, 60).
fn plate_with_hole() -> (Harness, DocumentId, Vec<GeometryId>) {
    let rect = Path::rectangle(Rect::new(Point::new(100.0, 50.0), mm(30.0), mm(20.0)).unwrap());
    opened_with_kinds(vec![(rect, ObjectKind::Rectangle), (circle_path(125.0, 60.0, 5.0), ObjectKind::Circle)])
}

fn vm(h: &Harness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn edit(h: &mut Harness, doc: DocumentId, edit: StructureEdit) {
    h.send(UiIntent::Structure { document: doc, edit });
}

fn define(h: &mut Harness, doc: DocumentId, ids: &[GeometryId], name: &str) {
    select(h, doc, ids);
    edit(h, doc, StructureEdit::DefinePart { name: name.to_owned() });
}

fn object_count(h: &Harness, doc: DocumentId) -> usize {
    vm(h, doc).objects.len()
}

fn bounds(h: &Harness, doc: DocumentId, id: GeometryId) -> burnburn_geometry::Bounds {
    h.scene(doc).unwrap().items.iter().find(|i| i.source == id).and_then(|i| i.path.bounds()).expect("in scene")
}

/// The ids of the objects the instances made, new since `known`.
fn new_objects(h: &Harness, doc: DocumentId, known: &[GeometryId]) -> Vec<GeometryId> {
    vm(h, doc).objects.iter().map(|o| o.id).filter(|id| !known.contains(id)).collect()
}

#[test]
fn a_part_is_defined_from_the_selection_with_its_sizes_as_parameters_and_the_objects_stay() {
    let (mut h, doc, ids) = plate_with_hole();
    assert_eq!(vm(&h, doc).structure.selected, 0);
    select(&mut h, doc, &ids);
    assert_eq!(vm(&h, doc).structure.selected, 2);
    edit(&mut h, doc, StructureEdit::DefinePart { name: "Beugel".to_owned() });
    let parts = vm(&h, doc).structure.parts;
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].name, "Beugel");
    let params: Vec<(&str, &str)> = parts[0].parameters.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect();
    assert!(params.contains(&("breedte", "30")) && params.contains(&("hoogte", "20")), "{params:?}");
    assert!(params.contains(&("straal", "5")), "{params:?}");
    assert!(parts[0].instances.is_empty(), "defining places nothing");
    assert_eq!(object_count(&h, doc), 2, "the drawn objects stay where they are");

    h.invoke(doc, CommandKey::Undo);
    assert!(vm(&h, doc).structure.parts.is_empty(), "one undo step takes the definition away");
}

#[test]
fn a_definition_needs_a_selection_and_a_name_and_says_so() {
    let (mut h, doc, ids) = plate_with_hole();
    edit(&mut h, doc, StructureEdit::DefinePart { name: "Beugel".to_owned() });
    assert!(vm(&h, doc).structure.parts.is_empty(), "nothing selected");
    assert!(vm(&h, doc).problem.is_some());
    select(&mut h, doc, &ids);
    edit(&mut h, doc, StructureEdit::DefinePart { name: "   ".to_owned() });
    assert!(vm(&h, doc).structure.parts.is_empty(), "no name");
    assert_eq!(vm(&h, doc).undo, None, "nothing was changed");
}

#[test]
fn an_instance_lays_the_shapes_at_the_typed_position_and_every_instance_follows_a_changed_parameter() {
    let (mut h, doc, ids) = plate_with_hole();
    define(&mut h, doc, &ids, "Beugel");
    let def = vm(&h, doc).structure.parts[0].id;
    edit(&mut h, doc, StructureEdit::PlaceInstance { definition: def, x: "10".into(), y: "5".into() });
    edit(&mut h, doc, StructureEdit::PlaceInstance { definition: def, x: "200".into(), y: "5".into() });
    let part = vm(&h, doc).structure.parts[0].clone();
    assert_eq!(part.instances.len(), 2);
    assert!(part.instances.iter().all(|i| i.status == InstanceStatus::Valid));
    let made = new_objects(&h, doc, &ids);
    assert_eq!(made.len(), 4, "two shapes per instance");
    // The first shape of the first instance is the plate: 30 x 20 with its corner at (10, 5).
    let first = bounds(&h, doc, made[0]);
    assert!((first.min().x - 10.0).abs() < 1e-6 && (first.min().y - 5.0).abs() < 1e-6, "{first:?}");
    assert!((first.width() - 30.0).abs() < 1e-6 && (first.height() - 20.0).abs() < 1e-6);

    // One instance gets its own width; the definition changes for the others.
    let second = part.instances[1].id;
    edit(&mut h, doc, StructureEdit::SetOverride { instance: second, parameter: "breedte".into(), text: "25".into() });
    edit(&mut h, doc, StructureEdit::SetParameter { definition: def, parameter: "breedte".into(), text: "50".into() });
    let widths: Vec<f64> = made
        .iter()
        .filter(|id| (bounds(&h, doc, **id).height() - 20.0).abs() < 1e-6)
        .map(|id| bounds(&h, doc, *id).width())
        .collect();
    assert_eq!(widths.len(), 2, "the two plates");
    assert!(
        widths.contains(&50.0) && widths.contains(&25.0),
        "the definition reaches one, the override stays on the other: {widths:?}"
    );
    let shown = &vm(&h, doc).structure.parts[0];
    assert_eq!(shown.parameters.iter().find(|p| p.name == "breedte").unwrap().value, "50");
    assert_eq!(shown.instances[1].overrides.len(), 1, "the override is visible and not written into the definition");
}

#[test]
fn a_variant_is_named_gets_its_values_and_is_chosen_per_instance() {
    let (mut h, doc, ids) = plate_with_hole();
    define(&mut h, doc, &ids, "Beugel");
    let def = vm(&h, doc).structure.parts[0].id;
    edit(&mut h, doc, StructureEdit::PlaceInstance { definition: def, x: "0".into(), y: "0".into() });
    edit(&mut h, doc, StructureEdit::AddVariant { definition: def, name: "groot".into() });
    assert_eq!(vm(&h, doc).structure.parts[0].variants, ["groot"]);
    edit(
        &mut h,
        doc,
        StructureEdit::SetVariantValue {
            definition: def,
            variant: "groot".into(),
            parameter: "breedte".into(),
            text: "60".into(),
        },
    );
    let variants = &vm(&h, doc).structure.parts[0].variant_values;
    assert_eq!(variants.len(), 1);
    assert_eq!(
        (variants[0].name.as_str(), variants[0].values[0].name.as_str(), variants[0].values[0].value.as_str()),
        ("groot", "breedte", "60")
    );
    let instance = vm(&h, doc).structure.parts[0].instances[0].id;
    edit(&mut h, doc, StructureEdit::SetVariant { instance, variant: Some("groot".into()) });
    let made = new_objects(&h, doc, &ids);
    let plate_width = bounds(&h, doc, made[0]).width();
    assert!((plate_width - 60.0).abs() < 1e-6, "the variant's width: {plate_width}");

    // Names and values that do not fit change nothing.
    let before = vm(&h, doc).structure.parts[0].clone();
    for e in [
        StructureEdit::AddVariant { definition: def, name: "groot".into() },
        StructureEdit::AddVariant { definition: def, name: " ".into() },
        StructureEdit::AddVariant { definition: 999, name: "x".into() },
        StructureEdit::SetVariantValue {
            definition: def,
            variant: "groot".into(),
            parameter: "breedte".into(),
            text: "abc".into(),
        },
        StructureEdit::SetVariantValue {
            definition: def,
            variant: "groot".into(),
            parameter: "breedte".into(),
            text: "-3".into(),
        },
        StructureEdit::SetVariantValue {
            definition: def,
            variant: "bestaat niet".into(),
            parameter: "breedte".into(),
            text: "3".into(),
        },
        StructureEdit::SetVariantValue {
            definition: def,
            variant: "groot".into(),
            parameter: "diepte".into(),
            text: "3".into(),
        },
        StructureEdit::SetParameter { definition: def, parameter: "breedte".into(), text: "0".into() },
        StructureEdit::SetParameter { definition: def, parameter: "nee".into(), text: "3".into() },
    ] {
        edit(&mut h, doc, e.clone());
        assert_eq!(vm(&h, doc).structure.parts[0], before, "{e:?}");
    }
    // An empty value takes the variant's own value away again.
    edit(
        &mut h,
        doc,
        StructureEdit::SetVariantValue {
            definition: def,
            variant: "groot".into(),
            parameter: "breedte".into(),
            text: String::new(),
        },
    );
    assert!(vm(&h, doc).structure.parts[0].variant_values[0].values.is_empty());
}

#[test]
fn an_instance_goes_by_itself_and_a_part_only_when_nothing_uses_it() {
    let (mut h, doc, ids) = plate_with_hole();
    define(&mut h, doc, &ids, "Beugel");
    let def = vm(&h, doc).structure.parts[0].id;
    edit(&mut h, doc, StructureEdit::PlaceInstance { definition: def, x: "0".into(), y: "0".into() });
    assert_eq!(object_count(&h, doc), 4);
    edit(&mut h, doc, StructureEdit::RemovePart { definition: def });
    assert_eq!(vm(&h, doc).structure.parts.len(), 1, "an instance still uses it");
    assert!(vm(&h, doc).problem.is_some());
    let instance = vm(&h, doc).structure.parts[0].instances[0].id;
    edit(&mut h, doc, StructureEdit::RemoveInstance { instance });
    assert_eq!(object_count(&h, doc), 2, "its shapes go with it");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(object_count(&h, doc), 4, "one undo step brings the instance back");
    h.invoke(doc, CommandKey::Redo);
    edit(&mut h, doc, StructureEdit::RemovePart { definition: def });
    assert!(vm(&h, doc).structure.parts.is_empty());
    edit(&mut h, doc, StructureEdit::RemoveInstance { instance: 4242 });
    assert!(vm(&h, doc).problem.is_some(), "an instance that is not there");
}

// ---------------------------------------------------------------------------------------------
// S06: references
// ---------------------------------------------------------------------------------------------

#[test]
fn the_edges_of_the_single_selected_object_are_listed_and_one_becomes_a_named_reference() {
    let (mut h, doc, ids) = plate_with_hole();
    assert!(vm(&h, doc).structure.edges.is_empty(), "nothing selected");
    select(&mut h, doc, &ids);
    assert!(vm(&h, doc).structure.edges.is_empty(), "two objects selected: no edge to choose");
    select(&mut h, doc, &ids[..1]);
    let s = vm(&h, doc).structure;
    assert_eq!(s.edge_object, Some(ids[0].raw()));
    let edges: Vec<(usize, usize, &str)> = s.edges.iter().map(|e| (e.subpath, e.segment, e.length.as_str())).collect();
    assert_eq!(edges.len(), 4, "a rectangle has four edges");
    assert!(edges.contains(&(0, 0, "30")) && edges.contains(&(0, 1, "20")), "{edges:?}");

    edit(
        &mut h,
        doc,
        StructureEdit::AddReference { name: "rechts".into(), object: ids[0].raw(), subpath: 0, segment: 1 },
    );
    let refs = vm(&h, doc).structure.references;
    assert_eq!(refs.len(), 1);
    assert_eq!(
        (refs[0].name.as_str(), refs[0].source.as_deref(), refs[0].status),
        ("rechts", Some("Pad 1"), ReferenceStatus::Bound)
    );
    assert!(refs[0].candidates.is_empty());

    // Not an edge, not an object, not twice the same name for nothing: refused without a change.
    let undo = vm(&h, doc).undo;
    edit(&mut h, doc, StructureEdit::AddReference { name: "x".into(), object: ids[0].raw(), subpath: 0, segment: 9 });
    edit(&mut h, doc, StructureEdit::AddReference { name: "x".into(), object: 999, subpath: 0, segment: 0 });
    assert_eq!(vm(&h, doc).structure.references.len(), 1);
    assert_eq!(vm(&h, doc).undo, undo);
    assert!(vm(&h, doc).problem.is_some());

    edit(&mut h, doc, StructureEdit::RemoveReference { reference: refs[0].id });
    assert!(vm(&h, doc).structure.references.is_empty());
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&h, doc).structure.references.len(), 1, "one undo step");
}

#[test]
fn deleting_the_referenced_object_leaves_a_visible_broken_link() {
    let (mut h, doc, ids) = plate_with_hole();
    select(&mut h, doc, &ids[..1]);
    edit(
        &mut h,
        doc,
        StructureEdit::AddReference { name: "rechts".into(), object: ids[0].raw(), subpath: 0, segment: 1 },
    );
    h.invoke(doc, CommandKey::Delete);
    let refs = vm(&h, doc).structure.references;
    assert_eq!(refs.len(), 1, "the reference is not removed silently");
    assert_eq!(refs[0].status, ReferenceStatus::Broken { reason: BrokenReason::ObjectMissing });
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

/// A plate minus a slot; a reference names the plate's bottom edge (one edge, bound). Moving the
/// slot across that edge cuts it in two: the reference is ambiguous with two candidates.
fn ambiguous_reference() -> (Harness, DocumentId, u64) {
    use burnburn_document::structure::{Modifier, ModifierStep};
    use burnburn_geometry::{Affine, BooleanOp, Vector};
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = |s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64, name: &str| {
        let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
        run(s, CommandPayload::CreateRectangle { rect, name: name.into() }).changes.created[0]
    };
    let plate = rect(&mut s, 0.0, 0.0, 100.0, 50.0, "Plaat");
    let slot = rect(&mut s, 40.0, 100.0, 20.0, 10.0, "Sleuf");
    let chain = ModifierChain {
        name: "Resultaat".into(),
        source: plate,
        steps: vec![ModifierStep {
            modifier: Modifier::Boolean { op: BooleanOp::Difference, with: vec![slot] },
            enabled: true,
        }],
        tolerance: burnburn_geometry::ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: burnburn_document::structure::EvalMode::Inline,
    };
    let out = run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain }));
    let result = out.changes.created[0];
    let snap = s.snapshot();
    let at = snap.object(result).unwrap().path().nearest(Point::new(10.0, 0.0)).unwrap().at;
    let target: GeometryRef = snap.edge_ref(result, at).unwrap();
    assert!(matches!(target.sub, Some(SubRef::Edge(_))));
    let out = run(
        &mut s,
        CommandPayload::Structure(StructureCommand::AddReference {
            reference: AssocReference { name: "onder".into(), target },
        }),
    );
    let StructureChange::Reference(r) = out.changes.structure[0] else { panic!() };
    run(
        &mut s,
        CommandPayload::Transform {
            targets: vec![slot],
            transform: Affine::translate(Vector::new(0.0, -105.0)).unwrap(),
        },
    );
    let mut h = Harness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/dubbel.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([4; 16]), (*s.snapshot()).clone())).unwrap();
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
    (h, doc, r.raw())
}

#[test]
fn an_ambiguous_reference_lists_its_candidates_and_only_the_operators_choice_rebinds_it() {
    let (mut h, doc, reference) = ambiguous_reference();
    let row = vm(&h, doc).structure.references[0].clone();
    assert_eq!(row.status, ReferenceStatus::Ambiguous { candidates: 2 });
    assert_eq!(row.candidates.len(), 2, "{:?}", row.candidates);
    assert_ne!(row.candidates[0], row.candidates[1]);

    edit(&mut h, doc, StructureEdit::RebindReference { reference, candidate: 7 });
    assert_eq!(
        vm(&h, doc).structure.references[0].status,
        ReferenceStatus::Ambiguous { candidates: 2 },
        "no such candidate"
    );
    edit(&mut h, doc, StructureEdit::RebindReference { reference, candidate: 1 });
    let row = vm(&h, doc).structure.references[0].clone();
    assert_eq!(row.status, ReferenceStatus::Bound);
    assert!(row.candidates.is_empty());
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(
        vm(&h, doc).structure.references[0].status,
        ReferenceStatus::Ambiguous { candidates: 2 },
        "one undo step"
    );

    // A reference that is bound is not rebound by this route.
    edit(&mut h, doc, StructureEdit::RebindReference { reference, candidate: 1 });
    edit(&mut h, doc, StructureEdit::RebindReference { reference, candidate: 0 });
    assert_eq!(vm(&h, doc).structure.references[0].status, ReferenceStatus::Bound);
    let _ = BTreeMap::<u8, u8>::new();
    let _ = square(0.0, 0.0, 1.0);
}
