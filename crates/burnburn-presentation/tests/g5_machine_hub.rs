//! G5-machine-rest: the machine hub read model through the public controller route. Devices and
//! support (M01, M02), upload status (M04), settings left alone (M09), the queue and the history
//! with a repeat route (W01-W03) and calibration proofs (W09). Fake controller and clock, no real
//! laser.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::time::Duration;

use burnburn_job::{Feature, MachineCapabilities, MachineProfile, Support};
use burnburn_machine::{ControllerIdentity, ControllerStatus, Reply, Request};
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, MachineAction, OperationField, ProofField, ProofInput, ProofKind, ProofProblem,
    QueueStage, RecipeField, RepeatRefusal, SupportView, UiIntent, UploadFailure, UploadStage,
};
use burnburn_test_support::Step;

use common::*;

fn press(r: &mut Rig, kind: MachineAction) {
    let button = r.h.machine_view().expect("machine panel").button(kind).cloned();
    let button = button.unwrap_or_else(|| panic!("no {kind:?} control"));
    assert!(button.enabled, "{kind:?} control is disabled");
    r.h.press(None, &button);
    pump(r);
}

fn hub(r: &mut Rig) -> burnburn_presentation::MachineHubViewModel {
    r.h.controller().machine_hub_view()
}

/// Upload, start and run to the end on a cooperating controller; returns the job.
fn produce(r: &mut Rig) -> burnburn_presentation::JobCardViewModel {
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(r, &job);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(r, MachineAction::Start);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    press(r, MachineAction::QueryStatus);
    r.clock.advance(Duration::from_secs(30));
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Finished)));
    press(r, MachineAction::QueryStatus);
    job
}

fn other_machine(id: &str) -> MachineProfile {
    profile(1).with_id_for_test(id)
}

trait WithId {
    fn with_id_for_test(self, id: &str) -> MachineProfile;
}

impl WithId for MachineProfile {
    fn with_id_for_test(self, id: &str) -> MachineProfile {
        use burnburn_job::{MachineFrame, OriginCorner, PowerRatio};
        MachineProfile::new(
            id,
            1,
            MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
            500.0,
            300.0,
            PowerRatio::new(0.0).unwrap(),
            PowerRatio::new(1.0).unwrap(),
        )
        .unwrap()
    }
}

// ---- M01 / M02: devices and support ----------------------------------------------------------

#[test]
fn the_hub_lists_the_known_machines_with_the_active_one_marked() {
    let mut h = HeadlessEditorHarness::new();
    assert!(h.controller().machine_hub_view().devices.is_empty(), "no machine yet");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.send(UiIntent::SetMachineProfile { profile: Box::new(other_machine("tweede")), ruida: RUIDA });
    let devices = h.controller().machine_hub_view().devices;
    let summary: Vec<(&str, u32, bool)> = devices.iter().map(|d| (d.id.as_str(), d.revision, d.active)).collect();
    assert_eq!(summary, [("tweede", 1, true), ("research-1500x900", 1, false)]);
    assert_eq!((devices[0].width_mm, devices[0].height_mm), (600.0, 400.0));
}

#[test]
fn the_hub_support_list_is_the_matrix_the_compiler_reads() {
    let mut h = HeadlessEditorHarness::new();
    assert!(h.controller().machine_hub_view().support.is_empty(), "no machine, no matrix");
    let capable = profile(1)
        .with_capabilities(MachineCapabilities::conservative().with(Feature::Tabs, Support::Supported))
        .unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(capable.clone()), ruida: RUIDA });
    let support = h.controller().machine_hub_view().support;
    assert_eq!(support.len(), Feature::ALL.len(), "every feature has a row");
    for row in &support {
        let expected = match capable.capabilities().support(row.feature) {
            Support::Supported => SupportView::Supported,
            Support::Unsupported => SupportView::Unsupported,
            Support::Unverified { reason } => SupportView::Unverified(reason),
        };
        assert_eq!(row.support, expected, "{:?}", row.feature);
    }
    assert_eq!(support.iter().find(|r| r.feature == Feature::Tabs).unwrap().support, SupportView::Supported);
    assert_ne!(support.iter().find(|r| r.feature == Feature::ScanAngle).unwrap().support, SupportView::Supported);
}

// ---- M04: upload status ------------------------------------------------------------------------

