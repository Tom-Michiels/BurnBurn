//! Compile, prepare and RD export of the current revision, the operations panel (Line/Fill), and the
//! CAM dialogs of the window: Optimization Settings (here), the test generators (`tests_gen`) and
//! Device Settings (`device`), plus the cut-order priority of the selected shapes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_cam::{CompileOptions, Placement, PreflightReport, Severity, StartPoint};
use burnburn_document::CommandPayload;
use burnburn_document::process::{
    CutPriority, FillSettings, Kerf, OperationDraft, OperationMode, OptimizationSettings, OrderKey, Passes,
    PowerPercent, PowerRange, ProcessCommand, ProcessSettings, ScanAxis, ScanDirection, Speed,
};
use burnburn_geometry::ProductionTolerance;
use burnburn_job::{JobPlan, PlanPreview};
use burnburn_types::{CancellationSource, DocumentId, Length, OperationId, RequestId, TaskId};

use crate::cam_dialogs::{
    CamDialog, DialogAction, DialogActionKind, DialogActionState, DialogError, DialogField, DialogNote, DialogRow,
    DialogValue, DialogViewModel,
};
use crate::cut_options::FieldState;
use crate::field::{format_mm, parse_length, parse_number};
use crate::intent::{FieldKey, OperationField, OperationKind};
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
    /// The production record the plan's variable text was resolved against (W3-UI-tekst).
    pub(super) record: Option<burnburn_generators::ProductionRecord>,
    /// The feeder the plan was made with (None: without one); another feeder in force makes it stale.
    pub(super) feeder: Option<burnburn_cam::FeederSettings>,
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
        let fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
        let clock = self.clock.as_ref().and_then(|c| c());
        self.with_session(document, |s| {
            // Variable text is resolved against one production record before it is compiled, so
            // preview, RD export and upload print the same text (E11).
            let (snapshot, bound) = match super::variable::bind(s, fonts.as_ref(), clock) {
                Ok(bound) => bound,
                Err(problem) => {
                    s.variable.set_problem(problem);
                    return error(Some(document), MessageKey::CompileFailed);
                }
            };
            s.variable.clear_problem();
            if let Some(record) = bound {
                s.variable.bind_task(id, record);
            }
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
            let input = match s.cam.feeder.for_compile() {
                Some(feeder) => {
                    s.cam.feeder.bind_task(id, feeder);
                    TaskInput::Fed { inner: Box::new(input), feeder }
                }
                None => input,
            };
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Compile, document, source),
                cancel: cancel.token(),
                input,
            };
            let update = submit(s, &tasks, request, cancel, document);
            if !s.pending.contains_key(&id) {
                s.variable.take_record(id);
                s.cam.feeder.take_task(id);
            }
            update
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
        stale: source.document_revision != s.doc.revision() || profile_changed || !s.cam.feeder.matches(c.feeder),
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

// ---------------------------------------------------------------------------------------------
// Cut order priority of the selected shapes (inspector field "Prioriteit")
// ---------------------------------------------------------------------------------------------

/// The inspector row of the cut priority: the common priority of the selection, empty when the
/// selected shapes differ.
pub(super) fn priority_field(s: &Session) -> Option<crate::view_model::FieldViewModel> {
    let snapshot = s.doc.snapshot();
    let ids = s.selection.ids();
    let first = snapshot.process().priority(*ids.first()?).get();
    let same = ids.iter().all(|id| snapshot.process().priority(*id).get() == first);
    Some(crate::view_model::FieldViewModel {
        key: FieldKey::Priority,
        label: MessageKey::Field(FieldKey::Priority),
        text: if same { first.to_string() } else { String::new() },
        unit: burnburn_types::Unit::Millimeter,
        editable: true,
        error: s.field_errors.get(&FieldKey::Priority).copied(),
    })
}

/// A commit of the priority field: one `SetPriority` per selected shape whose priority changes
/// (a single shape is one undo step). A value that is not a whole number in ±10 000 is refused
/// next to the field and sends nothing.
pub(super) fn commit_priority(
    s: &mut Session,
    document: DocumentId,
    text: &str,
    ids: &mut dyn FnMut() -> Option<RequestId>,
) -> AppUpdate {
    use crate::field::FieldError;
    let refuse = |s: &mut Session, why: FieldError| {
        s.field_errors.insert(FieldKey::Priority, MessageKey::InvalidLength(why));
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    };
    let value = match parse_number(text, &[]) {
        Ok(v) if v.fract() == 0.0 => v,
        Ok(_) => return refuse(s, FieldError::NotANumber),
        Err(e) => return refuse(s, e),
    };
    let priority = match i32::try_from(value as i64).ok().filter(|_| value.abs() <= f64::from(CutPriority::MAX)) {
        Some(v) => match CutPriority::new(v) {
            Ok(p) => p,
            Err(_) => return refuse(s, FieldError::NotFinite),
        },
        None => return refuse(s, FieldError::NotFinite),
    };
    s.field_errors.remove(&FieldKey::Priority);
    let snapshot = s.doc.snapshot();
    let targets: Vec<_> =
        s.selection.ids().iter().copied().filter(|g| snapshot.process().priority(*g) != priority).collect();
    let mut update = AppUpdate { view_changed: true, ..AppUpdate::default() };
    for geometry in targets {
        let edit = ProcessCommand::SetPriority { geometry, priority };
        let next = apply_edit(s, document, CommandPayload::Process(edit), ids());
        update.effects.extend(next.effects);
        update.scenes_changed.extend(next.scenes_changed);
    }
    update
}

