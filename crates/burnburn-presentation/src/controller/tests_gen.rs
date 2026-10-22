//! Material Test, Interval Test and Focus Test (W3-UI-cam, W2-L3): the dialogs build a private test
//! job beside the document and compile it with the ordinary compiler as a cancellable background
//! task. The document, its revision and its undo history are never touched; the result (plan,
//! preview, preflight) lives in the dialog and is written as RD only on request.
//!
//! A job is valid for the draft that made it: editing a field cancels the job in flight and drops
//! the result; only the newest job's outcome is accepted (task identity); a changed machine
//! profile marks the result stale.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_cam::test_job::{
    FocusTestSpec, IntervalTestSpec, MaterialTestSpec, TestAxis, TestBorder, TestCellMode, TestError, TestLabels,
    TestParameter, generate_focus_test, generate_interval_test, generate_material_test,
};
use burnburn_cam::{PreflightReport, Severity};
use burnburn_document::process::{Passes, PowerPercent, PowerRange, ProcessSettings, Speed};
use burnburn_job::{Feature, JobPlan, PlanPreview, Support};
use burnburn_types::{CancellationSource, DocumentId, Length, TaskId};

use crate::cam_dialogs::{
    CamDialog, DialogAction, DialogActionKind, DialogActionState, DialogChoice, DialogError, DialogField, DialogNote,
    DialogRow, DialogValue, DialogViewModel, TestResultView,
};
use crate::cut_options::FieldState;
use crate::field::{parse_length, parse_number, parse_position};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

use super::machine::Machine;
use super::process::{compile_options, export_failure};
use super::{AppController, AppUpdate, Session, error, submit};

type Errors = BTreeMap<DialogField, DialogError>;

/// The text and choices of one generator's fields.
struct TestDraft {
    values: BTreeMap<DialogField, DialogValue>,
    errors: Errors,
}

/// A generated test with everything derived from exactly that plan.
struct TestResult {
    plan: Arc<JobPlan>,
    preview: Arc<PlanPreview>,
    report: PreflightReport,
    cells: usize,
    saved_to: Option<PathBuf>,
}

/// The job a dialog is waiting for: compiling a test, or writing its RD.
struct InFlight {
    id: TaskId,
    cells: usize,
}

/// The drafts, results and running jobs of the three generators of one window.
#[derive(Default)]
pub(super) struct TestsState {
    drafts: BTreeMap<CamDialog, TestDraft>,
    results: BTreeMap<CamDialog, TestResult>,
    /// What the dialog says after the last action (failed, saved, refused).
    notes: BTreeMap<CamDialog, DialogNote>,
    inflight: BTreeMap<CamDialog, InFlight>,
}

fn text(s: &str) -> DialogValue {
    DialogValue::Text(s.to_owned())
}

fn flag_value(on: bool) -> DialogValue {
    DialogValue::Flag(on)
}

fn choice_value(field: DialogField, which: DialogChoice) -> DialogValue {
    DialogValue::Choice(field.choices().iter().position(|c| *c == which).unwrap_or(0))
}

impl TestDraft {
    /// LightBurn-like starting values: a 5 × 5 speed/power grid of 10 mm cells, a 5-step interval
    /// series, nine focus lines.
    fn defaults(dialog: CamDialog) -> Self {
        use DialogField as F;
        let mut v: BTreeMap<DialogField, DialogValue> = BTreeMap::new();
        let mut put = |f: DialogField, value: DialogValue| {
            v.insert(f, value);
        };
        put(F::BaseSpeed, text("20"));
        put(F::BasePower, text("50"));
        put(F::BasePasses, text("1"));
        put(F::LabelsEnabled, flag_value(true));
        put(F::LabelHeight, text("3"));
        match dialog {
            CamDialog::MaterialTest => {
                put(F::CellMode, choice_value(F::CellMode, DialogChoice::CellLine));
                put(F::ColumnParameter, choice_value(F::ColumnParameter, DialogChoice::ParamSpeed));
                put(F::ColumnMin, text("10"));
                put(F::ColumnMax, text("50"));
                put(F::ColumnCount, text("5"));
                put(F::RowsEnabled, flag_value(true));
                put(F::RowParameter, choice_value(F::RowParameter, DialogChoice::ParamPower));
                put(F::RowMin, text("20"));
                put(F::RowMax, text("100"));
                put(F::RowCount, text("5"));
                put(F::FillInterval, text("0.1"));
                put(F::FillOverscan, text("0"));
                put(F::CellWidth, text("10"));
                put(F::CellHeight, text("10"));
                put(F::CellGap, text("2"));
                put(F::BorderEnabled, flag_value(false));
                put(F::BorderMargin, text("3"));
            }
            CamDialog::IntervalTest => {
                put(F::IntervalMin, text("0.05"));
                put(F::IntervalMax, text("0.25"));
                put(F::IntervalCount, text("5"));
                put(F::FillOverscan, text("0"));
                put(F::CellWidth, text("15"));
                put(F::CellHeight, text("10"));
                put(F::CellGap, text("2"));
                put(F::BorderEnabled, flag_value(false));
                put(F::BorderMargin, text("3"));
            }
            _ => {
                put(F::FocusStart, text("-2"));
                put(F::FocusStep, text("0.5"));
                put(F::FocusCount, text("9"));
                put(F::LineLength, text("20"));
                put(F::LineSpacing, text("3"));
            }
        }
        TestDraft { values: v, errors: Errors::new() }
    }

