//! G9-editor P06: "Verdeel en nest" puts every instance without a plate on a plate (the row plan of
//! G8) and then lets the existing nest decide where it lies on that plate. The plates are laid side
//! by side from the origin, in the order of the plates, [`SHEET_GAP`] mm apart. One undo step; a
//! stale result is dropped; a refusal changes nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers

use std::collections::BTreeMap;
use std::path::PathBuf;

use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartInstance, PartShape, Plate, StructureChange, StructureCommand,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Affine, Vector};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, MessageKey, StructureEdit, StructureProblem,
    UiIntent,
};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

/// The space between two plates in the document (mm).
const SHEET_GAP: f64 = 10.0;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

/// "Plaatje": a rectangle `breedte x 20`; "klein" makes the width 25 (the default is 40).
fn plaatje() -> PartDefinition {
    let rect = PartShape::Rectangle {
        x: Dim::Mm(0.0),
        y: Dim::Mm(0.0),
        width: Dim::Param("breedte".into()),
        height: Dim::Mm(20.0),
    };
    PartDefinition {
        name: "Plaatje".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0))]),
        shapes: BTreeMap::from([(1, NamedShape { name: "omtrek".into(), shape: rect })]),
        variants: BTreeMap::from([("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))]))]),
    }
}

fn plate(name: &str, width: f64, height: f64) -> Plate {
    Plate { name: name.into(), width: mm(width), height: mm(height), material: "Berken".into(), thickness: None }
}

struct World {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    /// Instance 0 is "klein" (25 x 20) at x = 100, instance 1 is the default (40 x 20) at x = 200.
    instances: [u64; 2],
}

fn world(plates: &[(&str, f64, f64)]) -> World {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let out = run(&mut s, CommandPayload::Structure(StructureCommand::DefinePart { definition: plaatje() }));
    let StructureChange::Definition(def) = out.changes.structure[0] else { panic!() };
    let mut instances = [0; 2];
    for (n, variant) in [Some("klein"), None].into_iter().enumerate() {
        let instance = PartInstance {
            definition: def,
            variant: variant.map(str::to_owned),
            overrides: BTreeMap::new(),
            transform: Affine::translate(Vector::new(100.0 * (n + 1) as f64, 500.0)).unwrap(),
            plate: None,
        };
        let out = run(&mut s, CommandPayload::Structure(StructureCommand::PlaceInstance { instance }));
        let StructureChange::Instance(i) = out.changes.structure[0] else { panic!() };
        instances[n] = i.raw();
    }
    for (name, w, h) in plates {
        run(&mut s, CommandPayload::Structure(StructureCommand::CreatePlate { plate: plate(name, *w, *h) }));
    }
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/platen.burnburn");
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
    World { h, doc, instances }
}

impl World {
    fn vm(&self) -> DocumentViewModel {
        self.h.document(self.doc).expect("document view")
    }
    fn edit(&mut self, edit: StructureEdit) {
        self.h.send(UiIntent::Structure { document: self.doc, edit });
    }
    fn problem(&self) -> Option<MessageKey> {
        self.vm().problem.map(|p| p.message)
    }
    fn plate_of(&self, n: usize) -> Option<u64> {
        self.vm().structure.parts[0].instances.iter().find(|i| i.id == self.instances[n]).unwrap().plate_id
    }
    fn plate_id(&self, n: usize) -> u64 {
        self.vm().structure.plates[n].id
    }
    /// The bounds (min x, min y, max x, max y) of every outline, left to right.
    fn outlines(&self) -> Vec<(f64, f64, f64, f64)> {
        let mut items: Vec<_> = self
            .h
            .scene(self.doc)
            .unwrap()
            .items
            .iter()
            .filter_map(|i| i.path.bounds())
            .map(|b| (b.min().x, b.min().y, b.max().x, b.max().y))
            .collect();
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        items
    }
}

fn inside(b: (f64, f64, f64, f64), x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    b.0 >= x0 - 1e-6 && b.1 >= y0 - 1e-6 && b.2 <= x1 + 1e-6 && b.3 <= y1 + 1e-6
}

fn apart(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64), gap: f64) -> bool {
    a.2 + gap <= b.0 + 1e-6 || b.2 + gap <= a.0 + 1e-6 || a.3 + gap <= b.1 + 1e-6 || b.3 + gap <= a.1 + 1e-6
}

