//! Presentation side of the GUI requests (docs/coordination/gui.md, fase C): definitive text
//! input, the operations panel with raw field text, the bed camera port and its view model, and
//! autosave/recovery. Hosts only send intents and draw view models; nothing here is native.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use burnburn_document::UndoLabel;
use burnburn_document::text::TextLabel;
use burnburn_presentation::{
    BedCameraIntent, CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode,
    CameraPixelFormat, CameraPort, CommandKey, HeadlessEditorHarness, HostEffect, MessageKey, OperationField,
    OperationKind, SelectMode, UiIntent,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::{DocumentId, MonotonicTime};

fn rectangle(h: &mut HeadlessEditorHarness) -> DocumentId {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    doc
}

// ---------------------------------------------------------------------------------------------
// #4 definitive text input (IME commit → one text command; composition never reaches us)
// ---------------------------------------------------------------------------------------------

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let font = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), font)
}

#[test]
fn committed_text_is_one_text_command_and_extends_the_selected_text() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    let refused = h.send(UiIntent::InsertText { document: doc, text: "Plaat".into() });
    assert!(
        refused.effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::NoFontAvailable, .. }))
    );
    assert!(h.document(doc).unwrap().objects.is_empty(), "no font: nothing created");

    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    let rev = h.document(doc).unwrap().revision;
    h.send(UiIntent::InsertText { document: doc, text: "Plaat 7".into() });
    let vm = h.document(doc).unwrap();
    assert_eq!((vm.revision.raw(), vm.undo), (rev.raw() + 1, Some(UndoLabel::Text(TextLabel::Create))));
    assert!(!vm.objects.is_empty(), "outlines follow in the same transaction");
    assert!(!vm.selection.is_empty(), "the new text is selected");
    let texts = |h: &HeadlessEditorHarness| h.text_contents(doc);
    assert_eq!(texts(&h), vec!["Plaat 7".to_owned()]);

    // A second commit with the text selected appends to it: one edit, still one text object.
    h.send(UiIntent::InsertText { document: doc, text: "A".into() });
    assert_eq!(texts(&h), vec!["Plaat 7A".to_owned()]);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Text(TextLabel::Edit)));
    let rev = h.document(doc).unwrap().revision;
    h.send(UiIntent::InsertText { document: doc, text: String::new() });
    assert_eq!(h.document(doc).unwrap().revision, rev, "empty commit sends nothing");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(texts(&h), vec!["Plaat 7".to_owned()]);
}

// ---------------------------------------------------------------------------------------------
// #6 operations panel: line/fill with speed/power as raw text, changes via ProcessCommand
// ---------------------------------------------------------------------------------------------

#[test]
fn operations_panel_takes_raw_text_and_changes_go_through_process_commands() {
    let mut h = HeadlessEditorHarness::new();
    let doc = rectangle(&mut h);
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });

    let add = |h: &mut HeadlessEditorHarness, kind, speed: &str, power: &str| {
        h.send(UiIntent::AddOperationText {
            document: doc,
            name: "Snijden".into(),
            kind,
            speed: speed.into(),
            power: power.into(),
        })
    };
    let rev = h.document(doc).unwrap().revision;
    add(&mut h, OperationKind::Line, "veel", "20");
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.revision, rev, "invalid speed: no command");
    assert_eq!(vm.operation_error, Some(MessageKey::InvalidNumber(OperationField::Speed)));

    add(&mut h, OperationKind::Line, "100 mm/s", "20 %");
    add(&mut h, OperationKind::Fill, "300", "35,5");
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.revision.raw(), rev.raw() + 2, "one command per added operation");
    assert_eq!(vm.operation_error, None);
    let rows: Vec<_> = vm.operations.iter().map(|o| (o.kind, o.speed.clone(), o.power.clone(), o.targets)).collect();
    assert_eq!(
        rows,
        vec![
            (OperationKind::Line, "100".to_owned(), "20".to_owned(), 1),
            (OperationKind::Fill, "300".to_owned(), "35.5".to_owned(), 1),
        ]
    );

    let line = vm.operations[0].id;
    h.send(UiIntent::SetOperationField {
        document: doc,
        operation: line,
        field: OperationField::Power,
        text: "60".into(),
    });
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.operations[0].power, "60");
    assert_eq!(vm.revision.raw(), rev.raw() + 3);
    for (field, text) in [(OperationField::Power, "60 %"), (OperationField::Speed, "100")] {
        h.send(UiIntent::SetOperationField { document: doc, operation: line, field, text: text.into() });
        assert_eq!(h.document(doc).unwrap().revision, vm.revision, "unchanged {field:?}: no command");
    }
    h.send(UiIntent::SetOperationField {
        document: doc,
        operation: line,
        field: OperationField::Power,
        text: "120".into(),
    });
    let after = h.document(doc).unwrap();
    assert_eq!(after.revision, vm.revision, "out of range: refused");
    assert_eq!(after.operation_error, Some(MessageKey::InvalidNumber(OperationField::Power)));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.document(doc).unwrap().operations[0].power, "20", "one undo per field change");
}

