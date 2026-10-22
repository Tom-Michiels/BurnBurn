//! G9-trace on Windows: the comparison section of the preparation form lists a correction that does not bind
//! on variant B and links it to B's contours with a button, with the words and intents of the macOS panel
//! (`tests/g9_trace_relink_panel.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::g5::compared_with_unbound_correction;

use burnburn_presentation::{HeadlessEditorHarness, TraceRecipeIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
    let vm = h.view();
    let shown = PanelState { g5_shown: vec![P::TracePrep], ..PanelState::default() };
    g5::forms(&FormContext { vm: &vm, document: doc, locale }, &shown, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::TracePrep))
        .expect("the preparation form")
}

fn button(f: &Form, c: C) -> Option<(String, bool)> {
    f.lines.iter().find_map(|l| match l {
        FormLine::Buttons(row) => row.iter().find(|b| b.id == ControlId::G5(c)).map(|b| (b.title.clone(), b.enabled)),
        _ => None,
    })
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C) -> FormOutcome {
    let vm = h.view();
    let f = form(h, doc, Locale::En);
    let outcome = g5::react(
        P::TracePrep,
        &ControlId::G5(c),
        &f.values(),
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    if let FormOutcome::Intent(intent) = &outcome {
        h.send(intent.clone());
        h.run_tasks();
    }
    outcome
}

#[test]
fn an_unbound_correction_is_said_and_linked_by_its_button_and_taken_back_by_the_same_one() {
    let (mut h, doc) = compared_with_unbound_correction();
    let f = form(&h, doc, Locale::En);
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.starts_with("Correction 1 does not fit B: "))));
    assert_eq!(button(&f, C::CompareRelink { index: 0 }), Some(("Link correction 1 to B anyway".to_owned(), true)));
    assert_eq!(
        button(&form(&h, doc, Locale::Nl), C::CompareRelink { index: 0 }),
        Some(("Koppel correctie 1 toch aan B".to_owned(), true))
    );
    assert_eq!(
        press(&mut h, doc, C::CompareRelink { index: 0 }),
        FormOutcome::Intent(UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::RelinkCorrection { index: 0, on: true }
        })
    );
    assert_eq!(
        button(&form(&h, doc, Locale::En), C::CompareRelink { index: 0 }),
        Some(("Take correction 1 off B again".to_owned(), true))
    );
    assert_eq!(
        press(&mut h, doc, C::CompareRelink { index: 0 }),
        FormOutcome::Intent(UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::RelinkCorrection { index: 0, on: false }
        })
    );
}