#[test]
fn without_an_execution_there_is_no_upload_status() {
    let mut r = rig();
    assert_eq!(hub(&mut r).upload, None);
}

#[test]
fn the_upload_status_keeps_progress_the_transport_ack_and_confirmed_storage_apart() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    script_upload_acks(&r.peer, &job);
    r.h.press(Some(r.doc), job.upload.as_ref().unwrap());
    pump(&r);
    let status = hub(&mut r).upload.expect("an upload is on record");
    assert_eq!(status.stage, UploadStage::AckedNotConfirmed { chunks: chunk_count(&job) }, "an ACK is not storage");
    assert!(status.stop.as_ref().is_some_and(|b| b.enabled), "an upload can be stopped until it is stored");

    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    press(&mut r, MachineAction::ConfirmStored);
    let status = hub(&mut r).upload.unwrap();
    assert_eq!(status.stage, UploadStage::StoredConfirmed);
    assert_eq!(status.stop, None, "nothing left to stop once the job is stored");
}

#[test]
fn a_running_upload_shows_how_many_chunks_were_acknowledged_and_can_be_stopped() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    script_identity(&r.peer, MODEL);
    r.peer.script(
        Step::expect_matching("chunk 0", |q| matches!(q, Request::UploadChunk { index: 0, .. })).reply(Reply::Ack),
    );
    r.peer.script(Step::expect_matching("chunk 1", |q| matches!(q, Request::UploadChunk { index: 1, .. })));
    r.h.press(Some(r.doc), job.upload.as_ref().unwrap());
    pump(&r);
    let status = hub(&mut r).upload.unwrap();
    assert_eq!(status.stage, UploadStage::Sending { acked: 1, total: chunk_count(&job) });
    let stop = status.stop.expect("stop upload");
    assert!(stop.enabled);

    r.h.press(None, &stop);
    pump(&r);
    assert_eq!(hub(&mut r).upload.unwrap().stage, UploadStage::Stopped);
}

#[test]
fn a_refused_chunk_and_a_wrong_controller_are_named_without_inventing_a_reason() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    script_identity(&r.peer, MODEL);
    r.peer.script(
        Step::expect_matching("chunk 0", |q| matches!(q, Request::UploadChunk { index: 0, .. })).reply(Reply::Ack),
    );
    r.peer.script(
        Step::expect_matching("chunk 1", |q| matches!(q, Request::UploadChunk { index: 1, .. })).reply(Reply::Nak),
    );
    r.h.press(Some(r.doc), job.upload.as_ref().unwrap());
    pump(&r);
    assert_eq!(hub(&mut r).upload.unwrap().stage, UploadStage::Failed(UploadFailure::Rejected { chunk: 1 }));

    let mut wrong = rig();
    let job = prepare(&mut wrong.h, wrong.doc);
    let identity = ControllerIdentity { model: Some("ander-model".into()), firmware: None };
    wrong.peer.script(Step::expect(Request::QueryIdentity).reply(Reply::Identity(identity)));
    wrong.h.press(Some(wrong.doc), job.upload.as_ref().unwrap());
    pump(&wrong);
    assert_eq!(hub(&mut wrong).upload.unwrap().stage, UploadStage::Failed(UploadFailure::IdentityMismatch));
}

// ---- M09: settings stay alone --------------------------------------------------------------------

/// Normal job settings never reach controller parameters: a whole production and a settings edit
/// send nothing but identity, upload, read-back, start and status requests.
#[test]
fn job_settings_do_not_touch_vendor_parameters() {
    let mut r = rig();
    let before = r.peer.sent().len();
    let operation = r.h.document(r.doc).unwrap().operations[0].id;
    r.h.send(UiIntent::SetOperationField {
        document: r.doc,
        operation,
        field: OperationField::Speed,
        text: "250".into(),
    });
    assert_eq!(r.peer.sent().len(), before, "editing a recipe talks to no controller");

    produce(&mut r);
    for request in r.peer.sent() {
        assert!(
            matches!(
                request,
                Request::QueryIdentity
                    | Request::UploadChunk { .. }
                    | Request::QueryStored
                    | Request::Start
                    | Request::QueryStatus
            ),
            "unexpected controller request {request:?}"
        );
    }
}

// ---- W01 / W02: queue and counts ---------------------------------------------------------------------