// ---------------------------------------------------------------------------------------------
// IM27 camera port (moved from burnburn-macos): permission, start/stop, latest frame
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct FakeCamera {
    authorization: Mutex<Option<CameraAuthorization>>,
    requests: Mutex<u32>,
    started: Mutex<Vec<(CameraDeviceId, CameraMode)>>,
    stopped: Mutex<u32>,
    frame: Mutex<Option<Arc<CameraFrame>>>,
}

fn mode() -> CameraMode {
    CameraMode { width: 64, height: 48, fps_milli: 30_000, pixel_format: CameraPixelFormat::Bgra8 }
}

fn device() -> CameraDeviceId {
    CameraDeviceId("cam-1".into())
}

impl CameraPort for FakeCamera {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
        Ok(vec![CameraDevice { id: device(), name: "Bedcamera".into(), modes: vec![mode()] }])
    }
    fn authorization(&self) -> CameraAuthorization {
        self.authorization.lock().unwrap().unwrap_or(CameraAuthorization::NotDetermined)
    }
    fn request_authorization(&self) {
        *self.requests.lock().unwrap() += 1;
    }
    fn start(&self, d: &CameraDeviceId, m: &CameraMode) -> Result<(), CameraError> {
        self.started.lock().unwrap().push((d.clone(), *m));
        Ok(())
    }
    fn stop(&self, _d: &CameraDeviceId) {
        *self.stopped.lock().unwrap() += 1;
    }
    fn latest_frame(&self, _d: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        self.frame.lock().unwrap().clone()
    }
}

#[test]
fn camera_starts_only_after_permission_and_shows_the_latest_frame_as_visual_placement() {
    let mut h = HeadlessEditorHarness::new();
    assert!(h.controller().camera_view().is_none(), "no camera attached");
    let cam = Arc::new(FakeCamera::default());
    h.controller().attach_camera(cam.clone());
    let vm = h.controller().camera_view().expect("camera panel");
    assert_eq!(vm.authorization, CameraAuthorization::NotDetermined);
    assert_eq!(vm.devices.len(), 1);
    assert_eq!(*cam.requests.lock().unwrap(), 0, "never asks on its own");

    let start = UiIntent::BedCamera(BedCameraIntent::Start { device: device(), mode: mode() });
    let refused = h.send(start.clone());
    assert!(
        refused
            .effects
            .iter()
            .any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::CameraNotAuthorized, .. }))
    );
    assert!(cam.started.lock().unwrap().is_empty());
    h.send(UiIntent::BedCamera(BedCameraIntent::RequestAccess));
    assert_eq!(*cam.requests.lock().unwrap(), 1, "asked once, on the user's action");

    *cam.authorization.lock().unwrap() = Some(CameraAuthorization::Authorized);
    h.send(start);
    assert_eq!(*cam.started.lock().unwrap(), vec![(device(), mode())]);
    assert_eq!(h.controller().camera_view().unwrap().frame, None, "no frame yet");
    let image = burnburn_image::SourceImage::from_rgba8(64, 48, vec![0; 64 * 48 * 4]).unwrap();
    *cam.frame.lock().unwrap() =
        Some(Arc::new(CameraFrame { device: device(), mode: mode(), seq: 7, image, time: MonotonicTime::ZERO }));
    let vm = h.controller().camera_view().unwrap();
    let frame = vm.frame.expect("latest frame");
    assert_eq!((frame.seq, frame.width, frame.height), (7, 64, 48));
    assert_eq!(vm.active, Some((device(), mode())));
    assert_eq!(vm.note, MessageKey::VisualPlacementOnly, "camera pixels never feed CAM");
    h.send(UiIntent::BedCamera(BedCameraIntent::Stop));
    assert_eq!(*cam.stopped.lock().unwrap(), 1);
    assert_eq!(h.controller().camera_view().unwrap().active, None);
}

