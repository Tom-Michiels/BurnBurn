//! The machine hub window on Windows, headless (G5-machine-rest, G5-native): the same rows, words
//! and intents as the macOS window (`burnburn-macos/tests/g5_machine_hub_panel.rs`), as a panel
//! spec with control ids, plus the Venster ▸ Machine en productie menu item.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;

use burnburn_machine::{ExecutionId, ExecutionState};
use burnburn_presentation::{
    ActionId, ActionSubject, AppController, CommandKey, HeadlessEditorHarness, HistoryEntry, MachineAction,
    MachineButton, MachineHubViewModel, MessageKey, ProofInput, ProofKind, QueueRow, QueueStage, RecipeChange,
    RecipeField, RepeatView, UiIntent, UploadStage, UploadStatusView,
};
use burnburn_types::{DocumentId, RevisionId};
use burnburn_windows::app_menu::{AppAction, action_reaction};
use burnburn_windows::host_panels::app::{AppInput, AppSheet, app_spec, id as app_id, on_app_event};
use burnburn_windows::host_panels::hub::id;
use burnburn_windows::host_panels::{
    ControlKind, Family, HostRequest, PanelEvent, PanelReaction, PanelSpec, PanelValues, notify,
};
use burnburn_windows::hub_panel::{HubPanel, hub_panel};
use burnburn_windows::l10n::Locale;

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

/// A window with a compiled plan (for a real plan id) and two known machines.
fn rig() -> (HeadlessEditorHarness, DocumentId, MachineHubViewModel) {
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

/// The hub with an upload that can be stopped, two executions, a repeat and no refusal.
fn busy_hub(h: &HeadlessEditorHarness, doc: DocumentId, vm: &MachineHubViewModel) -> MachineHubViewModel {
    let plan = h.document(doc).unwrap().plan.unwrap().id;
    let stop = MachineButton {
        action: ActionId {
            kind: MachineAction::Cancel,
            subject: ActionSubject::Execution(ExecutionId::from_raw(3)),
            generation: 2,
        },
        enabled: true,
        seen: Some(ExecutionState::UploadAcked),
        label: MessageKey::StopUpload,
    };
    let entry = |id: u64, stage| HistoryEntry {
        execution: ExecutionId::from_raw(id),
        job_name: "Plaat".into(),
        plan,
        plan_revision: RevisionId::INITIAL,
        profile: "research-1500x900".into(),
        profile_revision: 1,
        rd_bytes: 1234,
        stage,
        estimated_s: Some(12.0),
        measured_s: None,
        order: None,
    };
    MachineHubViewModel {
        upload: Some(UploadStatusView {
            execution: ExecutionId::from_raw(3),
            job_name: "Plaat".into(),
            stage: UploadStage::AckedNotConfirmed { chunks: 5 },
            stop: Some(stop),
        }),
        queue: burnburn_presentation::QueueView {
            rows: vec![QueueRow {
                execution: ExecutionId::from_raw(3),
                job_name: "Plaat".into(),
                plan,
                stage: QueueStage::Uploading,
                order: None,
            }],
            counts: burnburn_presentation::QueueCounts { uploading: 1, ..Default::default() },
        },
        history: vec![entry(9, QueueStage::Completed), entry(4, QueueStage::Stored)],
        repeat: Some(RepeatView {
            execution: ExecutionId::from_raw(4),
            job_name: "Plaat".into(),
            plan,
            changes: vec![RecipeChange {
                operation: "Snijden".into(),
                field: RecipeField::Speed,
                frozen: "100".into(),
                current: "250".into(),
            }],
            refusal: None,
        }),
        ..vm.clone()
    }
}

struct Views {
    prefs: burnburn_presentation::PrefsViewModel,
    shortcuts: burnburn_presentation::ShortcutsViewModel,
    debug: burnburn_presentation::DebugLogViewModel,
    vm: std::sync::Arc<burnburn_presentation::AppViewModel>,
    hub: MachineHubViewModel,
    doc: Option<DocumentId>,
}

fn views(h: &mut HeadlessEditorHarness, hub: MachineHubViewModel, doc: Option<DocumentId>) -> Views {
    let c = h.controller();
    Views { prefs: c.prefs_view(), shortcuts: c.shortcuts_view(), debug: c.debug_log_view(), vm: h.view(), hub, doc }
}

impl Views {
    fn input(&self) -> AppInput<'_> {
        AppInput {
            locale: Locale::En,
            vm: &self.vm,
            document: self.doc,
            prefs: &self.prefs,
            shortcuts: &self.shortcuts,
            history: None,
            preview: None,
            rd: None,
            help: AppController::help_info(),
            debug: &self.debug,
            hub: &self.hub,
            support_text: "",
            shortcut_filter: "",
            shortcut_row: None,
            recording: None,
            pending_replace: None,
        }
    }

    fn spec(&self) -> PanelSpec {
        app_spec(AppSheet::Hub, &self.input())
    }

    fn react(&self, ev: PanelEvent) -> PanelReaction {
        on_app_event(AppSheet::Hub, &self.input(), &ev)
    }

    fn panel(&self) -> HubPanel {
        hub_panel(&self.hub, self.doc.is_some(), Locale::En)
    }
}

