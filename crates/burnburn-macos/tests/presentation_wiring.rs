//! The presentation functions of the integration round, as the host shows and forwards them,
//! headless against the real `AppController`: the rotation field, the operations panel (raw text,
//! DR-05), canvas text commits (`InsertText`), the recovery offer and the bed-camera panel (only
//! an explicit click asks for access; nothing starts by itself).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use burnburn_image::SourceImage;
use burnburn_macos::adapter::{NativeEvent, Reaction, response, translate};
use burnburn_macos::l10n::{HostText, Locale, host_text, message_or_key};
use burnburn_macos::lifecycle::{RECOVERY_ALERT_BUTTONS, RecoveryChoice, recovery_dir, recovery_message};
use burnburn_macos::panels::{
    CameraControl, INSPECTOR_FIELDS, OPERATION_KINDS, camera_intent, camera_panel, field_unit, operations_panel,
};
use burnburn_presentation::{
    BedCameraIntent, CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode,
    CameraPixelFormat, CameraPort, CommandKey, FieldKey, HeadlessEditorHarness, MessageKey, OperationField,
    OperationKind, UiIntent,
};
use burnburn_types::{DocumentId, MonotonicTime, StableId};

fn one_rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    assert_eq!(h.document(doc).unwrap().objects.len(), 1);
    (h, doc)
}

fn intent(r: Reaction) -> UiIntent {
    match r {
        Reaction::Intent(i) => i,
        other => panic!("expected an intent, got {other:?}"),
    }
}

fn field_text(h: &HeadlessEditorHarness, doc: DocumentId, key: FieldKey) -> String {
    h.document(doc).unwrap().inspector.unwrap().field(key).unwrap().text.clone()
}

// ---- Rotation ---------------------------------------------------------------------------------

#[test]
fn inspector_offers_rotation_in_degrees_and_a_commit_turns_the_selection() {
    assert_eq!(INSPECTOR_FIELDS[..], [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height, FieldKey::Rotation]);
    assert_eq!(field_unit(FieldKey::Rotation, Locale::Nl), "°");
    assert_eq!(field_unit(FieldKey::Width, Locale::Nl), "mm");
    assert_eq!(message_or_key(MessageKey::Field(FieldKey::Rotation), Locale::Nl), "Rotatie");

    let (mut h, doc) = one_rectangle();
    assert_eq!(field_text(&h, doc, FieldKey::Rotation), "0");
    // The field always shows 0: committing "0" again (Return, then end of editing) is nothing.
    let commit = |text: &str, h: &HeadlessEditorHarness| {
        translate(
            NativeEvent::FieldCommitted {
                document: doc,
                field: FieldKey::Rotation,
                text: text.into(),
                cause: burnburn_macos::adapter::CommitCause::Return,
            },
            &h.view(),
        )
    };
    assert_eq!(commit("0", &h), Reaction::Nothing);
    let sent = intent(commit("90", &h));
    assert_eq!(sent, UiIntent::CommitField { document: doc, field: FieldKey::Rotation, text: "90".into() });
    h.send(sent);
    assert_eq!(
        (field_text(&h, doc, FieldKey::Width), field_text(&h, doc, FieldKey::Height)),
        ("80".into(), "120".into())
    );
    assert_eq!(field_text(&h, doc, FieldKey::Rotation), "0", "rotation shows 0 again after the turn");
}

// ---- Operations panel ---------------------------------------------------------------------------

fn add(doc: DocumentId, name: &str, kind_index: isize, speed: &str, power: &str) -> NativeEvent {
    NativeEvent::AddOperationRequested {
        document: doc,
        name: name.into(),
        kind_index,
        speed: speed.into(),
        power: power.into(),
    }
}

#[test]
fn operations_panel_adds_with_raw_text_and_lists_the_operation() {
    assert_eq!(OPERATION_KINDS, [OperationKind::Line, OperationKind::Fill]);
    let (mut h, doc) = one_rectangle();
    let empty = operations_panel(&h.view(), doc, Locale::Nl);
    assert!(empty.rows.is_empty());
    assert_eq!(empty.empty_text, "Nog geen bewerkingen: selecteer objecten en voeg er een toe.");

    let sent = intent(translate(add(doc, "Snijden", 0, "100 mm/s", "35,5 %"), &h.view()));
    assert_eq!(
        sent,
        UiIntent::AddOperationText {
            document: doc,
            name: "Snijden".into(),
            kind: OperationKind::Line,
            speed: "100 mm/s".into(),
            power: "35,5 %".into(),
        },
        "the host forwards the typed text unparsed (DR-05)"
    );
    h.send(sent);
    let panel = operations_panel(&h.view(), doc, Locale::Nl);
    assert_eq!(panel.rows.len(), 1);
    let row = &panel.rows[0];
    assert_eq!(
        (row.name.as_str(), row.kind.as_str(), row.speed.as_str(), row.power.as_str(), row.targets.as_str()),
        ("Snijden", "Lijn", "100", "35.5", "1 object")
    );
    assert_eq!(panel.error, "");

    // Fill is the second kind; an unknown pop-up index sends nothing.
    let fill = intent(translate(add(doc, "Graveren", 1, "300", "20"), &h.view()));
    assert!(matches!(fill, UiIntent::AddOperationText { kind: OperationKind::Fill, .. }));
    for index in [-1, 2] {
        assert_eq!(translate(add(doc, "x", index, "1", "1"), &h.view()), Reaction::Nothing);
    }
}

