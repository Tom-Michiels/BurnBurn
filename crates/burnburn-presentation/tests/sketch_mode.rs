//! Sketch mode (W1-L1b, B8, S01–S04) through the same intents a native host sends: make a sketch,
//! draw into it with relation suggestions, select entities, add relations and dimensions, edit a
//! dimension, and read freedom and conflicts from the view model and the scene overlays.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::sketch::{
    Along, ConstraintId, DimensionKind, DimensionRole, EntityId, Relation, Sketch, SketchCommand, SketchId, SketchLabel,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_editor::{CanvasKey, Modifiers, Overlay, PointFreedom, SketchGlyph, SketchMark};
use burnburn_geometry::Point;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, ConstraintHealth, DocumentViewModel, HeadlessEditorHarness, HostEffect, MessageKey, SelectMode,
    SketchConstraintKind, SketchDimensionKind, SketchModeViewModel, SketchStatus, UiIntent,
};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, RequestId, StableId};

type H = HeadlessEditorHarness;

fn new_doc() -> (H, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    (h, doc)
}

/// A document holding `sketch`, opened the way a user would (saved, then opened).
fn opened(sketch: Sketch) -> (H, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let mut s = DocumentSession::new(DocumentId::from_raw(77));
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch) }),
    };
    s.apply_with(cmd, &[&SketchEvaluator::new(SketchSolverAdapter)]).expect("solves");
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from("/work/schets.burnburn");
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
    (h, doc)
}

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn mode(h: &H, doc: DocumentId) -> SketchModeViewModel {
    vm(h, doc).sketch_mode.expect("sketch mode is on")
}

fn kinds(h: &H, doc: DocumentId) -> Vec<SketchConstraintKind> {
    mode(h, doc).rows.iter().map(|r| r.kind).collect()
}

fn enabled(h: &H, key: CommandKey) -> bool {
    h.view().commands[&key].enabled
}

/// Cmd+Shift: the modifier that suppresses relation suggestions while drawing (gate B8).
fn suppress() -> Modifiers {
    Modifiers { command: true, shift: true, ..Modifiers::default() }
}

fn shift() -> Modifiers {
    Modifiers { shift: true, ..Modifiers::default() }
}

fn begin(h: &mut H, doc: DocumentId) -> SketchId {
    h.invoke(doc, CommandKey::SketchNew);
    mode(h, doc).sketch
}

fn select_entities(h: &mut H, doc: DocumentId, entities: &[EntityId]) {
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: entities.to_vec(),
        constraints: Vec::new(),
        mode: SelectMode::Replace,
    });
}

fn select_constraints(h: &mut H, doc: DocumentId, constraints: &[ConstraintId]) {
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: Vec::new(),
        constraints: constraints.to_vec(),
        mode: SelectMode::Replace,
    });
}

fn overlays(h: &H, doc: DocumentId) -> Vec<Overlay> {
    h.scene(doc).expect("scene").overlays.clone()
}

fn undo_label(h: &H, doc: DocumentId) -> Option<UndoLabel> {
    vm(h, doc).undo
}

fn e(raw: u32) -> EntityId {
    EntityId::from_raw(raw)
}

/// Two free lines, two circles and a few loose points. Entity ids: a=0 b=1 L1=2 c=3 d=4 L2=5,
/// centre e=6 C1=7, centre f=8 C2=9.
struct Loose {
    sketch: Sketch,
}

fn loose() -> Loose {
    let mut s = Sketch::new("Los");
    let a = s.point(0.0, 0.0).unwrap();
    let b = s.point(40.0, 2.0).unwrap();
    s.line(a, b).unwrap();
    let c = s.point(0.0, 20.0).unwrap();
    let d = s.point(38.0, 25.0).unwrap();
    s.line(c, d).unwrap();
    let centre1 = s.point(60.0, 10.0).unwrap();
    s.circle(centre1, 5.0).unwrap();
    let centre2 = s.point(80.0, 10.0).unwrap();
    s.circle(centre2, 5.1).unwrap();
    Loose { sketch: s }
}

// ---------------------------------------------------------------------------------------------
// Making a sketch
// ---------------------------------------------------------------------------------------------

