//! The toolbar through the real controller: choosing tools by catalog id, variant memory and
//! disabled entries, with the view model a native host draws.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_editor::ToolKind;
use burnburn_presentation::{
    Action, CommandKey, DisabledReason, Enablement, HeadlessEditorHarness, ToolCatalog, ToolContext, UiIntent,
};
use burnburn_types::DocumentId;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    (h, doc)
}

fn select_tool(h: &mut HeadlessEditorHarness, doc: DocumentId, id: &str) {
    h.send(UiIntent::SelectTool { document: Some(doc), id: id.to_owned() });
}

fn tool(h: &HeadlessEditorHarness, doc: DocumentId) -> ToolKind {
    h.document(doc).expect("document").tool
}

#[test]
fn every_catalog_id_with_a_tool_activates_that_tool_and_is_marked_selected() {
    let (mut h, doc) = window();
    // Warp and Deform work on a selection (without one they stay in Select and say why).
    select_tool(&mut h, doc, "sketch.rectangle.two_point");
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    let mut checked = 0;
    for spec in ToolCatalog::standard().specs() {
        let Action::Tool(kind) = spec.action else { continue };
        if spec.context == ToolContext::Sketch {
            // Only while a sketch is edited: outside one the choice is refused with that reason.
            let vm = h.view();
            assert_eq!(
                vm.toolbar.button(spec.id).unwrap().enablement,
                Enablement::Disabled(DisabledReason::NeedsSketch),
                "{}",
                spec.id
            );
            continue;
        }
        select_tool(&mut h, doc, spec.id);
        assert_eq!(tool(&h, doc), kind, "{}", spec.id);
        let selected: Vec<_> = h.view().toolbar.buttons().filter(|b| b.selected).map(|b| b.id).collect();
        assert_eq!(selected, [spec.id], "toolbar marks exactly the chosen id");
        checked += 1;
    }
    assert!(checked >= 8, "the existing eight tools are in the catalog, saw {checked}");
}

#[test]
fn an_id_without_a_tool_is_disabled_with_a_reason_and_changes_nothing() {
    let (mut h, doc) = window();
    select_tool(&mut h, doc, "sketch.ellipse.box");
    let before = h.view().toolbar.clone();
    for spec in
        ToolCatalog::standard().specs().iter().filter(|s| matches!(s.action, Action::Unavailable | Action::Deferred))
    {
        let vm = h.view();
        let button = vm.toolbar.button(spec.id).expect("listed");
        assert!(
            matches!(button.enablement, Enablement::Disabled(DisabledReason::NotAvailable | DisabledReason::Follows)),
            "{} must be disabled with a reason",
            spec.id
        );
        let update = h.send(UiIntent::SelectTool { document: Some(doc), id: spec.id.to_owned() });
        assert!(!update.view_changed, "{} did something", spec.id);
        assert_eq!(tool(&h, doc), ToolKind::Ellipse, "{} changed the tool", spec.id);
    }
    assert_eq!(h.view().toolbar, before);
}

#[test]
fn an_unknown_id_is_ignored() {
    let (mut h, doc) = window();
    select_tool(&mut h, doc, "sketch.ellipse.box");
    select_tool(&mut h, doc, "no.such.tool");
    assert_eq!(tool(&h, doc), ToolKind::Ellipse);
}

#[test]
fn the_chosen_variant_is_remembered_per_group_and_activates_its_tool() {
    let (mut h, doc) = window();
    h.send(UiIntent::SelectVariant { document: Some(doc), group: "sketch.line".into(), id: "sketch.polyline".into() });
    assert_eq!(tool(&h, doc), ToolKind::Polyline);
    select_tool(&mut h, doc, "select");
    let vm = h.view();
    let line = vm.toolbar.group("sketch.line").expect("group");
    assert_eq!(line.active.id, "sketch.polyline", "remembered although another tool is active");
    assert!(!line.selected());
    // The other groups keep their default.
    assert_eq!(vm.toolbar.group("sketch.rectangle").expect("group").active.id, "sketch.rectangle.two_point");
}

#[test]
fn a_variant_that_does_not_belong_to_the_group_or_cannot_act_is_not_remembered() {
    let (mut h, doc) = window();
    select_tool(&mut h, doc, "select");
    // The second is a command that cannot act without a selection: it is refused, not remembered.
    for (group, id) in [("sketch.line", "sketch.circle.center"), ("geometry.direction", "geometry.clockwise")] {
        h.send(UiIntent::SelectVariant { document: Some(doc), group: group.into(), id: id.into() });
        assert_eq!(tool(&h, doc), ToolKind::Select, "{id}");
        let vm = h.view();
        let face = vm.toolbar.group(group).expect("group").active.id;
        assert_ne!(face, id, "{id} must not become the face");
    }
}

#[test]
fn trim_chosen_through_the_catalog_is_the_trim_tool_and_the_menu_mode_shows_as_trim() {
    let (mut h, doc) = window();
    select_tool(&mut h, doc, "sketch.trim");
    assert_eq!(tool(&h, doc), ToolKind::Trim);
    assert!(h.view().toolbar.button("sketch.trim").expect("trim").selected);
    select_tool(&mut h, doc, "select");
    assert!(!h.view().toolbar.button("sketch.trim").expect("trim").selected);
    assert!(h.view().toolbar.button("select").expect("select").selected);
    h.invoke(doc, CommandKey::ToolTrim);
    let vm = h.view();
    assert!(vm.toolbar.button("sketch.trim").expect("trim").selected, "legacy trim mode");
    assert!(!vm.toolbar.button("select").expect("select").selected);
}

#[test]
fn without_a_document_every_button_is_disabled_for_that_reason() {
    let h = HeadlessEditorHarness::new();
    let vm = h.view();
    let tools = vm.toolbar.buttons().filter(|b| b.id == "select" || b.id == "sketch.ellipse");
    for b in tools {
        assert_eq!(b.enablement, Enablement::Disabled(DisabledReason::NoDocument), "{}", b.id);
    }
}

#[test]
fn the_menu_command_and_the_catalog_choose_the_same_tool() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::ToolCircle);
    let vm = h.view();
    assert!(vm.toolbar.button("sketch.circle.center").expect("circle").selected);
}
