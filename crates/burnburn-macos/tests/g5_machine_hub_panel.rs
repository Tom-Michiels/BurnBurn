//! G5-machine-rest, the machine hub window without AppKit: what it shows for the presentation's
//! hub view model and which intent each of its widgets is. The view models come from the real
//! `AppController` where it can produce them without a machine, and are built by hand for the
//! rest (they are plain data).

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_job::Feature;
use burnburn_machine::{ExecutionId, ExecutionState};
use burnburn_macos::app_menu::AppAction;
use burnburn_macos::hub_panel::{HubEvent, PROOF_KINDS, ProofForm, hub_intent, hub_panel};
use burnburn_macos::l10n::Locale;
use burnburn_macos::l10n_machine::{MachineText, fill, machine_text};
use burnburn_macos::panels::cam_support_line;
use burnburn_presentation::{
    ActionId, ActionSubject, CommandKey, HeadlessEditorHarness, HistoryEntry, MachineAction, MachineButton,
    MachineHubViewModel, MessageKey, ProofKind, ProofProblem, QueueCounts, QueueRow, QueueStage, QueueView,
    RecipeChange, RecipeField, RepeatRefusal, RepeatView, UiIntent, UploadFailure, UploadStage, UploadStatusView,
};
use burnburn_types::{DocumentId, RevisionId};

fn machine_profile(id: &str, width: f64) -> burnburn_job::MachineProfile {
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    MachineProfile::new(
        id,
        1,
        MachineFrame::new(width, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn ruida() -> burnburn_ruida::RuidaProfile {
    burnburn_ruida::RuidaProfile {
        magic: burnburn_ruida::Magic::FIXTURE_0X88,
        overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
    }
}

/// A window with a compiled plan (for a real `JobPlanId`) and two known machines.
fn controller_view() -> (HeadlessEditorHarness, DocumentId, MachineHubViewModel) {
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(machine_profile("research-1500x900", 1500.0)),
        ruida: ruida(),
    });
    h.send(UiIntent::SetMachineProfile { profile: Box::new(machine_profile("tweede", 600.0)), ruida: ruida() });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: burnburn_presentation::SelectMode::Replace });
    let p = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    h.send(UiIntent::AddOperation { document: doc, draft });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let vm = h.controller().machine_hub_view();
    (h, doc, vm)
}

fn stop_button() -> MachineButton {
    MachineButton {
        action: ActionId {
            kind: MachineAction::Cancel,
            subject: ActionSubject::Execution(ExecutionId::from_raw(3)),
            generation: 2,
        },
        enabled: true,
        seen: Some(ExecutionState::UploadAcked),
        label: MessageKey::StopUpload,
    }
}

fn upload(stage: UploadStage, stop: Option<MachineButton>) -> Option<UploadStatusView> {
    Some(UploadStatusView { execution: ExecutionId::from_raw(3), job_name: "Plaat".into(), stage, stop })
}

fn with_upload(vm: &MachineHubViewModel, stage: UploadStage, stop: Option<MachineButton>) -> MachineHubViewModel {
    MachineHubViewModel { upload: upload(stage, stop), ..vm.clone() }
}

#[test]
fn the_devices_and_the_support_list_are_those_of_the_hub_view_model() {
    let (_h, _doc, vm) = controller_view();
    let panel = hub_panel(&vm, true, Locale::Nl);
    assert_eq!(panel.devices.len(), 2);
    assert_eq!(panel.active_device, Some(0));
    assert_eq!(panel.devices[0], "tweede (revisie 1, 600 × 900 mm)");
    assert_eq!(panel.devices[1], "research-1500x900 (revisie 1, 1500 × 900 mm)");
    assert_eq!(panel.support.len(), Feature::ALL.len());
    for (line, row) in panel.support.iter().zip(&vm.support) {
        assert_eq!(*line, cam_support_line(row, Locale::Nl), "the same words as Device Settings");
    }
    let en = hub_panel(&vm, true, Locale::En);
    assert_eq!(en.devices[0], "tweede (revision 1, 600 × 900 mm)");

    let empty = HeadlessEditorHarness::new().controller().machine_hub_view();
    let panel = hub_panel(&empty, false, Locale::Nl);
    assert!(panel.devices.is_empty() && panel.support.is_empty());
    assert_eq!(panel.active_device, None);
}