#[test]
fn new_sketch_enters_sketch_mode_and_undo_leaves_it() {
    let (mut h, doc) = new_doc();
    assert!(vm(&h, doc).sketch_mode.is_none());
    assert!(!enabled(&h, CommandKey::SketchFinish), "nothing to finish yet");
    h.invoke(doc, CommandKey::SketchNew);
    let m = mode(&h, doc);
    assert_eq!(m.name, "Schets 1");
    assert_eq!(vm(&h, doc).sketches.len(), 1);
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::Create)));
    assert!(enabled(&h, CommandKey::SketchFinish));

    h.invoke(doc, CommandKey::Undo);
    assert!(vm(&h, doc).sketch_mode.is_none(), "the sketch is gone, so is the mode");
    assert!(vm(&h, doc).sketches.is_empty());
}

#[test]
fn finishing_leaves_the_sketch_and_selecting_its_geometry_lets_you_edit_it_again() {
    let (mut h, doc) = new_doc();
    let sketch = begin(&mut h, doc);
    h.invoke(doc, CommandKey::ToolLine);
    h.drag(doc, (0.0, 0.0), (30.0, 20.0));
    h.invoke(doc, CommandKey::SketchFinish);
    assert!(vm(&h, doc).sketch_mode.is_none());
    assert_eq!(vm(&h, doc).sketches[0].id, sketch, "the sketch stays in the document");
    assert!(!enabled(&h, CommandKey::SketchEdit), "nothing selected");

    h.invoke(doc, CommandKey::ToolSelect);
    h.click(doc, (15.0, 10.0), Modifiers::default());
    assert!(enabled(&h, CommandKey::SketchEdit), "the selected object comes from a sketch");
    h.invoke(doc, CommandKey::SketchEdit);
    assert_eq!(mode(&h, doc).sketch, sketch);
}

#[test]
fn a_drawn_rectangle_becomes_a_sketch_in_one_undo_entry_and_keeps_its_place() {
    let (mut h, doc) = new_doc();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    let drawn = h.scene(doc).unwrap().items[0].path.bounds().unwrap();
    assert!(enabled(&h, CommandKey::SketchFromSelection), "the new rectangle is selected");

    h.invoke(doc, CommandKey::SketchFromSelection);
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::CreateFrom)));
    assert_eq!(vm(&h, doc).sketches.len(), 1);
    assert_eq!(mode(&h, doc).rows.len(), 0, "no relation is guessed");
    let scene = h.scene(doc).unwrap();
    assert_eq!(scene.items.len(), 1, "the sketch's contour replaces the drawn rectangle");
    assert_eq!(scene.items[0].path.bounds().unwrap(), drawn);

    h.invoke(doc, CommandKey::Undo);
    assert!(vm(&h, doc).sketches.is_empty());
    assert!(vm(&h, doc).sketch_mode.is_none());
    assert_eq!(h.scene(doc).unwrap().items[0].path.bounds().unwrap(), drawn, "the drawn rectangle is back");
}

#[test]
fn a_selection_with_curves_cannot_become_a_sketch() {
    let (mut h, doc) = new_doc();
    h.invoke(doc, CommandKey::ToolEllipse);
    h.drag(doc, (0.0, 0.0), (30.0, 20.0));
    assert_eq!(vm(&h, doc).objects.len(), 1, "an ellipse is drawn and selected");
    let revision = vm(&h, doc).revision;
    let update = h.invoke(doc, CommandKey::SketchFromSelection);
    assert_eq!(vm(&h, doc).revision, revision, "nothing changed");
    assert!(
        update
            .effects
            .iter()
            .any(|f| matches!(f, HostEffect::ShowError { message: MessageKey::SketchConversionImpossible, .. })),
        "{:?}",
        update.effects
    );
    assert!(vm(&h, doc).sketch_mode.is_none());
}

// ---------------------------------------------------------------------------------------------
// Drawing with relation suggestions (S01)
// ---------------------------------------------------------------------------------------------

/// S01: a suggestion becomes a constraint only when the gesture ends without the suppress modifier.
#[test]
fn suppressed_suggestion_creates_no_constraint() {
    let (mut h, doc) = new_doc();
    begin(&mut h, doc);
    h.invoke(doc, CommandKey::ToolLine);

    h.drag_with(doc, (0.0, 0.0), (30.0, 1.0), suppress());
    assert_eq!(vm(&h, doc).sketches[0].constraints, 0, "suppressed: no horizontal relation");
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::Batch)));

    h.drag(doc, (0.0, 10.0), (30.0, 11.0));
    assert_eq!(kinds(&h, doc), vec![SketchConstraintKind::Horizontal], "not suppressed: horizontal");
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::Batch)));

    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&h, doc).sketches[0].constraints, 0, "one undo takes the whole second line back");
    assert_eq!(h.scene(doc).unwrap().items.len(), 1, "the first line stays");
}

