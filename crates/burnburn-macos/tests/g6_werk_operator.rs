//! The compact operator view as a panel, headless (G6-werk, W08): the machine, the approved jobs
//! with revision, size and material, and the orders with what was completed, in one place. The
//! controls are the machine service's own buttons, sent unchanged; the machine is a fake.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::rig as common;

use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, OrderDraft, Role, UiIntent};
use burnburn_types::DocumentId;
use common::*;

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::Operator, locale, &[]).expect("the operator panel").lines
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    lines(h, doc, locale)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn approved() -> Rig {
    let mut r = rig();
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    r.h.controller().set_role(Role::Operator);
    r
}

#[test]
fn there_is_no_operator_panel_without_a_machine_an_order_or_an_approved_job() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(g5_panel(&h.view(), doc, G5Panel::Operator, Locale::En, &[]).is_none());
    assert!(!menu_enabled(MenuItem::OperatorPanel, &h.view()));
}

#[test]
fn the_panel_gathers_machine_job_revision_size_material_and_order_counts() {
    let mut r = approved();
    let job_name = r.h.operator_view().jobs[0].name.clone();
    r.h.send(UiIntent::AddOrder {
        order: OrderDraft {
            name: "Order 7".into(),
            part: job_name.clone(),
            quantity: 3,
            remnant: Some("Rest A".into()),
        },
    });
    let shown = texts(&r.h, r.doc, Locale::En);
    assert!(shown.iter().any(|t| t.starts_with("werkplaats-ruida-1: ")), "{shown:?}");
    let job = shown.iter().find(|t| t.starts_with(&format!("{job_name}: revision "))).expect("the job line");
    assert!(job.contains("120 × 80 mm") && job.ends_with("material unknown"), "{job}");
    assert!(
        shown.iter().any(|t| t == &format!("Order 7: 0 of 3 completed ({job_name}) on remnant Rest A")),
        "{shown:?}"
    );
    let nl = texts(&r.h, r.doc, Locale::Nl);
    assert!(
        nl.iter().any(|t| t.contains("revisie") && t.contains("120 × 80 mm") && t.contains("materiaal onbekend")),
        "{nl:?}"
    );
    assert!(nl.iter().any(|t| t == &format!("Order 7: 0 van 3 voltooid ({job_name}) op restplaat Rest A")), "{nl:?}");
}

#[test]
fn an_upload_button_sends_the_machine_button_unchanged_and_a_disabled_one_sends_nothing() {
    let r = approved();
    let row = r.h.operator_view().jobs[0].clone();
    let upload = row.upload.clone().expect("upload control");
    assert!(upload.enabled);
    let intent = g5_intent(&r.h.view(), r.doc, G5Panel::Operator, C::OperatorUpload { index: 0 }, &[]);
    assert_eq!(
        intent,
        Some(UiIntent::Machine { document: row.document, action: upload.action, seen: upload.seen.clone() })
    );
    // An index that is not there sends nothing.
    assert_eq!(g5_intent(&r.h.view(), r.doc, G5Panel::Operator, C::OperatorUpload { index: 5 }, &[]), None);
    assert_eq!(g5_intent(&r.h.view(), r.doc, G5Panel::Operator, C::OperatorOrderUpload { index: 0 }, &[]), None);
}

#[test]
fn the_menu_item_has_its_tag_and_toggles_the_panel_while_there_is_something_to_show() {
    let r = approved();
    let vm = r.h.view();
    assert_eq!(menu_tag(MenuItem::OperatorPanel), 1033);
    assert_eq!(menu_item(1033), Some(MenuItem::OperatorPanel));
    assert!(menu_enabled(MenuItem::OperatorPanel, &vm));
    assert_eq!(menu_title(MenuItem::OperatorPanel, &vm, Locale::Nl), "Operatorweergave…");
    assert_eq!(translate(NativeEvent::Menu(MenuItem::OperatorPanel), &vm), Reaction::ToggleG5Panel(G5Panel::Operator));
}
