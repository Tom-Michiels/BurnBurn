//! Compile, prepare and RD export of the current revision, and the operations panel (Line/Fill).

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_cam::{CompileOptions, Placement, PreflightReport, Severity, StartPoint};
use burnburn_document::CommandPayload;
use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, ScanAxis, ScanDirection, Speed,
};
use burnburn_geometry::ProductionTolerance;
use burnburn_job::{JobPlan, PlanPreview};
use burnburn_types::{CancellationSource, DocumentId, Length, OperationId, RequestId, TaskId};

use crate::field::parse_number;
use crate::intent::{OperationField, OperationKind};
use crate::task::{TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::{MessageKey, PlanViewModel};

use super::machine::Machine;
use super::{
    AppController, AppUpdate, COMPILE_TOLERANCE_MM, DEFAULT_FILL_INTERVAL_MM, Session, apply_edit, error, layers,
    submit,
};

/// One compiled plan with what was derived from exactly that plan.
#[derive(Clone)]
pub(super) struct Compiled {
    pub(super) plan: Arc<JobPlan>,
    pub(super) preview: Arc<PlanPreview>,
    pub(super) report: PreflightReport,
    /// Where the laser window's setup put the plan; `None` for the plain compile.
    pub(super) placement: Option<crate::laser::AppliedPlacement>,
    /// Setup the plan was compiled with (0: plain). A different current key makes it stale.
    pub(super) setup_key: u64,
}

impl AppController {
    /// Compiles the current revision for the active machine (B12). Revision-checked: a plan for
    /// an older revision is never shown as the current one.
    pub(super) fn submit_compile(&mut self, document: DocumentId) -> AppUpdate {
        let Some(machine) = self.machine.clone() else { return error(Some(document), MessageKey::NoMachineProfile) };
        let Some(options) = compile_options() else { return error(Some(document), MessageKey::CompileFailed) };
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let laser = match self.laser_compile_input(document) {
            Ok(laser) => laser,
            Err(why) => {
                self.laser.last_refusal = Some(why);
                return error(Some(document), MessageKey::LaserRefused(why));
            }
        };
        // An active Print and Cut registration moves what is cut (never the document).
        let registration = match self.registration_for_compile(document) {
            Ok(m) => m,
            Err(message) => return error(Some(document), message),
        };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let source = snapshot.revision();
            let snapshot = match registration {
                Some(m) => Arc::new(snapshot.transformed(&m)),
                None => snapshot,
            };
            let cancel = CancellationSource::new();
            let input = match laser {
                None => TaskInput::Compile { snapshot, profile: machine.profile, options, assets: s.assets.clone() },
                Some((laser, setup_key)) => TaskInput::CompilePlaced {
                    snapshot,
                    profile: machine.profile,
                    options,
                    assets: s.assets.clone(),
                    laser: Arc::new(laser),
                    setup_key,
                },
            };
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Compile, document, source),
                cancel: cancel.token(),
                input,
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// C1: freezes the current plan — fresh for this revision and profile, without preflight
    /// errors — with its RD bytes. The job never follows later edits.
    pub(super) fn submit_prepare(&mut self, document: DocumentId) -> AppUpdate {
        let Some(machine) = self.machine.clone() else { return error(Some(document), MessageKey::NoMachineProfile) };
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::PrepareFailed) };
        if self.placement_stale(document) {
            return error(Some(document), MessageKey::LaserRefused(crate::laser::LaserWhy::PlacementStale));
        }
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            if !preparable(s, Some(&machine)) {
                return error(Some(document), MessageKey::PrepareFailed);
            }
            let Some(compiled) = s.compiled.as_ref() else { return AppUpdate::default() };
            let plan = Arc::clone(&compiled.plan);
            let source = plan.source().document_revision;
            s.preparing = Some(Arc::clone(&plan));
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::PrepareJob, document, source),
                cancel: cancel.token(),
                input: TaskInput::PrepareJob { plan, ruida: machine.ruida },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// Writes exactly the current plan (the one the preview shows) as RD; the document is untouched.
    pub(super) fn export_rd(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let Some(machine) = self.machine.clone() else { return error(Some(document), MessageKey::NoMachineProfile) };
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ExportFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let Some(compiled) = s.compiled.as_ref() else { return AppUpdate::default() };
            let plan = Arc::clone(&compiled.plan);
            let source = plan.source().document_revision;
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::ExportRd, document, source),
                cancel: cancel.token(),
                input: TaskInput::ExportRd { plan, ruida: machine.ruida, path, fs },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }
}

pub(super) fn compile_options() -> Option<CompileOptions> {
    let tolerance = ProductionTolerance::new(Length::from_mm(COMPILE_TOLERANCE_MM).ok()?).ok()?;
    Some(CompileOptions { tolerance, placement: Placement::Absolute, start: StartPoint::MachineOrigin })
}

pub(super) fn parse_speed(text: &str) -> Result<Speed, MessageKey> {
    let invalid = MessageKey::InvalidNumber(OperationField::Speed);
    parse_number(text, &["mm/s"]).ok().and_then(|v| Speed::mm_per_s(v).ok()).ok_or(invalid)
}