#[test]
fn the_queue_tells_uploaded_started_and_completed_apart_and_only_completed_counts() {
    let mut r = rig();
    assert_eq!(hub(&mut r).queue.rows, vec![]);
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    let queue = hub(&mut r).queue;
    assert_eq!(queue.rows.len(), 1);
    assert_eq!(queue.rows[0].stage, QueueStage::Stored);
    assert_eq!(queue.rows[0].plan, job.plan);
    assert_eq!((queue.counts.stored, queue.counts.started, queue.counts.completed), (1, 0, 0));

    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    let queue = hub(&mut r).queue;
    assert_eq!(queue.rows[0].stage, QueueStage::StartRequested);
    assert_eq!((queue.counts.stored, queue.counts.started, queue.counts.completed), (0, 1, 0));

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(hub(&mut r).queue.rows[0].stage, QueueStage::Running);

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Finished)));
    press(&mut r, MachineAction::QueryStatus);
    let queue = hub(&mut r).queue;
    assert_eq!(queue.rows[0].stage, QueueStage::Completed);
    assert_eq!((queue.counts.started, queue.counts.completed), (0, 1));
}

#[test]
fn an_execution_made_for_an_order_names_it_in_the_queue() {
    use burnburn_presentation::{OrderDraft, Role};
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    r.h.controller().set_role(Role::Operator);
    let part = r.h.document(r.doc).unwrap().title;
    r.h.send(UiIntent::AddOrder { order: OrderDraft { name: "Klant 17".into(), part, quantity: 2, remnant: None } });
    let order = r.h.operator_view().orders[0].clone();
    script_upload_acks(&r.peer, &job);
    let upload = order.upload.clone().unwrap();
    r.h.send(UiIntent::UploadForOrder { order: order.id, action: upload.action, seen: upload.seen });
    pump(&r);
    let queue = hub(&mut r).queue;
    assert_eq!(queue.rows[0].order, Some((order.id, "Klant 17".to_owned())));
}

// ---- W03: history and repeat -----------------------------------------------------------------------

#[test]
fn the_history_keeps_the_estimate_and_the_measured_duration_apart() {
    let mut r = rig();
    assert!(hub(&mut r).history.is_empty());
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    let entry = hub(&mut r).history[0].clone();
    assert_eq!(entry.stage, QueueStage::Stored);
    assert!(entry.estimated_s.is_some_and(|s| s > 0.0), "{:?}", entry.estimated_s);
    assert_eq!(entry.measured_s, None, "nothing was observed running yet");

    let mut r = rig();
    let job = produce(&mut r);
    let entry = hub(&mut r).history[0].clone();
    assert_eq!(entry.stage, QueueStage::Completed);
    assert_eq!(entry.plan, job.plan);
    assert_eq!(entry.plan_revision, job.plan_revision);
    assert_eq!(entry.rd_bytes, job.bytes);
    assert_eq!((entry.profile.as_str(), entry.profile_revision), ("research-1500x900", 1));
    assert_eq!(entry.measured_s, Some(30.0), "from the start acknowledgement to the reported end");
    assert!(entry.estimated_s.is_some());
}

#[test]
fn a_failed_execution_stays_in_the_history() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    script_identity(&r.peer, MODEL);
    r.peer.script(
        Step::expect_matching("chunk 0", |q| matches!(q, Request::UploadChunk { index: 0, .. })).reply(Reply::Nak),
    );
    r.h.press(Some(r.doc), job.upload.as_ref().unwrap());
    pump(&r);
    let history = hub(&mut r).history;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].stage, QueueStage::Failed);
}

