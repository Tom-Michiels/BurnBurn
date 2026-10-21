//! The catalogue tool strip of the Windows window, headless (W4-L8a): drawing tools, basic shapes,
//! node toolbar, sketch constraints — everything the tool register lists, panel by panel, with the
//! tool states of the view model, and the intents a click or a variant choice sends. Same
//! register and texts as the macOS strip (`toolbar.rs`, `tool_strip` native test there).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, PanelId, ToolCatalog, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::toolstrip::{
    StripAction, StripButton, StripPanel, strip, strip_layout, tool_intent, variant_intent,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::toolbar::{panel_title, tool_label};

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    (h, doc)
}

fn all_buttons(panels: &[StripPanel]) -> Vec<&StripButton> {
    panels.iter().flat_map(|p| &p.groups).flat_map(|g| std::iter::once(&g.button).chain(g.variants.iter())).collect()
}

#[test]
fn the_strip_shows_every_catalogue_tool_in_register_order_with_translated_panel_titles() {
    let (h, _) = window();
    let panels = strip(&h.view(), Locale::En);
    let titles: Vec<&str> = panels.iter().map(|p| p.title.as_str()).collect();
    let expected: Vec<String> = PanelId::ALL.iter().map(|p| panel_title(*p, Locale::En)).collect();
    assert_eq!(titles, expected.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(strip(&h.view(), Locale::Nl)[0].title, "MAKEN");
    // Every register id is on the strip (as a button or a variant), labelled in the window language.
    let buttons = all_buttons(&panels);
    for spec in ToolCatalog::standard().specs() {
        let b = buttons.iter().find(|b| b.id == spec.id).unwrap_or_else(|| panic!("{} missing", spec.id));
        assert_eq!(b.label, tool_label(spec.id, Locale::En));
        assert!(!b.tooltip.is_empty());
    }
}

#[test]
fn a_group_has_the_remembered_variant_on_its_face_and_a_single_tool_has_no_variants() {
    let (h, _) = window();
    let panels = strip(&h.view(), Locale::En);
    let groups: Vec<_> = panels.iter().flat_map(|p| &p.groups).collect();
    assert!(groups.iter().any(|g| g.group.is_some() && g.variants.len() > 1), "some tools have a drop-down");
    for g in &groups {
        if g.group.is_none() {
            assert!(g.variants.is_empty());
        } else {
            assert!(g.variants.iter().any(|v| v.id == g.button.id), "the face is one of the variants");
        }
    }
}

#[test]
fn the_selected_tool_is_marked_and_disabled_tools_name_their_reason() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::ToolRectangle);
    let panels = strip(&h.view(), Locale::En);
    let buttons = all_buttons(&panels);
    assert!(buttons.iter().any(|b| b.selected), "the active tool is marked");
    // Node tools need nodes: a disabled button says why instead of what it does.
    let disabled: Vec<_> = buttons.iter().filter(|b| !b.enabled).collect();
    assert!(!disabled.is_empty());
    assert!(disabled.iter().all(|b| b.tooltip.contains('\u{2014}')), "label \u{2014} reason");
    // The tooltip of an enabled tool carries its label.
    let enabled = buttons.iter().find(|b| b.enabled).unwrap();
    assert!(enabled.tooltip.starts_with(&enabled.label), "{} / {}", enabled.tooltip, enabled.label);
}

#[test]
fn a_click_chooses_the_tool_and_a_variant_choice_names_its_group() {
    let (h, doc) = window();
    let vm = h.view();
    assert_eq!(
        tool_intent(&vm, doc, "sketch.line"),
        Some(UiIntent::SelectTool { document: Some(doc), id: "sketch.line".into() })
    );
    assert_eq!(
        variant_intent(&vm, doc, "sketch.rectangle", "sketch.rectangle.center"),
        Some(UiIntent::SelectVariant {
            document: Some(doc),
            group: "sketch.rectangle".into(),
            id: "sketch.rectangle.center".into()
        })
    );
    let gone = <DocumentId as burnburn_types::StableId>::from_raw(999);
    assert_eq!(tool_intent(&vm, gone, "sketch.line"), None, "a window that is gone sends nothing");
    assert_eq!(variant_intent(&vm, gone, "sketch.rectangle", "x"), None);
}

#[test]
fn choosing_a_tool_through_the_strip_activates_it_in_the_document() {
    let (mut h, doc) = window();
    let vm = h.view();
    let i = tool_intent(&vm, doc, "sketch.line").unwrap();
    h.send(i);
    let panels = strip(&h.view(), Locale::En);
    let line = all_buttons(&panels).into_iter().find(|b| b.id == "sketch.line").unwrap().selected;
    assert!(line, "the line tool is the selected one now");
}

#[test]
fn the_strip_flows_over_rows_without_overlap_and_a_group_gets_a_drop_down() {
    let (h, _) = window();
    let panels = strip(&h.view(), Locale::En);
    for width in [400, 800, 1600] {
        let l = strip_layout(&panels, width, 1.0);
        assert!(!l.controls.is_empty() && l.height > 0);
        for (i, a) in l.controls.iter().enumerate() {
            assert!(a.rect.x >= 0 && a.rect.right() <= width, "{width}: {:?}", a.rect);
            assert!(a.rect.bottom() <= l.height);
            for b in l.controls.iter().skip(i + 1) {
                assert!(!a.rect.overlaps(&b.rect), "{width}: {:?} {:?}", a.rect, b.rect);
            }
        }
    }
    let narrow = strip_layout(&panels, 400, 1.0);
    let wide = strip_layout(&panels, 1600, 1.0);
    assert!(narrow.height > wide.height, "a narrow window needs more rows");
    // Every tool of the face is a control; every group with variants has one drop-down control.
    let tools = narrow.controls.iter().filter(|c| matches!(c.action, StripAction::Tool(_))).count();
    let faces: usize = panels.iter().map(|p| p.groups.len()).sum();
    assert_eq!(tools, faces);
    let drops = narrow.controls.iter().filter(|c| matches!(c.action, StripAction::Dropdown { .. })).count();
    let with_variants: usize =
        panels.iter().flat_map(|p| &p.groups).filter(|g| g.variants.len() > 1).count();
    assert_eq!(drops, with_variants);
    // The DPI scales the rows.
    assert!(strip_layout(&panels, 800, 2.0).height > strip_layout(&panels, 800, 1.0).height);
}
