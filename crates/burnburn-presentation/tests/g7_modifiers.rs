//! G7-editor E06: the modifier stack in the window. The structure panel's view model lists every
//! non-destructive chain with its steps in order; a step is switched off and on, moved, edited
//! and removed, and a chain is removed or applied (baked). Each action is one undo step, a refused
//! one changes nothing and says why.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers

use std::path::PathBuf;

use burnburn_document::structure::{
    EvalMode, Modifier, ModifierChain, ModifierStep, StructureChange, StructureCommand, StructureLabel,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{OffsetJoin, Point, ProductionTolerance, Rect, Vector};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    DocumentViewModel, HeadlessEditorHarness, HostEffect, MessageKey, ModifierKind, ModifierStatus, ModifierValueKind,
    SelectMode, StructureEdit, StructureProblem, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn chain(source: GeometryId, steps: Vec<Modifier>) -> ModifierChain {
    ModifierChain {
        name: "Patroon".into(),
        source,
        steps: steps.into_iter().map(|modifier| ModifierStep { modifier, enabled: true }).collect(),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    }
}

struct World {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    source: GeometryId,
    output: GeometryId,
    chain: u64,
}

/// A 10 mm square "Bron" with a chain [offset `offset` mm, three copies 20 mm apart].
fn world(offset: f64) -> World {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let source = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).changes.created[0];
    let steps = vec![
        Modifier::Offset { distance: mm(offset), join: OffsetJoin::Round },
        Modifier::LinearArray { count: 3, step: Vector::new(20.0, 0.0) },
    ];
    let out = run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(source, steps) }));
    let output = out.changes.created.first().copied().unwrap_or_else(|| GeometryId::from_raw(0));
    let StructureChange::Modifier(id) = out.changes.structure[0] else { panic!("a modifier") };
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/stapel.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([6; 16]), (*s.snapshot()).clone())).unwrap();
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
    World { h, doc, source, output, chain: id.raw() }
}

impl World {
    fn vm(&self) -> DocumentViewModel {
        self.h.document(self.doc).expect("document view")
    }
    fn edit(&mut self, edit: StructureEdit) {
        self.h.send(UiIntent::Structure { document: self.doc, edit });
    }
    fn output_subpaths(&self) -> usize {
        let scene = self.h.scene(self.doc).expect("scene");
        scene.items.iter().find(|i| i.source == self.output).map_or(0, |i| i.path.subpaths().len())
    }
    fn output_width(&self) -> f64 {
        let scene = self.h.scene(self.doc).expect("scene");
        scene.items.iter().find(|i| i.source == self.output).and_then(|i| i.path.bounds()).map_or(0.0, |b| b.width())
    }
    fn undo(&mut self) {
        self.h.invoke(self.doc, burnburn_presentation::CommandKey::Undo);
    }
    fn undo_label(&self) -> Option<UndoLabel> {
        self.vm().undo
    }
}

#[test]
fn the_stack_lists_the_chain_with_its_steps_in_order() {
    let w = world(1.0);
    let rows = w.vm().structure.modifiers;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!((row.id, row.name.as_str(), row.source), (w.chain, "Patroon", w.source.raw()));
    assert_eq!(row.source_name.as_deref(), Some("Bron"));
    assert_eq!(row.status, ModifierStatus::Valid);
    assert_eq!(row.outputs, 1);
    let steps: Vec<_> = row.steps.iter().map(|s| (s.index, s.kind, s.value.clone(), s.enabled, s.failed)).collect();
    assert_eq!(
        steps,
        [
            (0, ModifierKind::Offset, Some((ModifierValueKind::Distance, "1".to_owned())), true, false),
            (1, ModifierKind::LinearArray, Some((ModifierValueKind::Count, "3".to_owned())), true, false),
        ]
    );
}

#[test]
fn a_step_goes_off_and_on_in_one_undo_step_each_and_the_result_follows() {
    let mut w = world(1.0);
    assert!((w.output_width() - 52.0).abs() < 0.05, "offset 1 mm both sides, 3 copies: {}", w.output_width());
    w.edit(StructureEdit::ToggleModifierStep { modifier: w.chain, step: 0, enabled: false });
    assert!(!w.vm().structure.modifiers[0].steps[0].enabled);
    assert!((w.output_width() - 50.0).abs() < 0.05, "without the offset the square is 10 wide: {}", w.output_width());
    assert_eq!(w.undo_label(), Some(UndoLabel::Structure(StructureLabel::ToggleModifierStep)));
    w.edit(StructureEdit::ToggleModifierStep { modifier: w.chain, step: 0, enabled: true });
    assert!((w.output_width() - 52.0).abs() < 0.05);
    w.undo();
    assert!(!w.vm().structure.modifiers[0].steps[0].enabled, "one undo undoes only the last toggle");
}