#[test]
fn a_repeat_reuses_the_frozen_job_and_lists_what_changed_in_the_recipes_since() {
    let mut r = rig();
    let job = produce(&mut r);
    let first = hub(&mut r).history[0].execution;

    // The designer changes the recipe afterwards.
    let operation = r.h.document(r.doc).unwrap().operations[0].id;
    r.h.send(UiIntent::SetOperationField {
        document: r.doc,
        operation,
        field: OperationField::Speed,
        text: "250".into(),
    });
    r.h.send(UiIntent::SetOperationField {
        document: r.doc,
        operation,
        field: OperationField::Name,
        text: "Kontuur".into(),
    });
    r.h.send(UiIntent::RepeatExecution { execution: first });
    let repeat = hub(&mut r).repeat.expect("a repeat is in preparation");
    assert_eq!(repeat.plan, job.plan);
    assert_eq!(repeat.refusal, None);
    let changes: Vec<(&str, RecipeField, &str, &str)> =
        repeat.changes.iter().map(|c| (c.operation.as_str(), c.field, c.frozen.as_str(), c.current.as_str())).collect();
    assert!(changes.contains(&("Snijden", RecipeField::Speed, "100", "250")), "{changes:?}");
    assert!(changes.contains(&("Snijden", RecipeField::Name, "Snijden", "Kontuur")), "{changes:?}");
    assert_eq!(changes.len(), 2, "only what differs is listed: {changes:?}");

    // Confirming makes the frozen job (not a fresh compile) the window's job again.
    r.h.send(UiIntent::ConfirmRepeat { document: r.doc });
    assert_eq!(hub(&mut r).repeat, None);
    let again = r.h.document(r.doc).unwrap().job.expect("the job is prepared again");
    assert_eq!(again.plan, job.plan, "the snapshot, not the edited document");
    assert_eq!(again.rd_hash, job.rd_hash);
    assert!(again.design_changed, "and it is said that the design moved on");

    // Producing it again is a new execution of the same job.
    upload_and_store(&mut r, &again);
    let history = hub(&mut r).history;
    assert_eq!(history.len(), 2);
    assert_ne!(history[0].execution, history[1].execution);
    assert_eq!(history[0].plan, history[1].plan);
}

#[test]
fn a_repeat_is_refused_when_the_machine_profile_is_not_the_one_the_job_was_frozen_for() {
    let mut r = rig();
    produce(&mut r);
    let first = hub(&mut r).history[0].execution;
    r.h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(2)), ruida: RUIDA });
    r.h.send(UiIntent::RepeatExecution { execution: first });
    let repeat = hub(&mut r).repeat.expect("the repeat is shown, with its refusal");
    assert_eq!(repeat.refusal, Some(RepeatRefusal::ProfileChanged));
    let before = r.h.document(r.doc).unwrap().job;
    r.h.send(UiIntent::ConfirmRepeat { document: r.doc });
    assert_eq!(r.h.document(r.doc).unwrap().job, before, "nothing was replaced");

    r.h.send(UiIntent::CancelRepeat);
    assert_eq!(hub(&mut r).repeat, None);
}

#[test]
fn repeating_an_unknown_execution_changes_nothing() {
    let mut r = rig();
    r.h.send(UiIntent::RepeatExecution { execution: burnburn_machine::ExecutionId::from_raw(999) });
    assert_eq!(hub(&mut r).repeat, None);
}

// ---- W09: calibration proofs ---------------------------------------------------------------------------

fn proof(kind: ProofKind) -> ProofInput {
    ProofInput {
        kind,
        material: "berken-3mm".into(),
        speed: "200".into(),
        value: "0,15".into(),
        value_y: "-0,5".into(),
        method: "schuifmaat".into(),
        date: "2026-10-22".into(),
    }
}

fn problem(r: &mut Rig) -> Option<ProofProblem> {
    hub(r).calibration.problem
}

#[test]
fn a_proof_names_machine_material_date_and_method_and_keeps_its_scope() {
    let mut r = rig();
    r.h.send(UiIntent::RecordCalibration { input: proof(ProofKind::Kerf) });
    r.h.send(UiIntent::RecordCalibration { input: proof(ProofKind::ScanOffset) });
    r.h.send(UiIntent::RecordCalibration { input: proof(ProofKind::PointerOffset) });
    let rows = hub(&mut r).calibration.rows;
    assert_eq!(rows.len(), 3);
    assert_eq!(problem(&mut r), None);
    let kerf = &rows[0];
    assert_eq!(kerf.kind, ProofKind::Kerf);
    assert_eq!((kerf.machine.as_str(), kerf.material.as_str()), ("research-1500x900", "berken-3mm"));
    assert_eq!((kerf.method.as_str(), kerf.date.as_str()), ("schuifmaat", "2026-10-22"));
    assert_eq!(kerf.value_mm, 0.15, "a decimal comma is read");
    assert_eq!(kerf.speed_mm_s, None, "a kerf has a material, not a speed");
    let scan = &rows[1];
    assert_eq!((scan.kind, scan.speed_mm_s, scan.value_mm), (ProofKind::ScanOffset, Some(200.0), 0.15));
    let pointer = &rows[2];
    assert_eq!((pointer.kind, pointer.value_mm, pointer.value_y_mm), (ProofKind::PointerOffset, 0.15, Some(-0.5)));
    let ids: std::collections::BTreeSet<u64> = rows.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 3, "every proof has its own id");
}