    fn text(&self, f: DialogField) -> &str {
        self.values.get(&f).map_or("", DialogValue::text)
    }

    fn flag(&self, f: DialogField) -> bool {
        self.values.get(&f).is_some_and(DialogValue::flag)
    }

    fn choice(&self, f: DialogField) -> Option<DialogChoice> {
        self.values.get(&f).and_then(|v| v.choice(f))
    }
}

/// Fields shown by each generator, top to bottom.
fn fields_of(dialog: CamDialog) -> &'static [DialogField] {
    use DialogField as F;
    match dialog {
        CamDialog::MaterialTest => &[
            F::CellMode,
            F::ColumnParameter,
            F::ColumnMin,
            F::ColumnMax,
            F::ColumnCount,
            F::RowsEnabled,
            F::RowParameter,
            F::RowMin,
            F::RowMax,
            F::RowCount,
            F::BaseSpeed,
            F::BasePower,
            F::BasePasses,
            F::FillInterval,
            F::FillOverscan,
            F::CellWidth,
            F::CellHeight,
            F::CellGap,
            F::LabelsEnabled,
            F::LabelHeight,
            F::BorderEnabled,
            F::BorderMargin,
        ],
        CamDialog::IntervalTest => &[
            F::IntervalMin,
            F::IntervalMax,
            F::IntervalCount,
            F::FillOverscan,
            F::BaseSpeed,
            F::BasePower,
            F::BasePasses,
            F::CellWidth,
            F::CellHeight,
            F::CellGap,
            F::LabelsEnabled,
            F::LabelHeight,
            F::BorderEnabled,
            F::BorderMargin,
        ],
        _ => &[
            F::FocusStart,
            F::FocusStep,
            F::FocusCount,
            F::LineLength,
            F::LineSpacing,
            F::BaseSpeed,
            F::BasePower,
            F::BasePasses,
            F::LabelsEnabled,
            F::LabelHeight,
        ],
    }
}

fn has_z(machine: Option<&Machine>) -> bool {
    machine.is_some_and(|m| m.profile.capabilities().support(Feature::ZStep) == Support::Supported)
}

// ---------------------------------------------------------------------------------------------
// Parsing a draft into a spec
// ---------------------------------------------------------------------------------------------

fn number(d: &TestDraft, f: DialogField, suffixes: &[&str], errors: &mut Errors) -> Option<f64> {
    match parse_number(d.text(f), suffixes) {
        Ok(v) if v.is_finite() => Some(v),
        _ => {
            errors.insert(f, DialogError::Invalid);
            None
        }
    }
}

/// A size: not a number is `Invalid`, zero or less is `Size`.
fn size(d: &TestDraft, f: DialogField, errors: &mut Errors) -> Option<Length> {
    match parse_number(d.text(f), &["mm"]) {
        Ok(v) if v.is_finite() && v > 0.0 => Length::from_mm(v).ok().or_else(|| {
            errors.insert(f, DialogError::OutOfRange);
            None
        }),
        Ok(_) => {
            errors.insert(f, DialogError::Size);
            None
        }
        Err(_) => {
            errors.insert(f, DialogError::Invalid);
            None
        }
    }
}

