//! The machine hub's read model and intents (see `crate::machine_hub`): known machines and the
//! support matrix, the upload status, the queue and history of executions, a repeat from a frozen
//! job and the calibration proofs.

use std::sync::Arc;

use burnburn_document::process::{Operation, OperationMode};
use burnburn_job::{Feature, MachineProfile, Support};
use burnburn_machine::{ControllerStatus, Evidence, ExecutionId, ExecutionState, FailReason};
use burnburn_types::DocumentId;

use super::machine::Prepared;
use super::{AppController, AppUpdate, error};
use crate::cam_dialogs::{CapabilityRow, SupportView};
use crate::field::parse_number;
use crate::machine_hub::{
    DeviceRow, HistoryEntry, MachineHubViewModel, ProofField, ProofInput, ProofKind, ProofProblem, ProofRow, ProofView,
    QueueCounts, QueueRow, QueueStage, QueueView, RecipeChange, RecipeField, RepeatRefusal, RepeatView, UploadFailure,
    UploadStage, UploadStatusView,
};
use crate::production::{ActionSubject, ExecutionRecord, MachineAction, MachineStatus, OrderId};
use crate::view_model::MessageKey;

/// Window state of the hub (never document data).
#[derive(Debug, Default)]
pub(super) struct HubState {
    /// A repeat in preparation: the execution and the job it froze.
    repeat: Option<(ExecutionId, Arc<crate::production::PreparedJob>)>,
    proofs: Vec<Proof>,
    next_proof: u64,
    problem: Option<ProofProblem>,
}

#[derive(Debug, Clone)]
struct Proof {
    id: u64,
    kind: ProofKind,
    machine: String,
    material: String,
    speed_mm_s: Option<f64>,
    value_mm: f64,
    value_y_mm: Option<f64>,
    method: String,
    date: String,
}

impl AppController {
    pub fn machine_hub_view(&self) -> MachineHubViewModel {
        let status = self.machine_port.as_ref().map(|p| p.status());
        MachineHubViewModel {
            devices: self.hub_devices(),
            support: self.hub_support(),
            upload: status.as_ref().and_then(upload_status),
            queue: status.as_ref().map_or_else(QueueView::default, |st| self.hub_queue(st)),
            history: status.as_ref().map_or_else(Vec::new, |st| self.hub_history(st)),
            repeat: self.hub.repeat.as_ref().map(|(execution, job)| self.repeat_view(*execution, job)),
            calibration: ProofView { rows: self.hub.proofs.iter().map(proof_row).collect(), problem: self.hub.problem },
        }
    }

    fn hub_devices(&self) -> Vec<DeviceRow> {
        let active = self.machine.as_ref().map(|m| m.profile.revision().id.clone());
        self.known_profiles()
            .iter()
            .map(|p| {
                let id = p.revision().id.as_str();
                let declared = match self.machine.as_ref().filter(|m| m.profile.revision().id == id) {
                    Some(m) => m.declared.clone(),
                    None => self.libs_declared(id),
                };
                DeviceRow {
                id: p.revision().id.clone(),
                revision: p.revision().revision,
                width_mm: p.frame().width_mm(),
                height_mm: p.frame().height_mm(),
                active: active.as_deref() == Some(p.revision().id.as_str()),
                model: declared.model,
                firmware: declared.firmware,
                }
            })
            .collect()
    }

    /// The matrix the compiler reads, row by row.
    fn hub_support(&self) -> Vec<CapabilityRow> {
        let Some(machine) = self.machine.as_ref() else { return Vec::new() };
        let matrix = machine.profile.capabilities();
        Feature::ALL
            .into_iter()
            .map(|feature| CapabilityRow {
                feature,
                support: match matrix.support(feature) {
                    Support::Supported => SupportView::Supported,
                    Support::Unsupported => SupportView::Unsupported,
                    Support::Unverified { reason } => SupportView::Unverified(reason),
                },
            })
            .collect()
    }

    /// Job executions of the machine service, oldest first; a laser-off frame run is not one.
    fn hub_executions(status: &MachineStatus) -> Vec<&ExecutionRecord> {
        let frame_id = status.current.as_ref().filter(|_| status.current_is_frame).map(|r| r.execution.id);
        status.executions().filter(|r| Some(r.execution.id) != frame_id).collect()
    }