#[test]
fn the_suggestion_is_shown_while_drawing_and_only_if_not_suppressed() {
    let (mut h, doc) = new_doc();
    begin(&mut h, doc);
    h.invoke(doc, CommandKey::ToolLine);
    let suggested = |h: &H| {
        overlays(h, doc)
            .iter()
            .filter_map(|o| match o {
                Overlay::SketchSuggestion { glyph, .. } => Some(*glyph),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    h.begin_drag(doc, (0.0, 20.0), (30.0, 20.5));
    assert_eq!(suggested(&h), vec![SketchGlyph::Horizontal]);
    h.escape(doc);
    h.begin_drag_with(doc, (0.0, 20.0), (30.0, 20.5), suppress());
    assert!(suggested(&h).is_empty(), "suppressed while the modifier is held");
    h.escape(doc);
    assert!(suggested(&h).is_empty(), "gone with the gesture");
}

#[test]
fn a_rectangle_drawn_in_a_sketch_has_four_axis_relations_and_is_one_undo_entry() {
    let (mut h, doc) = new_doc();
    begin(&mut h, doc);
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    use SketchConstraintKind::{Horizontal, Vertical};
    assert_eq!(kinds(&h, doc), vec![Horizontal, Vertical, Horizontal, Vertical]);
    assert_eq!(mode(&h, doc).status, SketchStatus::Valid);
    assert_eq!(h.scene(doc).unwrap().items.len(), 1, "one closed contour");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(mode(&h, doc).rows.len(), 0);
    assert!(h.scene(doc).unwrap().items.is_empty());
}

#[test]
fn a_line_that_starts_on_an_end_point_shares_it_so_the_contour_stays_in_one_piece() {
    let (mut h, doc) = new_doc();
    begin(&mut h, doc);
    h.invoke(doc, CommandKey::ToolLine);
    h.drag(doc, (0.0, 0.0), (30.0, 20.0));
    h.drag(doc, (30.0, 20.5), (60.0, 40.0));
    assert_eq!(h.scene(doc).unwrap().items.len(), 1, "one chain, not two lines");
    h.drag_with(doc, (60.0, 40.5), (90.0, 20.0), suppress());
    assert_eq!(h.scene(doc).unwrap().items.len(), 2, "suppressed: the third line is separate");
}

#[test]
fn a_shape_a_sketch_cannot_hold_is_refused_and_changes_nothing() {
    let (mut h, doc) = new_doc();
    begin(&mut h, doc);
    // An ellipse is a sketch entity since G6-editor; a slot (lines and arcs in one outline) is not.
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.slot.center_to_center".to_owned() });
    let revision = vm(&h, doc).revision;
    for at in [(0.0, 0.0), (30.0, 0.0), (15.0, 5.0)] {
        h.click(doc, at, Modifiers::default());
    }
    assert_eq!(vm(&h, doc).revision, revision);
    assert_eq!(mode(&h, doc).refused, Some(MessageKey::SketchShapeUnsupported));
}

// ---------------------------------------------------------------------------------------------
// Selecting entities and relating them
// ---------------------------------------------------------------------------------------------

#[test]
fn clicking_selects_sketch_entities_and_shift_adds_more() {
    let (mut h, doc) = opened(loose().sketch);
    let sketch = vm(&h, doc).sketches[0].id;
    h.invoke(doc, CommandKey::SketchEdit);
    assert!(vm(&h, doc).sketch_mode.is_none(), "no object selected, so no mode");
    enter(&mut h, doc);
    assert_eq!(mode(&h, doc).sketch, sketch);

    h.click(doc, (20.0, 1.0), Modifiers::default());
    assert_eq!(mode(&h, doc).selected_entities, vec![e(2)], "the first line");
    h.click(doc, (19.0, 22.5), shift());
    assert_eq!(mode(&h, doc).selected_entities, vec![e(2), e(5)], "shift adds the second line");
    h.click(doc, (19.0, 22.5), shift());
    assert_eq!(mode(&h, doc).selected_entities, vec![e(2)], "shift on a selected entity removes it");
    h.click(doc, (0.0, 0.0), Modifiers::default());
    assert_eq!(mode(&h, doc).selected_entities, vec![e(0)], "a point wins over the line it ends");
    h.click(doc, (200.0, 200.0), Modifiers::default());
    assert!(mode(&h, doc).selected_entities.is_empty(), "empty canvas clears");
    select_entities(&mut h, doc, &[e(2)]);
    h.escape(doc);
    assert!(mode(&h, doc).selected_entities.is_empty(), "Esc clears the selection");
}

fn enter(h: &mut H, doc: DocumentId) {
    enter_at(h, doc, (20.0, 1.0));
}

/// Selects the sketch's contour under `at` and opens the sketch it came from.
fn enter_at(h: &mut H, doc: DocumentId, at: (f64, f64)) {
    h.click(doc, at, Modifiers::default());
    h.invoke(doc, CommandKey::SketchEdit);
}

#[test]
fn relation_commands_follow_what_is_selected() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    let off = |h: &H| {
        [
            CommandKey::SketchCoincident,
            CommandKey::SketchHorizontal,
            CommandKey::SketchVertical,
            CommandKey::SketchParallel,
            CommandKey::SketchPerpendicular,
            CommandKey::SketchEqual,
            CommandKey::SketchTangent,
            CommandKey::SketchFix,
            CommandKey::SketchDimension,
        ]
        .into_iter()
        .filter(|k| !enabled(h, *k))
        .collect::<Vec<_>>()
    };
    select_entities(&mut h, doc, &[]);
    assert_eq!(off(&h).len(), 9, "nothing selected: nothing applies");
    select_entities(&mut h, doc, &[e(2)]);
    assert!(enabled(&h, CommandKey::SketchHorizontal) && enabled(&h, CommandKey::SketchVertical));
    assert!(enabled(&h, CommandKey::SketchDimension), "a line has a length");
    assert!(!enabled(&h, CommandKey::SketchParallel), "needs two lines");
    select_entities(&mut h, doc, &[e(2), e(5)]);
    assert!(enabled(&h, CommandKey::SketchParallel) && enabled(&h, CommandKey::SketchPerpendicular));
    assert!(enabled(&h, CommandKey::SketchEqual));
    assert!(!enabled(&h, CommandKey::SketchHorizontal), "one line only");
    select_entities(&mut h, doc, &[e(1), e(4)]);
    assert!(enabled(&h, CommandKey::SketchCoincident));
    assert!(enabled(&h, CommandKey::SketchFix));
    select_entities(&mut h, doc, &[e(2), e(7)]);
    assert!(enabled(&h, CommandKey::SketchTangent), "a line and a circle");
    select_entities(&mut h, doc, &[e(7), e(9)]);
    assert!(enabled(&h, CommandKey::SketchEqual), "two circles");
}

#[test]
fn every_relation_command_adds_its_relation_as_one_undo_entry() {
    use SketchConstraintKind as K;
    let cases: [(CommandKey, Vec<EntityId>, K); 8] = [
        (CommandKey::SketchCoincident, vec![e(1), e(4)], K::Coincident),
        (CommandKey::SketchHorizontal, vec![e(2)], K::Horizontal),
        (CommandKey::SketchVertical, vec![e(5)], K::Vertical),
        (CommandKey::SketchParallel, vec![e(2), e(5)], K::Parallel),
        (CommandKey::SketchPerpendicular, vec![e(2), e(5)], K::Perpendicular),
        (CommandKey::SketchEqual, vec![e(7), e(9)], K::Equal),
        (CommandKey::SketchTangent, vec![e(2), e(7)], K::Tangent),
        (CommandKey::SketchFix, vec![e(0)], K::Fixed),
    ];
    for (command, selection, kind) in cases {
        let (mut h, doc) = opened(loose().sketch);
        enter(&mut h, doc);
        select_entities(&mut h, doc, &selection);
        let revision = vm(&h, doc).revision;
        h.invoke(doc, command);
        assert_eq!(kinds(&h, doc), vec![kind], "{command:?}");
        assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::AddConstraint)), "{command:?}");
        assert!(vm(&h, doc).revision > revision, "{command:?}");
        h.invoke(doc, CommandKey::Undo);
        assert!(mode(&h, doc).rows.is_empty(), "{command:?}: one undo removes it");
    }
}

