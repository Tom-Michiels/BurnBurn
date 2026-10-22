//! The confirmation before a start as a Windows form, headless (G7-werk, M06): same lines, words
//! and intents as the macOS panel (`burnburn-macos/tests/g7_werk_start_confirm.rs`). The form opens
//! by itself while presentation has a confirmation; the machine is a fake service.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::rig as rig_common;

use burnburn_presentation::{MachineAction, UiIntent};
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, FormContext, FormKind, FormLine, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P, machine_intent};
use rig_common::*;

fn form_texts(form: &burnburn_windows::forms::Form) -> Vec<String> {
    form.lines
        .iter()
        .flat_map(|l| match l {
            FormLine::Row(inner) => inner.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .filter_map(|l| match l {
            FormLine::Text(t) | FormLine::Heading(t) | FormLine::Problem(t) => Some(t.clone()),
            FormLine::Buttons(buttons) => Some(buttons.iter().map(|b| format!("[{}]", b.title)).collect::<String>()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_form_opens_by_itself_with_the_job_and_the_missing_proof_and_its_buttons_send_the_confirmation() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);

    // Nothing is asked yet: no form.
    let vm = r.h.view();
    let context = FormContext { vm: &vm, document: r.doc, locale: Locale::En };
    assert!(
        g5::forms(&context, &PanelState::default(), &Vec::new())
            .iter()
            .all(|f| f.kind != FormKind::G5(P::StartConfirm))
    );

    r.h.send(UiIntent::RequestStart { document: Some(r.doc) });
    let vm = r.h.view();
    let context = FormContext { vm: &vm, document: r.doc, locale: Locale::En };
    let form = g5::forms(&context, &PanelState::default(), &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::StartConfirm))
        .expect("the confirmation form");
    let text = form_texts(&form).join("\n");
    assert!(text.contains("revision") && text.contains("120 × 80 mm"), "{text}");
    assert!(text.contains("not proven on hardware") && text.contains("never repeated automatically"), "{text}");
    assert!(text.contains("[Start the job]") && text.contains("[Cancel]"), "{text}");

    let shown = r.h.document(r.doc).unwrap().start_confirm.unwrap();
    let outcome = g5::react(P::StartConfirm, &ControlId::G5(C::StartConfirm), &form.values(), &context);
    let FormOutcome::Intent(intent) = outcome else { panic!("{outcome:?}") };
    assert_eq!(intent, UiIntent::ConfirmStart { document: Some(r.doc), action: shown.action, seen: shown.seen });
    let outcome = g5::react(P::StartConfirm, &ControlId::G5(C::StartCancel), &form.values(), &context);
    assert_eq!(outcome, FormOutcome::Intent(UiIntent::CancelStart));
}

#[test]
fn the_start_control_of_the_machine_form_only_asks() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    let start = r.h.machine_view().unwrap().button(MachineAction::Start).cloned().unwrap();
    assert_eq!(machine_intent(Some(r.doc), &start), Some(UiIntent::RequestStart { document: Some(r.doc) }));
}
