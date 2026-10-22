//! P08 (G6-editor): the project search panel, headless. The folder is indexed on a worker, a query
//! filters what the index holds, a hit shows a preview and when the file was saved, and a frozen
//! production revision opens next to the current design as an unsaved copy. Searching and opening
//! only read: no production intent is recorded and nothing is compiled, uploaded or written.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use burnburn_document::structure::{Plate, StructureCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, FilesIntent, HeadlessEditorHarness, HistoryStore, HistoryStoreError, HostEffect, IntentRecord,
    OutcomeRecord, Recorded, SearchField, SearchPanelViewModel, TaskKind, UiIntent, UploadRecord,
};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn edit(s: &mut DocumentSession, payload: CommandPayload) {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap();
}

fn rect(s: &mut DocumentSession, name: &str, x: f64) {
    let r = Rect::new(Point::new(x, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    edit(s, CommandPayload::CreateRectangle { rect: r, name: name.into() });
}

/// /werk holds: deurbord (one object, a plate of birch plywood, a frozen revision "Order 4711" of
/// the design as it was, and a second object added after), onderzetters, and a broken file.
fn werk(h: &HeadlessEditorHarness) -> (PathBuf, PathBuf) {
    h.fs().create_dir_all_for_test(Path::new("/werk"));
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    rect(&mut s, "Deurbord", 0.0);
    let plate = Plate {
        name: "Plaat 1".into(),
        width: Length::from_mm(600.0).unwrap(),
        height: Length::from_mm(400.0).unwrap(),
        material: "Berken multiplex".into(),
        thickness: None,
    };
    edit(&mut s, CommandPayload::Structure(StructureCommand::CreatePlate { plate }));
    let mut deurbord = Project::new(ProjectUuid::from_bytes(*b"burnburn-search1"), (*s.snapshot()).clone());
    deurbord.freeze_revision("Order 4711").unwrap();
    // The design moves on after the production revision was frozen.
    rect(&mut s, "Extra paneel", 20.0);
    deurbord.document = (*s.snapshot()).clone();
    let deurbord_path = PathBuf::from("/werk/deurbord.burnburn");
    save_project(h.fs(), &deurbord_path, &deurbord).unwrap();
    let mut o = DocumentSession::new(DocumentId::from_raw(2));
    rect(&mut o, "Onderzetter", 0.0);
    let onderzetters = PathBuf::from("/werk/onderzetters.burnburn");
    save_project(
        h.fs(),
        &onderzetters,
        &Project::new(ProjectUuid::from_bytes(*b"burnburn-search2"), (*o.snapshot()).clone()),
    )
    .unwrap();
    h.fs().insert(Path::new("/werk/kapot.burnburn"), b"not a zip".to_vec());
    h.fs().set_modified_for_test(&deurbord_path, 1_781_751_845);
    (deurbord_path, onderzetters)
}

fn open_window(h: &mut HeadlessEditorHarness, path: &Path) -> DocumentId {
    h.send(UiIntent::Open { path: path.to_path_buf() });
    h.run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened")
}

fn search(h: &HeadlessEditorHarness, doc: DocumentId) -> SearchPanelViewModel {
    h.document(doc).unwrap().files.search.expect("the search panel is open")
}

fn files(h: &mut HeadlessEditorHarness, intent: FilesIntent) {
    h.send(UiIntent::Files(intent));
    h.run_tasks();
}

/// A window on deurbord with the search panel open (and the folder read).
fn searching() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let (deurbord, _) = werk(&h);
    let doc = open_window(&mut h, &deurbord);
    h.invoke(doc, CommandKey::SearchProjects);
    h.run_tasks();
    (h, doc)
}

#[test]
fn the_panel_opens_on_the_folder_of_the_project_and_lists_every_project_without_a_query() {
    let (h, doc) = searching();
    let s = search(&h, doc);
    assert_eq!(s.folder, "/werk");
    assert!(!s.scanning);
    assert_eq!(s.scanned, Some(2));
    assert_eq!(
        s.results.iter().map(|r| r.file_name.as_str()).collect::<Vec<_>>(),
        ["deurbord.burnburn", "onderzetters.burnburn"]
    );
    assert!(s.results.iter().all(|r| r.matched.is_empty()), "nothing was asked, nothing matched");
    assert_eq!(
        s.issues.iter().map(|i| i.file_name.as_str()).collect::<Vec<_>>(),
        ["kapot.burnburn"],
        "a broken file is listed"
    );
}

#[test]
fn a_query_shows_what_matched_in_the_project() {
    let (mut h, doc) = searching();
    files(&mut h, FilesIntent::SetSearchQuery { document: doc, query: "MULTIPLEX".into() });
    let s = search(&h, doc);
    assert_eq!(s.results.len(), 1);
    assert_eq!(
        (s.results[0].file_name.as_str(), s.results[0].matched.clone()),
        ("deurbord.burnburn", vec![SearchField::Material])
    );
    files(&mut h, FilesIntent::SetSearchQuery { document: doc, query: "4711".into() });
    assert_eq!(search(&h, doc).results[0].matched, [SearchField::Revision]);
    files(&mut h, FilesIntent::SetSearchQuery { document: doc, query: "onderzet".into() });
    let r = &search(&h, doc).results;
    assert_eq!((r.len(), r[0].file_name.as_str()), (1, "onderzetters.burnburn"));
    assert_eq!(r[0].matched, [SearchField::FileName, SearchField::Object]);
    files(&mut h, FilesIntent::SetSearchQuery { document: doc, query: "niets".into() });
    assert!(search(&h, doc).results.is_empty());
}

#[test]
fn a_hit_shows_a_preview_when_it_was_saved_and_its_frozen_revisions() {
    let (h, doc) = searching();
    let s = search(&h, doc);
    let deurbord = &s.results[0];
    let thumbnail = deurbord.thumbnail.as_ref().expect("a preview");
    assert!(thumbnail.coverage.iter().any(|c| *c > 0), "the outline is drawn");
    assert_eq!((thumbnail.width, thumbnail.height), (64, 64));
    assert_eq!(deurbord.modified, Some(1_781_751_845));
    assert_eq!(deurbord.objects, 2);
    assert_eq!(
        deurbord.revisions.iter().map(|r| (r.number, r.label.as_str())).collect::<Vec<_>>(),
        [(1, "Order 4711")]
    );
    assert_eq!(s.results[1].modified, None, "unknown stays unknown");
}

#[test]
fn a_found_project_opens_in_a_window_of_its_own() {
    let (mut h, doc) = searching();
    let before = h.view().documents.len();
    h.send(UiIntent::Files(FilesIntent::OpenFoundProject {
        document: doc,
        path: PathBuf::from("/werk/onderzetters.burnburn"),
    }));
    let effects: Vec<HostEffect> = h.run_tasks().into_iter().flat_map(|u| u.effects).collect();
    let opened = effects
        .iter()
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(*document),
            _ => None,
        })
        .expect("a window opened");
    assert_ne!(opened, doc);
    assert_eq!(h.view().documents.len(), before + 1);
    assert_eq!(h.document(opened).unwrap().objects[0].label, "Onderzetter");
    assert!(!h.document(doc).unwrap().dirty, "the design that was open is untouched");
}

