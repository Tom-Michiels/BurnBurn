//! Variable text (W3-UI-tekst, E11): the panel's edits, the serial counters and data rows of a
//! window, the binding of a `ProductionRecord` into compile (so preview, RD export and upload of
//! one compiled revision print the same text) and the counter commit on the chosen job event.
//!
//! Counters, the chosen data row and the series settings are window state: the project file does
//! not carry them yet (queue W2-L5c). A prepared job keeps the record it was compiled with, so
//! later edits of the counters or the data never change what it burns.

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_document::text::{TextCommand, TextId, TextObject, VariableText};
use burnburn_document::{
    CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus,
};
use burnburn_generators::{
    AdvancePolicy, Commit, CsvError, CutField, DataTable, FrozenRows, JobClock, JobEvent, OnEnd, ProductionCounters,
    ProductionRecord, RowError, SerialCounter, SerialSpec, Template, Token, VariableError, bake_command, resolve_text,
};
use burnburn_machine::{ExecutionId, ExecutionState, FailReason, UnknownReason};
use burnburn_text::TextEvaluator;
use burnburn_types::{DocumentId, RequestId, StableId, TaskId};

use crate::field::format_mm;
use crate::variable_panel::{
    CsvViewModel, SerialField, SerialViewModel, TestLine, VariableEdit, VariableIssue, VariablePanelViewModel,
    VariableProblem,
};

use super::text::{SharedFonts, single_text};
use super::{AppController, AppUpdate, Session, apply_edit};

/// Series a new window starts with: 1 to 999 999 by 1, no padding, starting over after the end.
fn default_serial() -> Option<SerialCounter> {
    SerialSpec::new(1, 999_999, 1, 0, OnEnd::Wrap).ok().map(SerialCounter::new)
}

/// Window state of the variable-text panel.
#[derive(Debug)]
pub(super) struct VariableState {
    counters: ProductionCounters,
    /// The chosen CSV: its display name and the frozen rows.
    rows: Option<(String, Arc<FrozenRows>)>,
    problem: Option<VariableProblem>,
    test: Option<Vec<TestLine>>,
    /// The record each compile in flight was bound to.
    in_flight: BTreeMap<TaskId, ProductionRecord>,
    /// Executions whose job events were already reported to the counters.
    reported: Vec<(ExecutionId, JobEvent)>,
}

impl Default for VariableState {
    fn default() -> Self {
        VariableState {
            counters: ProductionCounters::new(default_serial(), AdvancePolicy::Manual),
            rows: None,
            problem: None,
            test: None,
            in_flight: BTreeMap::new(),
            reported: Vec::new(),
        }
    }
}

impl VariableState {
    /// The record the finished compile `task` was bound to (taken: a task finishes once).
    pub(super) fn take_record(&mut self, task: TaskId) -> Option<ProductionRecord> {
        self.in_flight.remove(&task)
    }

    /// Remembers the record a compile is bound to until its result arrives.
    pub(super) fn bind_task(&mut self, task: TaskId, record: ProductionRecord) {
        self.in_flight.insert(task, record);
    }

    /// A compile was accepted: an earlier refusal no longer applies.
    pub(super) fn clear_problem(&mut self) {
        self.problem = None;
    }

    /// A compile was refused: why.
    pub(super) fn set_problem(&mut self, problem: VariableProblem) {
        self.problem = Some(problem);
    }
}

fn plain(issue: VariableIssue) -> VariableProblem {
    VariableProblem { issue, detail: String::new() }
}

fn with(issue: VariableIssue, detail: impl Into<String>) -> VariableProblem {
    VariableProblem { issue, detail: detail.into() }
}