#[test]
fn a_relation_that_does_not_fit_the_selection_changes_nothing_and_says_so() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    select_entities(&mut h, doc, &[e(2)]);
    let revision = vm(&h, doc).revision;
    h.invoke(doc, CommandKey::SketchParallel);
    assert_eq!(vm(&h, doc).revision, revision);
    assert_eq!(mode(&h, doc).refused, Some(MessageKey::SketchSelectionDoesNotFit));
}

// ---------------------------------------------------------------------------------------------
// Dimensions
// ---------------------------------------------------------------------------------------------

#[test]
fn a_dimension_takes_the_drawn_value_then_raw_text_and_the_geometry_follows() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    let sketch = mode(&h, doc).sketch;
    select_entities(&mut h, doc, &[e(2)]);
    let update =
        h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: String::new() });
    let row = mode(&h, doc).rows[0].clone();
    assert_eq!(row.kind, SketchConstraintKind::Length);
    assert_eq!(row.text.as_deref(), Some("40.05 mm"), "the value as drawn");
    assert_eq!(row.role, Some(DimensionRole::Driving));
    assert!(
        update
            .effects
            .iter()
            .any(|f| matches!(f, HostEffect::EditSketchDimension { constraint, .. } if *constraint == row.id)),
        "the host starts editing the new dimension: {:?}",
        update.effects
    );

    h.send(UiIntent::SetSketchDimension { document: doc, sketch, constraint: row.id, text: "60 mm".into() });
    let after = mode(&h, doc);
    assert_eq!(after.status, SketchStatus::Valid);
    assert_eq!(after.rows[0].value.as_deref(), Some("60"), "the line is 60 mm now");
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::SetDimensionValue)));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(mode(&h, doc).rows[0].value.as_deref(), Some("40.05"), "one undo restores the length");
}

