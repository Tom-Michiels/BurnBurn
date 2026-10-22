//! The machine hub window as plain data (G5-machine-rest), without AppKit: what the widgets show
//! for a [`MachineHubViewModel`] and which intent each widget event is. Tested headlessly; the
//! AppKit window (`appkit::panels::hub`) only copies this into widgets and reads widgets into
//! [`HubEvent`]s.

use burnburn_presentation::{
    MachineHubViewModel, ProofField, ProofInput, ProofKind, ProofProblem, QueueStage, RecipeField, RepeatRefusal,
    UiIntent, UploadFailure, UploadStage,
};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::l10n_machine::{MachineText as T, fill, machine_text};
use crate::panels::cam_support_line;

/// The kinds a calibration proof can have, in the order of the kind pop-up.
pub const PROOF_KINDS: [ProofKind; 3] = [ProofKind::Kerf, ProofKind::ScanOffset, ProofKind::PointerOffset];

/// A repeat in preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatPanel {
    pub header: String,
    /// One line per difference; one line saying there is none.
    pub changes: Vec<String>,
    /// Why the repeat cannot be confirmed, in words.
    pub refusal: Option<String>,
    pub can_confirm: bool,
}

/// Everything the hub window shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HubPanel {
    /// Pop-up items of the known machines.
    pub devices: Vec<String>,
    pub active_device: Option<usize>,
    /// One line per feature, the same words as Device Settings.
    pub support: Vec<String>,
    pub upload: String,
    pub stop_enabled: bool,
    pub queue_counts: String,
    pub queue: Vec<String>,
    /// Pop-up items of the history, newest first.
    pub history: Vec<String>,
    pub repeat: Option<RepeatPanel>,
    /// Pop-up items of the recorded proofs.
    pub proofs: Vec<String>,
    /// Why the last proof was refused; empty when it was not.
    pub proof_problem: String,
}

/// The kind captions of the form's pop-up.
pub fn proof_kind_title(kind: ProofKind, locale: Locale) -> &'static str {
    machine_text(
        match kind {
            ProofKind::Kerf => T::KindKerf,
            ProofKind::ScanOffset => T::KindScanOffset,
            ProofKind::PointerOffset => T::KindPointerOffset,
        },
        locale,
    )
}

/// A number the way the user reads it: no trailing zeros, the locale's decimal sign, one decimal at
/// most for durations.
fn number(v: f64, decimals: usize, locale: Locale) -> String {
    let text = format!("{v:.decimals$}");
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_owned() } else { text };
    text.replace('.', &locale.decimal_sign().to_string())
}

fn stage_text(stage: QueueStage, locale: Locale) -> &'static str {
    machine_text(
        match stage {
            QueueStage::Uploading => T::StageUploading,
            QueueStage::Stored => T::StageStored,
            QueueStage::StartRequested => T::StageStartRequested,
            QueueStage::Running => T::StageRunning,
            QueueStage::Completed => T::StageCompleted,
            QueueStage::Failed => T::StageFailed,
            QueueStage::Cancelled => T::StageCancelled,
            QueueStage::Unknown => T::StageUnknown,
        },
        locale,
    )
}

fn upload_text(stage: UploadStage, locale: Locale) -> String {
    match stage {
        UploadStage::VerifyingIdentity => machine_text(T::UploadVerifying, locale).to_owned(),
        UploadStage::Sending { acked, total } => {
            fill(T::UploadSending, locale, &[&acked.to_string(), &total.to_string()])
        }
        UploadStage::AckedNotConfirmed { chunks } => fill(T::UploadAcked, locale, &[&chunks.to_string()]),
        UploadStage::StoredConfirmed => machine_text(T::UploadStored, locale).to_owned(),
        UploadStage::Stopped => machine_text(T::UploadStopped, locale).to_owned(),
        UploadStage::Failed(failure) => match failure {
            UploadFailure::IdentityMismatch => machine_text(T::FailIdentityMismatch, locale).to_owned(),
            UploadFailure::IdentityUnconfirmed => machine_text(T::FailIdentityUnconfirmed, locale).to_owned(),
            UploadFailure::Rejected { chunk } => fill(T::FailRejected, locale, &[&chunk.to_string()]),
            UploadFailure::NotStored => machine_text(T::FailNotStored, locale).to_owned(),
            UploadFailure::ControllerFault => machine_text(T::FailFault, locale).to_owned(),
        },
    }
}

