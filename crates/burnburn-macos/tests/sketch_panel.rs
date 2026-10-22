//! The sketch panel and the Schets menu, headless (W1-L1b, B8): the constraint list shows the
//! view model's rows, edits become intents with the typed text unparsed (DR-05), the conflict
//! message selects what it names. No AppKit here; `tests/native/macos/sketch_mode.rs` drives the
//! real widgets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::sketch::{DimensionRole, EntityId};
use burnburn_editor::Modifiers;
use burnburn_macos::adapter::{MenuItem, NativeEvent, Reaction, menu_items, menu_tag, menu_title, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{SKETCH_COLUMNS, SketchColumn, sketch_panel, sketch_row_of};
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, SelectMode, SketchDimensionKind, SketchStatus, UiIntent,
};
use burnburn_types::DocumentId;

fn intent(h: &HeadlessEditorHarness, event: NativeEvent) -> Option<UiIntent> {
    match translate(event, &h.view()) {
        Reaction::Intent(i) => Some(i),
        _ => None,
    }
}

fn act(h: &mut HeadlessEditorHarness, event: NativeEvent) -> Option<UiIntent> {
    let i = intent(h, event)?;
    h.send(i.clone());
    Some(i)
}

/// A new sketch with a 20 × 10 rectangle drawn into it (four axis relations).
fn rectangle_sketch() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::SketchNew);
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    (h, doc)
}

fn e(raw: u32) -> EntityId {
    EntityId::from_raw(raw)
}

fn select_side(h: &mut HeadlessEditorHarness, doc: DocumentId, side: u32) {
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: vec![e(side)],
        constraints: vec![],
        mode: SelectMode::Replace,
    });
}

#[test]
fn there_is_no_panel_outside_sketch_mode() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(sketch_panel(&h.view(), doc, Locale::Nl).is_none());
}

#[test]
fn the_panel_lists_each_constraint_with_its_kind_and_status_in_words() {
    let (h, doc) = rectangle_sketch();
    let panel = sketch_panel(&h.view(), doc, Locale::Nl).expect("panel");
    let kinds: Vec<&str> = panel.rows.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, vec!["Horizontaal", "Verticaal", "Horizontaal", "Verticaal"]);
    assert!(panel.rows.iter().all(|r| r.status.contains("voldaan")), "{:?}", panel.rows);
    assert!(panel.rows.iter().all(|r| !r.editable), "relations have no value to edit");
    assert!(panel.freedom.contains("Vrijheidsgraden"), "{}", panel.freedom);
    assert!(panel.problem.is_empty());
    let en = sketch_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(en.rows[0].kind, "Horizontal");
    assert!(en.freedom.contains("Degrees of freedom"));
    assert_eq!(SKETCH_COLUMNS.map(|c| SketchColumn::from_id(c.id())), SKETCH_COLUMNS.map(Some));
}

/// S05: a curve that ended on another geometric branch is said in words next to the freedom
/// (shared by the macOS and Windows panels); without a flip the text is the plain freedom.
#[test]
fn a_branch_change_is_said_in_the_panel_next_to_the_freedom() {
    let (h, doc) = rectangle_sketch();
    let mut vm = (*h.view()).clone();
    let plain = sketch_panel(&vm, doc, Locale::Nl).unwrap().freedom;
    assert!(!plain.contains("andere kant"), "{plain}");
    let index = vm.documents.iter().position(|d| d.id == doc).unwrap();
    vm.documents[index].sketch_mode.as_mut().unwrap().branch_flips = 2;
    let nl = sketch_panel(&vm, doc, Locale::Nl).unwrap().freedom;
    assert!(nl.starts_with(&plain) && nl.contains("andere kant") && nl.contains('2'), "{nl}");
    let en = sketch_panel(&vm, doc, Locale::En).unwrap().freedom;
    assert!(en.contains("other side") && en.contains('2'), "{en}");
}

#[test]
fn a_dimension_row_shows_its_text_and_is_editable_and_a_fully_constrained_sketch_says_so() {
    let (mut h, doc) = rectangle_sketch();
    select_side(&mut h, doc, 4);
    h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: "20 mm".into() });
    let panel = sketch_panel(&h.view(), doc, Locale::En).unwrap();
    let row = panel.rows.last().unwrap();
    assert_eq!((row.kind.as_str(), row.value.as_str(), row.editable), ("Length", "20 mm", true));
    assert!(!row.driven);
    assert!(panel.rows[..4].iter().all(|r| r.value.is_empty()));
}