struct CountingJournal {
    calls: Mutex<Vec<&'static str>>,
}

impl HistoryStore for CountingJournal {
    fn record_upload(&self, _: &UploadRecord) -> Result<(burnburn_machine::ExecutionId, Recorded), HistoryStoreError> {
        self.calls.lock().unwrap().push("upload");
        Err(HistoryStoreError("unused".into()))
    }
    fn record_intent(&self, _: &IntentRecord) -> Result<Recorded, HistoryStoreError> {
        self.calls.lock().unwrap().push("intent");
        Err(HistoryStoreError("unused".into()))
    }
    fn record_outcome(&self, _: &OutcomeRecord) -> Result<Recorded, HistoryStoreError> {
        self.calls.lock().unwrap().push("outcome");
        Err(HistoryStoreError("unused".into()))
    }
    fn record_live_intent(&self, _: &burnburn_presentation::LiveIntentRecord) -> Result<Recorded, HistoryStoreError> {
        self.calls.lock().unwrap().push("live intent");
        Err(HistoryStoreError("unused".into()))
    }
    fn recover(
        &self,
        _: &burnburn_machine::MachineId,
    ) -> Result<burnburn_presentation::RecoveredMachine, HistoryStoreError> {
        Ok(Default::default())
    }
    fn record_order(&self, _: &burnburn_presentation::StoredOrder) -> Result<(), HistoryStoreError> {
        self.calls.lock().unwrap().push("order");
        Ok(())
    }
    fn orders(&self) -> Result<burnburn_presentation::StoredOrders, HistoryStoreError> {
        Ok(Default::default())
    }
}

