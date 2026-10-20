//! X03 (W3-UI-app): Help, support data and the debug log. The report is shown before it is
//! written, leaves project names, paths and machine identifiers out unless consent is given, and
//! the debug log holds intent names only.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use burnburn_io::fs::{FileSystem, FsOp};
use burnburn_presentation::{
    AppController, AppIntent, CommandKey, DEBUG_LOG_LINES, DiagnosticOptions, HeadlessEditorHarness, HistoryStore,
    HistoryStoreError, HostEffect, IntentRecord, OutcomeRecord, PrefKey, Recorded, UiIntent, UploadRecord,
};

fn open_secret_project(h: &mut HeadlessEditorHarness) -> burnburn_types::DocumentId {
    // A project whose name and folder identify a customer.
    let fs = h.shared_fs();
    fs.create_dir_all_for_test(Path::new("/klanten/acme-bv"));
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 30.0));
    h.send(UiIntent::Save { document: doc, path: Some(PathBuf::from("/klanten/acme-bv/offerte-kast.burnburn")) });
    h.run_tasks();
    doc
}

#[test]
fn the_report_says_what_it_is_and_leaves_project_names_and_paths_out_by_default() {
    let mut h = HeadlessEditorHarness::new();
    open_secret_project(&mut h);
    let report = h.controller().support_report(&DiagnosticOptions::default());
    let info = AppController::help_info();
    assert!(report.contains(info.version), "{report}");
    assert!(report.contains(info.os) && report.contains(info.arch), "{report}");
    assert!(report.contains("windows: 1"), "{report}");
    assert!(report.contains("project names and paths: left out"), "the report says what it left out:\n{report}");
    for secret in ["acme", "offerte", "klanten", "Rechthoek"] {
        assert!(!report.contains(secret), "{secret:?} must not be in the report:\n{report}");
    }
    assert!(report.contains("grid_snap_mm"), "preferences are part of the report");
}

#[test]
fn consent_adds_the_title_and_path_of_each_window_and_says_so() {
    let mut h = HeadlessEditorHarness::new();
    open_secret_project(&mut h);
    let report =
        h.controller().support_report(&DiagnosticOptions { include_project_paths: true, ..Default::default() });
    assert!(report.contains("/klanten/acme-bv/offerte-kast.burnburn"), "{report}");
    assert!(report.contains("project names and paths: included"), "{report}");
}

struct FakeJournal {
    asked_with: Mutex<Vec<DiagnosticOptions>>,
}

impl HistoryStore for FakeJournal {
    fn record_upload(&self, _: &UploadRecord) -> Result<(burnburn_machine::ExecutionId, Recorded), HistoryStoreError> {
        Err(HistoryStoreError("unused".into()))
    }
    fn record_intent(&self, _: &IntentRecord) -> Result<Recorded, HistoryStoreError> {
        Err(HistoryStoreError("unused".into()))
    }
    fn record_outcome(&self, _: &OutcomeRecord) -> Result<Recorded, HistoryStoreError> {
        Err(HistoryStoreError("unused".into()))
    }
    fn diagnostic_report(&self, options: &DiagnosticOptions) -> Result<String, HistoryStoreError> {
        self.asked_with.lock().unwrap().push(*options);
        Ok("JOURNAL-REPLAY-TEXT".into())
    }
    fn recover(
        &self,
        _: &burnburn_machine::MachineId,
    ) -> Result<burnburn_presentation::RecoveredMachine, HistoryStoreError> {
        Ok(Default::default())
    }
    fn record_order(&self, _: &burnburn_presentation::StoredOrder) -> Result<(), HistoryStoreError> {
        Ok(())
    }
    fn orders(&self) -> Result<burnburn_presentation::StoredOrders, HistoryStoreError> {
        Ok(Default::default())
    }
}

#[test]
fn the_journal_replay_is_included_with_the_same_consent_and_without_one_the_report_says_so() {
    let mut h = HeadlessEditorHarness::new();
    assert!(h.controller().support_report(&DiagnosticOptions::default()).contains("journal: not available"));
    let journal = Arc::new(FakeJournal { asked_with: Mutex::new(Vec::new()) });
    h.controller().attach_history(journal.clone()).expect("history");
    let options = DiagnosticOptions { include_project_paths: false, include_machine_ids: true };
    let report = h.controller().support_report(&options);
    assert!(report.contains("JOURNAL-REPLAY-TEXT"), "{report}");
    assert_eq!(*journal.asked_with.lock().unwrap(), vec![options], "the consent goes to the journal unchanged");
}

