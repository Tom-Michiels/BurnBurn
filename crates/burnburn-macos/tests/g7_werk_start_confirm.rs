//! The confirmation before a start as a panel, headless (G7-werk, M06): it opens by itself while
//! presentation has a confirmation, names job, revision, machine, profile, material, size and
//! placement in both languages, says honestly that the start has no hardware proof, and its buttons
//! send exactly the confirmation on screen. The Start control of the machine panel only asks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-presentation/tests/common/mod.rs"]
mod common;

use burnburn_macos::l10n::Locale;
use burnburn_macos::l10n_g7_werk::{G7Text, g7_text};
use burnburn_macos::panels::{
    G5Control as C, G5Line, G5Panel, g5_intent, g5_opens_by_itself, g5_panel, machine_intent,
};
use burnburn_presentation::{MachineAction, UiIntent};
use common::*;

fn texts(
    h: &burnburn_presentation::HeadlessEditorHarness,
    doc: burnburn_types::DocumentId,
    locale: Locale,
) -> Vec<String> {
    g5_panel(&h.view(), doc, G5Panel::StartConfirm, locale, &[])
        .expect("the confirmation panel")
        .lines
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Heading(t) => Some(t),
            G5Line::Button { title, .. } => Some(format!("[{title}]")),
            _ => None,
        })
        .collect()
}

fn stored() -> Rig {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    r
}

#[test]
fn there_is_no_panel_until_a_start_is_asked_for_and_it_opens_by_itself() {
    let mut r = stored();
    assert!(g5_opens_by_itself(G5Panel::StartConfirm));
    assert!(g5_panel(&r.h.view(), r.doc, G5Panel::StartConfirm, Locale::Nl, &[]).is_none());
    r.h.send(UiIntent::RequestStart { document: Some(r.doc) });
    assert!(g5_panel(&r.h.view(), r.doc, G5Panel::StartConfirm, Locale::Nl, &[]).is_some());
    r.h.send(UiIntent::CancelStart);
    assert!(g5_panel(&r.h.view(), r.doc, G5Panel::StartConfirm, Locale::Nl, &[]).is_none());
}

#[test]
fn the_panel_names_job_revision_machine_size_and_placement_and_the_missing_proof() {
    let mut r = stored();
    let revision = r.h.document(r.doc).unwrap().job.unwrap().plan_revision.to_string();
    r.h.send(UiIntent::RequestStart { document: Some(r.doc) });
    let nl = texts(&r.h, r.doc, Locale::Nl).join("\n");
    assert!(nl.contains("Start bevestigen") || nl.contains("Deze job wordt gestart"), "{nl}");
    assert!(nl.contains(&format!("revisie {revision}")), "{nl}");
    assert!(nl.contains(&format!("Machine: {MACHINE}")), "{nl}");
    assert!(
        nl.contains("model (door jou opgegeven): niet opgegeven; firmware (door jou opgegeven): niet opgegeven"),
        "{nl}"
    );
    assert!(nl.contains("research-1500x900"), "{nl}");
    assert!(nl.contains("Materiaal: onbekend"), "{nl}");
    assert!(nl.contains("120 × 80 mm"), "{nl}");
    assert!(nl.contains("op de ontworpen plek"), "{nl}");
    assert!(nl.contains("niet op hardware bewezen") && nl.contains("nooit automatisch herhaald"), "{nl}");
    assert!(nl.contains("[Start de job]") && nl.contains("[Annuleer]"), "{nl}");

    let en = texts(&r.h, r.doc, Locale::En).join("\n");
    assert!(en.contains(&format!("revision {revision}")), "{en}");
    assert!(en.contains("Material: unknown") && en.contains("at the designed place"), "{en}");
    assert!(en.contains("not proven on hardware") && en.contains("never repeated automatically"), "{en}");
    assert!(en.contains("[Start the job]") && en.contains("[Cancel]"), "{en}");
}

#[test]
fn the_buttons_send_the_confirmation_on_screen_or_close_it() {
    let mut r = stored();
    r.h.send(UiIntent::RequestStart { document: Some(r.doc) });
    let shown = r.h.document(r.doc).unwrap().start_confirm.unwrap();
    let vm = r.h.view();
    let confirm = g5_intent(&vm, r.doc, G5Panel::StartConfirm, C::StartConfirm, &[]);
    assert_eq!(
        confirm,
        Some(UiIntent::ConfirmStart { document: Some(r.doc), action: shown.action, seen: shown.seen.clone() }),
        "action and seen are copied unchanged"
    );
    assert_eq!(g5_intent(&vm, r.doc, G5Panel::StartConfirm, C::StartCancel, &[]), Some(UiIntent::CancelStart));
}

#[test]
fn the_start_control_of_the_machine_panel_only_asks_and_the_other_controls_are_unchanged() {
    let r = stored();
    let machine = r.h.machine_view().unwrap();
    let start = machine.button(MachineAction::Start).cloned().unwrap();
    assert_eq!(machine_intent(Some(r.doc), &start), Some(UiIntent::RequestStart { document: Some(r.doc) }));
    let query = machine.button(MachineAction::QueryStatus).cloned().unwrap();
    assert_eq!(
        machine_intent(Some(r.doc), &query),
        Some(UiIntent::Machine { document: Some(r.doc), action: query.action, seen: query.seen.clone() })
    );
}

#[test]
fn every_text_has_both_languages_with_the_same_placeholders() {
    for text in G7Text::ALL {
        let (nl, en) = (g7_text(*text, Locale::Nl), g7_text(*text, Locale::En));
        assert!(!nl.is_empty() && !en.is_empty(), "{text:?}");
        for i in 0..4 {
            let hole = format!("{{{i}}}");
            assert_eq!(nl.contains(&hole), en.contains(&hole), "{text:?} placeholder {i}");
        }
    }
}