// ---------------------------------------------------------------------------------------------
// CAM dialogs: state, open / set / action dispatch
// ---------------------------------------------------------------------------------------------

/// The open CAM dialogs of one window.
#[derive(Default)]
pub(super) struct CamState {
    pub(super) optimization: Option<OptimizationDraft>,
    pub(super) tests: super::tests_gen::TestsState,
    /// The Array dialog (AR02).
    pub(super) array: Option<super::array_dialog::ArrayDraft>,
    /// The Feeder dialog and the feeder in force (FE01); window state, never document data.
    pub(super) feeder: super::feeder_dialog::FeederState,
    /// The Preflight and Cutting order dialogs (C09, C06).
    pub(super) preflight: super::preflight_dialog::PreflightState,
    pub(super) order: super::order_dialog::OrderState,
    pub(super) sublayers: super::sublayers_dialog::SublayersState,
}

/// Every `Order by` level, in LightBurn's order.
const ORDER_KEYS: [OrderKey; 3] = [OrderKey::Layer, OrderKey::Group, OrderKey::Priority];
/// What the overlap tolerance shows while overlap removal has never been used (mm).
const DEFAULT_OVERLAP_TOLERANCE: &str = "0.1";

/// The draft of the Optimization Settings dialog.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct OptimizationDraft {
    /// All three levels: the ones in `Order by` first, in order, then the others (switched off).
    order: Vec<(OrderKey, bool)>,
    inner_first: bool,
    reduce_travel: bool,
    best_start: bool,
    best_direction: bool,
    hide_backlash: bool,
    remove_overlapping: bool,
    tolerance: String,
    errors: BTreeMap<DialogField, DialogError>,
}

impl OptimizationDraft {
    fn of(settings: &OptimizationSettings) -> Self {
        let mut order: Vec<(OrderKey, bool)> = settings.order_by().iter().map(|k| (*k, true)).collect();
        order.extend(ORDER_KEYS.iter().filter(|k| !settings.order_by().contains(k)).map(|k| (*k, false)));
        OptimizationDraft {
            order,
            inner_first: settings.inner_first(),
            reduce_travel: settings.reduce_travel(),
            best_start: settings.best_start(),
            best_direction: settings.best_direction(),
            hide_backlash: settings.hide_backlash(),
            remove_overlapping: settings.remove_overlapping_mm().is_some(),
            tolerance: settings.remove_overlapping_mm().map_or(DEFAULT_OVERLAP_TOLERANCE.to_owned(), format_mm),
            errors: BTreeMap::new(),
        }
    }

    /// The settings this draft describes, or the field that cannot be used.
    fn settings(&self) -> Result<OptimizationSettings, (DialogField, DialogError)> {
        let order: Vec<OrderKey> = self.order.iter().filter(|(_, on)| *on).map(|(k, _)| *k).collect();
        let tolerance = if self.remove_overlapping {
            let len =
                parse_length(&self.tolerance).map_err(|_| (DialogField::OverlapTolerance, DialogError::Invalid))?;
            Some(len)
        } else {
            None
        };
        OptimizationSettings::default()
            .with_order(order)
            .map_err(|_| (DialogField::OrderEnabled(OrderKey::Layer), DialogError::OutOfRange))?
            .with_inner_first(self.inner_first)
            .with_reduce_travel(self.reduce_travel)
            .with_best_start(self.best_start)
            .with_best_direction(self.best_direction)
            .with_hide_backlash(self.hide_backlash)
            .with_remove_overlapping(tolerance)
            .map_err(|_| (DialogField::OverlapTolerance, DialogError::OutOfRange))
    }