#[test]
fn choosing_a_device_sends_its_profile_id_and_a_stale_index_nothing() {
    let (_h, _doc, vm) = controller_view();
    assert_eq!(
        hub_intent(&vm, None, HubEvent::DeviceChosen(1)),
        Some(UiIntent::ChooseMachineProfile { id: "research-1500x900".into() })
    );
    assert_eq!(hub_intent(&vm, None, HubEvent::DeviceChosen(9)), None);
}

#[test]
fn the_upload_text_keeps_progress_the_transport_ack_and_storage_apart() {
    let (_h, _doc, vm) = controller_view();
    let text = |stage| hub_panel(&with_upload(&vm, stage, None), true, Locale::Nl).upload;
    assert_eq!(hub_panel(&vm, true, Locale::Nl).upload, machine_text(MachineText::UploadNone, Locale::Nl));

    let sending = text(UploadStage::Sending { acked: 1, total: 5 });
    assert!(sending.contains("1 van 5") && sending.contains("Nog niet opgeslagen"), "{sending}");
    let acked = text(UploadStage::AckedNotConfirmed { chunks: 5 });
    assert!(acked.contains("Opslag is nog niet bevestigd"), "{acked}");
    let stored = text(UploadStage::StoredConfirmed);
    assert!(stored.contains("terugleesactie"), "{stored}");
    for not_stored in [&sending, &acked] {
        assert!(!not_stored.to_lowercase().contains("opgeslagen op de controller"), "{not_stored}");
    }
    assert!(text(UploadStage::Stopped).contains("Upload gestopt"));
    let rejected = text(UploadStage::Failed(UploadFailure::Rejected { chunk: 1 }));
    assert!(rejected.contains("pakket 1") && rejected.contains("niet te onderscheiden"), "{rejected}");
    for failure in [
        UploadFailure::IdentityMismatch,
        UploadFailure::IdentityUnconfirmed,
        UploadFailure::NotStored,
        UploadFailure::ControllerFault,
    ] {
        assert!(!text(UploadStage::Failed(failure)).trim().is_empty(), "{failure:?}");
    }
    assert!(text(UploadStage::VerifyingIdentity).contains("gecontroleerd"));
}

#[test]
fn stop_upload_is_the_machine_panels_own_cancel_control_unchanged() {
    let (_h, doc, vm) = controller_view();
    let vm = with_upload(&vm, UploadStage::AckedNotConfirmed { chunks: 5 }, Some(stop_button()));
    assert!(hub_panel(&vm, true, Locale::Nl).stop_enabled);
    let b = stop_button();
    assert_eq!(
        hub_intent(&vm, Some(doc), HubEvent::StopUpload),
        Some(UiIntent::Machine { document: Some(doc), action: b.action, seen: b.seen })
    );
    let stored = with_upload(&vm, UploadStage::StoredConfirmed, None);
    assert!(!hub_panel(&stored, true, Locale::Nl).stop_enabled);
    assert_eq!(hub_intent(&stored, Some(doc), HubEvent::StopUpload), None, "nothing to stop");
    let disabled = with_upload(&vm, UploadStage::Stopped, Some(MachineButton { enabled: false, ..stop_button() }));
    assert!(!hub_panel(&disabled, true, Locale::Nl).stop_enabled);
    assert_eq!(hub_intent(&disabled, Some(doc), HubEvent::StopUpload), None);
}

