//! G7-editor S07/P06 in the window: plates are made, edited, removed and given objects and part
//! instances; a variant is renamed or removed; the shapes of a part are listed, added, removed and
//! edited and every instance follows. Each action is one undo step; a refused one changes nothing
//! and says why.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers

use std::collections::BTreeMap;
use std::path::PathBuf;

use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartInstance, PartShape, Plate, StructureChange, StructureCommand, StructureLabel,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{Affine, Point, Rect, Vector};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, FieldError, HeadlessEditorHarness, HostEffect, MessageKey, PartShapeKind,
    PlateField, SelectMode, ShapeField, StructureEdit, StructureProblem, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

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

/// "Beugel": a rectangle `breedte x 20` and a hole r 3 at (10, 10); variants "klein" and "groot".
fn bracket() -> PartDefinition {
    let rect = PartShape::Rectangle {
        x: Dim::Mm(0.0),
        y: Dim::Mm(0.0),
        width: Dim::Param("breedte".into()),
        height: Dim::Mm(20.0),
    };
    let hole = PartShape::Circle { x: Dim::Mm(10.0), y: Dim::Mm(10.0), radius: Dim::Mm(3.0) };
    PartDefinition {
        name: "Beugel".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0))]),
        shapes: BTreeMap::from([
            (1, NamedShape { name: "omtrek".into(), shape: rect }),
            (2, NamedShape { name: "gat".into(), shape: hole }),
        ]),
        variants: BTreeMap::from([
            ("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))])),
            ("groot".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(60.0))])),
        ]),
    }
}

struct World {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    part: u64,
    /// Two instances: the first uses "klein", the second no variant.
    instances: [u64; 2],
    /// A plain rectangle, not part of anything.
    plain: GeometryId,
}

fn world() -> World {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let out = run(&mut s, st(StructureCommand::DefinePart { definition: bracket() }));
    let StructureChange::Definition(def) = out.changes.structure[0] else { panic!() };
    let mut instances = [0; 2];
    for (n, variant) in [Some("klein"), None].into_iter().enumerate() {
        let instance = PartInstance {
            definition: def,
            variant: variant.map(str::to_owned),
            overrides: BTreeMap::new(),
            transform: Affine::translate(Vector::new(100.0 * n as f64, 0.0)).unwrap(),
            plate: None,
        };
        let out = run(&mut s, st(StructureCommand::PlaceInstance { instance }));
        let StructureChange::Instance(i) = out.changes.structure[0] else { panic!() };
        instances[n] = i.raw();
    }
    let rect = Rect::new(Point::new(0.0, 100.0), mm(10.0), mm(10.0)).unwrap();
    let plain = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Los".into() }).changes.created[0];
    run(
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

    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/platen.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([8; 16]), (*s.snapshot()).clone())).unwrap();
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
    World { h, doc, part: def.raw(), instances, plain }
}

impl World {
    fn vm(&self) -> DocumentViewModel {
        self.h.document(self.doc).expect("document view")
    }
    fn edit(&mut self, edit: StructureEdit) {
        self.h.send(UiIntent::Structure { document: self.doc, edit });
    }
    fn undo(&mut self) {
        self.h.invoke(self.doc, CommandKey::Undo);
    }
    fn label(&self) -> Option<UndoLabel> {
        self.vm().undo
    }
    fn problem(&self) -> Option<MessageKey> {
        self.vm().problem.map(|p| p.message)
    }
    fn select(&mut self, ids: Vec<GeometryId>) {
        self.h.send(UiIntent::Select { document: self.doc, ids, mode: SelectMode::Replace });
    }
    fn plate_id(&self, name: &str) -> u64 {
        self.vm().structure.plates.iter().find(|p| p.name == name).map(|p| p.id).expect("plate")
    }
    fn objects(&self) -> usize {
        self.vm().objects.len()
    }
}

fn label(l: StructureLabel) -> Option<UndoLabel> {
    Some(UndoLabel::Structure(l))
}

// --- Plates (P06) ----------------------------------------------------------------------------

