//! G5-UI-gaten E11: the serial counter, the advance policy and the chosen data row are saved with
//! the project and come back on reopen; a change of the counters alone makes the window unsaved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_generators::JobClock;
use burnburn_presentation::{
    AdvancePolicy, DocumentViewModel, HeadlessEditorHarness, HostEffect, SerialField, UiIntent, VariableEdit,
    VariablePanelViewModel,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;

const CSV: &str = "naam;nummer\nJansen;7\nPietersen;8\nAsa;9\n";

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let font = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), font)
}

fn prepare(h: &mut HeadlessEditorHarness) {
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.controller().attach_clock(Arc::new(|| Some(JobClock::new(1_781_751_845, -300).unwrap())));
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: VariableEdit) {
    h.send(UiIntent::Variable { document: doc, edit });
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn panel(h: &HeadlessEditorHarness, doc: DocumentId) -> VariablePanelViewModel {
    vm(h, doc).variable_panel.expect("a text is selected")
}

/// A window with a variable text, series 10..95 by 5 (four digits), the CSV loaded.
fn working() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    prepare(&mut h);
    let doc = h.new_document().unwrap();
    h.send(UiIntent::InsertText { document: doc, text: "SN-{serial} {csv:naam}".into() });
    edit(&mut h, doc, VariableEdit::SetVariable(true));
    for (field, text) in [
        (SerialField::Start, "10"),
        (SerialField::End, "95"),
        (SerialField::AdvanceBy, "5"),
        (SerialField::Digits, "4"),
    ] {
        edit(&mut h, doc, VariableEdit::Serial(field, text.into()));
    }
    edit(&mut h, doc, VariableEdit::LoadCsv { name: "klanten.csv".into(), bytes: CSV.as_bytes().to_vec() });
    (h, doc)
}

fn save(h: &mut HeadlessEditorHarness, doc: DocumentId, name: &str) -> PathBuf {
    h.fs().create_dir_all_for_test(std::path::Path::new("/out"));
    let path = PathBuf::from(format!("/out/{name}.burnburn"));
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    path
}

fn reopen(first: &HeadlessEditorHarness, path: PathBuf) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::with_fs(first.shared_fs());
    prepare(&mut h);
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: burnburn_presentation::SelectMode::Replace });
    (h, doc)
}

#[test]
fn the_counter_policy_and_chosen_row_survive_save_and_reopen() {
    let (mut h, doc) = working();
    edit(&mut h, doc, VariableEdit::Next);
    edit(&mut h, doc, VariableEdit::Next); // serial 20, row 3
    edit(&mut h, doc, VariableEdit::Row("2".into()));
    edit(&mut h, doc, VariableEdit::Policy(AdvancePolicy::OnJobStarted));
    edit(&mut h, doc, VariableEdit::Wrap(false));
    let before = panel(&h, doc);
    let path = save(&mut h, doc, "serie");

    let (h2, doc2) = reopen(&h, path);
    let after = panel(&h2, doc2);
    assert_eq!(after.serial, before.serial, "series and current value");
    assert_eq!(after.policy, AdvancePolicy::OnJobStarted);
    let (csv_before, csv_after) = (before.csv.unwrap(), after.csv.expect("the table came back"));
    assert_eq!((csv_after.name.as_str(), csv_after.row.as_str(), csv_after.rows), ("klanten.csv", "2", 3));
    assert_eq!(csv_after.columns, csv_before.columns);
    assert_eq!(after.preview, before.preview, "the label would print the same text");
}

#[test]
fn a_project_with_untouched_counters_gets_no_production_entry() {
    let mut h = HeadlessEditorHarness::new();
    prepare(&mut h);
    let doc = h.new_document().unwrap();
    h.send(UiIntent::InsertText { document: doc, text: "gewoon".into() });
    let path = save(&mut h, doc, "gewoon");
    let bytes = burnburn_io::fs::FileSystem::read(h.fs(), &path, 1 << 30).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("production.json"));
}

#[test]
fn changing_only_the_counters_makes_the_window_unsaved_until_the_next_save() {
    let (mut h, doc) = working();
    save(&mut h, doc, "a");
    assert!(!vm(&h, doc).dirty);
    edit(&mut h, doc, VariableEdit::Next);
    assert!(vm(&h, doc).dirty, "the counter moved: closing would lose it");
    save(&mut h, doc, "a");
    assert!(!vm(&h, doc).dirty);
}
