//! The variable-text panel and its place in the job (W3-UI-tekst, E11), headless: the panel's
//! edits, the counters and data rows, Test and Bake, the production record bound into compile
//! (so preview, RD export and upload of one revision print the same text) and the counter commit
//! on the chosen job event only.
//!
//! Oracles: a variable text and a literal text with the resolved content give byte-identical RD
//! (`rd_hash`); serials and dates follow the independent fixtures of `generators/tests/variable_text.rs`
//! (1 781 751 845 s at −300 min is 2026-06-17 22:04:05 local); a frozen job's bytes are compared with
//! the hash recorded when it was prepared.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use burnburn_document::UndoLabel;
use burnburn_document::text::{TextLabel, TextObject};
use burnburn_generators::JobClock;
use burnburn_machine::{Reply, Request};
use burnburn_presentation::{
    AdvancePolicy, CommandKey, DocumentViewModel, HeadlessEditorHarness, MachineAction, SerialField, TextEdit,
    UiIntent, VariableEdit, VariableIssue, VariablePanelViewModel,
};
use burnburn_test_support::Step;
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::{ContentHash, DocumentId, HashDomain};

use common::*;

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let font = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), font)
}

/// 2026-06-17 22:04:05 at UTC−5 (Python `datetime`, see `generators/tests/variable_text.rs`).
fn clock() -> JobClock {
    JobClock::new(1_781_751_845, -300).unwrap()
}

const CSV: &str = "naam;nummer\nJansen;7\nPietersen;\nÅsa;9\n";

fn prepare_harness(h: &mut HeadlessEditorHarness) {
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.controller().attach_clock(Arc::new(|| Some(clock())));
}

/// A window with a variable text `content` (selected), serial 10..95 by 5, four digits.
fn variable_doc(content: &str) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    prepare_harness(&mut h);
    let doc = h.new_document().unwrap();
    add_variable_text(&mut h, doc, content);
    for (field, text) in [
        (SerialField::Start, "10"),
        (SerialField::End, "95"),
        (SerialField::AdvanceBy, "5"),
        (SerialField::Digits, "4"),
    ] {
        edit(&mut h, doc, VariableEdit::Serial(field, text.into()));
    }
    (h, doc)
}

fn add_variable_text(h: &mut HeadlessEditorHarness, doc: DocumentId, content: &str) {
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: burnburn_presentation::SelectMode::Replace });
    h.send(UiIntent::InsertText { document: doc, text: content.into() });
    edit(h, doc, VariableEdit::SetVariable(true));
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

fn sources(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<TextObject> {
    h.controller().text_sources(doc)
}

fn problem(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<VariableIssue> {
    panel(h, doc).problem.map(|p| p.issue)
}

// ---------------------------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------------------------

#[test]
fn the_panel_follows_the_selected_text_and_switches_between_normal_and_variable() {
    let mut h = HeadlessEditorHarness::new();
    prepare_harness(&mut h);
    let doc = h.new_document().unwrap();
    assert!(vm(&h, doc).variable_panel.is_none(), "no text, no panel");

    h.send(UiIntent::InsertText { document: doc, text: "SN-{serial}".into() });
    let p = panel(&h, doc);
    assert!(!p.variable && p.preview.is_none(), "a new text is literal, braces and all");

    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, VariableEdit::SetVariable(true));
    let after = vm(&h, doc);
    assert_eq!((after.revision.raw(), after.undo), (rev.raw() + 1, Some(UndoLabel::Text(TextLabel::Edit))));
    assert!(sources(&mut h, doc)[0].variable.is_some());
    let p = panel(&h, doc);
    assert!(p.variable);
    assert_eq!(p.preview, Some(Ok("SN-1".to_owned())), "the default series starts at 1");

    edit(&mut h, doc, VariableEdit::SetVariable(true));
    assert_eq!(vm(&h, doc).revision, after.revision, "already variable: no command");
    edit(&mut h, doc, VariableEdit::SetVariable(false));
    assert!(sources(&mut h, doc)[0].variable.is_none());
    h.invoke(doc, CommandKey::Undo);
    assert!(sources(&mut h, doc)[0].variable.is_some(), "one undo step per switch");
}

#[test]
fn the_texts_own_offset_moves_it_along_the_series() {
    let (mut h, doc) = variable_doc("{serial}");
    assert_eq!(panel(&h, doc).preview, Some(Ok("0010".into())));
    edit(&mut h, doc, VariableEdit::Offset("3".into()));
    assert_eq!(sources(&mut h, doc)[0].variable.unwrap().offset, 3);
    assert_eq!(panel(&h, doc).preview, Some(Ok("0025".into())), "three steps of five on from 10");
    assert_eq!(panel(&h, doc).offset, "3");

    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, VariableEdit::Offset("drie".into()));
    assert_eq!(vm(&h, doc).revision, rev);
    assert_eq!(problem(&h, doc), Some(VariableIssue::InvalidOffset));
}