#[test]
fn the_queue_names_the_stage_of_every_execution_and_counts_only_what_the_evidence_supports() {
    let (_h, _doc, vm) = controller_view();
    let plan = vm_plan(&_h, _doc);
    let rows = vec![
        QueueRow {
            execution: ExecutionId::from_raw(1),
            job_name: "Plaat".into(),
            plan,
            stage: QueueStage::Completed,
            order: None,
        },
        QueueRow {
            execution: ExecutionId::from_raw(2),
            job_name: "Plaat".into(),
            plan,
            stage: QueueStage::Stored,
            order: Some((burnburn_presentation::OrderId(7), "Klant 17".into())),
        },
        QueueRow {
            execution: ExecutionId::from_raw(3),
            job_name: "Plaat".into(),
            plan,
            stage: QueueStage::Unknown,
            order: None,
        },
    ];
    let counts = QueueCounts { uploading: 0, stored: 1, started: 0, completed: 1, unknown: 1, failed: 0 };
    let vm = MachineHubViewModel { queue: QueueView { rows, counts }, ..vm };
    let panel = hub_panel(&vm, true, Locale::Nl);
    assert_eq!(
        panel.queue_counts,
        "Bezig met uploaden: 0 · Opgeslagen, niet gestart: 1 · Gestart: 0 · Voltooid: 1 · Onbekend: 1 · Mislukt of gestopt: 0"
    );
    assert_eq!(panel.queue[0], "Plaat — voltooid");
    assert_eq!(panel.queue[1], "Plaat — opgeslagen, niet gestart (order Klant 17)");
    assert_eq!(panel.queue[2], "Plaat — onbekend — controleer de machine");
    let none = hub_panel(&MachineHubViewModel { queue: QueueView::default(), ..vm }, true, Locale::Nl);
    assert_eq!(none.queue, [machine_text(MachineText::QueueNone, Locale::Nl)]);
}

fn vm_plan(h: &HeadlessEditorHarness, doc: DocumentId) -> burnburn_job::JobPlanId {
    h.document(doc).unwrap().plan.unwrap().id
}

fn entry(
    plan: burnburn_job::JobPlanId,
    id: u64,
    stage: QueueStage,
    est: Option<f64>,
    measured: Option<f64>,
) -> HistoryEntry {
    HistoryEntry {
        execution: ExecutionId::from_raw(id),
        job_name: "Plaat".into(),
        plan,
        plan_revision: RevisionId::INITIAL,
        profile: "research-1500x900".into(),
        profile_revision: 1,
        rd_bytes: 1234,
        stage,
        estimated_s: est,
        measured_s: measured,
        order: None,
    }
}

#[test]
fn a_history_row_says_the_estimate_and_the_measured_duration_apart() {
    let (h, doc, vm) = controller_view();
    let plan = vm_plan(&h, doc);
    let vm = MachineHubViewModel {
        history: vec![
            entry(plan, 2, QueueStage::Completed, Some(12.34), Some(30.0)),
            entry(plan, 1, QueueStage::Stored, None, None),
        ],
        ..vm
    };
    let panel = hub_panel(&vm, true, Locale::Nl);
    assert_eq!(
        panel.history[0],
        "Plaat — voltooid — profiel research-1500x900 r1 — 1234 bytes — schatting: 12,3 s — gemeten: 30 s"
    );
    assert_eq!(
        panel.history[1],
        "Plaat — opgeslagen, niet gestart — profiel research-1500x900 r1 — 1234 bytes — schatting: geen — gemeten: niet gemeten"
    );
    let en = hub_panel(&vm, true, Locale::En);
    assert!(
        en.history[0].contains("estimate: 12.3 s") && en.history[0].contains("measured: 30 s"),
        "{}",
        en.history[0]
    );
}

