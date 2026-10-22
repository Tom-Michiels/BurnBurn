//! G8-editor E06: steps are added to a modifier chain from the panel (offset, boolean, grid,
//! circle), and a chain is made from the selection with any of them as its first step. Each
//! action is one undo step; an invalid number or a missing operand is refused and changes
//! nothing. Oracles are the extents and counts of the copies, worked out by hand.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers

use std::path::PathBuf;

use burnburn_document::structure::{
    EvalMode, Modifier, ModifierChain, ModifierStep, StructureChange, StructureCommand, StructureLabel,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{OffsetJoin, Point, ProductionTolerance, Rect, Vector};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    BooleanKind, DocumentViewModel, HeadlessEditorHarness, HostEffect, MessageKey, ModifierKind, ModifierValueKind,
    NewModifierStep, SelectMode, StructureEdit, StructureProblem, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn rect_at(s: &mut DocumentSession, x: f64, y: f64, w: f64, name: &str) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(w)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: name.into() }).changes.created[0]
}

struct World {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    source: GeometryId,
    other: GeometryId,
    output: GeometryId,
    chain: u64,
}

/// A 10 mm square "Bron" with a chain [offset 1 mm, three copies 20 mm apart] (so three disjoint
/// squares of 12 mm), and a separate 2 mm square "Gat" inside the first copy.
fn world() -> World {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let source = rect_at(&mut s, 0.0, 0.0, 10.0, "Bron");
    let other = rect_at(&mut s, 4.0, 4.0, 2.0, "Gat");
    let chain = ModifierChain {
        name: "Patroon".into(),
        source,
        steps: vec![
            ModifierStep { modifier: Modifier::Offset { distance: mm(1.0), join: OffsetJoin::Round }, enabled: true },
            ModifierStep {
                modifier: Modifier::LinearArray { count: 3, step: Vector::new(20.0, 0.0) },
                enabled: true,
            },
        ],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    };
    let out = run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain }));
    let output = out.changes.created.first().copied().unwrap_or_else(|| GeometryId::from_raw(0));
    let StructureChange::Modifier(id) = out.changes.structure[0] else { panic!("a modifier") };
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/stapel.burnburn");
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
    World { h, doc, source, other, output, chain: id.raw() }
}

impl World {
    fn vm(&self) -> DocumentViewModel {
        self.h.document(self.doc).expect("document view")
    }
    fn edit(&mut self, edit: StructureEdit) {
        self.h.send(UiIntent::Structure { document: self.doc, edit });
    }
    fn select(&mut self, ids: &[GeometryId]) {
        self.h.send(UiIntent::Select { document: self.doc, ids: ids.to_vec(), mode: SelectMode::Replace });
    }
    fn item(&self, id: GeometryId) -> Option<(usize, f64, f64)> {
        let scene = self.h.scene(self.doc).expect("scene");
        let item = scene.items.iter().find(|i| i.source == id)?;
        let b = item.path.bounds()?;
        Some((item.path.subpaths().len(), b.width(), b.height()))
    }
    fn undo(&mut self) {
        self.h.invoke(self.doc, burnburn_presentation::CommandKey::Undo);
    }
    fn problem(&self) -> Option<MessageKey> {
        self.vm().problem.map(|p| p.message)
    }
    fn kinds(&self, chain: usize) -> Vec<ModifierKind> {
        self.vm().structure.modifiers[chain].steps.iter().map(|s| s.kind).collect()
    }
}

fn grid(columns: &str, rows: &str, pitch_x: &str, pitch_y: &str) -> NewModifierStep {
    NewModifierStep::Grid {
        columns: columns.into(),
        rows: rows.into(),
        pitch_x: pitch_x.into(),
        pitch_y: pitch_y.into(),
    }
}

#[test]
fn a_grid_step_is_appended_and_multiplies_the_copies() {
    let mut w = world();
    assert_eq!(w.item(w.output), Some((3, 52.0, 12.0)));
    w.edit(StructureEdit::AddModifierStep { modifier: w.chain, step: grid("2", "2", "30", "40") });
    assert_eq!(w.problem(), None);
    assert_eq!(w.kinds(0), [ModifierKind::Offset, ModifierKind::LinearArray, ModifierKind::GridArray]);
    // 12 mm squares: columns 30 mm apart widen 52 -> 82 mm, rows 40 mm apart heighten 12 -> 52 mm.
    let (subpaths, width, height) = w.item(w.output).unwrap();
    assert_eq!(subpaths, 12);
    assert!((width - 82.0).abs() < 1e-3 && (height - 52.0).abs() < 1e-3, "{width} x {height}");
    assert_eq!(w.vm().undo, Some(UndoLabel::Structure(StructureLabel::EditModifier)));
    w.undo();
    assert_eq!(w.kinds(0), [ModifierKind::Offset, ModifierKind::LinearArray]);
    assert_eq!(w.item(w.output), Some((3, 52.0, 12.0)), "one undo step brings the old result back");
}