#[test]
fn dimension_kinds_follow_the_selection() {
    use SketchConstraintKind as K;
    let cases: [(SketchDimensionKind, Vec<EntityId>, K); 6] = [
        (SketchDimensionKind::Auto, vec![e(2)], K::Length),
        (SketchDimensionKind::Auto, vec![e(0), e(4)], K::Distance),
        (SketchDimensionKind::Auto, vec![e(7)], K::Radius),
        (SketchDimensionKind::Diameter, vec![e(7)], K::Diameter),
        (SketchDimensionKind::Auto, vec![e(2), e(5)], K::Angle),
        (SketchDimensionKind::DistanceX, vec![e(0), e(1)], K::DistanceX),
    ];
    for (kind, selection, expected) in cases {
        let (mut h, doc) = opened(loose().sketch);
        enter(&mut h, doc);
        select_entities(&mut h, doc, &selection);
        h.send(UiIntent::AddSketchDimension { document: doc, kind, text: String::new() });
        assert_eq!(kinds(&h, doc), vec![expected], "{kind:?} on {selection:?}");
        assert_eq!(mode(&h, doc).status, SketchStatus::Valid, "{kind:?}: the drawn value is consistent");
    }
}

#[test]
fn a_dimension_with_typed_text_takes_it_raw() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    select_entities(&mut h, doc, &[e(7)]);
    h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Radius, text: "8 mm".into() });
    let row = mode(&h, doc).rows[0].clone();
    assert_eq!((row.text.as_deref(), row.value.as_deref()), (Some("8 mm"), Some("8")));
}

#[test]
fn double_clicking_a_dimension_label_starts_editing_it() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    select_entities(&mut h, doc, &[e(2)]);
    h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: String::new() });
    let row = mode(&h, doc).rows[0].clone();
    let label_at = overlays(&h, doc)
        .iter()
        .find_map(|o| match o {
            Overlay::SketchDimension { label_at, .. } => Some(*label_at),
            _ => None,
        })
        .expect("the dimension is drawn");

    h.click(doc, (label_at.x, label_at.y), Modifiers::default());
    assert_eq!(mode(&h, doc).selected_constraints, vec![row.id], "one click selects the dimension");
    let update = h.double_click(doc, (label_at.x, label_at.y));
    assert!(
        update
            .effects
            .iter()
            .any(|f| matches!(f, HostEffect::EditSketchDimension { constraint, text, .. } if *constraint == row.id && text == "40.05 mm")),
        "{:?}",
        update.effects
    );
}

#[test]
fn a_driven_dimension_measures_and_is_shown_in_brackets() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    let sketch = mode(&h, doc).sketch;
    select_entities(&mut h, doc, &[e(2)]);
    h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: String::new() });
    let id = mode(&h, doc).rows[0].id;
    h.send(UiIntent::SetSketchDimensionRole { document: doc, sketch, constraint: id, role: DimensionRole::Driven });
    assert_eq!(mode(&h, doc).rows[0].role, Some(DimensionRole::Driven));
    assert_eq!(mode(&h, doc).rows[0].health, ConstraintHealth::Measured);
    let text = overlays(&h, doc).iter().find_map(|o| match o {
        Overlay::SketchDimension { text, driven, .. } => Some((text.clone(), *driven)),
        _ => None,
    });
    assert_eq!(text, Some(("(40.05)".to_owned(), true)));
}

