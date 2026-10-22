//! The remnant library as a Windows form, headless (G7-werk, W07): same lines, words and intents as
//! the macOS panel (`burnburn-macos/tests/g7_werk_remnants.rs`); the form waits for its menu item.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::rig as rig_common;

use burnburn_presentation::{CommandKey, RemnantIntent, SelectMode, UiIntent};
use burnburn_windows::adapter::{MenuItem, command_id};
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, ControlValue, FormContext, FormKind, FormLine, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use rig_common::*;

fn texts(form: &burnburn_windows::forms::Form) -> String {
    form.lines
        .iter()
        .flat_map(|l| match l {
            FormLine::Row(inner) => inner.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .filter_map(|l| match l {
            FormLine::Text(t) | FormLine::Heading(t) | FormLine::Problem(t) => Some(t.clone()),
            FormLine::Field { caption, .. } => Some(format!("<{caption}>")),
            FormLine::Buttons(buttons) => Some(buttons.iter().map(|b| format!("[{}]", b.title)).collect::<String>()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_form_waits_for_its_menu_item_and_keeps_the_selection_with_the_typed_text() {
    let mut r = rig();
    r.h.invoke(r.doc, CommandKey::ToolRectangle);
    r.h.drag(r.doc, (10.0, 150.0), (210.0, 250.0));
    let sheet = r.h.document(r.doc).unwrap().objects.last().unwrap().id;
    r.h.invoke(r.doc, CommandKey::ToolSelect);
    r.h.send(UiIntent::Select { document: r.doc, ids: vec![sheet], mode: SelectMode::Replace });

    let vm = r.h.view();
    let context = FormContext { vm: &vm, document: r.doc, locale: Locale::Nl };
    let hidden = g5::forms(&context, &PanelState::default(), &Vec::new());
    assert!(hidden.iter().all(|f| f.kind != FormKind::G5(P::Remnants)));
    let shown = PanelState { g5_shown: vec![P::Remnants], ..PanelState::default() };
    let form = g5::forms(&context, &shown, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::Remnants))
        .expect("the remnant form");
    let text = texts(&form);
    assert!(text.contains("Er zijn nog geen restplaten bewaard.") && text.contains("[Bewaar selectie]"), "{text}");

    let mut values = form.values();
    let typed = [C::RemnantName, C::RemnantMaterial, C::RemnantThickness];
    values.retain(|(id, _)| !matches!(id, ControlId::G5(c) if typed.contains(c)));
    values.push((ControlId::G5(C::RemnantName), ControlValue::Text("Rest 1".into())));
    values.push((ControlId::G5(C::RemnantMaterial), ControlValue::Text("multiplex".into())));
    values.push((ControlId::G5(C::RemnantThickness), ControlValue::Text("3".into())));
    let outcome = g5::react(P::Remnants, &ControlId::G5(C::RemnantSave), &values, &context);
    let FormOutcome::Intent(intent) = outcome else { panic!("{outcome:?}") };
    assert_eq!(
        intent,
        UiIntent::Remnant(RemnantIntent::SaveFromSelection {
            document: r.doc,
            name: "Rest 1".into(),
            material: "multiplex".into(),
            thickness: "3".into(),
            note: String::new(),
            date: String::new(),
        })
    );
    // The same id as the Mac tag.
    assert_eq!(command_id(MenuItem::RemnantsPanel), 1070);
}