/// An overscan: zero or more.
fn overscan(d: &TestDraft, f: DialogField, errors: &mut Errors) -> Option<Length> {
    match parse_position(d.text(f)) {
        Ok(l) if l.mm() >= 0.0 => Some(l),
        _ => {
            errors.insert(f, DialogError::Invalid);
            None
        }
    }
}

/// `0,1 mm` or `254 lpi` (lines per inch).
fn interval(d: &TestDraft, f: DialogField, errors: &mut Errors) -> Option<Length> {
    let lower = d.text(f).trim().to_ascii_lowercase();
    let mm = match lower.strip_suffix("lpi").or_else(|| lower.strip_suffix("dpi")) {
        Some(n) => parse_number(n, &[]).ok().filter(|v| *v > 0.0 && v.is_finite()).map(|lpi| 25.4 / lpi),
        None => parse_length(&lower).ok().map(|l| l.mm()),
    };
    match mm.and_then(|v| Length::from_mm(v).ok()) {
        Some(l) => Some(l),
        None => {
            errors.insert(f, DialogError::Invalid);
            None
        }
    }
}

fn whole(d: &TestDraft, f: DialogField, errors: &mut Errors) -> Option<u32> {
    let v = number(d, f, &[], errors)?;
    if v.fract() != 0.0 {
        errors.insert(f, DialogError::NotWhole);
        return None;
    }
    if !(0.0..=f64::from(u32::MAX)).contains(&v) {
        errors.insert(f, DialogError::Count);
        return None;
    }
    Some(v as u32)
}

fn base(d: &TestDraft, errors: &mut Errors) -> Option<ProcessSettings> {
    let speed = number(d, DialogField::BaseSpeed, &["mm/s"], errors).and_then(|v| match Speed::mm_per_s(v) {
        Ok(s) => Some(s),
        Err(_) => {
            errors
                .insert(DialogField::BaseSpeed, if v <= 0.0 { DialogError::Invalid } else { DialogError::OutOfRange });
            None
        }
    });
    let power = number(d, DialogField::BasePower, &["%"], errors).and_then(|v| {
        let p = PowerPercent::new(v).ok().and_then(|p| PowerRange::new(p, p).ok());
        if p.is_none() {
            errors.insert(DialogField::BasePower, DialogError::OutOfRange);
        }
        p
    });
    let passes = number(d, DialogField::BasePasses, &[], errors).and_then(|v| {
        let n = (v.fract() == 0.0 && v >= 0.0 && v <= f64::from(u32::MAX)).then_some(v as u32);
        match n.map(Passes::new) {
            Some(Ok(p)) => Some(p),
            _ => {
                errors.insert(
                    DialogField::BasePasses,
                    if v.fract() == 0.0 { DialogError::OutOfRange } else { DialogError::NotWhole },
                );
                None
            }
        }
    });
    Some(ProcessSettings { speed: speed?, power: power?, passes: passes?, air_assist: true })
}

fn labels(d: &TestDraft, settings: ProcessSettings, errors: &mut Errors) -> Option<Option<TestLabels>> {
    if !d.flag(DialogField::LabelsEnabled) {
        return Some(None);
    }
    Some(Some(TestLabels { height: size(d, DialogField::LabelHeight, errors)?, settings }))
}

fn border(d: &TestDraft, settings: ProcessSettings, errors: &mut Errors) -> Option<Option<TestBorder>> {
    if !d.flag(DialogField::BorderEnabled) {
        return Some(None);
    }
    Some(Some(TestBorder { margin: size(d, DialogField::BorderMargin, errors)?, settings }))
}

fn parameter(c: Option<DialogChoice>) -> TestParameter {
    match c {
        Some(DialogChoice::ParamPower) => TestParameter::Power,
        Some(DialogChoice::ParamPasses) => TestParameter::Passes,
        Some(DialogChoice::ParamInterval) => TestParameter::Interval,
        Some(DialogChoice::ParamZOffset) => TestParameter::ZOffset,
        _ => TestParameter::Speed,
    }
}

fn suffixes_of(p: TestParameter) -> &'static [&'static str] {
    match p {
        TestParameter::Speed => &["mm/s"],
        TestParameter::Power => &["%"],
        TestParameter::Passes => &[],
        _ => &["mm"],
    }
}

fn axis(d: &TestDraft, fields: [DialogField; 4], errors: &mut Errors) -> Option<TestAxis> {
    let [param, min, max, count] = fields;
    let parameter = parameter(d.choice(param));
    let min = number(d, min, suffixes_of(parameter), errors);
    let max = number(d, max, suffixes_of(parameter), errors);
    let count = whole(d, count, errors);
    Some(TestAxis { parameter, min: min?, max: max?, count: count? })
}

