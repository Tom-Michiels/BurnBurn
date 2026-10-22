//! G8-trace, T10: the files of a batch come from a native panel with multiple selection instead of a
//! text box. Headless part: the button, what it asks of the host, and the intent that the chosen
//! files become. The NSOpenPanel itself is covered by `trace_batch_panel_native`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod g5_fixtures;

use std::path::PathBuf;

use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, batch_files_intent, g5_intent, g5_panel};
use burnburn_presentation::{HeadlessEditorHarness, TraceRecipeIntent, UiIntent};
use burnburn_types::{DocumentId, StableId};
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

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::TracePrep, locale, &[]).expect("the preparation panel").lines
}

fn button(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale, c: C) -> Option<(String, bool)> {
    lines(h, doc, locale).into_iter().find_map(|l| match l {
        G5Line::Button { control, title, enabled } if control == c => Some((title, enabled)),
        _ => None,
    })
}

fn run_intent(doc: DocumentId, paths: &[&str]) -> UiIntent {
    UiIntent::TraceRecipe {
        document: doc,
        intent: TraceRecipeIntent::RunBatch { paths: paths.iter().map(PathBuf::from).collect() },
    }
}

#[test]
fn the_batch_has_a_choose_files_button_in_both_languages_that_is_off_while_it_runs() {
    let (mut h, doc) = opened();
    assert_eq!(button(&h, doc, Locale::En, C::BatchChoose), Some(("Choose files…".to_owned(), true)));
    assert_eq!(button(&h, doc, Locale::Nl, C::BatchChoose), Some(("Kies bestanden…".to_owned(), true)));
    // While a batch runs the button is off (the panel cannot start a second one).
    let paths = vec![PathBuf::from("/work/two.png"), PathBuf::from("/work/three.png")];
    h.send(UiIntent::TraceRecipe { document: doc, intent: TraceRecipeIntent::RunBatch { paths } });
    assert_eq!(button(&h, doc, Locale::En, C::BatchChoose).map(|b| b.1), Some(false));
    h.run_tasks();
    assert_eq!(button(&h, doc, Locale::En, C::BatchChoose).map(|b| b.1), Some(true));
}

#[test]
fn the_choose_button_asks_the_host_for_a_panel_and_makes_no_intent_itself() {
    let (h, doc) = opened();
    let vm = h.view();
    assert_eq!(g5_intent(&vm, doc, G5Panel::TracePrep, C::BatchChoose, &[]), None);
    assert_eq!(
        translate(
            NativeEvent::G5Control {
                document: doc,
                panel: G5Panel::TracePrep,
                control: C::BatchChoose,
                values: vec![]
            },
            &vm
        ),
        Reaction::ShowTraceBatchPanel
    );
}

#[test]
fn a_running_batch_does_not_open_a_second_panel() {
    let (mut h, doc) = opened();
    h.send(UiIntent::TraceRecipe {
        document: doc,
        intent: TraceRecipeIntent::RunBatch { paths: vec![PathBuf::from("/work/two.png")] },
    });
    let vm = h.view();
    assert_eq!(
        translate(
            NativeEvent::G5Control {
                document: doc,
                panel: G5Panel::TracePrep,
                control: C::BatchChoose,
                values: vec![]
            },
            &vm
        ),
        Reaction::Nothing
    );
}

#[test]
fn the_chosen_files_become_one_batch_in_the_order_the_panel_gave_them() {
    let (mut h, doc) = opened();
    let vm = h.view();
    let paths = vec![PathBuf::from("/work/three.png"), PathBuf::from("/work/two.png")];
    let want = run_intent(doc, &["/work/three.png", "/work/two.png"]);
    assert_eq!(batch_files_intent(doc, paths.clone()), Some(want.clone()));
    assert_eq!(
        translate(NativeEvent::TraceBatchFilesChosen { document: doc, paths }, &vm),
        Reaction::Intent(want.clone())
    );
    // The intent is the batch the panel's text box would have started: same report either way.
    h.send(want);
    h.run_tasks();
    let report: Vec<String> = lines(&h, doc, Locale::En)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(report.iter().any(|t| t.contains("2 file(s)")), "{report:?}");
    assert!(report.iter().any(|t| t.starts_with("three.png")), "{report:?}");
}

#[test]
fn a_cancelled_or_empty_choice_and_a_closed_window_do_nothing() {
    let (h, doc) = opened();
    let vm = h.view();
    assert_eq!(batch_files_intent(doc, Vec::new()), None);
    assert_eq!(
        translate(NativeEvent::TraceBatchFilesChosen { document: doc, paths: Vec::new() }, &vm),
        Reaction::Nothing
    );
    let gone = DocumentId::from_raw(999);
    assert_eq!(
        translate(
            NativeEvent::TraceBatchFilesChosen { document: gone, paths: vec![PathBuf::from("/work/two.png")] },
            &vm
        ),
        Reaction::Nothing
    );
}

#[test]
fn files_chosen_twice_in_a_panel_are_traced_once() {
    // A host that reports the same URL twice (e.g. a symlink and its target in one selection) must not
    // make the batch trace the file twice: the first occurrence stays, the order is kept.
    let (_, doc) = opened();
    let paths = ["/work/two.png", "/work/three.png", "/work/two.png"].map(PathBuf::from).to_vec();
    assert_eq!(batch_files_intent(doc, paths), Some(run_intent(doc, &["/work/two.png", "/work/three.png"])));
}