#[test]
fn selecting_table_rows_selects_constraints_and_keeps_the_selected_entities() {
    let (mut h, doc) = rectangle_sketch();
    select_side(&mut h, doc, 5);
    let before = h.document(doc).unwrap().sketch_mode.unwrap();
    let sent = act(&mut h, NativeEvent::SketchRowsSelected { document: doc, rows: vec![0, 2] }).expect("intent");
    let now = h.document(doc).unwrap().sketch_mode.unwrap();
    assert_eq!(now.selected_constraints, vec![before.rows[0].id, before.rows[2].id]);
    assert_eq!(now.selected_entities, vec![e(5)], "the entities stay selected");
    assert!(matches!(sent, UiIntent::SketchSelect { mode: SelectMode::Replace, .. }));
    let panel = sketch_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(panel.remove_enabled);
    assert!(!panel.role_enabled, "no dimension selected");
    // Same rows again: nothing to send.
    assert_eq!(intent(&h, NativeEvent::SketchRowsSelected { document: doc, rows: vec![0, 2] }), None);
}

#[test]
fn a_value_edit_sends_the_raw_text_once_and_only_for_a_dimension() {
    let (mut h, doc) = rectangle_sketch();
    select_side(&mut h, doc, 4);
    h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: "20 mm".into() });
    let vm = h.document(doc).unwrap().sketch_mode.unwrap();
    let dimension = vm.rows.last().unwrap().clone();
    let row = vm.rows.len() - 1;
    let edit = |text: &str| intent(&h, NativeEvent::SketchValueCommitted { document: doc, row, text: text.into() });
    assert_eq!(
        edit("breedte + 5 mm"),
        Some(UiIntent::SetSketchDimension {
            document: doc,
            sketch: vm.sketch,
            constraint: dimension.id,
            text: "breedte + 5 mm".into()
        }),
        "raw text, not parsed by the host"
    );
    assert_eq!(edit("20 mm"), None, "unchanged text sends nothing (Return plus end of editing)");
    assert_eq!(
        intent(&h, NativeEvent::SketchValueCommitted { document: doc, row: 0, text: "5".into() }),
        None,
        "a relation has no value"
    );
    assert_eq!(intent(&h, NativeEvent::SketchValueCommitted { document: doc, row: 99, text: "5".into() }), None);
    act(&mut h, NativeEvent::SketchValueCommitted { document: doc, row, text: "30 mm".into() }).unwrap();
    assert_eq!(h.document(doc).unwrap().sketch_mode.unwrap().rows[row].value.as_deref(), Some("30"));
}

#[test]
fn meten_sturen_toggles_the_selected_dimension() {
    let (mut h, doc) = rectangle_sketch();
    select_side(&mut h, doc, 4);
    h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: String::new() });
    let vm = h.document(doc).unwrap().sketch_mode.unwrap();
    let id = vm.rows.last().unwrap().id;
    assert_eq!(vm.selected_constraints, vec![id], "the new dimension is selected");
    assert!(sketch_panel(&h.view(), doc, Locale::Nl).unwrap().role_enabled);
    let sent = act(&mut h, NativeEvent::SketchRoleRequested { document: doc }).expect("intent");
    assert_eq!(
        sent,
        UiIntent::SetSketchDimensionRole {
            document: doc,
            sketch: vm.sketch,
            constraint: id,
            role: DimensionRole::Driven
        }
    );
    let panel = sketch_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.rows.last().unwrap().driven);
    assert!(panel.rows.last().unwrap().status.contains("measured"));
    let back = act(&mut h, NativeEvent::SketchRoleRequested { document: doc }).unwrap();
    assert!(matches!(back, UiIntent::SetSketchDimensionRole { role: DimensionRole::Driving, .. }));
    select_side(&mut h, doc, 4);
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: vec![],
        constraints: vec![vm.rows[0].id],
        mode: SelectMode::Replace,
    });
    assert_eq!(intent(&h, NativeEvent::SketchRoleRequested { document: doc }), None, "a relation cannot be measured");
}

#[test]
fn the_conflict_is_said_in_words_and_clicking_it_selects_what_it_names() {
    let (mut h, doc) = rectangle_sketch();
    // Two contradicting lengths on one side.
    for text in ["20 mm", "30 mm"] {
        select_side(&mut h, doc, 4);
        h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: text.into() });
    }
    let vm = h.document(doc).unwrap().sketch_mode.unwrap();
    assert_eq!(vm.status, SketchStatus::Failed);
    let panel = sketch_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(panel.problem.contains("tegenstrijdig"), "{}", panel.problem);
    assert!(panel.problem_hint.contains("Klik"), "{}", panel.problem_hint);
    assert!(panel.rows.iter().any(|r| r.status.contains("conflict")));
    let sent = act(&mut h, NativeEvent::SketchProblemClicked { document: doc }).expect("intent");
    assert_eq!(sent, UiIntent::SelectSketchProblem { document: doc });
    let after = h.document(doc).unwrap().sketch_mode.unwrap();
    assert_eq!(after.selected_constraints, after.problem_constraints);
    assert!(sketch_panel(&h.view(), doc, Locale::En).unwrap().problem.contains("contradicting"));
}

