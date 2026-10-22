//! G8-editor S07/P06 in the window: formulas in the numbers of a shape, fixed shapes that can be
//! moved and resized, turning an instance, and distributing the instances over the plates. Each
//! action is one undo step; a refused one changes nothing and says why. Oracles are worked out by
//! hand from the sizes of the "Beugel" (rectangle `breedte x 20` with a hole r 3).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers

use std::collections::BTreeMap;
use std::path::PathBuf;

use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartInstance, PartShape, Plate, StructureChange, StructureCommand,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{Affine, Path, Point, Vector};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, FieldError, HeadlessEditorHarness, HostEffect, MessageKey, PlateField, ShapeField,
    StructureEdit, StructureProblem, UiIntent,
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

/// "Beugel": a rectangle `breedte x 20`, a hole r 3 at (10, 10) and a triangle of fixed geometry
/// (2, 3) (22, 3) (12, 13); "klein" makes the width 25.
fn bracket() -> PartDefinition {
    let rect = PartShape::Rectangle {
        x: Dim::Mm(0.0),
        y: Dim::Mm(0.0),
        width: Dim::Param("breedte".into()),
        height: Dim::Mm(20.0),
    };
    let hole = PartShape::Circle { x: Dim::Mm(10.0), y: Dim::Mm(10.0), radius: Dim::Mm(3.0) };
    let triangle =
        Path::polyline(&[Point::new(2.0, 3.0), Point::new(22.0, 3.0), Point::new(12.0, 13.0)], true).unwrap();
    PartDefinition {
        name: "Beugel".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0))]),
        shapes: BTreeMap::from([
            (1, NamedShape { name: "omtrek".into(), shape: rect }),
            (2, NamedShape { name: "gat".into(), shape: hole }),
            (3, NamedShape { name: "driehoek".into(), shape: PartShape::Fixed(triangle) }),
        ]),
        variants: BTreeMap::from([("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))]))]),
    }
}

struct World {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    part: u64,
    /// The first uses "klein" (outline 25 x 20), the second the default (40 x 20).
    instances: [u64; 2],
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
    run(
        &mut s,
        st(StructureCommand::CreatePlate {
            plate: Plate {
                name: "Plaat A".into(),
                width: mm(600.0),
                height: mm(400.0),
                material: "Berken".into(),
                thickness: None,
            },
        }),
    );
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/delen.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([9; 16]), (*s.snapshot()).clone())).unwrap();
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
    World { h, doc, part: def.raw(), instances }
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
    fn problem(&self) -> Option<MessageKey> {
        self.vm().problem.map(|p| p.message)
    }
    fn field(&mut self, shape: u32, field: ShapeField, text: &str) {
        self.edit(StructureEdit::SetPartShapeField { definition: self.part, shape, field, text: text.into() });
    }
    fn shape_fields(&self, shape: u32) -> Vec<(ShapeField, String)> {
        let part = &self.vm().structure.parts[0];
        part.shapes.iter().find(|s| s.key == shape).unwrap().fields.clone()
    }
    /// The scene item of every output named `Beugel/<name>`, left to right.
    fn items(&self, name: &str) -> Vec<(GeometryId, Path)> {
        let ids: Vec<GeometryId> =
            self.vm().objects.iter().filter(|o| o.label.contains(&format!("Beugel/{name}"))).map(|o| o.id).collect();
        let scene = self.h.scene(self.doc).unwrap();
        let mut found: Vec<(GeometryId, Path)> =
            scene.items.iter().filter(|i| ids.contains(&i.source)).map(|i| (i.source, (*i.path).clone())).collect();
        found.sort_by(|a, b| a.1.bounds().unwrap().min().x.total_cmp(&b.1.bounds().unwrap().min().x));
        found
    }
    /// Bounds of output `instance` (left to right) named `Beugel/<name>`.
    fn outline(&self, name: &str, instance: usize) -> (f64, f64, f64, f64) {
        let b = self.items(name)[instance].1.bounds().unwrap();
        (b.min().x, b.min().y, b.max().x, b.max().y)
    }
    fn instance_plate(&self, n: usize) -> Option<u64> {
        self.vm().structure.parts[0].instances.iter().find(|i| i.id == self.instances[n]).unwrap().plate_id
    }
    fn instance_rotation(&self, n: usize) -> String {
        self.vm().structure.parts[0].instances.iter().find(|i| i.id == self.instances[n]).unwrap().rotation.clone()
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// --- formulas ----------------------------------------------------------------------------------------------

#[test]
fn a_formula_is_typed_into_a_shape_number_and_every_instance_follows_it() {
    let mut w = world();
    w.field(1, ShapeField::Width, "breedte / 2 + 3");
    assert_eq!(w.problem(), None);
    assert!(w.shape_fields(1).contains(&(ShapeField::Width, "breedte / 2 + 3".to_owned())), "{:?}", w.shape_fields(1));
    // klein (25): 15,5 wide; the default (40): 23 wide.
    let first = w.outline("omtrek", 0);
    let second = w.outline("omtrek", 1);
    assert!(near(first.2 - first.0, 15.5) && near(second.2 - second.0, 23.0), "{first:?} {second:?}");
    // The hole follows a formula over the parameter too.
    w.field(2, ShapeField::Radius, "breedte / 10");
    let hole = w.outline("gat", 1);
    assert!(near(hole.2 - hole.0, 8.0), "radius 4 -> 8 across: {hole:?}");
    w.undo();
    let hole = w.outline("gat", 1);
    assert!(near(hole.2 - hole.0, 6.0), "one undo step: {hole:?}");
}

#[test]
fn a_plain_number_and_a_parameter_name_stay_what_they_were() {
    let mut w = world();
    w.field(1, ShapeField::Height, "35");
    assert!(w.shape_fields(1).contains(&(ShapeField::Height, "35".to_owned())));
    w.field(1, ShapeField::Width, "breedte");
    assert!(w.shape_fields(1).contains(&(ShapeField::Width, "breedte".to_owned())));
    w.field(1, ShapeField::X, "-5 mm");
    assert!(w.shape_fields(1).contains(&(ShapeField::X, "-5".to_owned())));
}

#[test]
fn a_text_that_is_no_formula_is_refused_and_changes_nothing() {
    let mut w = world();
    let undo = w.vm().undo;
    for bad in ["((", "breed / 2", "3 +", "breedte $ 2"] {
        w.field(1, ShapeField::Width, bad);
        assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::BadFormula)), "{bad}");
    }
    assert_eq!(w.vm().undo, undo);
    // A formula that comes out as no size fails that instance and says which parameter.
    w.field(1, ShapeField::Width, "breedte - 100");
    assert_eq!(w.problem(), None, "a definition may be valid for some variants only");
    let status = format!("{:?}", w.vm().structure.parts[0].instances[0].status);
    assert!(status.contains("breedte"), "{status}");
}