const COLUMNS: [DialogField; 4] =
    [DialogField::ColumnParameter, DialogField::ColumnMin, DialogField::ColumnMax, DialogField::ColumnCount];
const ROWS: [DialogField; 4] =
    [DialogField::RowParameter, DialogField::RowMin, DialogField::RowMax, DialogField::RowCount];

fn material_spec(d: &TestDraft, errors: &mut Errors) -> Option<MaterialTestSpec> {
    let fill = d.choice(DialogField::CellMode) == Some(DialogChoice::CellFill);
    let columns = axis(d, COLUMNS, errors);
    let rows = if d.flag(DialogField::RowsEnabled) { Some(axis(d, ROWS, errors)) } else { None };
    let mode = if fill {
        let interval = interval(d, DialogField::FillInterval, errors);
        let overscan = overscan(d, DialogField::FillOverscan, errors);
        Some(TestCellMode::Fill { interval: interval?, overscan: overscan? })
    } else {
        Some(TestCellMode::Line)
    };
    let settings = base(d, errors);
    let (w, h, gap) = (
        size(d, DialogField::CellWidth, errors),
        size(d, DialogField::CellHeight, errors),
        size(d, DialogField::CellGap, errors),
    );
    let (labels, border) = match settings {
        Some(s) => (labels(d, s, errors), border(d, s, errors)),
        None => (None, None),
    };
    Some(MaterialTestSpec {
        mode: mode?,
        columns: columns?,
        rows: match rows {
            Some(r) => Some(r?),
            None => None,
        },
        base: settings?,
        cell_width: w?,
        cell_height: h?,
        gap: gap?,
        labels: labels?,
        border: border?,
    })
}

fn interval_spec(d: &TestDraft, errors: &mut Errors) -> Option<IntervalTestSpec> {
    let min = interval(d, DialogField::IntervalMin, errors);
    let max = interval(d, DialogField::IntervalMax, errors);
    let count = whole(d, DialogField::IntervalCount, errors);
    let overscan = overscan(d, DialogField::FillOverscan, errors);
    let settings = base(d, errors);
    let (w, h, gap) = (
        size(d, DialogField::CellWidth, errors),
        size(d, DialogField::CellHeight, errors),
        size(d, DialogField::CellGap, errors),
    );
    let (labels, border) = match settings {
        Some(s) => (labels(d, s, errors), border(d, s, errors)),
        None => (None, None),
    };
    Some(IntervalTestSpec {
        min_interval: min?,
        max_interval: max?,
        count: count?,
        overscan: overscan?,
        base: settings?,
        cell_width: w?,
        cell_height: h?,
        gap: gap?,
        labels: labels?,
        border: border?,
    })
}

fn focus_spec(d: &TestDraft, errors: &mut Errors) -> Option<FocusTestSpec> {
    let start = number(d, DialogField::FocusStart, &["mm"], errors);
    let step = number(d, DialogField::FocusStep, &["mm"], errors);
    let count = whole(d, DialogField::FocusCount, errors);
    let length = size(d, DialogField::LineLength, errors);
    let spacing = size(d, DialogField::LineSpacing, errors);
    let settings = base(d, errors);
    let labels = settings.and_then(|s| labels(d, s, errors));
    Some(FocusTestSpec {
        z_start: start?,
        z_step: step?,
        count: count?,
        line_length: length?,
        spacing: spacing?,
        base: settings?,
        labels: labels?,
    })
}