    fn rows(&self) -> Vec<DialogRow> {
        let row = |field: DialogField, value: DialogValue| DialogRow {
            field,
            value,
            state: FieldState::Editable,
            error: self.errors.get(&field).copied(),
            blocked_choices: Vec::new(),
            reorder: None,
            labels: Vec::new(),
        };
        let last = self.order.len().saturating_sub(1);
        let mut rows: Vec<DialogRow> = self
            .order
            .iter()
            .enumerate()
            .map(|(i, (key, on))| DialogRow {
                reorder: Some((i > 0, i < last)),
                ..row(DialogField::OrderEnabled(*key), DialogValue::Flag(*on))
            })
            .collect();
        for (field, on) in [
            (DialogField::InnerFirst, self.inner_first),
            (DialogField::ReduceTravel, self.reduce_travel),
            (DialogField::BestStart, self.best_start),
            (DialogField::BestDirection, self.best_direction),
            (DialogField::HideBacklash, self.hide_backlash),
            (DialogField::RemoveOverlapping, self.remove_overlapping),
        ] {
            rows.push(row(field, DialogValue::Flag(on)));
        }
        let mut tolerance = row(DialogField::OverlapTolerance, DialogValue::Text(self.tolerance.clone()));
        if !self.remove_overlapping {
            tolerance.state = FieldState::Disabled(None);
        }
        rows.push(tolerance);
        rows
    }
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

impl AppController {
    /// The dialog as a host draws it; `None` while it is not open in this window.
    pub fn cam_dialog(&self, document: DocumentId, dialog: CamDialog) -> Option<DialogViewModel> {
        let s = self.sessions.get(&document)?;
        match dialog {
            CamDialog::Optimization => {
                let draft = s.cam.optimization.as_ref()?;
                let snapshot = s.doc.snapshot();
                let priority_on = draft.order.iter().any(|(k, on)| *k == OrderKey::Priority && *on);
                let mut notes = Vec::new();
                if !priority_on && snapshot.process().priorities().next().is_some() {
                    notes.push(DialogNote::PriorityNeedsOrderLevel);
                }
                Some(DialogViewModel {
                    dialog,
                    rows: draft.rows(),
                    notes,
                    actions: vec![
                        DialogActionState { action: DialogActionKind::Apply, enabled: true, reason: None },
                        DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
                    ],
                    capabilities: Vec::new(),
                    result: None,
                    busy: false,
                    lines: Vec::new(),
                })
            }
            CamDialog::Device => self.device_view(),
            CamDialog::Array => super::array_dialog::view(s),
            CamDialog::Feeder => super::feeder_dialog::view(s),
            CamDialog::Preflight => super::preflight_dialog::view(s, self.machine.as_ref()),
            CamDialog::Order => super::order_dialog::view(s),
            CamDialog::Sublayers => super::sublayers_dialog::view(s),
            CamDialog::MaterialTest | CamDialog::IntervalTest | CamDialog::FocusTest => {
                super::tests_gen::view(s, dialog, self.machine.as_ref())
            }
        }
    }

    /// Opens a CAM dialog with a fresh draft: the document's settings, the active profile, or the
    /// defaults of a generator (an earlier test result is kept until the dialog is closed).
    pub(super) fn cam_open(&mut self, document: DocumentId, dialog: CamDialog) -> AppUpdate {
        if !self.sessions.contains_key(&document) {
            return AppUpdate::default();
        }
        match dialog {
            CamDialog::Optimization => self.with_session(document, |s| {
                s.cam.optimization = Some(OptimizationDraft::of(s.doc.snapshot().process().optimization()));
                changed()
            }),
            CamDialog::Device => self.device_open(),
            CamDialog::Array => self.with_session(document, super::array_dialog::open),
            CamDialog::Feeder => self.with_session(document, super::feeder_dialog::open),
            CamDialog::Preflight => self.with_session(document, super::preflight_dialog::open),
            CamDialog::Order => self.with_session(document, super::order_dialog::open),
            CamDialog::Sublayers => self.with_session(document, super::sublayers_dialog::open),
            CamDialog::MaterialTest | CamDialog::IntervalTest | CamDialog::FocusTest => {
                self.with_session(document, |s| super::tests_gen::open(s, dialog))
            }
        }
    }