// --- fixed shapes --------------------------------------------------------------------------------------------

#[test]
fn a_fixed_shape_shows_its_box_and_can_be_moved_and_resized() {
    let mut w = world();
    assert_eq!(
        w.shape_fields(3),
        [
            (ShapeField::X, "2".to_owned()),
            (ShapeField::Y, "3".to_owned()),
            (ShapeField::Width, "20".to_owned()),
            (ShapeField::Height, "10".to_owned()),
        ]
    );
    w.field(3, ShapeField::X, "10");
    assert_eq!(w.problem(), None);
    let b = w.outline("driehoek", 0);
    assert!(near(b.0, 10.0) && near(b.2, 30.0) && near(b.1, 3.0) && near(b.3, 13.0), "moved: {b:?}");
    // Width 40 stretches it from its left edge: x 10..50, height unchanged.
    w.field(3, ShapeField::Width, "40 mm");
    let b = w.outline("driehoek", 0);
    assert!(near(b.0, 10.0) && near(b.2, 50.0) && near(b.1, 3.0) && near(b.3, 13.0), "wider: {b:?}");
    // Height 5 squeezes it from its lower edge: y 3..8.
    w.field(3, ShapeField::Height, "5");
    let b = w.outline("driehoek", 1);
    assert!(near(b.1, 3.0) && near(b.3, 8.0), "lower: {b:?}");
    // The box shown is the new one, and the triangle kept its shape: area 20 x 5 / 2 = 50.
    assert_eq!(
        w.shape_fields(3),
        [
            (ShapeField::X, "10".to_owned()),
            (ShapeField::Y, "3".to_owned()),
            (ShapeField::Width, "40".to_owned()),
            (ShapeField::Height, "5".to_owned()),
        ]
    );
    let area = w.items("driehoek")[0].1.signed_area().abs();
    assert!(near(area, 100.0), "40 x 5 / 2: {area}");
    w.undo();
    assert!(w.shape_fields(3).contains(&(ShapeField::Height, "10".to_owned())), "one undo step each");
}