#[test]
fn instances_go_to_their_plate_and_the_nest_lays_them_on_it() {
    let mut w = world(&[("Plaat A", 45.0, 25.0), ("Plaat B", 600.0, 400.0)]);
    let (a, b) = (w.plate_id(0), w.plate_id(1));
    w.edit(StructureEdit::DistributeAndNestOverPlates { gap: "0".into() });
    w.h.run_tasks();
    assert_eq!(w.problem(), None);
    // The 40 x 20 part only fits plate A (45 x 25); the 25 x 20 one goes to B.
    assert_eq!((w.plate_of(0), w.plate_of(1)), (Some(b), Some(a)));
    let outlines = w.outlines();
    assert_eq!(outlines.len(), 2);
    let on_a = outlines.iter().copied().find(|o| (o.2 - o.0 - 40.0).abs() < 1e-6 || (o.3 - o.1 - 40.0).abs() < 1e-6);
    let on_b = outlines.iter().copied().find(|o| (o.2 - o.0 - 25.0).abs() < 1e-6 || (o.3 - o.1 - 25.0).abs() < 1e-6);
    // Plate A lies at x 0..45, y 0..25; plate B beside it, at x 45 + gap.. , y 0..400.
    assert!(inside(on_a.unwrap(), 0.0, 0.0, 45.0, 25.0), "{on_a:?}");
    assert!(inside(on_b.unwrap(), 45.0 + SHEET_GAP, 0.0, 45.0 + SHEET_GAP + 600.0, 400.0), "{on_b:?}");
}

#[test]
fn parts_on_one_plate_do_not_overlap_and_keep_the_space_asked_for() {
    let mut w = world(&[("Plaat A", 600.0, 400.0)]);
    let a = w.plate_id(0);
    w.edit(StructureEdit::DistributeAndNestOverPlates { gap: "5".into() });
    w.h.run_tasks();
    assert_eq!(w.problem(), None);
    assert_eq!((w.plate_of(0), w.plate_of(1)), (Some(a), Some(a)));
    let outlines = w.outlines();
    assert_eq!(outlines.len(), 2);
    for o in &outlines {
        assert!(inside(*o, 0.0, 0.0, 600.0, 400.0), "{o:?}");
    }
    assert!(apart(outlines[0], outlines[1], 5.0), "{outlines:?}");
}

#[test]
fn it_is_one_undo_step() {
    let mut w = world(&[("Plaat A", 600.0, 400.0)]);
    let before = w.outlines();
    let undo = w.vm().undo;
    w.edit(StructureEdit::DistributeAndNestOverPlates { gap: "2".into() });
    w.h.run_tasks();
    assert_ne!(w.outlines(), before);
    w.h.invoke(w.doc, CommandKey::Undo);
    assert_eq!((w.plate_of(0), w.plate_of(1)), (None, None));
    assert_eq!(w.outlines(), before, "the instances are back where they were");
    assert_eq!(w.vm().undo, undo);
}

#[test]
fn an_edit_before_the_answer_arrives_drops_the_result() {
    let mut w = world(&[("Plaat A", 600.0, 400.0)]);
    w.edit(StructureEdit::DistributeAndNestOverPlates { gap: "2".into() });
    // The worker has not answered yet; the document changes first.
    let requests = w.h.take_task_requests();
    assert_eq!(requests.len(), 1, "one background task");
    w.edit(StructureEdit::RotateInstance { instance: w.instances[0], degrees: "90".into(), relative: false });
    let after_edit = w.outlines();
    for r in requests {
        w.h.complete_despite_cancel(r);
    }
    assert_eq!((w.plate_of(0), w.plate_of(1)), (None, None), "a stale result is never applied");
    assert_eq!(w.outlines(), after_edit);
}

#[test]
fn no_room_is_refused_before_anything_runs_and_changes_nothing() {
    let mut w = world(&[("Plaat A", 30.0, 30.0)]);
    let undo = w.vm().undo;
    w.edit(StructureEdit::DistributeAndNestOverPlates { gap: "0".into() });
    assert!(w.h.take_task_requests().is_empty(), "no task for a plan that cannot work");
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NoRoomOnPlates)));
    assert_eq!((w.plate_of(0), w.plate_of(1)), (None, None));
    assert_eq!(w.vm().undo, undo);
}

#[test]
fn it_refuses_without_plates_and_with_a_bad_gap() {
    let mut w = world(&[]);
    w.edit(StructureEdit::DistributeAndNestOverPlates { gap: "0".into() });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NoPlates)));
    let mut w = world(&[("Plaat A", 600.0, 400.0)]);
    w.edit(StructureEdit::DistributeAndNestOverPlates { gap: "-3".into() });
    assert!(matches!(w.problem(), Some(MessageKey::InvalidLength(_))), "{:?}", w.problem());
    assert!(w.h.take_task_requests().is_empty());
}