// ---------------------------------------------------------------------------------------------
// Removing constraints
// ---------------------------------------------------------------------------------------------

#[test]
fn selected_constraints_are_removed_together_in_one_undo_entry() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    for (command, selection) in [(CommandKey::SketchHorizontal, e(2)), (CommandKey::SketchVertical, e(5))] {
        select_entities(&mut h, doc, &[selection]);
        h.invoke(doc, command);
    }
    let ids: Vec<ConstraintId> = mode(&h, doc).rows.iter().map(|r| r.id).collect();
    assert_eq!(ids.len(), 2);
    assert!(!enabled(&h, CommandKey::SketchRemoveConstraint), "no constraint selected");
    select_constraints(&mut h, doc, &ids);
    assert!(enabled(&h, CommandKey::SketchRemoveConstraint));
    h.invoke(doc, CommandKey::SketchRemoveConstraint);
    assert!(mode(&h, doc).rows.is_empty());
    assert!(mode(&h, doc).selected_constraints.is_empty());
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(mode(&h, doc).rows.len(), 2, "one undo brings both back");
}

#[test]
fn the_delete_key_removes_the_selected_constraint() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    select_entities(&mut h, doc, &[e(2)]);
    h.invoke(doc, CommandKey::SketchHorizontal);
    let id = mode(&h, doc).rows[0].id;
    select_constraints(&mut h, doc, &[id]);
    h.key(doc, CanvasKey::Delete);
    assert!(mode(&h, doc).rows.is_empty());
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::RemoveConstraint)));
}

// ---------------------------------------------------------------------------------------------
// The four stories
// ---------------------------------------------------------------------------------------------

/// A slot `sleufbreedte` wide and 20 mm high; width from parameters.
fn slot() -> (Sketch, ConstraintId) {
    let mut s = Sketch::new("Sleuf");
    s.set_parameter("materiaaldikte", "3.1 mm").unwrap();
    s.set_parameter("speling", "0.12 mm").unwrap();
    s.set_parameter("sleufbreedte", "materiaaldikte + speling").unwrap();
    let p = [
        s.point(0.0, 0.0).unwrap(),
        s.point(3.0, 0.2).unwrap(),
        s.point(3.1, 19.0).unwrap(),
        s.point(0.1, 20.0).unwrap(),
    ];
    let l = [
        s.line(p[0], p[1]).unwrap(),
        s.line(p[1], p[2]).unwrap(),
        s.line(p[2], p[3]).unwrap(),
        s.line(p[3], p[0]).unwrap(),
    ];
    s.relate(Relation::Fix { point: p[0], at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Horizontal(l[0])).unwrap();
    s.relate(Relation::Vertical(l[1])).unwrap();
    s.relate(Relation::Horizontal(l[2])).unwrap();
    s.relate(Relation::Vertical(l[3])).unwrap();
    let width = s
        .dimension(
            DimensionKind::Distance { a: p[0], b: p[1], along: Along::X },
            "sleufbreedte",
            DimensionRole::Driving,
        )
        .unwrap();
    s.dimension(DimensionKind::Distance { a: p[1], b: p[2], along: Along::Y }, "20 mm", DimensionRole::Driving)
        .unwrap();
    (s, width)
}

fn value_of(h: &H, doc: DocumentId, id: ConstraintId) -> Option<String> {
    mode(h, doc).rows.iter().find(|r| r.id == id).and_then(|r| r.value.clone())
}

/// S02: one parameter edit moves everything derived from it, and one undo takes all of it back.
#[test]
fn parameter_change_with_derivatives_is_one_undo() {
    let (sketch, width) = slot();
    let (mut h, doc) = opened(sketch);
    enter_at(&mut h, doc, (1.6, 0.0));
    let id = mode(&h, doc).sketch;
    assert_eq!(value_of(&h, doc, width).as_deref(), Some("3.22"));
    let revision = vm(&h, doc).revision;

    h.send(UiIntent::SetSketchParameter {
        document: doc,
        sketch: id,
        name: "materiaaldikte".into(),
        text: "4 mm".into(),
    });
    assert_eq!(value_of(&h, doc, width).as_deref(), Some("4.12"), "sleufbreedte follows materiaaldikte");
    assert_eq!(mode(&h, doc).status, SketchStatus::Valid);
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Sketch(SketchLabel::SetParameter)));
    assert_eq!(h.scene(doc).unwrap().items[0].path.bounds().unwrap().width(), 4.12);

    h.invoke(doc, CommandKey::Undo);
    assert_eq!(value_of(&h, doc, width).as_deref(), Some("3.22"), "one undo undoes parameter and derivatives");
    assert!(mode(&h, doc).parameters.contains(&("materiaaldikte".to_owned(), "3.1 mm".to_owned())));
    assert!(vm(&h, doc).revision > revision);
}