#[test]
fn the_bundle_is_the_report_written_atomically_to_the_chosen_file_and_a_failure_is_reported() {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(Path::new("/out"));
    let options = DiagnosticOptions::default();
    let expected = h.controller().support_report(&options);
    h.send(UiIntent::App(AppIntent::ExportSupportBundle { options, path: PathBuf::from("/out/support.txt") }));
    h.run_tasks();
    let written = String::from_utf8(h.fs().read(Path::new("/out/support.txt"), u64::MAX).expect("written")).unwrap();
    assert_eq!(written, expected, "what was shown is what is written");

    h.fs().fail_on(FsOp::Replace, 0);
    h.send(UiIntent::App(AppIntent::ExportSupportBundle { options, path: PathBuf::from("/out/second.txt") }));
    let updates = h.run_tasks();
    assert!(
        updates.iter().flat_map(|u| &u.effects).any(|e| matches!(e, HostEffect::ShowError { .. })),
        "the failure is shown, not swallowed"
    );
    assert!(h.fs().read(Path::new("/out/second.txt"), u64::MAX).is_err(), "no half-written bundle");
}

#[test]
fn the_debug_log_is_off_by_default_and_records_intent_names_never_their_contents() {
    let mut h = HeadlessEditorHarness::new();
    h.send(UiIntent::NewDocument);
    assert!(!h.controller().debug_log_view().enabled);
    assert!(h.controller().debug_log_view().lines.is_empty(), "nothing is kept while it is off");

    h.send(UiIntent::App(AppIntent::SetDebugLog { enabled: true }));
    h.send(UiIntent::Open { path: PathBuf::from("/klanten/acme-bv/offerte-kast.burnburn") });
    h.send(UiIntent::NewDocument);
    let log = h.controller().debug_log_view();
    assert!(log.enabled);
    let text = log.lines.join("\n");
    assert!(text.contains("Open"), "{text}");
    assert!(text.contains("NewDocument"), "{text}");
    assert!(!text.contains("klanten") && !text.contains("acme") && !text.contains("offerte"), "{text}");
    // The same text goes into the support report, and nothing more specific.
    let report = h.controller().support_report(&DiagnosticOptions::default());
    assert!(report.contains("NewDocument") && !report.contains("acme"), "{report}");
}

#[test]
fn the_debug_log_collapses_repeats_stays_bounded_and_can_be_cleared() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::App(AppIntent::SetDebugLog { enabled: true }));
    for _ in 0..40 {
        h.hover(doc, (5.0, 5.0));
    }
    let lines = h.controller().debug_log_view().lines;
    let pointer: Vec<_> = lines.iter().filter(|l| l.contains("Pointer")).collect();
    assert_eq!(pointer.len(), 1, "{lines:?}");
    assert!(pointer[0].contains("×40"), "{pointer:?}");

    for i in 0..(DEBUG_LOG_LINES + 50) {
        let command = if i % 2 == 0 { CommandKey::SelectAll } else { CommandKey::InvertSelection };
        h.invoke(doc, command);
    }
    let lines = h.controller().debug_log_view().lines;
    assert_eq!(lines.len(), DEBUG_LOG_LINES, "the oldest lines go first");
    h.send(UiIntent::App(AppIntent::ClearDebugLog));
    assert!(h.controller().debug_log_view().lines.is_empty());
    assert!(h.controller().debug_log_view().enabled, "clearing does not switch it off");
    h.send(UiIntent::App(AppIntent::SetDebugLog { enabled: false }));
    h.invoke(doc, CommandKey::SelectAll);
    assert!(h.controller().debug_log_view().lines.is_empty());
}

#[test]
fn help_info_is_static_and_the_manual_is_a_link_only_when_this_build_knows_one() {
    let info = AppController::help_info();
    assert_eq!(info.app_name, "BurnBurn");
    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    if let Some(url) = info.manual_url {
        assert!(url.starts_with("https://"), "a manual link is https: {url}");
    }
}

#[test]
fn the_report_never_names_the_user_folder_or_the_settings_file() {
    let mut h = HeadlessEditorHarness::new();
    h.controller().set_user_dir(PathBuf::from("/Users/jan.jansen/Library/Application Support/BurnBurn"));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::GridSnap, text: "2".into() }));
    let report = h.controller().support_report(&DiagnosticOptions::default());
    assert!(!report.contains("jansen") && !report.contains("preferences.conf"), "{report}");
}
