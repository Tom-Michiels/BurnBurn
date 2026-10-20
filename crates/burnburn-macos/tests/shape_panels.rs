//! The shape section, the tool options and the geometry dialog, headless (W2-L1-tekenen): what
//! the panels show comes from presentation's view model in the window language, and a typed value
//! becomes one intent with the text unparsed (DR-05). `tests/native/macos/shape_tools.rs` drives
//! the real widgets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{MenuItem, NativeEvent, Reaction, menu_enabled, menu_title, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{ParamSection, geometry_dialog_panel, shapes_panel};
use burnburn_presentation::{CommandKey, GeometryDialogKind, HeadlessEditorHarness, ParamKey, SelectMode, UiIntent};
use burnburn_types::DocumentId;

fn intent(h: &HeadlessEditorHarness, event: NativeEvent) -> Option<UiIntent> {
    match translate(event, &h.view()) {
        Reaction::Intent(i) => Some(i),
        _ => None,
    }
}

fn star() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SelectTool { document: Some(doc), id: "shape.star".into() });
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    (h, doc)
}

#[test]
fn a_selected_basic_shape_shows_its_parameters_with_captions_in_the_window_language() {
    let (h, doc) = star();
    let nl = shapes_panel(&h.view(), doc, Locale::Nl);
    assert_eq!(nl.shape_title, "Vorm");
    let rows: Vec<_> = nl.shape.iter().map(|r| (r.key, r.label.as_str(), r.text.as_str())).collect();
    assert_eq!(rows, [(ParamKey::Points, "Punten", "5"), (ParamKey::Ratio, "Verhouding", "0.382")]);
    let en = shapes_panel(&h.view(), doc, Locale::En);
    assert_eq!(en.shape[0].label, "Points");
    // The star tool is still active: its options are the second section.
    assert_eq!(nl.tool_title, "Gereedschapsopties");
    assert_eq!(nl.tool.len(), 2);
}

#[test]
fn nothing_is_shown_without_a_shape_or_tool_options() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let panel = shapes_panel(&h.view(), doc, Locale::En);
    assert!(panel.shape.is_empty() && panel.tool.is_empty());
}

#[test]
fn a_typed_shape_parameter_is_one_intent_and_the_same_text_is_none() {
    let (h, doc) = star();
    let commit = |text: &str| {
        intent(
            &h,
            NativeEvent::ParamCommitted { document: doc, section: ParamSection::Shape, slot: 0, text: text.into() },
        )
    };
    assert_eq!(
        commit("7"),
        Some(UiIntent::CommitShapeParam { document: doc, key: ParamKey::Points, text: "7".into() })
    );
    assert_eq!(commit("5"), None, "Return and end of editing are one commit");
    let beyond = intent(
        &h,
        NativeEvent::ParamCommitted { document: doc, section: ParamSection::Shape, slot: 9, text: "1".into() },
    );
    assert_eq!(beyond, None, "a slot without a row");
}

#[test]
fn tool_options_become_set_tool_option_intents_and_a_refused_one_shows_its_reason() {
    let (mut h, doc) = star();
    let event = |text: &str| NativeEvent::ParamCommitted {
        document: doc,
        section: ParamSection::Tool,
        slot: 0,
        text: text.into(),
    };
    let i = intent(&h, event("9")).expect("an intent");
    assert_eq!(i, UiIntent::SetToolOption { document: doc, key: ParamKey::Points, text: "9".into() });
    h.send(UiIntent::SetToolOption { document: doc, key: ParamKey::Points, text: "1".into() });
    let panel = shapes_panel(&h.view(), doc, Locale::Nl);
    assert_eq!(panel.tool[0].text, "5", "the old value stays");
    assert!(!panel.tool[0].error.is_empty(), "the reason is in words");
    // A retry of the same text still goes while the row is in error.
    assert!(intent(&h, event("5")).is_some());
}

#[test]
fn the_mirror_option_is_a_toggle_row() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.mirror".into() });
    let panel = shapes_panel(&h.view(), doc, Locale::En);
    assert_eq!(panel.tool.len(), 1);
    assert!(panel.tool[0].toggle);
    assert_eq!(panel.tool[0].label, "Keep original");
    let on = intent(
        &h,
        NativeEvent::ParamCommitted { document: doc, section: ParamSection::Tool, slot: 0, text: "1".into() },
    );
    assert_eq!(on, Some(UiIntent::SetToolOption { document: doc, key: ParamKey::KeepOriginal, text: "1".into() }));
}

fn two_gapped_lines() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    for (a, b) in [((0.0, 0.0), (10.0, 0.0)), ((10.03, 0.0), (20.0, 0.0))] {
        h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.line".into() });
        h.drag(doc, a, b);
    }
    let ids = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
    (h, doc)
}

#[test]
fn the_geometry_dialog_has_a_title_a_label_and_the_default_and_confirm_is_unparsed() {
    let (mut h, doc) = two_gapped_lines();
    assert!(geometry_dialog_panel(&h.view(), doc, Locale::Nl).is_none());
    h.invoke(doc, CommandKey::ShapesJoin);
    let panel = geometry_dialog_panel(&h.view(), doc, Locale::Nl).expect("dialog");
    assert_eq!(panel.kind, GeometryDialogKind::Join);
    assert_eq!((panel.title.as_str(), panel.text.as_str()), ("Paden verbinden of sluiten", "0.05"));
    assert!(panel.label.contains("opening"), "{}", panel.label);
    assert_eq!(geometry_dialog_panel(&h.view(), doc, Locale::En).unwrap().title, "Join or Close Paths");
    let ok = intent(&h, NativeEvent::GeometryDialogConfirmed { document: doc, text: "0,05 mm".into() });
    assert_eq!(
        ok,
        Some(UiIntent::ApplyGeometryDialog { document: doc, text: "0,05 mm".into() }),
        "the comma goes unparsed"
    );
    let cancel = intent(&h, NativeEvent::GeometryDialogCancelled { document: doc });
    assert_eq!(cancel, Some(UiIntent::CancelGeometryDialog { document: doc }));
    h.send(UiIntent::CancelGeometryDialog { document: doc });
    assert!(
        intent(&h, NativeEvent::GeometryDialogConfirmed { document: doc, text: "1".into() }).is_none(),
        "no dialog, no intent"
    );
}

#[test]
fn a_refused_dialog_value_is_shown_in_the_sheet() {
    let (mut h, doc) = two_gapped_lines();
    h.invoke(doc, CommandKey::ShapesJoin);
    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "abc".into() });
    let panel = geometry_dialog_panel(&h.view(), doc, Locale::En).expect("still open");
    assert!(!panel.error.is_empty());
    assert_eq!(panel.text, "abc");
}

#[test]
fn the_shape_and_node_commands_are_menu_items_enabled_by_the_selection() {
    let (mut h, doc) = two_gapped_lines();
    let join = MenuItem::Command(CommandKey::ShapesJoin);
    assert_eq!(menu_title(join, &h.view(), Locale::Nl), "Verbind of sluit paden…");
    assert!(menu_enabled(join, &h.view()));
    assert!(!menu_enabled(MenuItem::Command(CommandKey::NodeDelete), &h.view()), "no node selected");
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(!menu_enabled(join, &h.view()));
}