#[test]
fn operation_field_commit_sends_only_changed_text_and_shows_presentations_error() {
    let (mut h, doc) = one_rectangle();
    h.send(intent(translate(add(doc, "Snijden", 0, "100", "20"), &h.view())));
    let edit = |field: OperationField, text: &str, h: &HeadlessEditorHarness| {
        translate(NativeEvent::OperationFieldCommitted { document: doc, row: 0, field, text: text.into() }, &h.view())
    };
    assert_eq!(edit(OperationField::Speed, "100", &h), Reaction::Nothing, "unchanged text is no commit");
    assert_eq!(
        translate(
            NativeEvent::OperationFieldCommitted {
                document: doc,
                row: 5,
                field: OperationField::Speed,
                text: "1".into()
            },
            &h.view()
        ),
        Reaction::Nothing,
        "a row that does not exist"
    );
    let operation = h.document(doc).unwrap().operations[0].id;
    let sent = intent(edit(OperationField::Speed, "250 mm/s", &h));
    assert_eq!(
        sent,
        UiIntent::SetOperationField { document: doc, operation, field: OperationField::Speed, text: "250 mm/s".into() }
    );
    h.send(sent);
    assert_eq!(operations_panel(&h.view(), doc, Locale::Nl).rows[0].speed, "250");

    h.send(intent(edit(OperationField::Power, "veel", &h)));
    let panel = operations_panel(&h.view(), doc, Locale::Nl);
    assert_eq!(panel.error, message_or_key(MessageKey::InvalidNumber(OperationField::Power), Locale::Nl));
    assert!(!panel.error.is_empty() && !panel.error.contains("InvalidNumber"), "{}", panel.error);
}

// ---- Canvas text --------------------------------------------------------------------------------

#[test]
fn committed_canvas_text_is_one_insert_text_intent() {
    let (h, doc) = one_rectangle();
    assert_eq!(
        intent(translate(NativeEvent::TextCommitted { document: doc, text: "日本".into() }, &h.view())),
        UiIntent::InsertText { document: doc, text: "日本".into() }
    );
    assert_eq!(
        translate(NativeEvent::TextCommitted { document: doc, text: String::new() }, &h.view()),
        Reaction::Nothing
    );
    let closed = DocumentId::from_raw(9_999);
    assert_eq!(
        translate(NativeEvent::TextCommitted { document: closed, text: "a".into() }, &h.view()),
        Reaction::Nothing
    );
}

// ---- Recovery -----------------------------------------------------------------------------------

#[test]
fn recovery_offer_restores_keeps_or_discards_per_file() {
    assert_eq!(RECOVERY_ALERT_BUTTONS, [RecoveryChoice::Restore, RecoveryChoice::Later, RecoveryChoice::Discard]);
    let (h, _doc) = one_rectangle();
    let path = PathBuf::from("/tmp/r/00ff.burnburn");
    let answer = |response: isize| translate(NativeEvent::RecoveryAnswered { path: path.clone(), response }, &h.view());
    assert_eq!(intent(answer(response::FIRST_BUTTON)), UiIntent::RestoreRecovery { path: path.clone() });
    assert_eq!(answer(response::SECOND_BUTTON), Reaction::Nothing, "Later keeps the file for the next start");
    assert_eq!(intent(answer(response::THIRD_BUTTON)), UiIntent::DiscardRecovery { path: path.clone() });
    assert_eq!(answer(42), Reaction::Nothing, "an unexpected answer never deletes");

    let (title, info) = recovery_message(&path, 1, 2, Locale::Nl);
    assert_eq!(title, "Herstel een niet-bewaarde versie? (1 van 2)");
    assert!(info.contains("00ff.burnburn"), "{info}");
    for choice in RECOVERY_ALERT_BUTTONS {
        assert!(!host_text(choice.text(), Locale::Nl).is_empty());
    }
}

#[test]
fn recovery_files_live_in_application_support_never_next_to_projects() {
    assert_eq!(
        recovery_dir(Path::new("/Users/x/Library/Application Support")),
        PathBuf::from("/Users/x/Library/Application Support/BurnBurn/Recovery")
    );
}