#[test]
fn a_plate_is_made_from_typed_text_in_one_undo_step() {
    let mut w = world();
    w.edit(StructureEdit::CreatePlate {
        name: "Plaat B".into(),
        width: "600".into(),
        height: "0,4 m".into(),
        material: "Acryl".into(),
        thickness: "3 mm".into(),
    });
    let plates = w.vm().structure.plates;
    assert_eq!(plates.len(), 2);
    let b = &plates[1];
    assert_eq!(
        (b.name.as_str(), b.width.as_str(), b.height.as_str(), b.material.as_str()),
        ("Plaat B", "600", "400", "Acryl")
    );
    assert_eq!(b.thickness.as_deref(), Some("3"));
    assert_eq!(w.label(), label(StructureLabel::CreatePlate));
    w.undo();
    assert_eq!(w.vm().structure.plates.len(), 1);
}

#[test]
fn a_plate_without_thickness_is_allowed_and_bad_input_is_refused_without_a_change() {
    let mut w = world();
    let make = |name: &str, width: &str, height: &str| StructureEdit::CreatePlate {
        name: name.into(),
        width: width.into(),
        height: height.into(),
        material: String::new(),
        thickness: String::new(),
    };
    w.edit(make("Zonder dikte", "100", "100"));
    assert_eq!(w.vm().structure.plates[1].thickness, None);

    let undo = w.label();
    for (edit, expected) in [
        (make("  ", "100", "100"), MessageKey::Structure(StructureProblem::NeedsName)),
        (make("X", "0", "100"), MessageKey::InvalidLength(FieldError::NotPositive)),
        (make("X", "100", "-5"), MessageKey::InvalidLength(FieldError::NotPositive)),
    ] {
        w.edit(edit);
        assert_eq!(w.problem(), Some(expected));
    }
    w.edit(make("X", "breed", "100"));
    assert!(matches!(w.problem(), Some(MessageKey::InvalidLength(_))));
    w.edit(StructureEdit::CreatePlate {
        name: "X".into(),
        width: "100".into(),
        height: "100".into(),
        material: String::new(),
        thickness: "-1".into(),
    });
    assert_eq!(w.problem(), Some(MessageKey::InvalidLength(FieldError::NotPositive)));
    assert_eq!((w.label(), w.vm().structure.plates.len()), (undo, 2));
}

#[test]
fn every_field_of_a_plate_is_edited_in_one_undo_step() {
    let mut w = world();
    let plate = w.plate_id("Plaat A");
    for (field, text) in [
        (PlateField::Width, "500"),
        (PlateField::Height, "300 mm"),
        (PlateField::Material, "MDF"),
        (PlateField::Thickness, "6"),
        (PlateField::Name, "Hoofdplaat"),
    ] {
        w.edit(StructureEdit::SetPlateField { plate, field, text: text.into() });
        assert_eq!(w.label(), label(StructureLabel::EditPlate), "{field:?}");
    }
    let p = w.vm().structure.plates[0].clone();
    assert_eq!(
        (p.name.as_str(), p.width.as_str(), p.height.as_str(), p.material.as_str(), p.thickness.as_deref()),
        ("Hoofdplaat", "500", "300", "MDF", Some("6"))
    );
    w.edit(StructureEdit::SetPlateField { plate, field: PlateField::Thickness, text: String::new() });
    assert_eq!(w.vm().structure.plates[0].thickness, None, "empty takes the thickness away");
    w.undo();
    assert_eq!(w.vm().structure.plates[0].thickness.as_deref(), Some("6"), "one field, one step");
}

#[test]
fn a_bad_plate_field_or_plate_is_refused_and_changes_nothing() {
    let mut w = world();
    let plate = w.plate_id("Plaat A");
    let undo = w.label();
    for (field, text) in
        [(PlateField::Width, "0"), (PlateField::Height, "-3"), (PlateField::Name, " "), (PlateField::Thickness, "0")]
    {
        w.edit(StructureEdit::SetPlateField { plate, field, text: text.into() });
        assert!(w.problem().is_some(), "{field:?} {text:?}");
        assert_eq!(w.label(), undo);
    }
    w.edit(StructureEdit::SetPlateField { plate: 999, field: PlateField::Name, text: "x".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownPlate)));
    w.edit(StructureEdit::RemovePlate { plate: 999 });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownPlate)));
}