/// A 100 × 40 rectangle (fixed corner, width and height driving) with a driven top-edge length.
fn plate() -> (Sketch, ConstraintId, ConstraintId) {
    let mut s = Sketch::new("Plaat");
    let p = [
        s.point(0.0, 0.0).unwrap(),
        s.point(98.0, 1.0).unwrap(),
        s.point(99.0, 41.0).unwrap(),
        s.point(1.0, 39.0).unwrap(),
    ];
    let l = [
        s.line(p[0], p[1]).unwrap(),
        s.line(p[1], p[2]).unwrap(),
        s.line(p[2], p[3]).unwrap(),
        s.line(p[3], p[0]).unwrap(),
    ];
    s.relate(Relation::Fix { point: p[0], at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Horizontal(l[0])).unwrap();
    s.relate(Relation::Vertical(l[1])).unwrap();
    s.relate(Relation::Horizontal(l[2])).unwrap();
    s.relate(Relation::Vertical(l[3])).unwrap();
    s.dimension(DimensionKind::Distance { a: p[0], b: p[1], along: Along::X }, "100 mm", DimensionRole::Driving)
        .unwrap();
    s.dimension(DimensionKind::Distance { a: p[1], b: p[2], along: Along::Y }, "40 mm", DimensionRole::Driving)
        .unwrap();
    let top = s.dimension(DimensionKind::Length { line: l[2] }, "", DimensionRole::Driven).unwrap();
    (s, top, ConstraintId::from_raw(0))
}

/// S03: turning a measuring dimension into a driving one that contradicts the others is accepted
/// and shown as a conflict; the last valid geometry stays, and the conflict is marked on the canvas.
#[test]
fn invalid_driving_conversion_shows_conflict() {
    let (sketch, top, _) = plate();
    let (mut h, doc) = opened(sketch);
    enter_at(&mut h, doc, (50.0, 0.0));
    let id = mode(&h, doc).sketch;
    assert_eq!(mode(&h, doc).rows.iter().find(|r| r.id == top).unwrap().health, ConstraintHealth::Measured);
    let size = h.scene(doc).unwrap().items[0].path.bounds().unwrap();
    assert!((size.width() - 100.0).abs() < 1e-6);

    h.send(UiIntent::SetSketchDimension { document: doc, sketch: id, constraint: top, text: "50 mm".into() });
    assert_eq!(mode(&h, doc).status, SketchStatus::Valid, "still only measuring");
    h.send(UiIntent::SetSketchDimensionRole {
        document: doc,
        sketch: id,
        constraint: top,
        role: DimensionRole::Driving,
    });

    let m = mode(&h, doc);
    assert_eq!(m.status, SketchStatus::Failed);
    assert_eq!(m.problem, Some(MessageKey::SketchConflict));
    assert!(m.problem_constraints.contains(&top), "{:?}", m.problem_constraints);
    assert!(m.problem_constraints.len() >= 2, "it conflicts with something");
    assert_eq!(m.rows.iter().find(|r| r.id == top).unwrap().health, ConstraintHealth::Conflicting);
    assert_eq!(h.scene(doc).unwrap().items[0].path.bounds().unwrap(), size, "last valid geometry stays");
    assert!(
        overlays(&h, doc).iter().any(|o| matches!(o, Overlay::SketchDimension { mark: SketchMark::Conflict, .. })),
        "the conflict is marked on the canvas, not only in the list"
    );

    h.invoke(doc, CommandKey::Undo);
    assert_eq!(mode(&h, doc).status, SketchStatus::Valid, "back to a valid sketch");
}

/// A feasible triangle (a fixed, |ab| = 10, |ac| = |cb| = 6).
fn triangle() -> (Sketch, ConstraintId) {
    let mut s = Sketch::new("Driehoek");
    let a = s.point(0.0, 0.0).unwrap();
    let b = s.point(10.0, 0.0).unwrap();
    let c = s.point(5.0, 5.0).unwrap();
    s.relate(Relation::Fix { point: a, at: Point::new(0.0, 0.0) }).unwrap();
    let ab =
        s.dimension(DimensionKind::Distance { a, b, along: Along::Direct }, "10 mm", DimensionRole::Driving).unwrap();
    s.dimension(DimensionKind::Distance { a, b: c, along: Along::Direct }, "6 mm", DimensionRole::Driving).unwrap();
    s.dimension(DimensionKind::Distance { a: c, b, along: Along::Direct }, "6 mm", DimensionRole::Driving).unwrap();
    s.line(a, b).unwrap();
    s.line(b, c).unwrap();
    s.line(c, a).unwrap();
    (s, ab)
}

/// S04: a sketch the solver cannot satisfy keeps its last valid geometry, says so in words, and
/// the message selects the constraints involved; undo is the way back.
#[test]
fn not_converged_sketch_keeps_its_geometry_and_names_the_constraints_involved() {
    let (sketch, ab) = triangle();
    let (mut h, doc) = opened(sketch);
    enter_at(&mut h, doc, (5.0, 0.0));
    let id = mode(&h, doc).sketch;
    let size = h.scene(doc).unwrap().items[0].path.bounds().unwrap();

    // |ab| = 20 can never be reached with |ac| + |cb| = 12.
    h.send(UiIntent::SetSketchDimension { document: doc, sketch: id, constraint: ab, text: "20 mm".into() });
    let m = mode(&h, doc);
    assert_eq!(m.status, SketchStatus::Failed);
    assert_eq!(m.problem, Some(MessageKey::SketchNotConverged), "the solver did not converge, which is not a conflict");
    assert!(m.rows.iter().all(|r| r.health == ConstraintHealth::Unmet), "every constraint of the triangle is unmet");
    assert!(!m.problem_constraints.is_empty(), "the unmet constraints are named");
    assert_eq!(h.scene(doc).unwrap().items[0].path.bounds().unwrap(), size, "last valid geometry stays");

    h.send(UiIntent::SelectSketchProblem { document: doc });
    let m = mode(&h, doc);
    assert_eq!(m.selected_constraints, m.problem_constraints, "the message selects what it names");
    assert!(m.rows.iter().filter(|r| r.selected).count() == m.problem_constraints.len());

    h.invoke(doc, CommandKey::Undo);
    assert_eq!(mode(&h, doc).status, SketchStatus::Valid);
}

// ---------------------------------------------------------------------------------------------
// What the canvas shows
// ---------------------------------------------------------------------------------------------

#[test]
fn the_canvas_shows_relation_glyphs_and_how_free_each_point_is() {
    let (mut h, doc) = opened(loose().sketch);
    enter(&mut h, doc);
    select_entities(&mut h, doc, &[e(2)]);
    h.invoke(doc, CommandKey::SketchHorizontal);
    select_entities(&mut h, doc, &[e(0)]);
    h.invoke(doc, CommandKey::SketchFix);
    let o = overlays(&h, doc);
    assert!(o.iter().any(|x| matches!(x, Overlay::SketchGlyph { glyph: SketchGlyph::Horizontal, .. })));
    assert!(o.iter().any(|x| matches!(x, Overlay::SketchGlyph { glyph: SketchGlyph::Fixed, .. })));
    assert!(o.iter().any(|x| matches!(x, Overlay::SketchPoint { freedom: PointFreedom::Fixed, .. })));
    assert!(
        o.iter().any(|x| matches!(x, Overlay::SketchPoint { freedom: PointFreedom::Free, .. })),
        "loose points are free"
    );
    assert!(mode(&h, doc).dof.is_some_and(|d| d > 0));
}

#[test]
fn a_fully_constrained_sketch_has_no_free_point_and_no_freedom_left() {
    let (sketch, _) = slot();
    let (mut h, doc) = opened(sketch);
    enter_at(&mut h, doc, (1.6, 0.0));
    assert_eq!(mode(&h, doc).dof, Some(0));
    assert!(!overlays(&h, doc).iter().any(|x| matches!(x, Overlay::SketchPoint { freedom: PointFreedom::Free, .. })));
}

#[test]
fn outside_sketch_mode_the_canvas_has_no_sketch_overlays() {
    let (sketch, _) = slot();
    let (h, doc) = opened(sketch);
    assert!(vm(&h, doc).sketch_mode.is_none());
    assert!(!overlays(&h, doc).iter().any(|x| matches!(
        x,
        Overlay::SketchGlyph { .. } | Overlay::SketchPoint { .. } | Overlay::SketchDimension { .. }
    )));
}