fn event(cid: u16, code: u16, values: PanelValues) -> PanelEvent {
    PanelEvent { id: cid, code, values, click_px: None, view_px: None, key: None }
}

fn click(cid: u16, values: PanelValues) -> PanelEvent {
    event(cid, notify::CLICKED, values)
}

fn intent(reaction: PanelReaction) -> UiIntent {
    match reaction {
        PanelReaction::Intent(i) => i,
        other => panic!("expected an intent, got {other:?}"),
    }
}

fn items(spec: &PanelSpec, cid: u16) -> Vec<String> {
    match &spec.control(cid).unwrap_or_else(|| panic!("control {cid} missing")).kind {
        ControlKind::Combo { items, .. } | ControlKind::List { items, .. } => items.clone(),
        other => panic!("{other:?}"),
    }
}

fn selected(spec: &PanelSpec, cid: u16) -> Option<usize> {
    match &spec.control(cid).unwrap().kind {
        ControlKind::Combo { selected, .. } | ControlKind::List { selected, .. } => *selected,
        other => panic!("{other:?}"),
    }
}

fn pick(cid: u16, row: usize) -> PanelValues {
    let mut v = PanelValues::default();
    v.selection.insert(cid, Some(row));
    v
}

#[test]
fn the_hub_shows_devices_support_upload_queue_and_history_like_the_mac_window() {
    let (mut h, doc, vm) = rig();
    let hub = busy_hub(&h, doc, &vm);
    let v = views(&mut h, hub, Some(doc));
    let (spec, panel) = (v.spec(), v.panel());
    assert_eq!(spec.title, "Machine and Production");
    assert_eq!(items(&spec, id::DEVICE), panel.devices);
    assert_eq!(selected(&spec, id::DEVICE), panel.active_device);
    assert_eq!(panel.devices.len(), 2);
    assert_eq!(spec.control(id::SUPPORT).unwrap().text, panel.support.join("\n"));
    assert_eq!(spec.control(id::UPLOAD).unwrap().text, panel.upload);
    assert!(spec.control(id::STOP).unwrap().enabled, "an upload that can be stopped");
    assert_eq!(spec.control(id::QUEUE_COUNTS).unwrap().text, panel.queue_counts);
    assert_eq!(items(&spec, id::QUEUE), panel.queue);
    assert_eq!(items(&spec, id::HISTORY), panel.history);
    assert_eq!(panel.history.len(), 2);
    // The repeat is in preparation: its header, differences and buttons show.
    let repeat = panel.repeat.clone().unwrap();
    assert_eq!(spec.control(id::REPEAT_HEADER).unwrap().text, repeat.header);
    assert!(spec.control(id::REPEAT_CHANGES).unwrap().text.contains("Snijden: speed (mm/s) 100 → 250"));
    assert!(spec.control(id::CONFIRM).unwrap().visible && spec.control(id::CONFIRM).unwrap().enabled);
    assert!(spec.control(id::CANCEL).unwrap().visible);
}

#[test]
fn without_a_repeat_or_an_upload_the_hub_hides_and_greys_what_does_not_apply() {
    let (mut h, _doc, vm) = rig();
    let v = views(&mut h, vm, None);
    let spec = v.spec();
    assert!(!spec.control(id::STOP).unwrap().enabled, "nothing to stop");
    for cid in [id::REPEAT_HEADER, id::REPEAT_CHANGES, id::CONFIRM, id::CANCEL] {
        assert!(!spec.control(cid).unwrap().visible, "{cid} is hidden without a repeat");
    }
    // A repeat without a window cannot be confirmed, and says why.
    let (mut h2, doc2, vm2) = rig();
    let busy = busy_hub(&h2, doc2, &vm2);
    let gone = views(&mut h2, busy, None);
    let spec = gone.spec();
    assert!(!spec.control(id::CONFIRM).unwrap().enabled);
    assert!(
        spec.control(id::REPEAT_REFUSAL).unwrap().text.contains("window"),
        "{:?}",
        spec.control(id::REPEAT_REFUSAL)
    );
}