#[test]
fn the_order_of_the_steps_is_part_of_the_result() {
    // 6 mm offsets: squares 20 apart, 10 wide. Offset first keeps three outlines; copying first
    // and then growing the three squares by 6 mm makes them touch and merge into one.
    let mut w = world(6.0);
    assert_eq!(w.output_subpaths(), 3);
    w.edit(StructureEdit::MoveModifierStep { modifier: w.chain, step: 1, up: true });
    let kinds: Vec<_> = w.vm().structure.modifiers[0].steps.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [ModifierKind::LinearArray, ModifierKind::Offset]);
    assert_eq!(w.output_subpaths(), 1, "grown after copying: merged");
    assert_eq!(w.undo_label(), Some(UndoLabel::Structure(StructureLabel::EditModifier)));
    w.undo();
    assert_eq!(w.output_subpaths(), 3);
}

#[test]
fn moving_beyond_either_end_is_refused_and_changes_nothing() {
    let mut w = world(1.0);
    let before = w.vm().undo;
    w.edit(StructureEdit::MoveModifierStep { modifier: w.chain, step: 0, up: true });
    assert_eq!(w.vm().problem.map(|p| p.message), Some(MessageKey::Structure(StructureProblem::CannotMoveStep)));
    w.edit(StructureEdit::MoveModifierStep { modifier: w.chain, step: 1, up: false });
    assert_eq!(w.vm().undo, before);
    assert_eq!(w.vm().structure.modifiers[0].steps[0].kind, ModifierKind::Offset);
}

#[test]
fn a_step_value_is_typed_with_its_unit_and_a_bad_one_is_refused() {
    let mut w = world(1.0);
    w.edit(StructureEdit::SetModifierStepValue { modifier: w.chain, step: 1, text: "5".into() });
    assert_eq!(w.vm().structure.modifiers[0].steps[1].value, Some((ModifierValueKind::Count, "5".to_owned())));
    assert!((w.output_width() - 92.0).abs() < 0.05);
    w.edit(StructureEdit::SetModifierStepValue { modifier: w.chain, step: 0, text: "0,5 in".into() });
    assert_eq!(w.vm().structure.modifiers[0].steps[0].value, Some((ModifierValueKind::Distance, "12.7".to_owned())));

    let undo = w.vm().undo;
    for (step, text) in [(1, "0"), (1, "2,5"), (1, "abc"), (0, "12 apples")] {
        w.edit(StructureEdit::SetModifierStepValue { modifier: w.chain, step, text: text.into() });
        assert!(w.vm().problem.is_some(), "{text:?} is refused with a reason");
        assert_eq!(w.vm().undo, undo, "{text:?} changed nothing");
    }
}

#[test]
fn a_step_without_a_single_number_says_so() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let source = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).changes.created[0];
    let other = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Ander".into() }).changes.created[0];
    let boolean = Modifier::Boolean { op: burnburn_geometry::BooleanOp::Union, with: vec![other] };
    run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(source, vec![boolean]) }));
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/bool.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([5; 16]), (*s.snapshot()).clone())).unwrap();
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
    let row = h.document(doc).unwrap().structure.modifiers[0].clone();
    assert_eq!((row.steps[0].kind, row.steps[0].value.clone()), (ModifierKind::Boolean, None));
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::SetModifierStepValue { modifier: row.id, step: 0, text: "3".into() },
    });
    let problem = h.document(doc).unwrap().problem.map(|p| p.message);
    assert_eq!(problem, Some(MessageKey::Structure(StructureProblem::StepHasNoValue)));
}