#[test]
fn a_circle_step_turns_copies_around_the_typed_centre() {
    let mut w = world();
    w.edit(StructureEdit::AddModifierStep {
        modifier: w.chain,
        step: NewModifierStep::Circle {
            count: "4".into(),
            center_x: "0".into(),
            center_y: "0".into(),
            sweep: "360".into(),
        },
    });
    assert_eq!(w.problem(), None);
    let row = &w.vm().structure.modifiers[0];
    assert_eq!(row.steps[2].kind, ModifierKind::CircularArray);
    assert_eq!(row.steps[2].value, Some((ModifierValueKind::Count, "4".to_owned())));
    // Quarter turns about the origin of squares spanning x -1..51: the extent is -51..51 both ways.
    let (subpaths, width, height) = w.item(w.output).unwrap();
    assert_eq!(subpaths, 12);
    assert!((width - 102.0).abs() < 1e-3 && (height - 102.0).abs() < 1e-3, "{width} x {height}");
}

#[test]
fn a_boolean_step_combines_with_the_other_selected_objects() {
    let mut w = world();
    // "Gat" lies inside the first 12 mm copy: subtracting it leaves a hole there (one more contour).
    w.select(&[w.other]);
    w.edit(StructureEdit::AddModifierStep {
        modifier: w.chain,
        step: NewModifierStep::Boolean { op: BooleanKind::Difference },
    });
    assert_eq!(w.problem(), None);
    assert_eq!(w.kinds(0), [ModifierKind::Offset, ModifierKind::LinearArray, ModifierKind::Boolean]);
    assert_eq!(w.item(w.output).map(|i| i.0), Some(4));
    w.undo();
    assert_eq!(w.item(w.output).map(|i| i.0), Some(3));

    // Intersection keeps only the part inside "Gat": one 2 x 2 mm square.
    w.edit(StructureEdit::AddModifierStep {
        modifier: w.chain,
        step: NewModifierStep::Boolean { op: BooleanKind::Intersection },
    });
    let (subpaths, width, height) = w.item(w.output).unwrap();
    assert_eq!(subpaths, 1);
    assert!((width - 2.0).abs() < 1e-3 && (height - 2.0).abs() < 1e-3, "{width} x {height}");
}

#[test]
fn a_boolean_step_without_operands_is_refused() {
    let mut w = world();
    let undo = w.vm().undo;
    for ids in [vec![], vec![w.source], vec![w.output]] {
        w.select(&ids);
        w.edit(StructureEdit::AddModifierStep {
            modifier: w.chain,
            step: NewModifierStep::Boolean { op: BooleanKind::Union },
        });
        assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NeedsOperands)), "{ids:?}");
    }
    assert_eq!(w.vm().undo, undo);
    assert_eq!(w.kinds(0).len(), 2);
}

#[test]
fn an_offset_step_is_appended_with_its_unit() {
    let mut w = world();
    w.edit(StructureEdit::AddModifierStep {
        modifier: w.chain,
        step: NewModifierStep::Offset { distance: "-0.5 mm".into() },
    });
    assert_eq!(w.problem(), None);
    let row = &w.vm().structure.modifiers[0];
    assert_eq!(row.steps[2].kind, ModifierKind::Offset);
    assert_eq!(row.steps[2].value, Some((ModifierValueKind::Distance, "-0.5".to_owned())));
    // Every 12 mm copy shrinks by 0.5 mm per side: 11 mm squares 20 mm apart.
    let (subpaths, width, _) = w.item(w.output).unwrap();
    assert_eq!(subpaths, 3);
    assert!((width - 51.0).abs() < 1e-2, "{width}");
}

