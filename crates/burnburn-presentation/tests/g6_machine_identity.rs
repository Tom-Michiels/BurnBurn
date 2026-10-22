//! G6-machine, M01: model and firmware belong to the machine profile. The user types them in
//! Device Settings, they are shown again (dialog and hub), they are kept with the profile in the
//! library, and they are what the identity check before an upload compares the controller's
//! reply with. A reply from a controller never becomes the declaration.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_io::memory::MemoryFileSystem;
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CamDialog, DialogAction, DialogField, DialogValue, DialogViewModel, HeadlessEditorHarness, MachineStatusKey,
    MessageKey, UiIntent,
};
use burnburn_types::DocumentId;

use common::{RUIDA, profile};

const DEV: CamDialog = CamDialog::Device;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: DEV });
    h.controller().cam_dialog(doc, DEV).expect("dialog open")
}

fn text(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: &str) {
    h.send(UiIntent::SetCamField { document: doc, dialog: DEV, field, value: DialogValue::Text(value.into()) });
}

fn apply(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: DEV, action: DialogAction::Apply });
}

fn declare(h: &mut HeadlessEditorHarness, doc: DocumentId, model: &str, firmware: &str) {
    open(h, doc);
    text(h, doc, DialogField::Model, model);
    text(h, doc, DialogField::Firmware, firmware);
    apply(h, doc);
}

#[test]
fn a_profile_without_a_declaration_opens_with_empty_model_and_firmware_fields() {
    let (mut h, doc) = window();
    let v = open(&mut h, doc);
    assert!(v.rows.iter().any(|r| r.field == DialogField::Model), "the dialog has a Model row");
    assert_eq!(v.text(DialogField::Model), "");
    assert_eq!(v.text(DialogField::Firmware), "");
    assert_eq!(h.controller().declared_identity(), (None, None));
}

#[test]
fn the_declaration_is_stored_with_the_machine_and_shown_again_in_the_dialog() {
    let (mut h, doc) = window();
    declare(&mut h, doc, "  RDC6445G ", "v8.02");
    assert_eq!(h.controller().declared_identity(), (Some("RDC6445G".into()), Some("v8.02".into())), "trimmed");
    let v = open(&mut h, doc);
    assert_eq!(v.text(DialogField::Model), "RDC6445G");
    assert_eq!(v.text(DialogField::Firmware), "v8.02");
}

#[test]
fn declaring_only_the_identity_is_no_new_profile_revision_and_leaves_plans_alone() {
    let (mut h, doc) = window();
    let before = h.controller().machine_profile().unwrap().revision().clone();
    declare(&mut h, doc, "RDC6445G", "");
    assert_eq!(h.controller().machine_profile().unwrap().revision(), &before, "support and compile do not depend on it");
}

#[test]
fn blank_text_clears_the_declaration() {
    let (mut h, doc) = window();
    declare(&mut h, doc, "RDC6445G", "v8.02");
    declare(&mut h, doc, "", "   ");
    assert_eq!(h.controller().declared_identity(), (None, None));
}

fn other_machine() -> MachineProfile {
    MachineProfile::new(
        "tweede-machine",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        500.0,
        300.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn with_library() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::with_fs(Arc::new(MemoryFileSystem::new()));
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    let doc = h.new_document().expect("window");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc)
}

#[test]
fn the_declaration_is_kept_with_a_replaced_machine_and_comes_back_when_it_is_chosen() {
    let (mut h, doc) = with_library();
    declare(&mut h, doc, "RDC6445G", "v8.02");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(other_machine()), ruida: RUIDA });
    h.run_tasks();
    assert_eq!(h.controller().declared_identity(), (None, None), "another machine has its own declaration");
    h.send(UiIntent::ChooseMachineProfile { id: "research-1500x900".into() });
    h.run_tasks();
    assert_eq!(h.controller().declared_identity(), (Some("RDC6445G".into()), Some("v8.02".into())));
}

#[test]
fn the_hub_lists_the_declared_identity_of_every_known_machine() {
    let (mut h, doc) = with_library();
    declare(&mut h, doc, "RDC6445G", "v8.02");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(other_machine()), ruida: RUIDA });
    h.run_tasks();
    let devices = h.controller().machine_hub_view().devices;
    let row = |id: &str| devices.iter().find(|d| d.id == id).unwrap().clone();
    assert_eq!((row("research-1500x900").model, row("research-1500x900").firmware), (Some("RDC6445G".into()), Some("v8.02".into())));
    assert_eq!((row("tweede-machine").model, row("tweede-machine").firmware), (None, None), "not declared is shown as such");
}

// ---- against the machine service ---------------------------------------------------------------------------

#[test]
fn the_declared_model_is_what_the_identity_check_before_an_upload_compares() {
    // The service was configured with the model the fake controller reports. The user declares
    // another model: the same controller now fails the check, and no job byte is sent.
    let mut r = common::rig();
    declare(&mut r.h, r.doc, "RDC6442S", "");
    let job = common::prepare(&mut r.h, r.doc);
    common::script_identity(&r.peer, common::MODEL);
    r.h.press(Some(r.doc), job.upload.as_ref().expect("upload control"));
    common::pump(&r);
    let vm = r.h.machine_view().unwrap();
    assert_eq!((vm.status, vm.message), (MachineStatusKey::Failed, Some(MessageKey::IdentityMismatch)));
    assert!(common::uploaded_bytes(&r.peer).is_empty());
}

#[test]
fn a_matching_declaration_uploads_and_a_reply_never_fills_in_the_declaration() {
    let mut r = common::rig();
    declare(&mut r.h, r.doc, common::MODEL, "");
    let job = common::prepare(&mut r.h, r.doc);
    common::script_upload_acks(&r.peer, &job);
    r.h.press(Some(r.doc), job.upload.as_ref().expect("upload control"));
    common::pump(&r);
    assert!(!common::uploaded_bytes(&r.peer).is_empty(), "the declared model matched, the job went out");
    // The controller reported a model in its reply; the firmware it did not report stays undeclared
    // and nothing it said was copied into the profile.
    assert_eq!(r.h.controller().declared_identity(), (Some(common::MODEL.into()), None));
}

#[test]
fn without_a_declaration_the_host_configured_identity_stays_in_force() {
    let mut r = common::rig();
    let job = common::prepare(&mut r.h, r.doc);
    common::script_upload_acks(&r.peer, &job);
    r.h.press(Some(r.doc), job.upload.as_ref().expect("upload control"));
    common::pump(&r);
    assert!(!common::uploaded_bytes(&r.peer).is_empty());
    assert_eq!(r.h.controller().declared_identity(), (None, None), "an ACK or a reply declares nothing");
}