#[test]
fn a_fixed_shape_takes_lengths_only_and_no_size_of_zero() {
    let mut w = world();
    let undo = w.vm().undo;
    w.field(3, ShapeField::Width, "0");
    assert_eq!(w.problem(), Some(MessageKey::InvalidLength(FieldError::NotPositive)));
    w.field(3, ShapeField::Height, "-4");
    assert_eq!(w.problem(), Some(MessageKey::InvalidLength(FieldError::NotPositive)));
    // A name or a formula needs numbers that follow parameters; fixed geometry has none.
    w.field(3, ShapeField::X, "breedte");
    assert_eq!(w.problem(), Some(MessageKey::InvalidLength(FieldError::NotANumber)));
    w.field(3, ShapeField::Radius, "3");
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NoSuchField)));
    assert_eq!(w.vm().undo, undo);
}

// --- turning an instance -----------------------------------------------------------------------------------------

#[test]
fn an_instance_turns_about_the_middle_of_its_outline() {
    let mut w = world();
    assert_eq!(w.instance_rotation(0), "0");
    // The first instance: outline x 0..25, y 0..20, middle (12,5; 10). Turned to 90 degrees it spans
    // x 2,5..22,5 and y -2,5..22,5 around the same middle.
    w.edit(StructureEdit::RotateInstance { instance: w.instances[0], degrees: "90".into(), relative: false });
    assert_eq!(w.problem(), None);
    let b = w.outline("omtrek", 0);
    assert!(near(b.0, 2.5) && near(b.2, 22.5) && near(b.1, -2.5) && near(b.3, 22.5), "{b:?}");
    assert_eq!(w.instance_rotation(0), "90");
    assert_eq!(w.instance_rotation(1), "0", "the other instance is untouched");
    let revision = w.vm().revision;
    w.edit(StructureEdit::RotateInstance { instance: w.instances[0], degrees: "90".into(), relative: false });
    assert_eq!(w.vm().revision, revision, "the angle it has already: no command");
    // By 90 more: 180, the outline is the original box again, upside down.
    w.edit(StructureEdit::RotateInstance { instance: w.instances[0], degrees: "90".into(), relative: true });
    assert_eq!(w.instance_rotation(0), "180");
    let b = w.outline("omtrek", 0);
    assert!(near(b.0, 0.0) && near(b.2, 25.0) && near(b.1, 0.0) && near(b.3, 20.0), "{b:?}");
    // 0 turns it back; one undo step each.
    w.edit(StructureEdit::RotateInstance { instance: w.instances[0], degrees: "0".into(), relative: false });
    assert_eq!(w.instance_rotation(0), "0");
    w.undo();
    assert_eq!(w.instance_rotation(0), "180");
    assert_eq!(w.vm().undo, Some(UndoLabel::Structure(burnburn_document::structure::StructureLabel::EditInstance)));
}