/// Which fields a generator's `TestError` is about.
fn error_fields(dialog: CamDialog, e: &TestError, d: &TestDraft) -> Vec<(DialogField, DialogError)> {
    use DialogField as F;
    let axis_fields = |axis: &str| -> [DialogField; 3] {
        match (dialog, axis) {
            (CamDialog::IntervalTest, _) => [F::IntervalMin, F::IntervalMax, F::IntervalCount],
            (CamDialog::FocusTest, _) => [F::FocusStart, F::FocusStep, F::FocusCount],
            (_, "rows") => [F::RowMin, F::RowMax, F::RowCount],
            _ => [F::ColumnMin, F::ColumnMax, F::ColumnCount],
        }
    };
    match e {
        TestError::Count { axis } => vec![(axis_fields(axis)[2], DialogError::Count)],
        TestError::Range { axis, .. } => {
            let [min, max, _] = axis_fields(axis);
            vec![(min, DialogError::OutOfRange), (max, DialogError::OutOfRange)]
        }
        TestError::NotWhole { axis, .. } => {
            let [min, max, _] = axis_fields(axis);
            vec![(min, DialogError::NotWhole), (max, DialogError::NotWhole)]
        }
        TestError::ParameterNotForMode { .. } => {
            let on_columns = parameter(d.choice(F::ColumnParameter)) == TestParameter::Interval;
            vec![(if on_columns { F::ColumnParameter } else { F::RowParameter }, DialogError::ParameterNotForMode)]
        }
        TestError::SameParameter(_) => vec![(F::RowParameter, DialogError::SameParameter)],
        TestError::Size { field } => {
            let f = match *field {
                "cell width" => F::CellWidth,
                "cell height" => F::CellHeight,
                "gap" => F::CellGap,
                "label height" => F::LabelHeight,
                "border margin" => F::BorderMargin,
                "line length" => F::LineLength,
                "spacing" => F::LineSpacing,
                "minimum interval" => F::IntervalMin,
                "maximum interval" => F::IntervalMax,
                _ => F::CellWidth,
            };
            vec![(f, DialogError::Size)]
        }
        TestError::TooManyCells { .. } => vec![(
            if dialog == CamDialog::IntervalTest { F::IntervalCount } else { F::ColumnCount },
            DialogError::TooManyCells,
        )],
        TestError::Unsupported { feature, .. } => vec![(F::FocusStart, DialogError::Unsupported(*feature))],
        _ => vec![(F::BaseSpeed, DialogError::OutOfRange)],
    }
}

// ---------------------------------------------------------------------------------------------
// The dialog
// ---------------------------------------------------------------------------------------------

pub(super) fn view(s: &Session, dialog: CamDialog, machine: Option<&Machine>) -> Option<DialogViewModel> {
    let state = &s.cam.tests;
    let draft = state.drafts.get(&dialog)?;
    let z = has_z(machine);
    let focus_blocked = dialog == CamDialog::FocusTest && !z;
    let z_reason = MessageKey::MissingCapability(Feature::ZStep);
    let mode_fill = draft.choice(DialogField::CellMode) == Some(DialogChoice::CellFill);
    let rows = fields_of(dialog)
        .iter()
        .map(|&field| {
            use DialogField as F;
            let switched_off = match field {
                F::RowParameter | F::RowMin | F::RowMax | F::RowCount => !draft.flag(F::RowsEnabled),
                F::FillInterval | F::FillOverscan if dialog == CamDialog::MaterialTest => !mode_fill,
                F::LabelHeight => !draft.flag(F::LabelsEnabled),
                F::BorderMargin => !draft.flag(F::BorderEnabled),
                _ => false,
            };
            let state = if focus_blocked {
                FieldState::Disabled(Some(z_reason))
            } else if switched_off {
                FieldState::Disabled(None)
            } else {
                FieldState::Editable
            };
            let blocked_choices = if !z && matches!(field, F::ColumnParameter | F::RowParameter) {
                vec![(DialogChoice::ParamZOffset, z_reason)]
            } else {
                Vec::new()
            };
            DialogRow {
                field,
                value: draft.values.get(&field).cloned().unwrap_or(DialogValue::Text(String::new())),
                state,
                error: draft.errors.get(&field).copied(),
                blocked_choices,
                reorder: None,
                labels: Vec::new(),
            }
        })
        .collect();

    let busy = state.inflight.contains_key(&dialog);
    let result = state.results.get(&dialog).map(|r| {
        let errors = r.report.findings.iter().filter(|f| f.severity() == Severity::Error).count();
        TestResultView {
            plan: r.plan.id(),
            cells: r.cells,
            preview: Arc::clone(&r.preview),
            errors,
            warnings: r.report.findings.len() - errors,
            stale: machine.is_none_or(|m| *m.profile.revision() != r.plan.source().machine_profile),
            saved_to: r.saved_to.clone(),
        }
    });
    let mut notes = vec![DialogNote::TestIsSeparate];
    if machine.is_none() {
        notes.push(DialogNote::NoMachineProfile);
    }
    if focus_blocked {
        notes.push(DialogNote::FocusNeedsZ);
    }
    notes.extend(state.notes.get(&dialog).copied());
    let generate_reason = if machine.is_none() {
        Some(DialogNote::NoMachineProfile)
    } else if focus_blocked {
        Some(DialogNote::FocusNeedsZ)
    } else {
        None
    };
    let exportable = machine.is_some() && !busy && result.as_ref().is_some_and(|r| !r.stale);
    Some(DialogViewModel {
        dialog,
        rows,
        notes,
        actions: vec![
            DialogActionState {
                action: DialogActionKind::Generate,
                enabled: generate_reason.is_none() && !busy,
                reason: generate_reason,
            },
            DialogActionState { action: DialogActionKind::ExportRd, enabled: exportable, reason: None },
            DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
        ],
        capabilities: Vec::new(),
        result,
        busy,
        lines: Vec::new(),
    })
}