#[test]
fn no_problem_means_nothing_to_click() {
    let (h, doc) = rectangle_sketch();
    assert_eq!(intent(&h, NativeEvent::SketchProblemClicked { document: doc }), None);
}

#[test]
fn a_parameter_is_set_with_its_raw_expression() {
    let (mut h, doc) = rectangle_sketch();
    let sketch = h.document(doc).unwrap().sketch_mode.unwrap().sketch;
    let sent = act(
        &mut h,
        NativeEvent::SketchParameterCommitted { document: doc, name: "dikte".into(), text: "3,1 mm".into() },
    )
    .expect("intent");
    assert_eq!(
        sent,
        UiIntent::SetSketchParameter { document: doc, sketch, name: "dikte".into(), text: "3,1 mm".into() },
        "the expression is not parsed here"
    );
    // W4-UI-gaten (S02): presentation reads the decimal comma and stores the unambiguous text.
    let panel = sketch_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(panel.parameters.contains(&("dikte".to_owned(), "3.1 mm".to_owned())));
    assert_eq!(
        intent(&h, NativeEvent::SketchParameterCommitted { document: doc, name: "  ".into(), text: "3 mm".into() }),
        None,
        "a parameter needs a name"
    );
    assert_eq!(
        intent(
            &h,
            NativeEvent::SketchParameterCommitted { document: doc, name: "dikte".into(), text: "3.1 mm".into() }
        ),
        None,
        "unchanged"
    );
}

#[test]
fn the_row_of_a_new_dimension_is_found_to_start_editing_it() {
    let (mut h, doc) = rectangle_sketch();
    select_side(&mut h, doc, 4);
    let update =
        h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: String::new() });
    let constraint = update
        .effects
        .iter()
        .find_map(|f| match f {
            burnburn_presentation::HostEffect::EditSketchDimension { constraint, .. } => Some(*constraint),
            _ => None,
        })
        .expect("edit effect");
    assert_eq!(sketch_row_of(&h.view(), doc, constraint), Some(4));
}

#[test]
fn the_schets_menu_commands_all_have_tags_and_titles_that_follow_the_selection() {
    let (mut h, doc) = rectangle_sketch();
    let sketch_items = [
        CommandKey::SketchNew,
        CommandKey::SketchFromSelection,
        CommandKey::SketchEdit,
        CommandKey::SketchFinish,
        CommandKey::SketchCoincident,
        CommandKey::SketchHorizontal,
        CommandKey::SketchVertical,
        CommandKey::SketchParallel,
        CommandKey::SketchPerpendicular,
        CommandKey::SketchEqual,
        CommandKey::SketchTangent,
        CommandKey::SketchFix,
        CommandKey::SketchDimension,
        CommandKey::SketchRemoveConstraint,
    ];
    for key in sketch_items {
        assert!(menu_items().contains(&MenuItem::Command(key)), "{key:?} is a menu item");
        assert!(menu_tag(MenuItem::Command(key)) >= 1);
        assert!(!menu_title(MenuItem::Command(key), &h.view(), Locale::Nl).starts_with('['), "{key:?} has a title");
    }
    select_side(&mut h, doc, 4);
    assert!(h.view().commands[&CommandKey::SketchHorizontal].enabled);
    assert!(!h.view().commands[&CommandKey::SketchParallel].enabled);
    let _ = Modifiers::default();
}

#[test]
fn the_schets_toolbar_runs_the_sketch_commands_and_follows_their_enablement() {
    use burnburn_macos::panels::{SKETCH_BUTTONS, SKETCH_TOOLBAR, sketch_toolbar_command, sketch_toolbar_enabled};
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert_eq!(sketch_toolbar_command(0), Some(CommandKey::SketchNew));
    assert_eq!(sketch_toolbar_command(SKETCH_TOOLBAR.len() as isize), None);
    assert_eq!(sketch_toolbar_command(-1), None);
    assert_eq!(
        sketch_toolbar_enabled(&h.view()),
        vec![true, false, false, false],
        "only New applies in an empty document"
    );
    let sent = act(&mut h, NativeEvent::PanelCommand { document: doc, command: CommandKey::SketchNew }).unwrap();
    assert_eq!(sent, UiIntent::Invoke { document: Some(doc), command: CommandKey::SketchNew });
    assert_eq!(sketch_toolbar_enabled(&h.view()), vec![true, false, false, true], "Finish applies in sketch mode");
    // Every panel button is a command of the menu.
    for key in SKETCH_BUTTONS {
        assert!(menu_items().contains(&MenuItem::Command(key)));
    }
}