fn bool_text(value: &str, locale: Locale) -> String {
    machine_text(if value == "true" { T::On } else { T::Off }, locale).to_owned()
}

fn change_line(change: &burnburn_presentation::RecipeChange, locale: Locale) -> String {
    let field = |t| machine_text(t, locale);
    let (name, frozen, current) = match change.field {
        RecipeField::Removed => return fill(T::ChangeRemoved, locale, &[&change.operation]),
        RecipeField::Added => return fill(T::ChangeAdded, locale, &[&change.operation]),
        RecipeField::Name => (field(T::FieldName), change.frozen.clone(), change.current.clone()),
        RecipeField::Mode => (field(T::FieldMode), change.frozen.clone(), change.current.clone()),
        RecipeField::Speed => (field(T::FieldSpeed), change.frozen.clone(), change.current.clone()),
        RecipeField::PowerMin => (field(T::FieldPowerMin), change.frozen.clone(), change.current.clone()),
        RecipeField::PowerMax => (field(T::FieldPowerMax), change.frozen.clone(), change.current.clone()),
        RecipeField::Passes => (field(T::FieldPasses), change.frozen.clone(), change.current.clone()),
        RecipeField::AirAssist => {
            (field(T::FieldAirAssist), bool_text(&change.frozen, locale), bool_text(&change.current, locale))
        }
        RecipeField::Output => {
            (field(T::FieldOutput), bool_text(&change.frozen, locale), bool_text(&change.current, locale))
        }
    };
    fill(T::ChangeLine, locale, &[&change.operation, name, &frozen, &current])
}

fn proof_problem_text(problem: ProofProblem, locale: Locale) -> &'static str {
    machine_text(
        match problem {
            ProofProblem::NoMachine => T::ProblemNoMachine,
            ProofProblem::Invalid(ProofField::Material) => T::ProblemMaterial,
            ProofProblem::Invalid(ProofField::Speed) => T::ProblemSpeed,
            ProofProblem::Invalid(ProofField::Value) => T::ProblemValue,
            ProofProblem::Invalid(ProofField::ValueY) => T::ProblemValueY,
            ProofProblem::Invalid(ProofField::Method) => T::ProblemMethod,
            ProofProblem::Invalid(ProofField::Date) => T::ProblemDate,
            ProofProblem::DuplicateSpeed => T::ProblemDuplicateSpeed,
        },
        locale,
    )
}

fn proof_line(row: &burnburn_presentation::ProofRow, locale: Locale) -> String {
    let value = number(row.value_mm, 3, locale);
    let measured = match row.kind {
        ProofKind::Kerf => format!("{value} mm"),
        ProofKind::ScanOffset => {
            format!("{} mm/s → {value} mm", number(row.speed_mm_s.unwrap_or(0.0), 3, locale))
        }
        ProofKind::PointerOffset => {
            format!("x {value} mm, y {} mm", number(row.value_y_mm.unwrap_or(0.0), 3, locale))
        }
    };
    let material = if row.material.is_empty() {
        machine_text(T::ProofNoMaterial, locale).to_owned()
    } else {
        row.material.clone()
    };
    fill(
        T::ProofLine,
        locale,
        &[proof_kind_title(row.kind, locale), &row.machine, &material, &measured, &row.method, &row.date],
    )
}