#[test]
fn serial_settings_are_parsed_here_and_previous_next_reset_walk_the_series() {
    let (mut h, doc) = variable_doc("{serial}");
    let s = panel(&h, doc).serial;
    assert_eq!(
        (s.current.as_str(), s.start.as_str(), s.end.as_str(), s.advance_by.as_str(), s.digits.as_str(), s.wrap),
        ("10", "10", "95", "5", "4", true)
    );
    edit(&mut h, doc, VariableEdit::Next);
    assert_eq!(panel(&h, doc).serial.current, "15");
    assert_eq!(panel(&h, doc).preview, Some(Ok("0015".into())));
    edit(&mut h, doc, VariableEdit::Previous);
    edit(&mut h, doc, VariableEdit::Previous);
    assert_eq!(panel(&h, doc).serial.current, "95", "previous wraps backwards to the last value");
    edit(&mut h, doc, VariableEdit::Reset);
    assert_eq!(panel(&h, doc).serial.current, "10");

    edit(&mut h, doc, VariableEdit::Serial(SerialField::Current, "95".into()));
    assert_eq!(panel(&h, doc).serial.current, "95");
    // Typed input that is not a value of the series, or not a number, changes nothing and says why.
    for (field, text, issue) in [
        (SerialField::Current, "96", VariableIssue::NotInSeries),
        (SerialField::Start, "begin", VariableIssue::InvalidNumber(SerialField::Start)),
        (SerialField::End, "5", VariableIssue::InvalidSerial),
        (SerialField::AdvanceBy, "0", VariableIssue::InvalidSerial),
        (SerialField::Digits, "21", VariableIssue::InvalidSerial),
        (SerialField::Digits, "-1", VariableIssue::InvalidNumber(SerialField::Digits)),
    ] {
        let before = panel(&h, doc).serial;
        edit(&mut h, doc, VariableEdit::Serial(field, text.into()));
        assert_eq!(problem(&h, doc), Some(issue), "{field:?} {text:?}");
        assert_eq!(panel(&h, doc).serial, before, "{field:?} {text:?}: nothing changed");
    }

    // A series that stops refuses to pass its end instead of wrapping.
    edit(&mut h, doc, VariableEdit::Wrap(false));
    assert!(!panel(&h, doc).serial.wrap);
    edit(&mut h, doc, VariableEdit::Next);
    assert_eq!(problem(&h, doc), Some(VariableIssue::SerialExhausted));
    assert_eq!(panel(&h, doc).serial.current, "95", "the counter stays on its last value");
    edit(&mut h, doc, VariableEdit::Wrap(true));
    edit(&mut h, doc, VariableEdit::Next);
    assert_eq!(panel(&h, doc).serial.current, "10");
    assert_eq!(problem(&h, doc), None, "the next accepted edit clears the reason");
}

#[test]
fn the_series_keeps_the_current_value_when_it_is_still_in_the_series() {
    let (mut h, doc) = variable_doc("{serial}");
    edit(&mut h, doc, VariableEdit::Next); // 15
    edit(&mut h, doc, VariableEdit::Serial(SerialField::End, "200".into()));
    assert_eq!(panel(&h, doc).serial.current, "15", "End moved, the counter did not");
    edit(&mut h, doc, VariableEdit::Serial(SerialField::Start, "20".into()));
    assert_eq!(panel(&h, doc).serial.current, "20", "15 is no longer in the series: back to the start");
}