#[test]
fn reopening_old_revision_creates_no_production_intent() {
    let mut h = HeadlessEditorHarness::new();
    let journal = Arc::new(CountingJournal { calls: Mutex::new(Vec::new()) });
    h.controller().attach_history(journal.clone()).expect("history");
    let (deurbord, _) = werk(&h);
    let read = |h: &HeadlessEditorHarness, p: &Path| burnburn_io::fs::FileSystem::read(h.fs(), p, u64::MAX).unwrap();
    let file_before = read(&h, &deurbord);
    let doc = open_window(&mut h, &deurbord);
    h.invoke(doc, CommandKey::SearchProjects);
    h.run_tasks();
    let windows = h.view().documents.len();

    // The old production revision opens next to the current design, as an unsaved copy.
    h.send(UiIntent::Files(FilesIntent::OpenFoundRevision { document: doc, path: deurbord.clone(), number: 1 }));
    let mut kinds = Vec::new();
    let mut opened = None;
    for request in h.take_task_requests() {
        kinds.push(request.input.kind());
        let update = h.complete(request);
        opened = opened.or_else(|| {
            update.effects.iter().find_map(|e| match e {
                HostEffect::OpenWindow { document } => Some(*document),
                _ => None,
            })
        });
    }
    let opened = opened.expect("the revision opened in a window");
    assert_eq!(h.view().documents.len(), windows + 1, "next to the current design");
    let revision = h.document(opened).unwrap();
    assert_eq!(
        revision.objects.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
        ["Deurbord"],
        "the design as it was frozen"
    );
    assert!(revision.title.contains("Order 4711"), "{:?}", revision.title);
    assert_eq!(h.document(doc).unwrap().objects.len(), 2, "the current design keeps its second object");

    // Nothing of production happened: no compile, prepare, export or upload task, and the journal
    // was never asked to record an intent, an upload, an outcome or an order.
    let production =
        [TaskKind::Compile, TaskKind::PrepareJob, TaskKind::ExportRd, TaskKind::ExportGcode, TaskKind::TestJob];
    assert!(kinds.iter().all(|k| !production.contains(k)), "{kinds:?}");
    assert!(journal.calls.lock().unwrap().is_empty(), "{:?}", journal.calls.lock().unwrap());
    assert!(revision.job.is_none(), "no job exists for the reopened revision");
    assert_eq!(read(&h, &deurbord), file_before, "the project file was not written");
    // The copy saves under a name of its own: it cannot overwrite the project it came from.
    let update = h.send(UiIntent::Save { document: opened, path: None });
    assert!(update.effects.iter().any(|e| matches!(e, HostEffect::ShowSavePanel { .. })), "{:?}", update.effects);
}

#[test]
fn an_unknown_revision_or_folder_is_said_not_ignored() {
    let (mut h, doc) = searching();
    let windows = h.view().documents.len();
    h.send(UiIntent::Files(FilesIntent::OpenFoundRevision {
        document: doc,
        path: PathBuf::from("/werk/deurbord.burnburn"),
        number: 9,
    }));
    let effects: Vec<HostEffect> = h.run_tasks().into_iter().flat_map(|u| u.effects).collect();
    assert!(effects.iter().any(|e| matches!(e, HostEffect::ShowError { .. })), "{effects:?}");
    assert_eq!(h.view().documents.len(), windows, "no window for a revision that is not there");
    files(&mut h, FilesIntent::SetSearchFolder { document: doc, folder: "/bestaat/niet".into() });
    let s = search(&h, doc);
    assert!(s.folder_unreadable && s.results.is_empty(), "{s:?}");
    files(&mut h, FilesIntent::SetSearchFolder { document: doc, folder: "/werk".into() });
    assert!(!search(&h, doc).folder_unreadable);
}

#[test]
fn closing_the_panel_and_a_newer_scan_replace_what_was_shown() {
    let (mut h, doc) = searching();
    files(&mut h, FilesIntent::CloseSearch { document: doc });
    assert!(h.document(doc).unwrap().files.search.is_none());
    h.invoke(doc, CommandKey::SearchProjects);
    // Two scans in a row: only the newest answer counts.
    h.send(UiIntent::Files(FilesIntent::SetSearchFolder { document: doc, folder: "/werk".into() }));
    h.send(UiIntent::Files(FilesIntent::RescanSearch { document: doc }));
    let requests = h.take_task_requests();
    assert!(requests.len() >= 2);
    for request in requests.into_iter().rev() {
        h.complete(request);
    }
    let s = search(&h, doc);
    assert!(!s.scanning);
    assert_eq!(s.scanned, Some(2));
}