    fn order_of(&self, execution: ExecutionId) -> Option<(OrderId, String)> {
        let id = self.order_executions.get(&execution)?;
        let order = self.orders.iter().find(|o| o.id == *id)?;
        Some((*id, order.draft.name.clone()))
    }

    fn hub_queue(&self, status: &MachineStatus) -> QueueView {
        let rows: Vec<QueueRow> = Self::hub_executions(status)
            .into_iter()
            .map(|r| QueueRow {
                execution: r.execution.id,
                job_name: r.job.name.clone(),
                plan: r.job.plan,
                stage: stage_of(&r.execution.state),
                order: self.order_of(r.execution.id),
            })
            .collect();
        let mut counts = QueueCounts::default();
        for row in &rows {
            match row.stage {
                QueueStage::Uploading => counts.uploading += 1,
                QueueStage::Stored => counts.stored += 1,
                QueueStage::StartRequested | QueueStage::Running => counts.started += 1,
                QueueStage::Completed => counts.completed += 1,
                QueueStage::Unknown => counts.unknown += 1,
                QueueStage::Failed | QueueStage::Cancelled => counts.failed += 1,
            }
        }
        QueueView { rows, counts }
    }

    fn hub_history(&self, status: &MachineStatus) -> Vec<HistoryEntry> {
        let active = self.machine.as_ref().map(|m| &m.profile);
        let mut entries: Vec<HistoryEntry> = Self::hub_executions(status)
            .into_iter()
            .map(|r| HistoryEntry {
                execution: r.execution.id,
                job_name: r.job.name.clone(),
                plan: r.job.plan,
                plan_revision: r.job.plan_revision,
                profile: r.job.profile.id.clone(),
                profile_revision: r.job.profile.revision,
                rd_bytes: r.job.rd.len(),
                stage: stage_of(&r.execution.state),
                estimated_s: estimate(r, active),
                measured_s: measured(r),
                order: self.order_of(r.execution.id),
            })
            .collect();
        entries.reverse();
        entries
    }

    // ---- repeat (W03) ----------------------------------------------------------------------------