#[test]
fn csv_rows_are_chosen_listed_and_cleared_and_a_bad_file_keeps_the_old_one() {
    let (mut h, doc) = variable_doc("{csv:naam}");
    assert!(panel(&h, doc).csv.is_none());
    edit(&mut h, doc, VariableEdit::LoadCsv { name: "klanten.csv".into(), bytes: CSV.as_bytes().to_vec() });
    let csv = panel(&h, doc).csv.expect("the file is loaded");
    assert_eq!((csv.name.as_str(), csv.rows, csv.row.as_str()), ("klanten.csv", 3, "1"));
    assert_eq!(csv.columns, ["naam", "nummer"], "the delimiter is recognised from the header");
    assert_eq!(panel(&h, doc).preview, Some(Ok("Jansen".into())));

    edit(&mut h, doc, VariableEdit::Row("3".into()));
    assert_eq!(panel(&h, doc).preview, Some(Ok("Åsa".into())));
    for text in ["0", "4", "x", ""] {
        edit(&mut h, doc, VariableEdit::Row(text.into()));
        assert_eq!(problem(&h, doc), Some(VariableIssue::RowOutOfRange), "{text:?}");
        assert_eq!(panel(&h, doc).csv.unwrap().row, "3", "{text:?}: the chosen row stays");
    }

    // Not UTF-8: refused, the loaded data stays.
    edit(&mut h, doc, VariableEdit::LoadCsv { name: "kapot.csv".into(), bytes: vec![0x6e, 0xff, 0xfe] });
    assert_eq!(problem(&h, doc), Some(VariableIssue::CsvUnreadable));
    assert_eq!(panel(&h, doc).csv.unwrap().name, "klanten.csv");

    edit(&mut h, doc, VariableEdit::ClearCsv);
    assert!(panel(&h, doc).csv.is_none());
    assert_eq!(panel(&h, doc).preview.unwrap().unwrap_err().issue, VariableIssue::NoData, "no data, no label");
}

#[test]
fn a_missing_value_is_an_error_naming_column_and_row_never_an_empty_label() {
    let (mut h, doc) = variable_doc("{csv:nummer}");
    edit(&mut h, doc, VariableEdit::LoadCsv { name: "klanten.csv".into(), bytes: CSV.as_bytes().to_vec() });
    assert_eq!(panel(&h, doc).preview, Some(Ok("7".into())));
    edit(&mut h, doc, VariableEdit::Row("2".into())); // Pietersen has no number
    let missing = panel(&h, doc).preview.unwrap().unwrap_err();
    assert_eq!(missing.issue, VariableIssue::MissingValue);
    assert!(missing.detail.contains("nummer") && missing.detail.contains('2'), "{missing:?}");

    edit(&mut h, doc, VariableEdit::Test);
    let lines = panel(&h, doc).test.expect("Test ran");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].template, "{csv:nummer}");
    assert_eq!(lines[0].result.clone().unwrap_err().issue, VariableIssue::MissingValue);

    let (mut h2, doc2) = variable_doc("{csv:kolom}");
    edit(&mut h2, doc2, VariableEdit::LoadCsv { name: "k.csv".into(), bytes: CSV.as_bytes().to_vec() });
    assert_eq!(panel(&h2, doc2).preview.unwrap().unwrap_err().issue, VariableIssue::MissingColumn);
}

#[test]
fn test_resolves_every_variable_text_of_the_document_and_skips_literal_text() {
    let (mut h, doc) = variable_doc("SN-{serial}");
    add_variable_text(&mut h, doc, "{date:%Y-%m-%d} {cut:C01.speed}");
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: burnburn_presentation::SelectMode::Replace });
    h.send(UiIntent::InsertText { document: doc, text: "{literal}".into() });
    assert_eq!(h.text_contents(doc).len(), 3);
    edit(&mut h, doc, VariableEdit::Test);
    let lines = panel(&h, doc).test.expect("Test ran");
    assert_eq!(lines.len(), 2, "the literal text is not variable");
    assert_eq!(lines[0].result, Ok("SN-0010".into()));
    // The date follows the clock's fixed zone; no layer C01 exists in this document.
    assert_eq!(lines[1].result.clone().unwrap_err().issue, VariableIssue::NoCutValue);
    // A change clears the old result.
    edit(&mut h, doc, VariableEdit::Next);
    assert!(panel(&h, doc).test.is_none());
}