#[test]
fn a_repeat_lists_each_difference_in_words_and_says_when_it_is_refused() {
    let (h, doc, vm) = controller_view();
    let plan = vm_plan(&h, doc);
    let change = |field, frozen: &str, current: &str| RecipeChange {
        operation: "Snijden".into(),
        field,
        frozen: frozen.into(),
        current: current.into(),
    };
    let repeat = RepeatView {
        execution: ExecutionId::from_raw(1),
        job_name: "Plaat".into(),
        plan,
        changes: vec![
            change(RecipeField::Speed, "100", "250"),
            change(RecipeField::Name, "Snijden", "Kontuur"),
            change(RecipeField::AirAssist, "true", "false"),
            change(RecipeField::Removed, "Snijden", ""),
            RecipeChange {
                operation: "Graveren".into(),
                field: RecipeField::Added,
                frozen: String::new(),
                current: "Graveren".into(),
            },
        ],
        refusal: None,
    };
    let with = |repeat: RepeatView| MachineHubViewModel { repeat: Some(repeat), ..vm.clone() };
    let panel = hub_panel(&with(repeat.clone()), true, Locale::Nl).repeat.unwrap();
    assert!(panel.header.contains("Plaat"), "{}", panel.header);
    assert_eq!(panel.changes[0], "Snijden: snelheid (mm/s) 100 → 250");
    assert_eq!(panel.changes[1], "Snijden: naam Snijden → Kontuur");
    assert_eq!(panel.changes[2], "Snijden: luchtondersteuning aan → uit");
    assert_eq!(panel.changes[3], "Snijden: bestaat niet meer in het document");
    assert_eq!(panel.changes[4], "Graveren: nieuw in het document");
    assert!(panel.can_confirm && panel.refusal.is_none());

    let no_changes =
        hub_panel(&with(RepeatView { changes: Vec::new(), ..repeat.clone() }), true, Locale::Nl).repeat.unwrap();
    assert_eq!(no_changes.changes, [machine_text(MachineText::RepeatNoChanges, Locale::Nl)]);

    let refused = hub_panel(
        &with(RepeatView { refusal: Some(RepeatRefusal::ProfileChanged), ..repeat.clone() }),
        true,
        Locale::Nl,
    )
    .repeat
    .unwrap();
    assert!(!refused.can_confirm);
    assert_eq!(refused.refusal.as_deref(), Some(machine_text(MachineText::RepeatProfileChanged, Locale::Nl)));

    let no_window = hub_panel(&with(repeat), false, Locale::Nl).repeat.unwrap();
    assert!(!no_window.can_confirm, "a job is prepared in a window");
    assert_eq!(no_window.refusal.as_deref(), Some(machine_text(MachineText::RepeatNeedsWindow, Locale::Nl)));
}

#[test]
fn the_repeat_events_are_the_presentation_intents() {
    let (h, doc, vm) = controller_view();
    let plan = vm_plan(&h, doc);
    let vm = MachineHubViewModel {
        history: vec![
            entry(plan, 9, QueueStage::Completed, None, None),
            entry(plan, 4, QueueStage::Completed, None, None),
        ],
        ..vm
    };
    assert_eq!(
        hub_intent(&vm, Some(doc), HubEvent::Repeat(1)),
        Some(UiIntent::RepeatExecution { execution: ExecutionId::from_raw(4) })
    );
    assert_eq!(hub_intent(&vm, Some(doc), HubEvent::Repeat(5)), None);
    assert_eq!(hub_intent(&vm, Some(doc), HubEvent::ConfirmRepeat), Some(UiIntent::ConfirmRepeat { document: doc }));
    assert_eq!(hub_intent(&vm, None, HubEvent::ConfirmRepeat), None, "no window, no job card");
    assert_eq!(hub_intent(&vm, Some(doc), HubEvent::CancelRepeat), Some(UiIntent::CancelRepeat));
}

