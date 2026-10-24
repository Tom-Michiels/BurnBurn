//! LFC: a reason a panel section cannot be used now is said once, as the section's `unavailable`,
//! not as a text or problem line among its rows (headless, one test per panel).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_host_models::form::{FormSection, FormSpec};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Panel, g5_form, g5_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, OrderDraft, SelectMode, UiIntent};
use burnburn_types::DocumentId;

fn spec(h: &HeadlessEditorHarness, doc: DocumentId, panel: G5Panel) -> FormSpec {
    let model = g5_panel(&h.view(), doc, panel, Locale::En, &[]).expect("the panel is open");
    g5_form(&model.title, &model.lines).spec
}

fn section<'a>(spec: &'a FormSpec, heading: &str) -> &'a FormSection {
    spec.sections
        .iter()
        .find(|s| s.heading.as_deref() == Some(heading))
        .unwrap_or_else(|| panic!("no section {heading:?} in {:?}", spec.sections))
}

/// Every place the reason is said, in all sections: as the reason of a section, or as a row.
fn times_said(spec: &FormSpec, reason: &str) -> usize {
    spec.sections.iter().flat_map(FormSection::texts).filter(|t| t == reason).count()
}

#[test]
fn the_structure_panel_says_once_why_no_reference_can_be_made() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });
    let spec = spec(&h, doc, G5Panel::Structure);
    let reference = section(&spec, "Make a reference");
    let reason = reference.unavailable.clone().expect("the section is unavailable");
    assert!(reason.starts_with("Select exactly one object"), "{reason}");
    assert!(reference.rows.is_empty(), "no row repeats the reason: {:?}", reference.rows);
    assert_eq!(times_said(&spec, &reason), 1);
}

#[test]
fn the_operator_panel_says_once_that_no_machine_is_connected() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::AddOrder {
        order: OrderDraft { name: "Order 1".into(), part: "Part".into(), quantity: 1, remnant: None },
    });
    let spec = spec(&h, doc, G5Panel::Operator);
    let machine = spec.sections.first().expect("a machine section");
    assert_eq!(machine.unavailable.as_deref(), Some("No machine connected."));
    assert_eq!(times_said(&spec, "No machine connected."), 1);
}

#[test]
fn the_recognition_panel_says_once_that_a_camera_must_be_started() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let spec = spec(&h, doc, G5Panel::Recognition);
    let reason = "Start a camera first (camera management).";
    assert!(spec.sections.iter().any(|s| s.unavailable.as_deref() == Some(reason)), "{:?}", spec.sections);
    assert_eq!(times_said(&spec, reason), 1);
}