#[test]
fn a_proof_with_a_missing_or_unusable_part_is_refused_in_words_and_recorded_nowhere() {
    let mut r = rig();
    let cases: Vec<(ProofInput, ProofProblem)> = vec![
        (ProofInput { method: " ".into(), ..proof(ProofKind::Kerf) }, ProofProblem::Invalid(ProofField::Method)),
        (ProofInput { date: String::new(), ..proof(ProofKind::Kerf) }, ProofProblem::Invalid(ProofField::Date)),
        (ProofInput { material: String::new(), ..proof(ProofKind::Kerf) }, ProofProblem::Invalid(ProofField::Material)),
        (ProofInput { value: "-0,1".into(), ..proof(ProofKind::Kerf) }, ProofProblem::Invalid(ProofField::Value)),
        (ProofInput { value: "veel".into(), ..proof(ProofKind::Kerf) }, ProofProblem::Invalid(ProofField::Value)),
        (ProofInput { speed: "0".into(), ..proof(ProofKind::ScanOffset) }, ProofProblem::Invalid(ProofField::Speed)),
        (
            ProofInput { value_y: "x".into(), ..proof(ProofKind::PointerOffset) },
            ProofProblem::Invalid(ProofField::ValueY),
        ),
    ];
    for (input, expected) in cases {
        r.h.send(UiIntent::RecordCalibration { input: input.clone() });
        assert_eq!(problem(&mut r), Some(expected), "{input:?}");
        assert!(hub(&mut r).calibration.rows.is_empty(), "{input:?}");
    }
}

#[test]
fn a_second_scan_offset_for_the_same_speed_is_refused_and_a_valid_proof_clears_the_problem() {
    let mut r = rig();
    r.h.send(UiIntent::RecordCalibration { input: proof(ProofKind::ScanOffset) });
    r.h.send(UiIntent::RecordCalibration { input: proof(ProofKind::ScanOffset) });
    assert_eq!(problem(&mut r), Some(ProofProblem::DuplicateSpeed));
    assert_eq!(hub(&mut r).calibration.rows.len(), 1);
    r.h.send(UiIntent::RecordCalibration { input: ProofInput { speed: "300".into(), ..proof(ProofKind::ScanOffset) } });
    assert_eq!(problem(&mut r), None);
    assert_eq!(hub(&mut r).calibration.rows.len(), 2);
}

#[test]
fn a_proof_needs_an_active_machine() {
    let mut h = HeadlessEditorHarness::new();
    h.send(UiIntent::RecordCalibration { input: proof(ProofKind::Kerf) });
    let view = h.controller().machine_hub_view().calibration;
    assert_eq!(view.problem, Some(ProofProblem::NoMachine));
    assert!(view.rows.is_empty());
}

#[test]
fn a_changed_calibration_makes_the_compiled_plan_stale_but_keeps_the_frozen_job() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    assert!(!r.h.document(r.doc).unwrap().plan.unwrap().stale);

    r.h.send(UiIntent::RecordCalibration { input: proof(ProofKind::ScanOffset) });
    let view = r.h.document(r.doc).unwrap();
    assert!(view.plan.unwrap().stale, "a plan made for the old calibration is not the current one");
    assert_eq!(view.job.map(|j| j.plan), Some(job.plan), "the prepared job keeps its explicit old snapshot");

    // Compiling again follows the new calibration.
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    assert!(!r.h.document(r.doc).unwrap().plan.unwrap().stale);

    // Removing the proof is a calibration change too.
    let id = hub(&mut r).calibration.rows[0].id;
    r.h.send(UiIntent::RemoveCalibration { id });
    assert!(hub(&mut r).calibration.rows.is_empty());
    assert!(r.h.document(r.doc).unwrap().plan.unwrap().stale);
}

#[test]
fn removing_an_unknown_proof_changes_nothing() {
    let mut r = rig();
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    r.h.send(UiIntent::RemoveCalibration { id: 42 });
    assert!(!r.h.document(r.doc).unwrap().plan.unwrap().stale);
}