    pub(super) fn cam_set(
        &mut self,
        document: DocumentId,
        dialog: CamDialog,
        field: DialogField,
        value: DialogValue,
    ) -> AppUpdate {
        match dialog {
            CamDialog::Optimization => self.with_session(document, |s| {
                let Some(draft) = s.cam.optimization.as_mut() else { return AppUpdate::default() };
                set_optimization_field(draft, field, &value);
                changed()
            }),
            CamDialog::Device => self.device_set(field, value),
            CamDialog::Array => self.with_session(document, |s| super::array_dialog::set(s, field, value)),
            CamDialog::Feeder => self.with_session(document, |s| super::feeder_dialog::set(s, field, value)),
            CamDialog::Preflight => self.with_session(document, |s| super::preflight_dialog::set(s, field, value)),
            CamDialog::Order => self.with_session(document, |s| super::order_dialog::set(s, field, value)),
            CamDialog::Sublayers => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| super::sublayers_dialog::set(s, document, field, value, request))
            }
            CamDialog::MaterialTest | CamDialog::IntervalTest | CamDialog::FocusTest => {
                self.with_session(document, |s| super::tests_gen::set(s, dialog, field, value))
            }
        }
    }

    pub(super) fn cam_action(&mut self, document: DocumentId, dialog: CamDialog, action: DialogAction) -> AppUpdate {
        match dialog {
            CamDialog::Optimization => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| optimization_action(s, document, action, request))
            }
            CamDialog::Device => self.device_action(document, action),
            CamDialog::Array => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| super::array_dialog::action(s, document, action, request))
            }
            CamDialog::Feeder => self.with_session(document, |s| super::feeder_dialog::action(s, document, action)),
            CamDialog::Preflight => {
                let machine = self.machine.clone();
                self.with_session(document, |s| super::preflight_dialog::action(s, document, action, machine.as_ref()))
            }
            CamDialog::Order => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| super::order_dialog::action(s, document, action, request))
            }
            CamDialog::Sublayers => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| super::sublayers_dialog::action(s, document, action, request))
            }
            CamDialog::MaterialTest | CamDialog::IntervalTest | CamDialog::FocusTest => {
                self.tests_action(document, dialog, action)
            }
        }
    }
}

fn set_optimization_field(draft: &mut OptimizationDraft, field: DialogField, value: &DialogValue) {
    draft.errors.remove(&field);
    match (field, value) {
        (DialogField::OrderEnabled(key), DialogValue::Flag(on)) => {
            if let Some(entry) = draft.order.iter_mut().find(|(k, _)| *k == key) {
                entry.1 = *on;
            }
        }
        (DialogField::InnerFirst, DialogValue::Flag(on)) => draft.inner_first = *on,
        (DialogField::ReduceTravel, DialogValue::Flag(on)) => draft.reduce_travel = *on,
        (DialogField::BestStart, DialogValue::Flag(on)) => draft.best_start = *on,
        (DialogField::BestDirection, DialogValue::Flag(on)) => draft.best_direction = *on,
        (DialogField::HideBacklash, DialogValue::Flag(on)) => draft.hide_backlash = *on,
        (DialogField::RemoveOverlapping, DialogValue::Flag(on)) => {
            draft.remove_overlapping = *on;
            draft.errors.remove(&DialogField::OverlapTolerance);
        }
        (DialogField::OverlapTolerance, DialogValue::Text(t)) => draft.tolerance = t.clone(),
        _ => {}
    }
}

fn optimization_action(
    s: &mut Session,
    document: DocumentId,
    action: DialogAction,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(draft) = s.cam.optimization.as_mut() else { return AppUpdate::default() };
    match action {
        DialogAction::MoveUp(key) | DialogAction::MoveDown(key) => {
            let up = matches!(action, DialogAction::MoveUp(_));
            if let Some(i) = draft.order.iter().position(|(k, _)| *k == key) {
                let j = if up { i.checked_sub(1) } else { Some(i + 1).filter(|j| *j < draft.order.len()) };
                if let Some(j) = j {
                    draft.order.swap(i, j);
                }
            }
            changed()
        }
        DialogAction::Close => {
            s.cam.optimization = None;
            changed()
        }
        DialogAction::Apply => {
            draft.errors.clear();
            let settings = match draft.settings() {
                Ok(settings) => settings,
                Err((field, why)) => {
                    draft.errors.insert(field, why);
                    return changed();
                }
            };
            let unchanged = settings == *s.doc.snapshot().process().optimization();
            if unchanged {
                s.cam.optimization = None;
                return changed();
            }
            let edit = ProcessCommand::SetOptimization { settings };
            let update = apply_edit(s, document, CommandPayload::Process(edit), request);
            if update.effects.iter().all(|e| !matches!(e, super::HostEffect::ShowError { .. })) {
                s.cam.optimization = None;
            }
            update
        }
        _ => AppUpdate::default(),
    }
}

/// What the host is told when writing RD failed: the feature the Ruida backend has no proven form
/// for, or the general failure.
pub(super) fn export_failure(error: &crate::task::TaskError) -> MessageKey {
    use crate::cam_dialogs::RdRefusal;
    use burnburn_ruida::LowerError;
    match error {
        crate::task::TaskError::Lower(LowerError::Unsupported { what, .. }) => {
            MessageKey::RdRefused(RdRefusal::of(what))
        }
        crate::task::TaskError::Lower(LowerError::TooManyParts) => MessageKey::RdRefused(RdRefusal::TooManyParts),
        crate::task::TaskError::Lower(LowerError::NegativeCoordinate { .. }) => {
            MessageKey::RdRefused(RdRefusal::NegativeCoordinate)
        }
        _ => MessageKey::ExportFailed,
    }
}