/// Cancels the job a dialog waits for (best effort; the task identity decides what counts).
fn forget_job(s: &mut Session, dialog: CamDialog) {
    if let Some(job) = s.cam.tests.inflight.remove(&dialog)
        && let Some((_, cancel)) = s.pending.get(&job.id)
    {
        cancel.cancel();
    }
}

pub(super) fn open(s: &mut Session, dialog: CamDialog) -> AppUpdate {
    s.cam.tests.drafts.entry(dialog).or_insert_with(|| TestDraft::defaults(dialog));
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn set(s: &mut Session, dialog: CamDialog, field: DialogField, value: DialogValue) -> AppUpdate {
    if !s.cam.tests.drafts.contains_key(&dialog) {
        return AppUpdate::default();
    }
    // The result described the old fields.
    forget_job(s, dialog);
    s.cam.tests.results.remove(&dialog);
    s.cam.tests.notes.remove(&dialog);
    if let Some(draft) = s.cam.tests.drafts.get_mut(&dialog) {
        draft.errors.clear();
        draft.values.insert(field, value);
    }
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

fn close(s: &mut Session, dialog: CamDialog) -> AppUpdate {
    forget_job(s, dialog);
    s.cam.tests.drafts.remove(&dialog);
    s.cam.tests.results.remove(&dialog);
    s.cam.tests.notes.remove(&dialog);
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

impl AppController {
    pub(super) fn tests_action(&mut self, document: DocumentId, dialog: CamDialog, action: DialogAction) -> AppUpdate {
        match action {
            DialogAction::Close => self.with_session(document, |s| close(s, dialog)),
            DialogAction::Generate => self.generate_test(document, dialog),
            DialogAction::ExportRd(path) => self.export_test(document, dialog, path),
            _ => AppUpdate::default(),
        }
    }

    /// Builds the test job from the draft and compiles it in the background. A refused draft puts
    /// its errors on the rows and starts nothing; a newer job cancels the older one.
    fn generate_test(&mut self, document: DocumentId, dialog: CamDialog) -> AppUpdate {
        let machine = self.machine.clone();
        let id = self.next_id::<TaskId>();
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let Some(machine) = machine else { return error(Some(document), MessageKey::NoMachineProfile) };
            let Some(id) = id else { return error(Some(document), MessageKey::CommandRejected) };
            let Some(options) = compile_options() else { return error(Some(document), MessageKey::CompileFailed) };
            let Some(draft) = s.cam.tests.drafts.get(&dialog) else { return AppUpdate::default() };
            let z = has_z(Some(&machine));
            let mut errors = Errors::new();
            let job = match dialog {
                CamDialog::MaterialTest => material_spec(draft, &mut errors).and_then(|spec| {
                    let needs_z = [(COLUMNS[0], true), (ROWS[0], spec.rows.is_some())]
                        .into_iter()
                        .filter(|(f, used)| *used && parameter(draft.choice(*f)) == TestParameter::ZOffset)
                        .map(|(f, _)| f)
                        .collect::<Vec<_>>();
                    if !z && !needs_z.is_empty() {
                        for f in needs_z {
                            errors.insert(f, DialogError::Unsupported(Feature::ZStep));
                        }
                        return None;
                    }
                    generate_material_test(&spec).map_err(|e| collect(&mut errors, dialog, &e, draft)).ok()
                }),
                CamDialog::IntervalTest => interval_spec(draft, &mut errors).and_then(|spec| {
                    generate_interval_test(&spec).map_err(|e| collect(&mut errors, dialog, &e, draft)).ok()
                }),
                _ => {
                    if !z {
                        s.cam.tests.notes.insert(dialog, DialogNote::FocusNeedsZ);
                        return AppUpdate { view_changed: true, ..AppUpdate::default() };
                    }
                    focus_spec(draft, &mut errors).and_then(|spec| {
                        generate_focus_test(&spec, &machine.profile)
                            .map_err(|e| collect(&mut errors, dialog, &e, draft))
                            .ok()
                    })
                }
            };
            if let Some(draft) = s.cam.tests.drafts.get_mut(&dialog) {
                draft.errors = errors;
            }
            let Some(job) = job else { return AppUpdate { view_changed: true, ..AppUpdate::default() } };

            forget_job(s, dialog);
            s.cam.tests.results.remove(&dialog);
            s.cam.tests.notes.remove(&dialog);
            let cells = job.cells().len();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: s.doc.revision(),
                key: revision_key(TaskKind::TestJob, document, s.doc.revision()),
                cancel: cancel.token(),
                input: TaskInput::CompileTest { job: Arc::new(job), profile: machine.profile, options },
            };
            let update = submit(s, &tasks, request, cancel, document);
            if update.effects.is_empty() {
                s.cam.tests.inflight.insert(dialog, InFlight { id, cells });
            }
            update
        })
    }

    /// Writes the generated test as RD (never the project): the plan the dialog shows, lowered by
    /// the same backend as every other export.
    fn export_test(&mut self, document: DocumentId, dialog: CamDialog, path: PathBuf) -> AppUpdate {
        let machine = self.machine.clone();
        let id = self.next_id::<TaskId>();
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let (Some(machine), Some(id)) = (machine, id) else {
                return error(Some(document), MessageKey::ExportFailed);
            };
            let Some(result) = s.cam.tests.results.get(&dialog) else { return AppUpdate::default() };
            if *machine.profile.revision() != result.plan.source().machine_profile {
                return AppUpdate::default();
            }
            let plan = Arc::clone(&result.plan);
            let cells = result.cells;
            forget_job(s, dialog);
            s.cam.tests.notes.remove(&dialog);
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: s.doc.revision(),
                key: revision_key(TaskKind::TestJob, document, s.doc.revision()),
                cancel: cancel.token(),
                input: TaskInput::ExportTest { plan, ruida: machine.ruida, path, fs },
            };
            let update = submit(s, &tasks, request, cancel, document);
            if update.effects.is_empty() {
                s.cam.tests.inflight.insert(dialog, InFlight { id, cells });
            }
            update
        })
    }
}