pub(super) fn parse_power(text: &str) -> Result<PowerRange, MessageKey> {
    let invalid = MessageKey::InvalidNumber(OperationField::Power);
    let p = parse_number(text, &["%"]).ok().and_then(|v| PowerPercent::new(v).ok()).ok_or(invalid)?;
    PowerRange::new(p, p).map_err(|_| invalid)
}

pub(super) fn refused(s: &mut Session, key: MessageKey) -> AppUpdate {
    s.operation_error = Some(key);
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

/// Operations panel: a new Line or Fill operation for the selection (one command).
pub(super) fn add_operation_text(
    s: &mut Session,
    document: DocumentId,
    name: String,
    kind: OperationKind,
    speed: &str,
    power: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let speed = match parse_speed(speed) {
        Ok(v) => v,
        Err(k) => return refused(s, k),
    };
    let power = match parse_power(power) {
        Ok(v) => v,
        Err(k) => return refused(s, k),
    };
    let mode = match kind {
        OperationKind::Line => OperationMode::Line { kerf: Kerf::NONE },
        OperationKind::Fill => {
            let fill = Length::from_mm(DEFAULT_FILL_INTERVAL_MM).ok().zip(Length::from_mm(0.0).ok()).and_then(
                |(interval, overscan)| {
                    FillSettings::new(interval, ScanAxis::X, ScanDirection::Bidirectional, overscan).ok()
                },
            );
            match fill {
                Some(f) => OperationMode::Fill(f),
                None => return refused(s, MessageKey::CommandRejected),
            }
        }
        _ => return refused(s, MessageKey::CommandRejected),
    };
    let Ok(passes) = Passes::new(1) else { return refused(s, MessageKey::CommandRejected) };
    let draft = OperationDraft {
        name,
        mode,
        settings: ProcessSettings { speed, power, passes, air_assist: true },
        output_enabled: true,
    };
    s.operation_error = None;
    let targets = s.selection.ids().to_vec();
    apply_edit(s, document, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets }), request)
}

/// Operations panel: one field of an operation (one `UpdateOperation`; unchanged = nothing).
pub(super) fn set_operation_field(
    s: &mut Session,
    document: DocumentId,
    operation: OperationId,
    field: OperationField,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(op) = snapshot.process().operation(operation) else { return AppUpdate::default() };
    let mut draft = op.draft();
    let changed = match field {
        OperationField::Name => {
            let name = text.trim();
            let changed = !name.is_empty() && name != draft.name;
            if changed {
                draft.name = name.to_owned();
            }
            Ok(changed)
        }
        OperationField::Speed => parse_speed(text).map(|v| {
            let changed = v != draft.settings.speed;
            draft.settings.speed = v;
            changed
        }),
        OperationField::Power => parse_power(text).map(|v| {
            let changed = v != draft.settings.power;
            draft.settings.power = v;
            changed
        }),
        OperationField::PowerMin | OperationField::PowerMax => {
            let min = field == OperationField::PowerMin;
            layers::parse_percent(text, field).and_then(|p| {
                let (lo, hi) = if min { (p, draft.settings.power.max()) } else { (draft.settings.power.min(), p) };
                let range = PowerRange::new(lo, hi).map_err(|_| MessageKey::InvalidNumber(field))?;
                let changed = range != draft.settings.power;
                draft.settings.power = range;
                Ok(changed)
            })
        }
        OperationField::Passes => layers::parse_passes(text).map(|v| {
            let changed = v != draft.settings.passes;
            draft.settings.passes = v;
            changed
        }),
        OperationField::Kerf => match draft.mode {
            OperationMode::Line { kerf } => layers::parse_kerf(text).map(|v| {
                draft.mode = OperationMode::Line { kerf: v };
                v != kerf
            }),
            _ => Ok(false),
        },
    };
    match changed {
        Err(k) => refused(s, k),
        Ok(false) => {
            s.operation_error = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
        Ok(true) => {
            s.operation_error = None;
            let edit = ProcessCommand::UpdateOperation { operation, draft };
            apply_edit(s, document, CommandPayload::Process(edit), request)
        }
    }
}

pub(super) fn plan_view(c: &Compiled, s: &Session, machine: Option<&Machine>) -> PlanViewModel {
    let source = c.plan.source();
    let profile_changed = machine.is_none_or(|m| *m.profile.revision() != source.machine_profile);
    let errors = c.report.findings.iter().filter(|f| f.severity() == Severity::Error).count();
    PlanViewModel {
        id: c.plan.id(),
        revision: source.document_revision,
        stale: source.document_revision != s.doc.revision() || profile_changed,
        preview: Arc::clone(&c.preview),
        findings: c.report.findings.clone(),
        errors,
        warnings: c.report.findings.len() - errors,
    }
}

/// A plan can be frozen when it is fresh for this revision and profile and has no errors.
pub(super) fn preparable(s: &Session, machine: Option<&Machine>) -> bool {
    s.compiled.as_ref().is_some_and(|c| {
        let view = plan_view(c, s, machine);
        !view.stale && view.errors == 0
    })
}

impl AppController {
    /// Operations panel: a new operation from a ready draft for the selection (one command).
    pub(super) fn add_operation(&mut self, document: DocumentId, draft: OperationDraft) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let targets = s.selection.ids().to_vec();
            let edit = ProcessCommand::AddOperation { draft, targets };
            apply_edit(s, document, CommandPayload::Process(edit), request)
        })
    }
}