#[test]
fn bake_replaces_the_template_by_the_resolved_text_in_one_undo_step() {
    let (mut h, doc) = variable_doc("SN-{serial}");
    edit(&mut h, doc, VariableEdit::Next);
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, VariableEdit::Bake);
    let after = vm(&h, doc);
    assert_eq!((after.revision.raw(), after.undo), (rev.raw() + 1, Some(UndoLabel::Text(TextLabel::Edit))));
    assert_eq!(h.text_contents(doc), ["SN-0015"]);
    assert!(sources(&mut h, doc)[0].variable.is_none(), "baked text is literal");
    assert!(!panel(&h, doc).variable);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.text_contents(doc), ["SN-{serial}"]);
    assert!(panel(&h, doc).variable);

    // A template that cannot be resolved is not baked.
    let (mut h, doc) = variable_doc("{csv:naam}");
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, VariableEdit::Bake);
    assert_eq!(vm(&h, doc).revision, rev);
    assert_eq!(problem(&h, doc), Some(VariableIssue::NoData));
    // Bake on literal text does nothing.
    edit(&mut h, doc, VariableEdit::SetVariable(false));
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, VariableEdit::Bake);
    assert_eq!(vm(&h, doc).revision, rev);
}

#[test]
fn auto_advance_is_a_choice_of_job_event_and_manual_is_the_default() {
    let (mut h, doc) = variable_doc("{serial}");
    assert_eq!(panel(&h, doc).policy, AdvancePolicy::Manual);
    edit(&mut h, doc, VariableEdit::Policy(AdvancePolicy::OnUploadConfirmed));
    assert_eq!(panel(&h, doc).policy, AdvancePolicy::OnUploadConfirmed);
    // The panel has no unsupported neighbours: the text toolbar still works next to it.
    h.send(UiIntent::Text { document: doc, edit: TextEdit::Uppercase(true) });
    assert!(vm(&h, doc).text_toolbar.unwrap().variable);
}

// ---------------------------------------------------------------------------------------------
// The job: record binding and counter commit
// ---------------------------------------------------------------------------------------------

/// The rig's rectangle job plus a text with its own cut operation.
fn text_rig(content: &str, variable: bool) -> Rig {
    let mut r = rig();
    prepare_harness(&mut r.h);
    let doc = r.doc;
    r.h.send(UiIntent::Select { document: doc, ids: vec![], mode: burnburn_presentation::SelectMode::Replace });
    // The Text tool's click places the text on the bed (at the origin it would lie outside).
    r.h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.text".into() });
    r.h.click(doc, (20.0, 50.0), Default::default());
    r.h.send(UiIntent::InsertText { document: doc, text: content.into() });
    r.h.invoke(doc, CommandKey::ToolSelect);
    if variable {
        edit(&mut r.h, doc, VariableEdit::SetVariable(true));
        for (field, text) in [
            (SerialField::Start, "10"),
            (SerialField::End, "95"),
            (SerialField::AdvanceBy, "5"),
            (SerialField::Digits, "4"),
        ] {
            edit(&mut r.h, doc, VariableEdit::Serial(field, text.into()));
        }
    }
    r.h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    r
}

fn compile_hash(r: &mut Rig) -> ContentHash {
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    r.h.invoke(r.doc, CommandKey::PrepareJob);
    r.h.run_tasks();
    r.h.document(r.doc).unwrap().job.expect("prepared job").rd_hash
}

#[test]
fn compile_binds_the_record_so_the_job_equals_the_literal_text() {
    // E11: preview, RD export and upload print what the record says, not the template.
    let mut variable = text_rig("SN-{serial}", true);
    let mut literal = text_rig("SN-0010", false);
    let (a, b) = (compile_hash(&mut variable), compile_hash(&mut literal));
    assert_eq!(a, b, "the variable job is byte-identical to the literal one");
    assert!(panel(&variable.h, variable.doc).bound_record.is_some(), "the compile says which record it used");
    assert!(vm(&literal.h, literal.doc).variable_panel.is_some_and(|p| p.bound_record.is_none()));

    // Another serial prints another text, and the document itself is untouched by compiling.
    edit(&mut variable.h, variable.doc, VariableEdit::Next);
    let c = compile_hash(&mut variable);
    assert_ne!(c, a);
    assert_eq!(variable.h.text_contents(variable.doc).last().map(String::as_str), Some("SN-{serial}"));
    let mut next_literal = text_rig("SN-0015", false);
    assert_eq!(c, compile_hash(&mut next_literal));
}

#[test]
fn the_date_comes_from_the_clock_in_its_fixed_zone() {
    let mut variable = text_rig("{date:%Y-%m-%d %H:%M}", true);
    let mut literal = text_rig("2026-06-17 22:04", false);
    assert_eq!(compile_hash(&mut variable), compile_hash(&mut literal));
}