#[test]
fn selected_objects_and_instances_are_assigned_to_a_plate_and_taken_off_again() {
    let mut w = world();
    let plate = w.plate_id("Plaat A");
    assert_eq!((w.vm().structure.plates[0].objects, w.vm().structure.plates[0].instances), (0, 0));

    w.select(vec![w.plain]);
    w.edit(StructureEdit::AssignSelectionToPlate { plate: Some(plate) });
    assert_eq!(w.vm().structure.plates[0].objects, 1);
    assert_eq!(w.label(), label(StructureLabel::AssignToPlate));

    w.edit(StructureEdit::SetInstancePlate { instance: w.instances[0], plate: Some(plate) });
    let p = w.vm().structure.plates[0].clone();
    assert_eq!((p.objects, p.instances), (1, 1));
    assert_eq!(w.vm().structure.parts[0].instances[0].plate.as_deref(), Some("Plaat A"));
    assert_eq!(w.label(), label(StructureLabel::EditInstance));

    w.edit(StructureEdit::AssignSelectionToPlate { plate: None });
    assert_eq!(w.vm().structure.plates[0].objects, 0, "off the plate again");
    w.edit(StructureEdit::SetInstancePlate { instance: w.instances[0], plate: None });
    assert_eq!(w.vm().structure.plates[0].instances, 0);
    w.undo();
    assert_eq!(w.vm().structure.plates[0].instances, 1);
}

#[test]
fn assigning_needs_a_selection_a_known_plate_and_no_part_output() {
    let mut w = world();
    let plate = w.plate_id("Plaat A");
    let undo = w.label();
    w.select(Vec::new());
    w.edit(StructureEdit::AssignSelectionToPlate { plate: Some(plate) });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NeedsSelection)));
    w.select(vec![w.plain]);
    w.edit(StructureEdit::AssignSelectionToPlate { plate: Some(999) });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownPlate)));
    // A part's outline is not assigned by itself: its instance goes.
    let output = w.vm().objects.iter().map(|o| o.id).find(|id| *id != w.plain).expect("a part output");
    w.select(vec![output]);
    w.edit(StructureEdit::AssignSelectionToPlate { plate: Some(plate) });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::PartOutputSelected)));
    w.edit(StructureEdit::SetInstancePlate { instance: 999, plate: Some(plate) });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownInstance)));
    w.edit(StructureEdit::SetInstancePlate { instance: w.instances[0], plate: Some(999) });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownPlate)));
    assert_eq!(w.label(), undo);
}

#[test]
fn removing_a_plate_unassigns_what_was_on_it_and_one_undo_brings_it_back() {
    let mut w = world();
    let plate = w.plate_id("Plaat A");
    w.select(vec![w.plain]);
    w.edit(StructureEdit::AssignSelectionToPlate { plate: Some(plate) });
    w.edit(StructureEdit::SetInstancePlate { instance: w.instances[1], plate: Some(plate) });
    w.edit(StructureEdit::RemovePlate { plate });
    assert!(w.vm().structure.plates.is_empty());
    assert_eq!(w.vm().structure.parts[0].instances[1].plate, None);
    assert_eq!(w.label(), label(StructureLabel::RemovePlate));
    w.undo();
    let p = w.vm().structure.plates[0].clone();
    assert_eq!((p.objects, p.instances), (1, 1));
}

// --- Variants (S07) ----------------------------------------------------------------------------

#[test]
fn a_variant_is_renamed_with_its_instances_in_one_undo_step() {
    let mut w = world();
    w.edit(StructureEdit::RenameVariant { definition: w.part, from: "klein".into(), to: "smal".into() });
    let part = w.vm().structure.parts[0].clone();
    assert_eq!(part.variants, ["groot", "smal"]);
    assert_eq!(part.instances[0].variant.as_deref(), Some("smal"));
    assert_eq!(part.instances[0].effective[0].value, "25", "it still draws the same variant");
    assert_eq!(w.label(), label(StructureLabel::RenameVariant));
    w.undo();
    assert_eq!(w.vm().structure.parts[0].instances[0].variant.as_deref(), Some("klein"));
}

