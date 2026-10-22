//! G8-trace, T10 on Windows: the files of a batch come from a file dialog with multiple selection
//! (`IFileOpenDialog` + `FOS_ALLOWMULTISELECT`) instead of a text box. Headless part: the button, the
//! dialog it asks for, and the intent the chosen files become; the same words and intent as the
//! macOS panel (`tests/g8_trace_batch_files.rs` there). The dialog itself runs only on Windows and
//! stays a manual check.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::g5 as g5_fixtures;

use std::path::PathBuf;

use burnburn_presentation::{HeadlessEditorHarness, TraceRecipeIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{
    ControlId, ControlValue, FilePick, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use g5_fixtures::ring_png;

fn opened() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/in.png"), &ring_png()).unwrap();
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/two.png"), &ring_png()).unwrap();
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/three.png"), &ring_png()).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    (h, doc)
}

fn shown() -> PanelState {
    PanelState { g5_shown: vec![P::TracePrep], ..PanelState::default() }
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
    let vm = h.view();
    g5::forms(&FormContext { vm: &vm, document: doc, locale }, &shown(), &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::TracePrep))
        .expect("the preparation form")
}

fn button_title(f: &Form, c: C) -> Option<(String, bool)> {
    f.lines.iter().find_map(|l| match l {
        FormLine::Buttons(row) => row.iter().find(|b| b.id == ControlId::G5(c)).map(|b| (b.title.clone(), b.enabled)),
        _ => None,
    })
}

fn press(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> FormOutcome {
    let vm = h.view();
    let f = form(h, doc, Locale::En);
    g5::react(P::TracePrep, &ControlId::G5(c), &f.values(), &FormContext { vm: &vm, document: doc, locale: Locale::En })
}

fn run_intent(doc: DocumentId, paths: &[&str]) -> UiIntent {
    UiIntent::TraceRecipe {
        document: doc,
        intent: TraceRecipeIntent::RunBatch { paths: paths.iter().map(PathBuf::from).collect() },
    }
}

#[test]
fn the_form_has_a_choose_files_button_in_both_languages() {
    let (h, doc) = opened();
    assert_eq!(button_title(&form(&h, doc, Locale::En), C::BatchChoose), Some(("Choose files…".to_owned(), true)));
    assert_eq!(button_title(&form(&h, doc, Locale::Nl), C::BatchChoose), Some(("Kies bestanden…".to_owned(), true)));
}

#[test]
fn the_button_asks_for_a_multiple_selection_dialog_and_sends_nothing_itself() {
    let (h, doc) = opened();
    assert_eq!(press(&h, doc, C::BatchChoose), FormOutcome::PickFile(FilePick::TraceBatch));
}

#[test]
fn a_running_batch_does_not_ask_for_another_dialog() {
    let (mut h, doc) = opened();
    h.send(run_intent(doc, &["/work/two.png"]));
    assert_eq!(button_title(&form(&h, doc, Locale::En), C::BatchChoose).map(|b| b.1), Some(false));
    assert_eq!(press(&h, doc, C::BatchChoose), FormOutcome::Nothing);
}

#[test]
fn the_chosen_files_become_one_batch_each_once_in_the_order_given() {
    let (mut h, doc) = opened();
    let paths = ["/work/three.png", "/work/two.png", "/work/three.png"].map(PathBuf::from).to_vec();
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::En };
    let outcome = g5::batch_files_chosen(paths, &ctx);
    let want = run_intent(doc, &["/work/three.png", "/work/two.png"]);
    assert_eq!(outcome, FormOutcome::Intent(want.clone()));
    h.send(want);
    h.run_tasks();
    let f = form(&h, doc, Locale::En);
    let report: Vec<&str> = f
        .lines
        .iter()
        .filter_map(|l| match l {
            FormLine::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert!(report.iter().any(|t| t.contains("2 file(s)")), "{report:?}");
}

#[test]
fn cancel_sends_nothing() {
    let (h, doc) = opened();
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::En };
    assert_eq!(g5::batch_files_chosen(Vec::new(), &ctx), FormOutcome::Nothing);
}

#[test]
fn a_value_for_the_files_memo_is_still_read_from_the_form() {
    // The typed list stays: choosing files is a second way to give them, not a replacement.
    let (h, doc) = opened();
    let vm = h.view();
    let f = form(&h, doc, Locale::En);
    let mut values = f.values();
    let memo = ControlId::G5(C::BatchFiles);
    let text = ControlValue::Text("/work/two.png\n/work/three.png".into());
    match values.iter_mut().find(|(id, _)| *id == memo) {
        Some(slot) => slot.1 = text,
        None => values.push((memo, text)),
    }
    let outcome = g5::react(
        P::TracePrep,
        &ControlId::G5(C::BatchRun),
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    assert_eq!(outcome, FormOutcome::Intent(run_intent(doc, &["/work/two.png", "/work/three.png"])));
}