#[test]
fn bad_numbers_and_unknown_chains_are_refused_and_change_nothing() {
    let mut w = world();
    let undo = w.vm().undo;
    let bad = [
        (grid("0", "2", "30", "40"), StructureProblem::InvalidCount),
        (grid("2", "2.5", "30", "40"), StructureProblem::InvalidCount),
        (grid("100000", "100000", "30", "40"), StructureProblem::InvalidCount),
        (
            NewModifierStep::Circle { count: "0".into(), center_x: "0".into(), center_y: "0".into(), sweep: "360".into() },
            StructureProblem::InvalidCount,
        ),
        (
            NewModifierStep::Circle { count: "3".into(), center_x: "0".into(), center_y: "0".into(), sweep: "0".into() },
            StructureProblem::InvalidSweep,
        ),
        (
            NewModifierStep::Circle { count: "3".into(), center_x: "0".into(), center_y: "0".into(), sweep: "400".into() },
            StructureProblem::InvalidSweep,
        ),
    ];
    for (step, expected) in bad {
        w.edit(StructureEdit::AddModifierStep { modifier: w.chain, step: step.clone() });
        assert_eq!(w.problem(), Some(MessageKey::Structure(expected)), "{step:?}");
        assert_eq!(w.vm().undo, undo, "{step:?}");
    }
    w.edit(StructureEdit::AddModifierStep { modifier: w.chain, step: grid("2", "2", "zwiep", "40") });
    assert!(matches!(w.problem(), Some(MessageKey::InvalidLength(_))), "{:?}", w.problem());
    w.edit(StructureEdit::AddModifierStep { modifier: 999, step: grid("2", "2", "30", "40") });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::UnknownModifier)));
    assert_eq!(w.vm().undo, undo);
    assert_eq!(w.kinds(0).len(), 2);
}

#[test]
fn a_chain_is_made_on_the_selection_with_any_first_step() {
    let mut w = world();
    w.select(&[w.other]);
    w.edit(StructureEdit::CreateModifierChain { step: grid("3", "1", "5", "0") });
    assert_eq!(w.problem(), None);
    let rows = w.vm().structure.modifiers;
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[1].source, rows[1].steps.len(), rows[1].steps[0].kind), (w.other.raw(), 1, ModifierKind::GridArray));
    assert_eq!(w.vm().undo, Some(UndoLabel::Structure(StructureLabel::CreateModifier)));
    w.undo();
    assert_eq!(w.vm().structure.modifiers.len(), 1);

    // A boolean chain: the first selected object is the source, the others are the operands.
    w.select(&[w.source, w.other]);
    w.edit(StructureEdit::CreateModifierChain {
        step: NewModifierStep::Boolean { op: BooleanKind::Difference },
    });
    assert_eq!(w.problem(), None);
    let rows = w.vm().structure.modifiers;
    assert_eq!((rows[1].source, rows[1].steps[0].kind), (w.source.raw(), ModifierKind::Boolean));
    // 10 mm square with a 2 mm hole: two contours, extent still 10 mm.
    let new_output = w.h.scene(w.doc).unwrap().items.iter().map(|i| i.source).find(|id| ![w.source, w.other, w.output].contains(id));
    let (subpaths, width, _) = w.item(new_output.unwrap()).unwrap();
    assert_eq!(subpaths, 2);
    assert!((width - 10.0).abs() < 1e-3);
}

#[test]
fn a_chain_without_a_selection_or_with_a_lone_boolean_is_refused() {
    let mut w = world();
    w.select(&[]);
    w.edit(StructureEdit::CreateModifierChain { step: grid("2", "2", "30", "40") });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NeedsSelection)));
    w.select(&[w.other]);
    w.edit(StructureEdit::CreateModifierChain { step: NewModifierStep::Boolean { op: BooleanKind::Union } });
    assert_eq!(w.problem(), Some(MessageKey::Structure(StructureProblem::NeedsOperands)));
    assert_eq!(w.vm().structure.modifiers.len(), 1);
}

#[test]
fn the_view_model_marks_the_chains_of_the_selection() {
    let mut w = world();
    w.select(&[]);
    assert!(!w.vm().structure.modifiers[0].in_selection);
    w.select(&[w.other]);
    assert!(!w.vm().structure.modifiers[0].in_selection);
    w.select(&[w.source]);
    assert!(w.vm().structure.modifiers[0].in_selection, "the source");
    w.select(&[w.output]);
    assert!(w.vm().structure.modifiers[0].in_selection, "an output");
}