#[test]
fn the_calibration_form_becomes_a_proof_intent_and_a_stale_choice_nothing() {
    let (mut h, doc, vm) = controller_view();
    let form = ProofForm {
        kind: 1,
        material: "berken-3mm".into(),
        speed: "200".into(),
        value: "0,05".into(),
        value_y: String::new(),
        method: "schuifmaat".into(),
        date: "2026-10-22".into(),
    };
    let intent = hub_intent(&vm, Some(doc), HubEvent::RecordProof(form.clone())).unwrap();
    let UiIntent::RecordCalibration { input } = &intent else { panic!("{intent:?}") };
    assert_eq!(input.kind, PROOF_KINDS[1]);
    assert_eq!((input.speed.as_str(), input.value.as_str(), input.method.as_str()), ("200", "0,05", "schuifmaat"));
    assert_eq!(hub_intent(&vm, Some(doc), HubEvent::RecordProof(ProofForm { kind: 7, ..form })), None);

    // Through the controller: the recorded proof is a row, removable by its position.
    h.send(intent);
    let vm = h.controller().machine_hub_view();
    let panel = hub_panel(&vm, true, Locale::Nl);
    assert_eq!(panel.proofs.len(), 1);
    assert_eq!(
        panel.proofs[0],
        "Scanoffset (per snelheid) — tweede — berken-3mm — 200 mm/s → 0,05 mm — schuifmaat — 2026-10-22"
    );
    assert_eq!(panel.proof_problem, "");
    let id = vm.calibration.rows[0].id;
    assert_eq!(hub_intent(&vm, Some(doc), HubEvent::RemoveProof(0)), Some(UiIntent::RemoveCalibration { id }));
    assert_eq!(hub_intent(&vm, Some(doc), HubEvent::RemoveProof(3)), None);
}

#[test]
fn every_refused_proof_is_said_in_its_own_words() {
    use burnburn_presentation::{ProofField as F, ProofView};
    let (mut h, _doc, vm) = controller_view();
    let table = [
        (ProofProblem::NoMachine, MachineText::ProblemNoMachine),
        (ProofProblem::Invalid(F::Material), MachineText::ProblemMaterial),
        (ProofProblem::Invalid(F::Speed), MachineText::ProblemSpeed),
        (ProofProblem::Invalid(F::Value), MachineText::ProblemValue),
        (ProofProblem::Invalid(F::ValueY), MachineText::ProblemValueY),
        (ProofProblem::Invalid(F::Method), MachineText::ProblemMethod),
        (ProofProblem::Invalid(F::Date), MachineText::ProblemDate),
        (ProofProblem::DuplicateSpeed, MachineText::ProblemDuplicateSpeed),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (problem, text) in table {
        let shown =
            MachineHubViewModel { calibration: ProofView { rows: Vec::new(), problem: Some(problem) }, ..vm.clone() };
        for locale in [Locale::Nl, Locale::En] {
            assert_eq!(hub_panel(&shown, true, locale).proof_problem, machine_text(text, locale), "{problem:?}");
        }
        assert!(seen.insert(hub_panel(&shown, true, Locale::Nl).proof_problem), "{problem:?} repeats a text");
    }

    // Through the controller: a refused proof reaches the panel.
    h.send(UiIntent::RecordCalibration {
        input: burnburn_presentation::ProofInput {
            kind: ProofKind::Kerf,
            material: String::new(),
            speed: String::new(),
            value: "0,1".into(),
            value_y: String::new(),
            method: "x".into(),
            date: "2026-10-22".into(),
        },
    });
    let panel = hub_panel(&h.controller().machine_hub_view(), true, Locale::Nl);
    assert_eq!(panel.proof_problem, machine_text(MachineText::ProblemMaterial, Locale::Nl));
    assert!(panel.proofs.is_empty());
}

#[test]
fn every_hub_text_has_both_languages_and_the_templates_fill_completely() {
    for text in MachineText::ALL {
        for locale in [Locale::Nl, Locale::En] {
            assert!(!machine_text(*text, locale).trim().is_empty(), "{text:?} {locale:?}");
        }
    }
    let filled = fill(MachineText::QueueCounts, Locale::En, &["1", "2", "3", "4", "5", "6"]);
    assert!(!filled.contains('{'), "{filled}");
    assert_ne!(machine_text(MachineText::HubTitle, Locale::Nl), machine_text(MachineText::HubTitle, Locale::En));
}

#[test]
fn the_hub_is_a_window_menu_item_that_needs_no_window() {
    let empty = HeadlessEditorHarness::new();
    assert!(AppAction::MachineHub.enabled(&empty.view()));
    assert_eq!(AppAction::from_tag(AppAction::MachineHub.tag()), Some(AppAction::MachineHub));
    assert!(AppAction::MachineHub.title(Locale::Nl).starts_with("Machine en productie"));
    assert!(AppAction::MachineHub.title(Locale::En).starts_with("Machine and Production"));
}