    pub(super) fn hub_repeat(&mut self, execution: ExecutionId) -> AppUpdate {
        let Some(status) = self.machine_port.as_ref().map(|p| p.status()) else { return AppUpdate::default() };
        let Some(record) = Self::hub_executions(&status).into_iter().find(|r| r.execution.id == execution) else {
            return AppUpdate::default();
        };
        self.hub.repeat = Some((execution, Arc::clone(&record.job)));
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    fn repeat_view(&self, execution: ExecutionId, job: &Arc<crate::production::PreparedJob>) -> RepeatView {
        let refusal = self.repeat_refusal(job);
        RepeatView { execution, job_name: job.name.clone(), plan: job.plan, changes: self.recipe_changes(job), refusal }
    }

    fn repeat_refusal(&self, job: &crate::production::PreparedJob) -> Option<RepeatRefusal> {
        match self.machine.as_ref() {
            Some(m) if *m.profile.revision() == job.profile => None,
            _ => Some(RepeatRefusal::ProfileChanged),
        }
    }

    /// Operations of the frozen project against those of the active document, matched by id.
    fn recipe_changes(&self, job: &crate::production::PreparedJob) -> Vec<RecipeChange> {
        let Some(frozen) = job.inputs.as_ref().map(|i| i.project.document.process().clone()) else {
            return Vec::new();
        };
        let Some(current) = self.active.and_then(|d| self.sessions.get(&d)).map(|s| s.doc.snapshot()) else {
            return Vec::new();
        };
        let current = current.process();
        let mut out = Vec::new();
        for old in frozen.operations() {
            match current.operations().find(|c| c.id() == old.id()) {
                None => out.push(change(old.name(), RecipeField::Removed, old.name(), "")),
                Some(now) => compare(old, now, &mut out),
            }
        }
        for now in current.operations() {
            if !frozen.operations().any(|o| o.id() == now.id()) {
                out.push(change(now.name(), RecipeField::Added, "", now.name()));
            }
        }
        out
    }

    pub(super) fn hub_confirm_repeat(&mut self, document: DocumentId) -> AppUpdate {
        let Some((_, job)) = self.hub.repeat.clone() else { return AppUpdate::default() };
        if self.repeat_refusal(&job).is_some() {
            return error(Some(document), MessageKey::RepeatRefused);
        }
        let approved = self.approved_plans.contains(&job.plan.hash());
        let Some(s) = self.sessions.get_mut(&document) else { return AppUpdate::default() };
        s.prepared = Some(Prepared { job, approved, placed_from: None, record: None });
        self.hub.repeat = None;
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    pub(super) fn hub_cancel_repeat(&mut self) -> AppUpdate {
        self.hub.repeat = None;
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    // ---- calibration proofs (W09) --------------------------------------------------------------------

    pub(super) fn hub_record_calibration(&mut self, input: ProofInput) -> AppUpdate {
        // The proofs of earlier runs are read first, so ids and the duplicate check see them.
        self.ensure_libraries();
        match self.new_proof(&input) {
            Ok(proof) => {
                self.hub.proofs.push(proof);
                self.hub.next_proof += 1;
                self.hub.problem = None;
                self.proofs_changed();
            }
            Err(problem) => self.hub.problem = Some(problem),
        }
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    pub(super) fn hub_remove_calibration(&mut self, id: u64) -> AppUpdate {
        let before = self.hub.proofs.len();
        self.hub.proofs.retain(|p| p.id != id);
        if self.hub.proofs.len() != before {
            self.proofs_changed();
        }
        self.hub.problem = None;
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    /// Plans compiled with the old calibration are not the current ones any more; frozen jobs keep
    /// their explicit snapshot.
    fn proofs_changed(&mut self) {
        self.refresh_calibration();
        self.flush_calibration();
    }

    /// The calibration of machine `machine`: per material the newest kerf proof, per speed the
    /// newest scan-offset proof (newest by date, then by id). Proofs of other machines and the
    /// pointer offset are not part of it.
    pub(in crate::controller) fn calibration_of(&self, machine: &str) -> burnburn_job::Calibration {
        use burnburn_job::{KerfProof, ProofRef, ScanOffsetProof};
        let newest = |a: &&Proof, b: &&Proof| (&a.date, a.id).cmp(&(&b.date, b.id));
        let of_machine = || self.hub.proofs.iter().filter(|p| p.machine == machine);
        let mut kerf: Vec<KerfProof> = Vec::new();
        for material in of_machine().filter(|p| p.kind == ProofKind::Kerf).map(|p| p.material.clone()) {
            if kerf.iter().any(|k| k.material == material) {
                continue;
            }
            let best = of_machine().filter(|p| p.kind == ProofKind::Kerf && p.material == material).max_by(newest);
            if let Some(p) = best {
                kerf.push(KerfProof {
                    material,
                    kerf_mm: p.value_mm,
                    proof: ProofRef { id: p.id, date: p.date.clone() },
                });
            }
        }
        let mut scan: Vec<ScanOffsetProof> = Vec::new();
        for speed in of_machine().filter(|p| p.kind == ProofKind::ScanOffset).filter_map(|p| p.speed_mm_s) {
            if scan.iter().any(|s| s.speed_mm_s == speed) {
                continue;
            }
            let best = of_machine()
                .filter(|p| p.kind == ProofKind::ScanOffset && p.speed_mm_s == Some(speed))
                .max_by(newest);
            if let Some(p) = best {
                scan.push(ScanOffsetProof {
                    speed_mm_s: speed,
                    offset_mm: p.value_mm,
                    proof: ProofRef { id: p.id, date: p.date.clone() },
                });
            }
        }
        burnburn_job::Calibration::new(kerf, scan).unwrap_or_default()
    }

    /// Makes the active profile carry what the proofs measured. A new measurement is a new
    /// revision of the profile, so every plan compiled before it is stale; a frozen job keeps the
    /// revision it was made for.
    pub(in crate::controller) fn refresh_calibration(&mut self) {
        let Some(machine) = self.machine.as_ref() else { return };
        let calibration = self.calibration_of(&machine.profile.revision().id);
        if calibration == *machine.profile.calibration() {
            return;
        }
        if let Ok(profile) = (*machine.profile).clone().with_calibration(calibration)
            && let Some(machine) = self.machine.as_mut()
        {
            machine.profile = Arc::new(profile);
        }
    }

    /// The proofs and the next id as the calibration file holds them.
    pub(super) fn calibration_file(&self) -> burnburn_io::calibration_library::CalibrationLibrary {
        burnburn_io::calibration_library::CalibrationLibrary {
            proofs: self.hub.proofs.iter().map(Proof::to_record).collect(),
            next_id: self.hub.next_proof + 1,
        }
    }

    /// The proofs the file holds join the ones recorded in this session (those get the next ids).
    /// Returns whether session proofs were added, so the file has to be written again.
    pub(super) fn proofs_loaded(&mut self, library: burnburn_io::calibration_library::CalibrationLibrary) -> bool {
        let session = std::mem::take(&mut self.hub.proofs);
        self.hub.proofs = library.proofs.iter().map(Proof::from_record).collect();
        self.hub.next_proof = library.next_id.saturating_sub(1);
        let joined = !session.is_empty();
        for mut p in session {
            self.hub.next_proof += 1;
            p.id = self.hub.next_proof;
            self.hub.proofs.push(p);
        }
        joined
    }

    /// The red-dot pointer offset (x, y in mm) of the active machine, from its newest pointer proof.
    pub fn pointer_offset_mm(&self) -> Option<(f64, f64)> {
        let machine = self.machine.as_ref()?.profile.revision().id.clone();
        self.hub
            .proofs
            .iter()
            .filter(|p| p.kind == ProofKind::PointerOffset && p.machine == machine)
            .max_by(|a, b| (&a.date, a.id).cmp(&(&b.date, b.id)))
            .and_then(|p| p.value_y_mm.map(|y| (p.value_mm, y)))
    }

    fn new_proof(&self, input: &ProofInput) -> Result<Proof, ProofProblem> {
        let machine: &MachineProfile = &self.machine.as_ref().ok_or(ProofProblem::NoMachine)?.profile;
        let text = |field, value: &str| {
            let v = value.trim();
            if v.is_empty() { Err(ProofProblem::Invalid(field)) } else { Ok(v.to_owned()) }
        };
        let number = |field, value: &str| {
            parse_number(value, &["mm"]).ok().filter(|v| v.is_finite()).ok_or(ProofProblem::Invalid(field))
        };
        let method = text(ProofField::Method, &input.method)?;
        let date = iso_date(&input.date).ok_or(ProofProblem::Invalid(ProofField::Date))?;
        let machine_id = machine.revision().id.clone();
        let (material, speed_mm_s, value_mm, value_y_mm) = match input.kind {
            ProofKind::Kerf => {
                let material = text(ProofField::Material, &input.material)?;
                let kerf = number(ProofField::Value, &input.value)?;
                if kerf < 0.0 {
                    return Err(ProofProblem::Invalid(ProofField::Value));
                }
                (material, None, kerf, None)
            }
            ProofKind::ScanOffset => {
                let speed = parse_number(&input.speed, &["mm/s"])
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .ok_or(ProofProblem::Invalid(ProofField::Speed))?;
                let offset = number(ProofField::Value, &input.value)?;
                let duplicate =
                    self.hub.proofs.iter().any(|p| {
                        p.kind == ProofKind::ScanOffset && p.machine == machine_id && p.speed_mm_s == Some(speed)
                    });
                if duplicate {
                    return Err(ProofProblem::DuplicateSpeed);
                }
                (input.material.trim().to_owned(), Some(speed), offset, None)
            }
            ProofKind::PointerOffset => {
                let x = number(ProofField::Value, &input.value)?;
                let y = number(ProofField::ValueY, &input.value_y)?;
                (input.material.trim().to_owned(), None, x, Some(y))
            }
        };
        Ok(Proof {
            id: self.hub.next_proof + 1,
            kind: input.kind,
            machine: machine_id,
            material,
            speed_mm_s,
            value_mm,
            value_y_mm,
            method,
            date,
        })
    }
}

impl Proof {
    fn to_record(&self) -> burnburn_io::calibration_library::ProofRecord {
        use burnburn_io::calibration_library::{ProofKindRecord, ProofRecord};
        ProofRecord {
            id: self.id,
            kind: match self.kind {
                ProofKind::Kerf => ProofKindRecord::Kerf,
                ProofKind::ScanOffset => ProofKindRecord::ScanOffset,
                ProofKind::PointerOffset => ProofKindRecord::PointerOffset,
            },
            machine: self.machine.clone(),
            material: self.material.clone(),
            speed_mm_s: self.speed_mm_s,
            value_mm: self.value_mm,
            value_y_mm: self.value_y_mm,
            method: self.method.clone(),
            date: self.date.clone(),
        }
    }

    fn from_record(r: &burnburn_io::calibration_library::ProofRecord) -> Proof {
        use burnburn_io::calibration_library::ProofKindRecord;
        Proof {
            id: r.id,
            kind: match r.kind {
                ProofKindRecord::Kerf => ProofKind::Kerf,
                ProofKindRecord::ScanOffset => ProofKind::ScanOffset,
                ProofKindRecord::PointerOffset => ProofKind::PointerOffset,
            },
            machine: r.machine.clone(),
            material: r.material.clone(),
            speed_mm_s: r.speed_mm_s,
            value_mm: r.value_mm,
            value_y_mm: r.value_y_mm,
            method: r.method.clone(),
            date: r.date.clone(),
        }
    }
}

fn proof_row(p: &Proof) -> ProofRow {
    ProofRow {
        id: p.id,
        kind: p.kind,
        machine: p.machine.clone(),
        material: p.material.clone(),
        speed_mm_s: p.speed_mm_s,
        value_mm: p.value_mm,
        value_y_mm: p.value_y_mm,
        method: p.method.clone(),
        date: p.date.clone(),
    }
}

/// `YYYY-MM-DD`, nothing else.
fn iso_date(text: &str) -> Option<String> {
    let t = text.trim();
    let bytes = t.as_bytes();
    let digits = |r: std::ops::Range<usize>| bytes.get(r).is_some_and(|b| b.iter().all(u8::is_ascii_digit));
    (bytes.len() == 10 && digits(0..4) && bytes[4] == b'-' && digits(5..7) && bytes[7] == b'-' && digits(8..10))
        .then(|| t.to_owned())
}

fn stage_of(state: &ExecutionState) -> QueueStage {
    match state {
        ExecutionState::VerifyingIdentity
        | ExecutionState::Reverifying
        | ExecutionState::Uploading { .. }
        | ExecutionState::UploadAcked => QueueStage::Uploading,
        ExecutionState::Stored => QueueStage::Stored,
        ExecutionState::StartRequested => QueueStage::StartRequested,
        ExecutionState::Running => QueueStage::Running,
        ExecutionState::Completed => QueueStage::Completed,
        ExecutionState::Failed(_) => QueueStage::Failed,
        ExecutionState::Cancelled => QueueStage::Cancelled,
        ExecutionState::Unknown(_) => QueueStage::Unknown,
    }
}

/// The upload of the current execution, while it is under way or if it ended before the job was
/// stored. After storage the machine panel takes over.
fn upload_status(status: &MachineStatus) -> Option<UploadStatusView> {
    let record = status.current.as_ref().filter(|_| !status.current_is_frame)?;
    let e = &record.execution;
    let acked = e.evidence.iter().filter(|x| matches!(x.evidence, Evidence::ChunkAcked { .. })).count();
    let stage = match &e.state {
        ExecutionState::VerifyingIdentity | ExecutionState::Reverifying => UploadStage::VerifyingIdentity,
        ExecutionState::Uploading { acked, total } => UploadStage::Sending { acked: *acked, total: *total },
        ExecutionState::UploadAcked => UploadStage::AckedNotConfirmed { chunks: acked },
        ExecutionState::Stored if !e.start_sent => UploadStage::StoredConfirmed,
        ExecutionState::Failed(reason) => UploadStage::Failed(match reason {
            FailReason::IdentityMismatch => UploadFailure::IdentityMismatch,
            FailReason::IdentityUnconfirmed => UploadFailure::IdentityUnconfirmed,
            FailReason::UploadRejected { index } => UploadFailure::Rejected { chunk: *index },
            FailReason::NotStored => UploadFailure::NotStored,
            FailReason::ControllerFault => UploadFailure::ControllerFault,
            FailReason::OperatorDidNotRun | FailReason::OperatorPartial => return None,
        }),
        ExecutionState::Cancelled
            if !e.evidence.iter().any(|x| matches!(x.evidence, Evidence::StorageConfirmed { .. })) =>
        {
            UploadStage::Stopped
        }
        _ => return None,
    };
    let stoppable = matches!(
        stage,
        UploadStage::VerifyingIdentity | UploadStage::Sending { .. } | UploadStage::AckedNotConfirmed { .. }
    );
    let stop = stoppable.then(|| crate::view_model::MachineButton {
        action: status.action_id(MachineAction::Cancel, ActionSubject::Execution(e.id)),
        enabled: !e.state.is_terminal(),
        seen: Some(e.state.clone()),
        label: MessageKey::StopUpload,
    });
    Some(UploadStatusView { execution: e.id, job_name: record.job.name.clone(), stage, stop })
}

/// The frozen plan's constant-velocity estimate, when the plan is at hand and the active profile
/// is the one it was frozen for.
fn estimate(record: &ExecutionRecord, active: Option<&Arc<MachineProfile>>) -> Option<f64> {
    let inputs = record.job.inputs.as_ref()?;
    let profile = active.filter(|p| *p.revision() == record.job.profile)?;
    Some(burnburn_job::preview(&inputs.plan, profile).estimated_seconds)
}

/// From the start acknowledgement to the controller reporting the end; both must be on record.
fn measured(record: &ExecutionRecord) -> Option<f64> {
    let evidence = &record.execution.evidence;
    let started = evidence.iter().find(|x| matches!(x.evidence, Evidence::StartAcked))?.at;
    let ended = evidence.iter().find(|x| matches!(x.evidence, Evidence::Status(ControllerStatus::Finished)))?.at;
    ended.offset().checked_sub(started.offset()).map(|d| d.as_secs_f64())
}

fn change(operation: &str, field: RecipeField, frozen: &str, current: &str) -> RecipeChange {
    RecipeChange { operation: operation.to_owned(), field, frozen: frozen.to_owned(), current: current.to_owned() }
}

fn number_text(v: f64) -> String {
    let text = format!("{v:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn mode_name(mode: &OperationMode) -> &'static str {
    match mode {
        OperationMode::Line { .. } => "line",
        OperationMode::Fill(_) => "fill",
        OperationMode::OffsetFill(_) => "offset-fill",
        OperationMode::Image(_) => "image",
        _ => "other",
    }
}

fn compare(old: &Operation, now: &Operation, out: &mut Vec<RecipeChange>) {
    let name = old.name();
    if old.name() != now.name() {
        out.push(change(name, RecipeField::Name, old.name(), now.name()));
    }
    if old.mode() != now.mode() {
        out.push(change(name, RecipeField::Mode, mode_name(&old.mode()), mode_name(&now.mode())));
    }
    let (a, b) = (old.settings(), now.settings());
    if a.speed.get_mm_per_s() != b.speed.get_mm_per_s() {
        out.push(change(
            name,
            RecipeField::Speed,
            &number_text(a.speed.get_mm_per_s()),
            &number_text(b.speed.get_mm_per_s()),
        ));
    }
    if a.power.min().get() != b.power.min().get() {
        out.push(change(
            name,
            RecipeField::PowerMin,
            &number_text(a.power.min().get()),
            &number_text(b.power.min().get()),
        ));
    }
    if a.power.max().get() != b.power.max().get() {
        out.push(change(
            name,
            RecipeField::PowerMax,
            &number_text(a.power.max().get()),
            &number_text(b.power.max().get()),
        ));
    }
    if a.passes.get() != b.passes.get() {
        out.push(change(name, RecipeField::Passes, &a.passes.get().to_string(), &b.passes.get().to_string()));
    }
    if a.air_assist != b.air_assist {
        out.push(change(name, RecipeField::AirAssist, &a.air_assist.to_string(), &b.air_assist.to_string()));
    }
    if old.output_enabled() != now.output_enabled() {
        out.push(change(
            name,
            RecipeField::Output,
            &old.output_enabled().to_string(),
            &now.output_enabled().to_string(),
        ));
    }
}