#[test]
fn a_template_that_cannot_be_resolved_stops_the_compile_and_says_why() {
    // No CSV chosen.
    let mut r = text_rig("{csv:naam}", true);
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    let v = r.h.document(r.doc).unwrap();
    assert!(v.plan.is_none(), "no plan with an unresolved label");
    assert_eq!(v.variable_panel.unwrap().problem.map(|p| p.issue), Some(VariableIssue::NoData));

    // A cell that is empty in the chosen row.
    let mut r = text_rig("{csv:nummer}", true);
    edit(&mut r.h, r.doc, VariableEdit::LoadCsv { name: "k.csv".into(), bytes: CSV.as_bytes().to_vec() });
    edit(&mut r.h, r.doc, VariableEdit::Row("2".into()));
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    let v = r.h.document(r.doc).unwrap();
    assert!(v.plan.is_none());
    assert_eq!(v.variable_panel.unwrap().problem.map(|p| p.issue), Some(VariableIssue::MissingValue));
    // Choosing a row with a value compiles.
    edit(&mut r.h, r.doc, VariableEdit::Row("1".into()));
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    assert!(r.h.document(r.doc).unwrap().plan.is_some());
    assert_eq!(problem(&r.h, r.doc), None);

    // A date without a clock.
    let mut r = rig();
    let (src, font) = fonts();
    r.h.controller().attach_fonts(src, font);
    r.h.send(UiIntent::InsertText { document: r.doc, text: "{date}".into() });
    edit(&mut r.h, r.doc, VariableEdit::SetVariable(true));
    r.h.send(UiIntent::AddOperation { document: r.doc, draft: cut() });
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    assert!(r.h.document(r.doc).unwrap().plan.is_none());
    assert_eq!(problem(&r.h, r.doc), Some(VariableIssue::NoClock));
}

#[test]
fn frozen_job_keeps_chosen_data_rows() {
    // E11 in the UI: the prepared job keeps the row, serial and time it was prepared with.
    let mut r = text_rig("{csv:naam} #{serial}", true);
    let doc = r.doc;
    edit(&mut r.h, doc, VariableEdit::LoadCsv { name: "klanten.csv".into(), bytes: CSV.as_bytes().to_vec() });
    let mut first = text_rig("Jansen #0010", false);
    let job = prepare(&mut r.h, doc);
    assert_eq!(job.rd_hash, compile_hash(&mut first));

    // The user moves on: another row, a new serial, another table. The frozen job does not follow.
    edit(&mut r.h, doc, VariableEdit::Row("3".into()));
    edit(&mut r.h, doc, VariableEdit::Next);
    edit(&mut r.h, doc, VariableEdit::LoadCsv { name: "andere.csv".into(), bytes: b"naam\nAnders\n".to_vec() });
    let card = r.h.document(doc).unwrap().job.expect("job kept");
    assert_eq!((card.plan, card.rd_hash), (job.plan, job.rd_hash));

    // The upload sends exactly the bytes of the frozen job.
    upload_and_store(&mut r, &job);
    assert_eq!(ContentHash::of(HashDomain::Rd, &uploaded_bytes(&r.peer)), job.rd_hash);

    // A new compile with the new data is another plan.
    r.h.invoke(doc, CommandKey::Compile);
    r.h.run_tasks();
    let plan = r.h.document(doc).unwrap().plan.expect("recompiled");
    assert_ne!(plan.id, job.plan);
}

fn counters(r: &Rig) -> (String, String) {
    let p = r.h.document(r.doc).unwrap().variable_panel.unwrap();
    (p.serial.current, p.csv.map_or_else(String::new, |c| c.row))
}

fn with_policy(policy: AdvancePolicy) -> (Rig, burnburn_presentation::JobCardViewModel) {
    let mut r = text_rig("{csv:naam} #{serial}", true);
    let doc = r.doc;
    edit(&mut r.h, doc, VariableEdit::LoadCsv { name: "klanten.csv".into(), bytes: CSV.as_bytes().to_vec() });
    edit(&mut r.h, doc, VariableEdit::Policy(policy));
    let job = prepare(&mut r.h, doc);
    (r, job)
}

fn sync(r: &mut Rig) {
    r.h.controller().sync_production();
}