fn collect(errors: &mut Errors, dialog: CamDialog, e: &TestError, draft: &TestDraft) {
    for (field, why) in error_fields(dialog, e, draft) {
        errors.insert(field, why);
    }
}

/// Delivers the outcome of a test job. Only the job the dialog still waits for counts (a
/// cancelled or replaced one is ignored, even if its worker finished anyway).
pub(super) fn finished(s: &mut Session, id: TaskId, result: Result<TaskOutput, TaskError>, update: &mut AppUpdate) {
    let Some((dialog, job)) =
        s.cam.tests.inflight.iter().find(|(_, j)| j.id == id).map(|(d, j)| (*d, InFlight { id: j.id, cells: j.cells }))
    else {
        return;
    };
    s.cam.tests.inflight.remove(&dialog);
    update.view_changed = true;
    match result {
        Ok(TaskOutput::Compiled { plan, preview, report }) => {
            s.cam.tests.results.insert(dialog, TestResult { plan, preview, report, cells: job.cells, saved_to: None });
        }
        Ok(TaskOutput::ExportedRd { path, .. }) => {
            if let Some(r) = s.cam.tests.results.get_mut(&dialog) {
                r.saved_to = Some(path);
            }
            s.cam.tests.notes.insert(dialog, DialogNote::TestSaved);
        }
        Err(TaskError::Cancelled) => {}
        Err(e @ (TaskError::Lower(_) | TaskError::Encode(_) | TaskError::Write(_))) => {
            let note = match export_failure(&e) {
                MessageKey::RdRefused(reason) => DialogNote::RdRefused(reason),
                _ => DialogNote::ExportFailed,
            };
            s.cam.tests.notes.insert(dialog, note);
        }
        Ok(_) | Err(_) => {
            s.cam.tests.notes.insert(dialog, DialogNote::TestCompileFailed);
        }
    }
}
