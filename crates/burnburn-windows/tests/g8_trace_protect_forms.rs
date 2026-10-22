//! G8-trace, T05 and T06 on Windows: the preparation form offers the two protect buttons and lists the
//! protected corners, with the words and intents of the macOS panel (`tests/g8_trace_protect_panel.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::g5 as g5_fixtures;

use burnburn_presentation::{HeadlessEditorHarness, TracePick, TracePrepIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use g5_fixtures::ring_png;

fn opened() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/in.png"), &ring_png()).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: std::path::PathBuf::from("/work/in.png") });
    h.run_tasks();
    (h, doc)
}

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
fn the_form_has_the_two_protect_buttons_and_they_start_pointing() {
    let (mut h, doc) = opened();
    assert_eq!(
        button(&form(&h, doc, Locale::En), C::PickProtectContours),
        Some(("Click contours to protect".to_owned(), true))
    );
    assert_eq!(
        button(&form(&h, doc, Locale::Nl), C::PickProtectCorners),
        Some(("Hoeken aanklikken om te beschermen".to_owned(), true))
    );
    assert_eq!(
        press(&mut h, doc, C::PickProtectContours),
        FormOutcome::Intent(UiIntent::TracePick { document: doc, pick: TracePick::BeginProtectContours })
    );
    assert_eq!(
        press(&mut h, doc, C::PickProtectCorners),
        FormOutcome::Intent(UiIntent::TracePick { document: doc, pick: TracePick::BeginProtectCorners })
    );
}

#[test]
fn a_protected_corner_is_listed_and_released_by_its_button() {
    let (mut h, doc) = opened();
    press(&mut h, doc, C::PickProtectCorners);
    h.send(UiIntent::TracePick { document: doc, pick: TracePick::ProtectAt { x: 32.0, y: 12.0, reach_px: 3.0 } });
    h.run_tasks();
    let f = form(&h, doc, Locale::En);
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.starts_with("Protected corner 1: at ("))));
    assert_eq!(
        press(&mut h, doc, C::CornerRemove { index: 0 }),
        FormOutcome::Intent(UiIntent::TracePrep {
            document: doc,
            intent: TracePrepIntent::RemoveProtectedCorner { index: 0 }
        })
    );
}