#[test]
fn the_counter_advances_once_on_the_chosen_event_only() {
    // Upload confirmed: not when the upload is merely acknowledged, once when it is stored.
    let (mut r, job) = with_policy(AdvancePolicy::OnUploadConfirmed);
    assert_eq!(counters(&r), ("10".into(), "1".into()), "preparing a job moves nothing");
    script_upload_acks(&r.peer, &job);
    let upload = job.upload.clone().unwrap();
    r.h.press(Some(r.doc), &upload);
    pump(&r);
    sync(&mut r);
    assert_eq!(counters(&r), ("10".into(), "1".into()), "an ACK is not a stored file");
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    let confirm = r.h.machine_view().unwrap().button(MachineAction::ConfirmStored).cloned().unwrap();
    r.h.press(None, &confirm);
    pump(&r);
    sync(&mut r);
    assert_eq!(counters(&r), ("15".into(), "2".into()), "stored: serial and row advance together");
    sync(&mut r);
    sync(&mut r);
    assert_eq!(counters(&r), ("15".into(), "2".into()), "the same job never advances twice");

    // Job started: the start request is not enough, a running job counts once.
    let (mut r, job) = with_policy(AdvancePolicy::OnJobStarted);
    upload_and_store(&mut r, &job);
    sync(&mut r);
    assert_eq!(counters(&r), ("10".into(), "1".into()), "stored is not started");
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    let start = r.h.machine_view().unwrap().button(MachineAction::Start).cloned().unwrap();
    r.h.press(None, &start);
    pump(&r);
    sync(&mut r);
    assert_eq!(counters(&r), ("10".into(), "1".into()), "a start ACK is not proof that the job runs");
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(burnburn_machine::ControllerStatus::Running)));
    let query = r.h.machine_view().unwrap().button(MachineAction::QueryStatus).cloned().unwrap();
    r.h.press(None, &query);
    pump(&r);
    sync(&mut r);
    assert_eq!(counters(&r), ("15".into(), "2".into()));
    sync(&mut r);
    assert_eq!(counters(&r), ("15".into(), "2".into()));

    // Manual: no event moves anything.
    let (mut r, job) = with_policy(AdvancePolicy::Manual);
    upload_and_store(&mut r, &job);
    sync(&mut r);
    assert_eq!(counters(&r), ("10".into(), "1".into()));
}

#[test]
fn a_failed_upload_never_consumes_a_number() {
    // The controller reports another model: nothing is uploaded and the execution fails.
    let (mut r, job) = with_policy(AdvancePolicy::OnUploadConfirmed);
    script_identity(&r.peer, "RDC9999X");
    let upload = job.upload.clone().unwrap();
    r.h.press(Some(r.doc), &upload);
    pump(&r);
    sync(&mut r);
    assert_eq!(counters(&r), ("10".into(), "1".into()), "the job is retried with the same number and row");
    assert!(uploaded_bytes(&r.peer).is_empty());

    // The controller refuses a chunk: the upload failed after bytes were sent.
    let (mut r, job) = with_policy(AdvancePolicy::OnUploadConfirmed);
    script_identity(&r.peer, MODEL);
    r.peer.script(
        Step::expect_matching("chunk 0", |q| matches!(q, Request::UploadChunk { index: 0, .. })).reply(Reply::Nak),
    );
    let upload = job.upload.clone().unwrap();
    r.h.press(Some(r.doc), &upload);
    pump(&r);
    sync(&mut r);
    assert_eq!(r.h.machine_view().unwrap().status, burnburn_presentation::MachineStatusKey::Failed);
    assert_eq!(counters(&r), ("10".into(), "1".into()), "a rejected upload never consumes a number");
    sync(&mut r);
    assert_eq!(counters(&r), ("10".into(), "1".into()));
}

#[test]
fn a_manual_move_after_preparing_makes_the_job_forget_its_commit() {
    // The user repositions the counters by hand; the old job must not advance them again.
    let (mut r, job) = with_policy(AdvancePolicy::OnUploadConfirmed);
    edit(&mut r.h, r.doc, VariableEdit::Next);
    assert_eq!(counters(&r), ("15".into(), "2".into()));
    upload_and_store(&mut r, &job);
    sync(&mut r);
    assert_eq!(counters(&r), ("15".into(), "2".into()), "the counters were moved by hand, the job cannot commit");
}