pub fn hub_panel(vm: &MachineHubViewModel, document_open: bool, locale: Locale) -> HubPanel {
    let devices = vm
        .devices
        .iter()
        .map(|d| {
            fill(
                T::DeviceItem,
                locale,
                &[&d.id, &d.revision.to_string(), &number(d.width_mm, 3, locale), &number(d.height_mm, 3, locale)],
            )
        })
        .collect();
    let queue = if vm.queue.rows.is_empty() {
        vec![machine_text(T::QueueNone, locale).to_owned()]
    } else {
        vm.queue
            .rows
            .iter()
            .map(|row| {
                let stage = stage_text(row.stage, locale);
                match &row.order {
                    None => fill(T::QueueRow, locale, &[&row.job_name, stage]),
                    Some((_, name)) => format!("{} (order {name})", fill(T::QueueRow, locale, &[&row.job_name, stage])),
                }
            })
            .collect()
    };
    let c = vm.queue.counts;
    let count = |n: usize| n.to_string();
    let queue_counts = fill(
        T::QueueCounts,
        locale,
        &[
            &count(c.uploading),
            &count(c.stored),
            &count(c.started),
            &count(c.completed),
            &count(c.unknown),
            &count(c.failed),
        ],
    );
    let history = vm
        .history
        .iter()
        .map(|e| {
            let estimate = e.estimated_s.map_or_else(
                || machine_text(T::NoEstimate, locale).to_owned(),
                |s| fill(T::Seconds, locale, &[&number(s, 1, locale)]),
            );
            let measured = e.measured_s.map_or_else(
                || machine_text(T::NotMeasured, locale).to_owned(),
                |s| fill(T::Seconds, locale, &[&number(s, 1, locale)]),
            );
            fill(
                T::HistoryRow,
                locale,
                &[
                    &e.job_name,
                    stage_text(e.stage, locale),
                    &e.profile,
                    &e.profile_revision.to_string(),
                    &e.rd_bytes.to_string(),
                    &estimate,
                    &measured,
                ],
            )
        })
        .collect();
    let repeat = vm.repeat.as_ref().map(|r| {
        let refusal = match (r.refusal, document_open) {
            (Some(RepeatRefusal::ProfileChanged), _) => Some(machine_text(T::RepeatProfileChanged, locale).to_owned()),
            (Some(RepeatRefusal::NoWindow), _) | (None, false) => {
                Some(machine_text(T::RepeatNeedsWindow, locale).to_owned())
            }
            (None, true) => None,
        };
        let changes = if r.changes.is_empty() {
            vec![machine_text(T::RepeatNoChanges, locale).to_owned()]
        } else {
            r.changes.iter().map(|c| change_line(c, locale)).collect()
        };
        crate::hub_panel::RepeatPanel {
            header: fill(T::RepeatHeader, locale, &[&r.job_name]),
            changes,
            can_confirm: refusal.is_none(),
            refusal,
        }
    });
    HubPanel {
        active_device: vm.devices.iter().position(|d| d.active),
        devices,
        support: vm.support.iter().map(|row| cam_support_line(row, locale)).collect(),
        upload: vm
            .upload
            .as_ref()
            .map_or_else(|| machine_text(T::UploadNone, locale).to_owned(), |u| upload_text(u.stage, locale)),
        stop_enabled: vm.upload.as_ref().and_then(|u| u.stop.as_ref()).is_some_and(|b| b.enabled),
        queue_counts,
        queue,
        history,
        repeat,
        proofs: vm.calibration.rows.iter().map(|row| proof_line(row, locale)).collect(),
        proof_problem: vm.calibration.problem.map_or_else(String::new, |p| proof_problem_text(p, locale).to_owned()),
    }
}

/// What the widgets report, as plain data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubEvent {
    DeviceChosen(usize),
    StopUpload,
    Repeat(usize),
    ConfirmRepeat,
    CancelRepeat,
    RecordProof(ProofForm),
    RemoveProof(usize),
}

/// The calibration form as typed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProofForm {
    pub kind: usize,
    pub material: String,
    pub speed: String,
    pub value: String,
    pub value_y: String,
    pub method: String,
    pub date: String,
}

/// The intent of a widget event; a stale index, a disabled control or a missing window sends nothing.
pub fn hub_intent(vm: &MachineHubViewModel, document: Option<DocumentId>, event: HubEvent) -> Option<UiIntent> {
    match event {
        HubEvent::DeviceChosen(i) => Some(UiIntent::ChooseMachineProfile { id: vm.devices.get(i)?.id.clone() }),
        HubEvent::StopUpload => {
            let stop = vm.upload.as_ref()?.stop.as_ref().filter(|b| b.enabled)?;
            Some(UiIntent::Machine { document, action: stop.action, seen: stop.seen.clone() })
        }
        HubEvent::Repeat(i) => Some(UiIntent::RepeatExecution { execution: vm.history.get(i)?.execution }),
        HubEvent::ConfirmRepeat => Some(UiIntent::ConfirmRepeat { document: document? }),
        HubEvent::CancelRepeat => Some(UiIntent::CancelRepeat),
        HubEvent::RecordProof(form) => Some(UiIntent::RecordCalibration {
            input: ProofInput {
                kind: *PROOF_KINDS.get(form.kind)?,
                material: form.material,
                speed: form.speed,
                value: form.value,
                value_y: form.value_y,
                method: form.method,
                date: form.date,
            },
        }),
        HubEvent::RemoveProof(i) => Some(UiIntent::RemoveCalibration { id: vm.calibration.rows.get(i)?.id }),
    }
}