#[test]
fn turning_refuses_bad_text_and_unknown_instances() {
    let mut w = world();
    let undo = w.vm().undo;
    w.edit(StructureEdit::RotateInstance { instance: w.instances[0], degrees: "veel".into(), relative: false });
    assert!(matches!(w.problem(), Some(MessageKey::InvalidLength(_))), "{:?}", w.problem());
    w.edit(StructureEdit::RotateInstance { instance: 999, degrees: "90".into(), relative: false });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownInstance)));
    assert_eq!(w.vm().undo, undo);
}

// --- distributing over plates ------------------------------------------------------------------------------------

#[test]
fn every_instance_without_a_plate_is_put_on_a_plate_in_one_undo_step() {
    let mut w = world();
    let a = w.vm().structure.plates[0].id;
    w.edit(StructureEdit::DistributeOverPlates { gap: "5".into() });
    assert_eq!(w.problem(), None);
    assert_eq!((w.instance_plate(0), w.instance_plate(1)), (Some(a), Some(a)));
    assert_eq!(w.vm().structure.plates[0].instances, 2);
    assert!(matches!(w.vm().undo, Some(UndoLabel::Compound(_))), "{:?}", w.vm().undo);
    w.undo();
    assert_eq!((w.instance_plate(0), w.instance_plate(1)), (None, None), "one undo step for both");
}

#[test]
fn what_does_not_fit_the_first_plate_goes_to_the_next_and_the_first_is_not_overfilled() {
    let mut w = world();
    // Plate A becomes 30 x 30: the klein outline (25 x 20) fits, the 40 x 20 one does not.
    let a = w.vm().structure.plates[0].id;
    for (field, text) in [(PlateField::Width, "30"), (PlateField::Height, "30")] {
        w.edit(StructureEdit::SetPlateField { plate: a, field, text: text.into() });
    }
    w.edit(StructureEdit::CreatePlate {
        name: "Plaat B".into(),
        width: "600".into(),
        height: "400".into(),
        material: String::new(),
        thickness: String::new(),
    });
    let b = w.vm().structure.plates[1].id;
    w.edit(StructureEdit::DistributeOverPlates { gap: "0".into() });
    assert_eq!(w.problem(), None);
    assert_eq!((w.instance_plate(0), w.instance_plate(1)), (Some(a), Some(b)));
}

#[test]
fn nothing_is_moved_when_one_instance_finds_no_room() {
    let mut w = world();
    let a = w.vm().structure.plates[0].id;
    for (field, text) in [(PlateField::Width, "30"), (PlateField::Height, "30")] {
        w.edit(StructureEdit::SetPlateField { plate: a, field, text: text.into() });
    }
    let undo = w.vm().undo;
    w.edit(StructureEdit::DistributeOverPlates { gap: "0".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NoRoomOnPlates)));
    assert_eq!((w.instance_plate(0), w.instance_plate(1)), (None, None), "not even the one that fits");
    assert_eq!(w.vm().undo, undo);
}

#[test]
fn distributing_refuses_without_plates_without_work_and_with_a_bad_gap() {
    let mut w = world();
    w.edit(StructureEdit::DistributeOverPlates { gap: "abc".into() });
    assert!(matches!(w.problem(), Some(MessageKey::InvalidLength(_))), "{:?}", w.problem());
    w.edit(StructureEdit::DistributeOverPlates { gap: "-3".into() });
    assert!(matches!(w.problem(), Some(MessageKey::InvalidLength(_))), "a gap is not negative: {:?}", w.problem());
    w.edit(StructureEdit::DistributeOverPlates { gap: String::new() });
    assert_eq!(w.problem(), None, "an empty gap is none");
    w.edit(StructureEdit::DistributeOverPlates { gap: "0".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NothingToDistribute)));
    let plate = w.vm().structure.plates[0].id;
    w.edit(StructureEdit::RemovePlate { plate });
    w.edit(StructureEdit::DistributeOverPlates { gap: "0".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NoPlates)));
    let _ = GeometryId::from_raw(0);
}