#[test]
fn a_rename_that_clashes_or_names_nothing_is_refused() {
    let mut w = world();
    let undo = w.label();
    w.edit(StructureEdit::RenameVariant { definition: w.part, from: "klein".into(), to: "groot".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::VariantExists)));
    w.edit(StructureEdit::RenameVariant { definition: w.part, from: "klein".into(), to: "  ".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NeedsName)));
    w.edit(StructureEdit::RenameVariant { definition: w.part, from: "weg".into(), to: "x".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownVariant)));
    w.edit(StructureEdit::RenameVariant { definition: 999, from: "klein".into(), to: "x".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownPart)));
    assert_eq!(w.label(), undo);
}

#[test]
fn an_unused_variant_is_removed_and_a_used_one_is_refused() {
    let mut w = world();
    w.edit(StructureEdit::RemoveVariant { definition: w.part, name: "groot".into() });
    assert_eq!(w.vm().structure.parts[0].variants, ["klein"]);
    assert_eq!(w.label(), label(StructureLabel::EditPart));
    w.undo();
    assert_eq!(w.vm().structure.parts[0].variants, ["groot", "klein"]);

    let undo = w.label();
    w.edit(StructureEdit::RemoveVariant { definition: w.part, name: "klein".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::VariantInUse)));
    w.edit(StructureEdit::RemoveVariant { definition: w.part, name: "weg".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownVariant)));
    assert_eq!(w.label(), undo);
    // Once the instance chose another variant, the old one can go.
    w.edit(StructureEdit::SetVariant { instance: w.instances[0], variant: None });
    w.edit(StructureEdit::RemoveVariant { definition: w.part, name: "klein".into() });
    assert_eq!(w.vm().structure.parts[0].variants, ["groot"]);
}

// --- The shapes of a part (S07) ------------------------------------------------------------------

fn fields(w: &World, shape: u32) -> Vec<(ShapeField, String)> {
    w.vm().structure.parts[0].shapes.iter().find(|s| s.key == shape).expect("shape").fields.clone()
}

#[test]
fn the_panel_lists_the_shapes_of_a_part_with_literals_and_parameter_names() {
    let w = world();
    let shapes = w.vm().structure.parts[0].shapes.clone();
    assert_eq!(shapes.len(), 2);
    assert_eq!((shapes[0].key, shapes[0].name.as_str(), shapes[0].kind), (1, "omtrek", PartShapeKind::Rectangle));
    assert_eq!(
        shapes[0].fields,
        [
            (ShapeField::X, "0".to_owned()),
            (ShapeField::Y, "0".to_owned()),
            (ShapeField::Width, "breedte".to_owned()),
            (ShapeField::Height, "20".to_owned()),
        ]
    );
    assert_eq!((shapes[1].name.as_str(), shapes[1].kind), ("gat", PartShapeKind::Circle));
    assert_eq!(
        shapes[1].fields,
        [(ShapeField::X, "10".to_owned()), (ShapeField::Y, "10".to_owned()), (ShapeField::Radius, "3".to_owned())]
    );
}

#[test]
fn a_shape_number_changes_every_instance_in_one_undo_step() {
    let mut w = world();
    let widest = |w: &World| {
        let scene = w.h.scene(w.doc).unwrap();
        scene.items.iter().filter_map(|i| i.path.bounds()).map(|b| b.width()).fold(0.0_f64, f64::max)
    };
    assert_eq!(widest(&w), 40.0);
    w.edit(StructureEdit::SetPartShapeField {
        definition: w.part,
        shape: 1,
        field: ShapeField::Width,
        text: "55 mm".into(),
    });
    assert_eq!(widest(&w), 55.0, "the plain instance follows the literal");
    assert_eq!(fields(&w, 1)[2], (ShapeField::Width, "55".to_owned()));
    assert_eq!(w.label(), label(StructureLabel::EditPart));
    // The parameter's name brings it back to the parameter.
    w.edit(StructureEdit::SetPartShapeField {
        definition: w.part,
        shape: 1,
        field: ShapeField::Width,
        text: "breedte".into(),
    });
    assert_eq!(fields(&w, 1)[2], (ShapeField::Width, "breedte".to_owned()));
    assert_eq!(widest(&w), 40.0);
    w.undo();
    assert_eq!(widest(&w), 55.0);

    w.edit(StructureEdit::SetPartShapeField {
        definition: w.part,
        shape: 2,
        field: ShapeField::Radius,
        text: "4".into(),
    });
    assert_eq!(fields(&w, 2)[2], (ShapeField::Radius, "4".to_owned()));
    w.edit(StructureEdit::SetPartShapeField { definition: w.part, shape: 2, field: ShapeField::X, text: "-2".into() });
    assert_eq!(fields(&w, 2)[0], (ShapeField::X, "-2".to_owned()), "a position may be negative");
}