#[test]
fn removing_a_step_is_one_undo_step_and_the_last_step_stays() {
    let mut w = world(1.0);
    w.edit(StructureEdit::RemoveModifierStep { modifier: w.chain, step: 0 });
    let steps = w.vm().structure.modifiers[0].steps.clone();
    assert_eq!(steps.len(), 1);
    assert_eq!((steps[0].index, steps[0].kind), (0, ModifierKind::LinearArray), "steps are renumbered");
    assert_eq!(w.undo_label(), Some(UndoLabel::Structure(StructureLabel::EditModifier)));
    w.edit(StructureEdit::RemoveModifierStep { modifier: w.chain, step: 0 });
    assert_eq!(w.vm().structure.modifiers[0].steps.len(), 1, "the chain keeps its last step");
    assert_eq!(w.vm().problem.map(|p| p.message), Some(MessageKey::Structure(StructureProblem::LastStep)));
    w.undo();
    assert_eq!(w.vm().structure.modifiers[0].steps.len(), 2);
}

#[test]
fn removing_the_chain_removes_its_outputs_and_keeps_the_source() {
    let mut w = world(1.0);
    let objects = w.vm().objects.len();
    w.edit(StructureEdit::RemoveModifier { modifier: w.chain });
    assert!(w.vm().structure.modifiers.is_empty());
    assert_eq!(w.vm().objects.len(), objects - 1);
    assert!(w.vm().objects.iter().any(|o| o.id == w.source));
    assert_eq!(w.undo_label(), Some(UndoLabel::Structure(StructureLabel::RemoveModifier)));
    w.undo();
    assert_eq!(w.vm().structure.modifiers.len(), 1);
    assert_eq!(w.vm().objects.len(), objects);
}

#[test]
fn applying_the_chain_keeps_the_outputs_as_plain_objects_and_undo_brings_the_chain_back() {
    let mut w = world(1.0);
    let objects = w.vm().objects.len();
    let width = w.output_width();
    w.edit(StructureEdit::BakeModifier { modifier: w.chain });
    assert!(w.vm().structure.modifiers.is_empty());
    assert_eq!(w.vm().objects.len(), objects, "nothing disappears");
    assert!((w.output_width() - width).abs() < 1e-9, "the outline is the same");
    assert_eq!(w.undo_label(), Some(UndoLabel::Structure(StructureLabel::BakeModifier)));
    w.undo();
    assert_eq!(w.vm().structure.modifiers.len(), 1);
    assert!((w.output_width() - width).abs() < 1e-9);
}

#[test]
fn a_step_that_cannot_be_computed_is_named_and_the_chain_is_failed() {
    let w = world(-30.0);
    let row = &w.vm().structure.modifiers[0];
    assert_eq!(row.status, ModifierStatus::Failed { step: Some(0) });
    assert!(row.steps[0].failed && !row.steps[1].failed);
}

#[test]
fn an_unknown_chain_or_step_is_refused() {
    let mut w = world(1.0);
    let undo = w.vm().undo;
    w.edit(StructureEdit::BakeModifier { modifier: 999 });
    assert_eq!(w.vm().problem.map(|p| p.message), Some(MessageKey::Structure(StructureProblem::UnknownModifier)));
    w.edit(StructureEdit::ToggleModifierStep { modifier: w.chain, step: 9, enabled: false });
    assert_eq!(w.vm().problem.map(|p| p.message), Some(MessageKey::Structure(StructureProblem::UnknownStep)));
    assert_eq!(w.vm().undo, undo);
}

#[test]
fn an_offset_chain_is_made_on_the_selected_object() {
    let mut w = world(1.0);
    // The chain's source is plain; give it a second, separate chain.
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![w.source], mode: SelectMode::Replace });
    w.edit(StructureEdit::CreateOffsetModifier { distance: "2 mm".into() });
    let rows = w.vm().structure.modifiers;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].steps[0].value, Some((ModifierValueKind::Distance, "2".to_owned())));
    assert_eq!(w.undo_label(), Some(UndoLabel::Structure(StructureLabel::CreateModifier)));
    w.undo();
    assert_eq!(w.vm().structure.modifiers.len(), 1);

    w.edit(StructureEdit::CreateOffsetModifier { distance: "zwiep".into() });
    assert!(w.vm().problem.is_some());
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![], mode: SelectMode::Replace });
    w.edit(StructureEdit::CreateOffsetModifier { distance: "2".into() });
    assert_eq!(w.vm().problem.map(|p| p.message), Some(MessageKey::Structure(StructureProblem::NeedsSelection)));
}