fn problem_of(e: &VariableError) -> VariableProblem {
    match e {
        VariableError::NotInSeries { .. } => plain(VariableIssue::NotInSeries),
        VariableError::SerialExhausted => plain(VariableIssue::SerialExhausted),
        VariableError::BadSerial { reason } => with(VariableIssue::InvalidSerial, *reason),
        VariableError::BadClock => plain(VariableIssue::NoClock),
        VariableError::BadDateFormat { at } | VariableError::BadTemplate { at } => {
            with(VariableIssue::BadTemplate, at.to_string())
        }
        VariableError::UnknownToken { name, .. } => with(VariableIssue::BadTemplate, name.clone()),
        VariableError::MissingColumn { column } => with(VariableIssue::MissingColumn, column.clone()),
        // Rows are counted from 1 in everything the user sees.
        VariableError::MissingValue { column, row } => {
            with(VariableIssue::MissingValue, format!("{column} ({})", row + 1))
        }
        VariableError::NoRow => plain(VariableIssue::NoData),
        VariableError::NoSerial => plain(VariableIssue::NoSerial),
        VariableError::NoCutValue { layer, field } => with(VariableIssue::NoCutValue, format!("{layer}.{field:?}")),
        VariableError::Row(RowError::RowOutOfRange { row, rows }) => {
            with(VariableIssue::RowOutOfRange, format!("{} / {rows}", row + 1))
        }
        VariableError::Row(RowError::BadTemplate { at }) => with(VariableIssue::BadTemplate, at.to_string()),
        VariableError::Row(_) | VariableError::StaleRecord => plain(VariableIssue::StaleJob),
        VariableError::NotVariable => plain(VariableIssue::Refused),
        _ => plain(VariableIssue::Refused),
    }
}