#[test]
fn a_shape_number_that_does_not_fit_is_refused_and_changes_nothing() {
    let mut w = world();
    let undo = w.label();
    let part = w.part;
    let set =
        |field, text: &str| StructureEdit::SetPartShapeField { definition: part, shape: 1, field, text: text.into() };
    w.edit(set(ShapeField::Width, "0"));
    assert_eq!(w.problem(), Some(MessageKey::InvalidLength(FieldError::NotPositive)));
    w.edit(set(ShapeField::Height, "-4"));
    assert_eq!(w.problem(), Some(MessageKey::InvalidLength(FieldError::NotPositive)));
    w.edit(set(ShapeField::Radius, "3"));
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NoSuchField)));
    w.edit(set(ShapeField::Width, "acht"));
    assert!(matches!(w.problem(), Some(MessageKey::InvalidLength(_))));
    w.edit(StructureEdit::SetPartShapeField { definition: w.part, shape: 99, field: ShapeField::X, text: "1".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownShape)));
    assert_eq!(w.label(), undo);
}

#[test]
fn a_shape_is_added_to_and_removed_from_every_instance_in_one_undo_step() {
    let mut w = world();
    let before = w.objects();
    w.edit(StructureEdit::AddPartShape { definition: w.part, kind: PartShapeKind::Circle, name: "gat 2".into() });
    assert_eq!(w.objects(), before + 2, "one new outline per instance");
    let shapes = w.vm().structure.parts[0].shapes.clone();
    assert_eq!((shapes[2].key, shapes[2].name.as_str(), shapes[2].kind), (3, "gat 2", PartShapeKind::Circle));
    assert_eq!(w.label(), label(StructureLabel::EditPart));

    w.edit(StructureEdit::AddPartShape { definition: w.part, kind: PartShapeKind::Rectangle, name: "plaat".into() });
    assert_eq!(w.objects(), before + 4);
    w.edit(StructureEdit::RemovePartShape { definition: w.part, shape: 3 });
    assert_eq!(w.objects(), before + 2);
    assert_eq!(w.vm().structure.parts[0].shapes.iter().map(|s| s.key).collect::<Vec<_>>(), [1, 2, 4]);
    w.undo();
    assert_eq!(w.objects(), before + 4);
}

#[test]
fn renaming_a_shape_keeps_its_numbers_and_empty_names_or_unknown_shapes_are_refused() {
    let mut w = world();
    w.edit(StructureEdit::RenamePartShape { definition: w.part, shape: 2, name: "boorgat".into() });
    assert_eq!(w.vm().structure.parts[0].shapes[1].name, "boorgat");
    assert_eq!(fields(&w, 2)[2], (ShapeField::Radius, "3".to_owned()));
    let undo = w.label();
    w.edit(StructureEdit::RenamePartShape { definition: w.part, shape: 2, name: " ".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NeedsName)));
    w.edit(StructureEdit::RenamePartShape { definition: w.part, shape: 9, name: "x".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownShape)));
    w.edit(StructureEdit::AddPartShape { definition: w.part, kind: PartShapeKind::Fixed, name: "x".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::CannotAddFixed)));
    w.edit(StructureEdit::AddPartShape { definition: w.part, kind: PartShapeKind::Circle, name: "".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NeedsName)));
    assert_eq!(w.label(), undo);
}