#[test]
fn the_hubs_controls_become_the_presentation_intents_and_a_stale_choice_sends_nothing() {
    let (mut h, doc, vm) = rig();
    let hub = busy_hub(&h, doc, &vm);
    let v = views(&mut h, hub, Some(doc));
    assert_eq!(
        intent(v.react(event(id::DEVICE, notify::CHANGED, pick(id::DEVICE, 1)))),
        UiIntent::ChooseMachineProfile { id: "research-1500x900".into() }
    );
    assert_eq!(v.react(event(id::DEVICE, notify::CHANGED, pick(id::DEVICE, 9))), PanelReaction::Nothing);
    let stop = v.hub.upload.as_ref().unwrap().stop.clone().unwrap();
    assert_eq!(
        intent(v.react(click(id::STOP, PanelValues::default()))),
        UiIntent::Machine { document: Some(doc), action: stop.action, seen: stop.seen }
    );
    assert_eq!(
        intent(v.react(click(id::REPEAT, pick(id::HISTORY, 1)))),
        UiIntent::RepeatExecution { execution: ExecutionId::from_raw(4) }
    );
    assert_eq!(v.react(click(id::REPEAT, PanelValues::default())), PanelReaction::Nothing, "no row chosen");
    assert_eq!(intent(v.react(click(id::CONFIRM, PanelValues::default()))), UiIntent::ConfirmRepeat { document: doc });
    assert_eq!(intent(v.react(click(id::CANCEL, PanelValues::default()))), UiIntent::CancelRepeat);
}

#[test]
fn the_calibration_form_is_sent_as_typed_and_a_proof_can_be_removed() {
    let (mut h, doc, vm) = rig();
    let v = views(&mut h, vm, Some(doc));
    let spec = v.spec();
    assert_eq!(items(&spec, id::KIND).len(), 3);
    let mut values = PanelValues::default();
    values.selection.insert(id::KIND, Some(1));
    for (cid, text) in [
        (id::MATERIAL, "Berken 3 mm"),
        (id::SPEED, "300"),
        (id::VALUE, "0,12"),
        (id::VALUE_Y, ""),
        (id::METHOD, "meetklok"),
        (id::DATE, "2026-10-01"),
    ] {
        values.texts.insert(cid, text.to_owned());
    }
    assert_eq!(
        intent(v.react(click(id::RECORD, values))),
        UiIntent::RecordCalibration {
            input: ProofInput {
                kind: ProofKind::ScanOffset,
                material: "Berken 3 mm".into(),
                speed: "300".into(),
                value: "0,12".into(),
                value_y: String::new(),
                method: "meetklok".into(),
                date: "2026-10-01".into(),
            }
        }
    );
    // Recording it makes it a row; the chosen row can be removed.
    let mut h = h;
    h.send(UiIntent::RecordCalibration {
        input: ProofInput {
            kind: ProofKind::Kerf,
            material: "Berken".into(),
            speed: String::new(),
            value: "0,15".into(),
            value_y: String::new(),
            method: "schuifmaat".into(),
            date: "2026-10-01".into(),
        },
    });
    let hub = h.controller().machine_hub_view();
    assert_eq!(hub.calibration.rows.len(), 1);
    let id_of_row = hub.calibration.rows[0].id;
    let v = views(&mut h, hub, Some(doc));
    assert_eq!(items(&v.spec(), id::PROOFS).len(), 1);
    assert_eq!(intent(v.react(click(id::REMOVE, pick(id::PROOFS, 0)))), UiIntent::RemoveCalibration { id: id_of_row });
    assert_eq!(v.react(click(id::REMOVE, PanelValues::default())), PanelReaction::Nothing);
}

#[test]
fn the_hub_has_unique_ids_inside_the_app_block_and_a_window_menu_item_that_needs_no_window() {
    let (mut h, doc, vm) = rig();
    let hub = busy_hub(&h, doc, &vm);
    let v = views(&mut h, hub, Some(doc));
    let mut seen = HashSet::new();
    for c in v.spec().controls() {
        assert!(seen.insert(c.id), "duplicate id {}", c.id);
        assert!(c.id >= app_id::PREF_CAPTION && c.id < app_id::PREF_CAPTION + 200, "{} outside the app block", c.id);
    }
    assert!(AppSheet::ALL.contains(&AppSheet::Hub));

    let empty = HeadlessEditorHarness::new();
    let help = AppController::help_info();
    assert!(AppAction::MachineHub.enabled(&empty.view(), false));
    assert_eq!(AppAction::MachineHub.title(Locale::En), "Machine and Production…");
    assert_eq!(AppAction::from_id(AppAction::MachineHub.id()), Some(AppAction::MachineHub));
    assert_eq!(
        action_reaction(AppAction::MachineHub, &empty.view(), &help, Locale::En),
        PanelReaction::Host(HostRequest::ShowSheet(Family::App, AppSheet::Hub.code()))
    );
}