// ---- Bed camera ---------------------------------------------------------------------------------

#[derive(Default)]
struct FakeCamera {
    auth: Mutex<Option<CameraAuthorization>>,
    log: Mutex<Vec<String>>,
}

fn mode(width: u32, pixel_format: CameraPixelFormat) -> CameraMode {
    CameraMode { width, height: width * 9 / 16, fps_milli: 30_000, pixel_format }
}

impl CameraPort for FakeCamera {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
        Ok(vec![CameraDevice {
            id: CameraDeviceId("cam-1".into()),
            name: "Bedcamera".into(),
            modes: vec![mode(1920, CameraPixelFormat::Yuv420Video), mode(1280, CameraPixelFormat::Bgra8)],
        }])
    }
    fn authorization(&self) -> CameraAuthorization {
        self.auth.lock().unwrap().unwrap_or(CameraAuthorization::NotDetermined)
    }
    fn request_authorization(&self) {
        self.log.lock().unwrap().push("request".into());
    }
    fn start(&self, device: &CameraDeviceId, mode: &CameraMode) -> Result<(), CameraError> {
        self.log.lock().unwrap().push(format!("start {} {}", device.0, mode.width));
        Ok(())
    }
    fn stop(&self, device: &CameraDeviceId) {
        self.log.lock().unwrap().push(format!("stop {}", device.0));
    }
    fn latest_frame(&self, device: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        Some(Arc::new(CameraFrame {
            device: device.clone(),
            mode: mode(1280, CameraPixelFormat::Bgra8),
            seq: 7,
            image: SourceImage::from_rgba8(2, 1, vec![0; 8]).unwrap(),
            time: MonotonicTime::from_offset(std::time::Duration::from_secs(1)),
        }))
    }
}

#[test]
fn camera_panel_without_port_offers_nothing() {
    let panel = camera_panel(None, Locale::Nl);
    assert_eq!(panel.status, "Geen camera beschikbaar in deze versie");
    assert!(!panel.access_enabled && !panel.start_enabled && !panel.stop_enabled);
    assert_eq!(camera_intent(None, CameraControl::Start, 0), None);
}

#[test]
fn camera_asks_access_only_on_click_and_starts_the_chosen_device_in_bgra() {
    let mut h = HeadlessEditorHarness::new();
    let camera = Arc::new(FakeCamera::default());
    h.controller().attach_camera(camera.clone());
    let vm = h.controller().camera_view();
    let panel = camera_panel(vm.as_ref(), Locale::Nl);
    assert_eq!(panel.status, "Nog geen toegang gevraagd");
    assert_eq!(panel.devices, ["Bedcamera"]);
    assert!(panel.access_enabled && !panel.start_enabled && !panel.stop_enabled);
    assert_eq!(panel.note, message_or_key(MessageKey::VisualPlacementOnly, Locale::Nl));
    assert!(camera.log.lock().unwrap().is_empty(), "showing the panel asks and starts nothing");

    assert_eq!(camera_intent(vm.as_ref(), CameraControl::Start, 0), None, "no start without access");
    let ask = camera_intent(vm.as_ref(), CameraControl::RequestAccess, 0).unwrap();
    assert_eq!(ask, UiIntent::BedCamera(BedCameraIntent::RequestAccess));
    h.send(ask);
    assert_eq!(*camera.log.lock().unwrap(), ["request"]);

    *camera.auth.lock().unwrap() = Some(CameraAuthorization::Authorized);
    let vm = h.controller().camera_view();
    let panel = camera_panel(vm.as_ref(), Locale::Nl);
    assert!(!panel.access_enabled && panel.start_enabled && !panel.stop_enabled, "{panel:?}");
    assert_eq!(camera_intent(vm.as_ref(), CameraControl::Start, 3), None, "unknown device row");
    let start = camera_intent(vm.as_ref(), CameraControl::Start, 0).unwrap();
    assert_eq!(
        start,
        UiIntent::BedCamera(BedCameraIntent::Start {
            device: CameraDeviceId("cam-1".into()),
            mode: mode(1280, CameraPixelFormat::Bgra8)
        }),
        "the adapter decodes BGRA only"
    );
    h.send(start);
    let vm = h.controller().camera_view();
    let panel = camera_panel(vm.as_ref(), Locale::Nl);
    assert!(panel.stop_enabled && !panel.start_enabled);
    assert_eq!(panel.frame, "Beeld 7: 2 × 1 px");
    assert_eq!(panel.selected, Some(0));
    h.send(camera_intent(vm.as_ref(), CameraControl::Stop, 0).unwrap());
    assert_eq!(camera.log.lock().unwrap().last().map(String::as_str), Some("stop cam-1"));
    assert_eq!(host_text(HostText::CameraTitle, Locale::En), "Bed camera");
}