fn refused(s: &mut Session, p: VariableProblem) -> AppUpdate {
    s.variable.problem = Some(p);
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

fn done(s: &mut Session) -> AppUpdate {
    s.variable.problem = None;
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

fn whole_number(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

/// The series with `field` set to `text`. The counter keeps its value while it is still a value
/// of the new series, else it goes to the start.
fn serial_with(counter: &SerialCounter, field: SerialField, text: &str) -> Result<SerialCounter, VariableProblem> {
    let invalid = || plain(VariableIssue::InvalidNumber(field));
    let spec = counter.spec();
    if field == SerialField::Current {
        let mut next = *counter;
        let value = whole_number(text).ok_or_else(invalid)?;
        next.set_current(value).map_err(|_| plain(VariableIssue::NotInSeries))?;
        return Ok(next);
    }
    let (mut start, mut end, mut by, mut digits) = (spec.start(), spec.end(), spec.advance_by(), spec.width());
    match field {
        SerialField::Start => start = whole_number(text).ok_or_else(invalid)?,
        SerialField::End => end = whole_number(text).ok_or_else(invalid)?,
        SerialField::AdvanceBy => by = whole_number(text).ok_or_else(invalid)?,
        SerialField::Digits => {
            digits = usize::try_from(whole_number(text).ok_or_else(invalid)?).map_err(|_| invalid())?
        }
        SerialField::Current => {}
    }
    rebuilt(counter, start, end, by, digits, spec.on_end())
}

fn rebuilt(
    old: &SerialCounter,
    start: i64,
    end: i64,
    by: i64,
    digits: usize,
    on_end: OnEnd,
) -> Result<SerialCounter, VariableProblem> {
    let spec = SerialSpec::new(start, end, by, digits, on_end).map_err(|e| problem_of(&e))?;
    let mut next = SerialCounter::new(spec);
    // A value that is no longer in the series is dropped: the counter stays at the start.
    let _ = next.set_current(old.current());
    Ok(next)
}

/// Delimiter of a CSV: the one of `,` `;` and tab that occurs most in the header line.
fn delimiter(bytes: &[u8]) -> char {
    let header = bytes.split(|b| *b == b'\n').next().unwrap_or_default();
    [',', ';', '\t']
        .into_iter()
        .max_by_key(|d| (header.iter().filter(|b| **b == *d as u8).count(), *d == ','))
        .unwrap_or(',')
}

fn load_csv(s: &mut Session, clock: Option<JobClock>, name: String, bytes: &[u8]) -> AppUpdate {
    let table = match DataTable::from_utf8(bytes, delimiter(bytes)) {
        Ok(t) => t,
        Err(e) => {
            let detail = match e {
                CsvError::NotUtf8 { valid_up_to } => format!("UTF-8 ({valid_up_to})"),
                CsvError::Ragged { line, .. } | CsvError::UnterminatedQuote { line } => line.to_string(),
                _ => String::new(),
            };
            return refused(s, with(VariableIssue::CsvUnreadable, detail));
        }
    };
    let frozen_at = clock.and_then(|c| c.format("%Y-%m-%d").ok()).unwrap_or_default();
    s.variable.rows = Some((name, Arc::new(FrozenRows::freeze(table, &frozen_at))));
    s.variable.counters.set_row(0);
    done(s)
}

/// Cut values a `{cut:LAYER.field}` reference can resolve: the operations by their layer name.
fn cut_values(snapshot: &DocumentSnapshot) -> Vec<(String, CutField, String)> {
    let mut out = Vec::new();
    for op in snapshot.process().operations() {
        let settings = op.settings();
        let layer = op.name().to_owned();
        out.push((layer.clone(), CutField::Name, layer.clone()));
        out.push((layer.clone(), CutField::Speed, format_mm(settings.speed.get_mm_per_s())));
        out.push((layer.clone(), CutField::Power, format_mm(settings.power.max().get())));
        out.push((layer.clone(), CutField::MinPower, format_mm(settings.power.min().get())));
        out.push((layer, CutField::Passes, settings.passes.get().to_string()));
    }
    out
}

fn variable_texts(snapshot: &DocumentSnapshot) -> Vec<(TextId, TextObject)> {
    snapshot.text_layer().texts().filter(|(_, t)| t.variable.is_some()).map(|(id, t)| (id, t.clone())).collect()
}

/// The record the next job would be bound to: live counters, the chosen row, the clock and the
/// cut values of the document. A clock is only demanded when a template names a date.
fn record(
    s: &Session,
    snapshot: &DocumentSnapshot,
    texts: &[(TextId, TextObject)],
    clock: Option<JobClock>,
) -> Result<ProductionRecord, VariableProblem> {
    let uses_date = texts.iter().any(|(_, t)| {
        Template::parse(&t.content).is_ok_and(|template| template.tokens().iter().any(|k| matches!(k, Token::Date(_))))
    });
    let clock = match clock {
        Some(c) => c,
        None if uses_date => return Err(plain(VariableIssue::NoClock)),
        // No date is printed: the clock only sits in the record's identity.
        None => JobClock::new(0, 0).map_err(|e| problem_of(&e))?,
    };
    let cut = cut_values(snapshot);
    let cut: Vec<(&str, CutField, &str)> = cut.iter().map(|(l, f, v)| (l.as_str(), *f, v.as_str())).collect();
    s.variable
        .counters
        .record(clock, s.variable.rows.as_ref().map(|(_, r)| Arc::clone(r)), &cut)
        .map_err(|e| problem_of(&e))
}

fn resolved(t: &TextObject, record: &ProductionRecord, extra_offset: i64) -> Result<TextObject, VariableProblem> {
    resolve_text(t, record, extra_offset).map_err(|e| problem_of(&e))
}

/// Normal ⇄ Variable, the offset and Bake are edits of the text itself.
fn edit_text(
    s: &mut Session,
    document: DocumentId,
    request: Option<RequestId>,
    source: TextObject,
    id: TextId,
) -> AppUpdate {
    s.variable.test = None;
    let before = s.doc.revision();
    let update =
        apply_edit(s, document, CommandPayload::Text(TextCommand::Set { text: id, source: Box::new(source) }), request);
    if s.doc.revision() == before {
        return refused(s, plain(VariableIssue::Refused));
    }
    s.variable.problem = None;
    update
}

pub(super) fn edit(
    s: &mut Session,
    document: DocumentId,
    _fonts: Option<&SharedFonts>,
    clock: Option<JobClock>,
    edit: VariableEdit,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let text = single_text(s, &snapshot).and_then(|id| snapshot.text(id).map(|t| (id, t.clone())));
    // Whatever was shown before (Test result, reason) belongs to the old state.
    let keep_test = matches!(edit, VariableEdit::Test);
    if !keep_test {
        s.variable.test = None;
    }
    match edit {
        VariableEdit::SetVariable(on) => {
            let Some((id, mut t)) = text else { return AppUpdate::default() };
            if t.variable.is_some() == on {
                return done(s);
            }
            t.variable = on.then(VariableText::default);
            edit_text(s, document, request, t, id)
        }
        VariableEdit::Offset(text_in) => {
            let Some((id, mut t)) = text else { return AppUpdate::default() };
            let Some(variable) = t.variable else { return AppUpdate::default() };
            let Some(offset) = whole_number(&text_in) else { return refused(s, plain(VariableIssue::InvalidOffset)) };
            if variable.offset == offset {
                return done(s);
            }
            t.variable = Some(VariableText { offset });
            edit_text(s, document, request, t, id)
        }
        VariableEdit::Serial(field, input) => {
            let Some(current) = s.variable.counters.serial().copied() else { return AppUpdate::default() };
            match serial_with(&current, field, &input) {
                Ok(next) => {
                    if next != current {
                        s.variable.counters.set_serial(Some(next));
                    }
                    done(s)
                }
                Err(p) => refused(s, p),
            }
        }
        VariableEdit::Wrap(wrap) => {
            let Some(c) = s.variable.counters.serial().copied() else { return AppUpdate::default() };
            let spec = c.spec();
            let on_end = if wrap { OnEnd::Wrap } else { OnEnd::Stop };
            match rebuilt(&c, spec.start(), spec.end(), spec.advance_by(), spec.width(), on_end) {
                Ok(next) => {
                    if next != c {
                        s.variable.counters.set_serial(Some(next));
                    }
                    done(s)
                }
                Err(p) => refused(s, p),
            }
        }
        VariableEdit::Policy(policy) => {
            if s.variable.counters.policy() != policy {
                s.variable.counters.set_policy(policy);
            }
            done(s)
        }
        VariableEdit::Previous => match s.variable.counters.step_back_manually() {
            Ok(()) => done(s),
            Err(e) => refused(s, problem_of(&e)),
        },
        VariableEdit::Next => match s.variable.counters.advance_manually() {
            Ok(()) => done(s),
            Err(e) => refused(s, problem_of(&e)),
        },
        VariableEdit::Reset => {
            s.variable.counters.reset();
            done(s)
        }
        VariableEdit::Row(input) => {
            let Some(rows) = s.variable.rows.as_ref().map(|(_, r)| r.table.rows.len()) else {
                return refused(s, plain(VariableIssue::NoData));
            };
            match input.trim().parse::<usize>() {
                Ok(n) if (1..=rows).contains(&n) => {
                    s.variable.counters.set_row(n - 1);
                    done(s)
                }
                _ => refused(s, with(VariableIssue::RowOutOfRange, format!("1..{rows}"))),
            }
        }
        VariableEdit::LoadCsv { name, bytes } => load_csv(s, clock, name, &bytes),
        VariableEdit::ClearCsv => {
            s.variable.rows = None;
            s.variable.counters.set_row(0);
            done(s)
        }
        VariableEdit::Test => {
            let texts = variable_texts(&snapshot);
            let lines = match record(s, &snapshot, &texts, clock) {
                Ok(r) => texts
                    .iter()
                    .map(|(_, t)| TestLine {
                        template: t.content.clone(),
                        result: resolved(t, &r, 0).map(|x| x.content),
                    })
                    .collect(),
                Err(p) => texts
                    .iter()
                    .map(|(_, t)| TestLine { template: t.content.clone(), result: Err(p.clone()) })
                    .collect(),
            };
            s.variable.test = Some(lines);
            s.variable.problem = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
        VariableEdit::Bake => {
            let Some((id, t)) = text else { return AppUpdate::default() };
            if t.variable.is_none() {
                return AppUpdate::default();
            }
            let rec = match record(s, &snapshot, &[(id, t.clone())], clock) {
                Ok(r) => r,
                Err(p) => return refused(s, p),
            };
            match bake_command(id, &t, &rec) {
                Ok(TextCommand::Set { source, .. }) => edit_text(s, document, request, *source, id),
                Ok(_) => refused(s, plain(VariableIssue::Refused)),
                Err(e) => refused(s, problem_of(&e)),
            }
        }
    }
}

pub(super) fn panel_view(s: &Session, clock: Option<JobClock>) -> Option<VariablePanelViewModel> {
    let snapshot = s.doc.snapshot();
    let id = single_text(s, &snapshot)?;
    let t = snapshot.text(id)?;
    let counters = &s.variable.counters;
    let serial = counters.serial()?;
    let spec = serial.spec();
    let preview = t.variable.map(|_| {
        let texts = [(id, t.clone())];
        record(s, &snapshot, &texts, clock).and_then(|r| resolved(t, &r, 0)).map(|x| x.content)
    });
    Some(VariablePanelViewModel {
        variable: t.variable.is_some(),
        offset: t.variable.map_or_else(|| "0".to_owned(), |v| v.offset.to_string()),
        serial: SerialViewModel {
            current: serial.current().to_string(),
            start: spec.start().to_string(),
            end: spec.end().to_string(),
            advance_by: spec.advance_by().to_string(),
            digits: spec.width().to_string(),
            wrap: spec.on_end() == OnEnd::Wrap,
        },
        policy: counters.policy(),
        csv: s.variable.rows.as_ref().map(|(name, rows)| CsvViewModel {
            name: name.clone(),
            columns: rows.table.headers.clone(),
            rows: rows.table.rows.len(),
            row: (counters.row() + 1).to_string(),
        }),
        preview,
        test: s.variable.test.clone(),
        problem: s.variable.problem.clone(),
        bound_record: s.compiled.as_ref().and_then(|c| c.record.as_ref()).map(|r| r.id().to_hex()),
    })
}

/// What a compile of `s` is bound to: the snapshot with every variable text resolved against one
/// production record, under the revision of the document (so preview, RD export and upload keep
/// naming one revision). Documents without variable text compile as they are.
pub(super) fn bind(
    s: &mut Session,
    fonts: Option<&SharedFonts>,
    clock: Option<JobClock>,
) -> Result<(Arc<DocumentSnapshot>, Option<ProductionRecord>), VariableProblem> {
    let snapshot = s.doc.snapshot();
    let texts = variable_texts(&snapshot);
    if texts.is_empty() {
        return Ok((snapshot, None));
    }
    let record = record(s, &snapshot, &texts, clock)?;
    let fonts = fonts.ok_or_else(|| plain(VariableIssue::NoFonts))?;
    // A scratch session: the document and its history are not touched.
    let mut scratch = DocumentSession::from_snapshot(s.doc.id(), (*snapshot).clone());
    let evaluator = TextEvaluator::new(fonts.clone());
    for (n, (id, text)) in texts.iter().enumerate() {
        let source = resolved(text, &record, 0)?;
        let cmd = DocumentCommand {
            document: scratch.id(),
            expected: scratch.revision(),
            request: RequestId::from_raw(n as u64 + 1),
            payload: CommandPayload::Text(TextCommand::Set { text: *id, source: Box::new(source) }),
        };
        scratch.apply_with(cmd, &[&evaluator]).map_err(|_| plain(VariableIssue::Refused))?;
    }
    let bound = scratch.snapshot();
    for (id, text) in &texts {
        match bound.derived_record(DerivedOwner::Text(*id)).map(|r| &r.status) {
            Some(EvalStatus::Valid) => {}
            Some(EvalStatus::Failed { diagnosis: EvalDiagnosis::FontMissing { .. } }) => {
                return Err(with(VariableIssue::FontMissing, text.font.family.clone()));
            }
            _ => return Err(plain(VariableIssue::Refused)),
        }
    }
    let restamped = (*bound).clone().restamped(snapshot.revision());
    Ok((Arc::new(restamped), Some(record)))
}

/// The job events a machine state implies (a job that runs was also stored). Uncertain states
/// (`Unknown`) imply none: automation never counts what it cannot confirm.
fn events_of(state: &ExecutionState) -> &'static [JobEvent] {
    match state {
        ExecutionState::Stored | ExecutionState::StartRequested => &[JobEvent::UploadConfirmed],
        ExecutionState::Running | ExecutionState::Completed => &[JobEvent::UploadConfirmed, JobEvent::JobStarted],
        ExecutionState::Failed(FailReason::UploadRejected { .. } | FailReason::NotStored)
        | ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { .. }) => &[JobEvent::UploadFailed],
        ExecutionState::Cancelled => &[JobEvent::JobCancelled],
        _ => &[],
    }
}

impl AppController {
    /// Reports what the machine service knows to the serial counters: each execution of a
    /// prepared job that carries a production record gives the job events its state implies, once.
    /// The counters move only on the event the user chose (`AdvancePolicy`), never after a failed
    /// upload and never twice for one job. Hosts call this with their machine status refresh; it
    /// does nothing when no prepared job uses variable text.
    pub fn sync_production(&mut self) -> AppUpdate {
        let Some(port) = self.machine_port.clone() else { return AppUpdate::default() };
        if !self.sessions.values().any(|s| s.prepared.as_ref().is_some_and(|p| p.record.is_some())) {
            return AppUpdate::default();
        }
        let status = port.status();
        let mut changed = false;
        for rec in status.executions() {
            let events = events_of(&rec.execution.state);
            if events.is_empty() {
                continue;
            }
            for s in self.sessions.values_mut() {
                let Some(job_record) =
                    s.prepared.as_ref().filter(|p| p.job.plan == rec.job.plan).and_then(|p| p.record.clone())
                else {
                    continue;
                };
                for event in events {
                    if s.variable.reported.contains(&(rec.execution.id, *event)) {
                        continue;
                    }
                    s.variable.reported.push((rec.execution.id, *event));
                    match s.variable.counters.commit(&job_record, *event) {
                        Ok(Commit::Advanced) => {
                            s.variable.problem = None;
                            changed = true;
                        }
                        Ok(Commit::Ignored) => {}
                        Err(e) => {
                            s.variable.problem = Some(problem_of(&e));
                            changed = true;
                        }
                    }
                }
            }
        }
        AppUpdate { view_changed: changed, ..AppUpdate::default() }
    }
}