// ---------------------------------------------------------------------------------------------
// #1 autosave / recovery (recovery file via io, never over the project file)
// ---------------------------------------------------------------------------------------------

fn recovery_dir() -> PathBuf {
    PathBuf::from("/app/recovery")
}

fn with_recovery() -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(Path::new("/work"));
    h.fs().create_dir_all_for_test(&recovery_dir());
    h.controller().set_recovery_dir(recovery_dir());
    h
}

fn recovery_files(h: &HeadlessEditorHarness) -> Vec<PathBuf> {
    h.fs().files().into_iter().filter(|p| p.starts_with(recovery_dir())).collect()
}

#[test]
fn autosave_writes_a_recovery_file_never_the_project_and_keeps_the_document_dirty() {
    let mut h = with_recovery();
    let doc = rectangle(&mut h);
    h.send(UiIntent::Save { document: doc, path: Some("/work/plaat.burnburn".into()) });
    h.run_tasks();
    let saved = burnburn_io::fs::FileSystem::read(h.fs(), Path::new("/work/plaat.burnburn"), 1 << 24).unwrap();
    h.send(UiIntent::Autosave);
    assert!(h.take_task_requests().is_empty(), "a clean document needs no recovery file");

    h.invoke(doc, CommandKey::Undo);
    h.send(UiIntent::Autosave);
    h.run_tasks();
    assert_eq!(recovery_files(&h).len(), 1, "one recovery file for the dirty document");
    assert!(h.document(doc).unwrap().dirty, "autosave is not a save");
    let project = burnburn_io::fs::FileSystem::read(h.fs(), Path::new("/work/plaat.burnburn"), 1 << 24).unwrap();
    assert_eq!(project, saved, "the project file is never touched");
    h.send(UiIntent::Autosave);
    assert!(h.take_task_requests().is_empty(), "unchanged since the last autosave");

    // An explicit save makes the recovery file obsolete.
    h.send(UiIntent::Save { document: doc, path: None });
    h.run_tasks();
    assert!(recovery_files(&h).is_empty(), "removed after a successful save");
}

#[test]
fn recovery_is_offered_at_start_and_restores_a_dirty_untitled_window() {
    let mut h = with_recovery();
    let doc = rectangle(&mut h);
    h.send(UiIntent::Autosave);
    h.run_tasks();
    let files = recovery_files(&h);
    assert_eq!(files.len(), 1);

    // Next start: a new controller over the same disk.
    let fs = h.shared_fs();
    let mut next = HeadlessEditorHarness::with_fs(fs);
    next.controller().set_recovery_dir(recovery_dir());
    next.send(UiIntent::CheckRecovery);
    let updates = next.run_tasks();
    let offered: Vec<_> = updates
        .iter()
        .flat_map(|u| &u.effects)
        .filter_map(|e| match e {
            HostEffect::OfferRecovery { files } => Some(files.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(offered, vec![files.clone()]);
    next.send(UiIntent::RestoreRecovery { path: files[0].clone() });
    let updates = next.run_tasks();
    let opened = updates.iter().flat_map(|u| &u.effects).find_map(|e| match e {
        HostEffect::OpenWindow { document } => Some(*document),
        _ => None,
    });
    let restored = next.document(opened.expect("window for the recovered document")).unwrap();
    assert!(restored.dirty, "a recovered document is unsaved");
    assert_eq!(restored.objects.len(), h.document(doc).unwrap().objects.len());
    assert!(next.view().documents.iter().all(|d| d.title != "plaat.burnburn"));

    // Discarding the offer removes the file (explicit choice).
    next.send(UiIntent::DiscardRecovery { path: files[0].clone() });
    next.run_tasks();
    assert!(recovery_files(&next).is_empty());
}
