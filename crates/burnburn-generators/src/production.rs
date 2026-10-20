//! The immutable production record a job is bound to, the live counters, and bake (W1-L5c).
//!
//! A [`ProductionRecord`] captures serial number, data row, job clock (with its fixed zone) and
//! the cut-settings values once. All variable text of a job resolves from it, before preview and
//! compile, so preview, RD export and upload of one compiled revision show the same text. The
//! live [`ProductionCounters`] move on a defined [`JobEvent`] and only for the record they were
//! prepared for: a failed upload never consumes a number.

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_document::text::{TextCommand, TextId, TextObject};
use burnburn_types::{CanonicalWriter, ContentHash, HashDomain};

use crate::rows::{FrozenRows, RowBinding, RowError};
use crate::variable::{CutField, JobClock, OnEnd, SerialCounter, Template, Token, VariableError};

/// Everything a job's variable text resolves from, captured once.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionRecord {
    id: ContentHash,
    serial: Option<SerialCounter>,
    clock: JobClock,
    rows: Option<(Arc<FrozenRows>, usize)>,
    cut: BTreeMap<(String, CutField), String>,
    generation: u64,
}

impl ProductionRecord {
    pub fn builder(clock: JobClock) -> RecordBuilder {
        RecordBuilder { clock, serial: None, rows: None, cut: BTreeMap::new(), generation: 0 }
    }

    /// Identity of the record's content (serial series and value, clock and zone, data set and
    /// row, cut values). Equal inputs give an equal id; any change gives another.
    pub fn id(&self) -> ContentHash {
        self.id
    }
    pub fn clock(&self) -> JobClock {
        self.clock
    }
    pub fn serial(&self) -> Option<&SerialCounter> {
        self.serial.as_ref()
    }
    pub fn row(&self) -> Option<(&FrozenRows, usize)> {
        self.rows.as_ref().map(|(r, i)| (r.as_ref(), *i))
    }
    /// The counters' generation this record was prepared at.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Resolves `template`; `object_offset` is the object's own serial offset in series steps.
    pub fn resolve(&self, template: &str, object_offset: i64) -> Result<String, VariableError> {
        let template = Template::parse(template)?;
        let mut out = String::new();
        for token in template.tokens() {
            match token {
                Token::Literal(s) => out.push_str(s),
                Token::Serial => {
                    let counter = self.serial.as_ref().ok_or(VariableError::NoSerial)?;
                    out.push_str(&counter.format(counter.value_at_offset(object_offset)?));
                }
                Token::Date(format) => out.push_str(&self.clock.format(format)?),
                Token::Csv(column) => {
                    let (rows, row) = self.rows.as_ref().ok_or(VariableError::NoRow)?;
                    let index = rows
                        .table
                        .headers
                        .iter()
                        .position(|h| h == column)
                        .ok_or_else(|| VariableError::MissingColumn { column: column.clone() })?;
                    match rows.table.rows.get(*row).and_then(|cells| cells.get(index)) {
                        Some(v) if !v.is_empty() => out.push_str(v),
                        _ => return Err(VariableError::MissingValue { column: column.clone(), row: *row }),
                    }
                }
                Token::Cut { layer, field } => {
                    let v = self
                        .cut
                        .get(&(layer.clone(), *field))
                        .ok_or_else(|| VariableError::NoCutValue { layer: layer.clone(), field: *field })?;
                    out.push_str(v);
                }
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct RecordBuilder {
    clock: JobClock,
    serial: Option<SerialCounter>,
    rows: Option<(Arc<FrozenRows>, usize)>,
    cut: BTreeMap<(String, CutField), String>,
    generation: u64,
}

impl RecordBuilder {
    pub fn serial(mut self, counter: SerialCounter) -> Self {
        self.serial = Some(counter);
        self
    }

    /// Binds the record to one row of a frozen data set; refuses a binding made for other data
    /// or for a row that does not exist.
    pub fn row(mut self, rows: Arc<FrozenRows>, binding: RowBinding) -> Result<Self, VariableError> {
        if rows.hash != binding.rows {
            return Err(RowError::StaleRows.into());
        }
        if binding.row >= rows.table.rows.len() {
            return Err(RowError::RowOutOfRange { row: binding.row, rows: rows.table.rows.len() }.into());
        }
        self.rows = Some((rows, binding.row));
        Ok(self)
    }

    pub fn cut_value(mut self, layer: &str, field: CutField, value: &str) -> Self {
        self.cut.insert((layer.to_owned(), field), value.to_owned());
        self
    }

    pub fn build(self) -> ProductionRecord {
        let id = record_id(&self);
        ProductionRecord {
            id,
            serial: self.serial,
            clock: self.clock,
            rows: self.rows,
            cut: self.cut,
            generation: self.generation,
        }
    }
}

fn record_id(b: &RecordBuilder) -> ContentHash {
    let mut w = CanonicalWriter::new();
    w.tag("burnburn-production-record");
    w.u64(u64::from_le_bytes(b.clock.utc_seconds().to_le_bytes()));
    w.u64(u64::from_le_bytes(i64::from(b.clock.offset_minutes()).to_le_bytes()));
    match &b.serial {
        None => w.u64(0),
        Some(c) => {
            w.u64(1);
            let spec = c.spec();
            for v in [spec.start(), spec.end(), spec.advance_by(), c.current()] {
                w.u64(u64::from_le_bytes(v.to_le_bytes()));
            }
            w.u64(spec.width() as u64);
            w.tag(match spec.on_end() {
                OnEnd::Wrap => "wrap",
                OnEnd::Stop => "stop",
            });
        }
    }
    match &b.rows {
        None => w.u64(0),
        Some((rows, row)) => {
            w.u64(1);
            w.bytes(rows.hash.as_bytes());
            w.u64(*row as u64);
        }
    }
    w.u64(b.cut.len() as u64);
    for ((layer, field), value) in &b.cut {
        w.str(layer);
        w.tag(&format!("{field:?}"));
        w.str(value);
    }
    // Only strings and integers: finishing cannot meet a non-finite number.
    ContentHash::of(HashDomain::EvalGenerator, &w.finish().unwrap_or_default())
}

/// What a job reports about itself; counters advance on one of these, chosen by the user.
/// Which event LightBurn advances on is not verified (differential test open): the default is
/// [`AdvancePolicy::Manual`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobEvent {
    UploadConfirmed,
    UploadFailed,
    JobStarted,
    JobCancelled,
}

/// When the live counters advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AdvancePolicy {
    /// Only [`ProductionCounters::advance_manually`].
    #[default]
    Manual,
    OnUploadConfirmed,
    OnJobStarted,
}

impl AdvancePolicy {
    fn event(self) -> Option<JobEvent> {
        match self {
            AdvancePolicy::Manual => None,
            AdvancePolicy::OnUploadConfirmed => Some(JobEvent::UploadConfirmed),
            AdvancePolicy::OnJobStarted => Some(JobEvent::JobStarted),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Commit {
    Advanced,
    /// The event is not the one the policy advances on; nothing moved.
    Ignored,
}

/// The live serial and row position of a project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionCounters {
    serial: Option<SerialCounter>,
    row: usize,
    policy: AdvancePolicy,
    generation: u64,
}

impl ProductionCounters {
    pub fn new(serial: Option<SerialCounter>, policy: AdvancePolicy) -> Self {
        ProductionCounters { serial, row: 0, policy, generation: 0 }
    }
    pub fn serial(&self) -> Option<&SerialCounter> {
        self.serial.as_ref()
    }
    pub fn row(&self) -> usize {
        self.row
    }
    pub fn policy(&self) -> AdvancePolicy {
        self.policy
    }
    /// Counts the advances; a record is only committed against the generation it was made at.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Captures the current state as the record of the next job. With `rows`, the current row
    /// must exist (running out of rows is an error here, not a blank label later).
    pub fn record(
        &self,
        clock: JobClock,
        rows: Option<Arc<FrozenRows>>,
        cut: &[(&str, CutField, &str)],
    ) -> Result<ProductionRecord, VariableError> {
        let mut builder = ProductionRecord::builder(clock);
        builder.generation = self.generation;
        if let Some(serial) = self.serial {
            builder = builder.serial(serial);
        }
        if let Some(rows) = rows {
            let binding = RowBinding { rows: rows.hash, row: self.row };
            builder = builder.row(rows, binding)?;
        }
        for (layer, field, value) in cut {
            builder = builder.cut_value(layer, *field, value);
        }
        Ok(builder.build())
    }

    /// Reports `event` for the job of `record`. Advances serial and row once, only when `event` is
    /// the one the policy names and the record is the current one; a repeated report for the
    /// same record is [`VariableError::StaleRecord`]. Nothing changes on an error.
    pub fn commit(&mut self, record: &ProductionRecord, event: JobEvent) -> Result<Commit, VariableError> {
        if self.policy.event() != Some(event) {
            return Ok(Commit::Ignored);
        }
        if record.generation != self.generation {
            return Err(VariableError::StaleRecord);
        }
        self.advance(record.row().is_some())?;
        Ok(Commit::Advanced)
    }

    /// Steps serial and row by hand (the panel's Next button). Nothing changes on an error.
    pub fn advance_manually(&mut self) -> Result<(), VariableError> {
        self.advance(true)
    }

    /// Steps serial and row back by hand (the panel's Previous button); the row stops at 0.
    /// Nothing changes on an error.
    pub fn step_back_manually(&mut self) -> Result<(), VariableError> {
        let mut serial = self.serial;
        if let Some(s) = serial.as_mut() {
            s.step_back()?;
        }
        self.serial = serial;
        self.row = self.row.saturating_sub(1);
        self.generation += 1;
        Ok(())
    }

    /// Back to the start value and the first row (the panel's Reset button).
    pub fn reset(&mut self) {
        if let Some(s) = self.serial.as_mut() {
            s.reset();
        }
        self.row = 0;
        self.generation += 1;
    }

    /// Chooses the data row of the next job.
    pub fn set_row(&mut self, row: usize) {
        self.row = row;
        self.generation += 1;
    }

    /// Replaces the serial series (Start/End/Advance by or the current value changed by hand).
    pub fn set_serial(&mut self, serial: Option<SerialCounter>) {
        self.serial = serial;
        self.generation += 1;
    }

    /// Chooses the event the counters advance on.
    pub fn set_policy(&mut self, policy: AdvancePolicy) {
        self.policy = policy;
        self.generation += 1;
    }

    fn advance(&mut self, with_row: bool) -> Result<(), VariableError> {
        let mut serial = self.serial;
        if let Some(s) = serial.as_mut() {
            s.advance()?;
        }
        self.serial = serial;
        if with_row {
            self.row += 1;
        }
        self.generation += 1;
        Ok(())
    }
}

/// `text` with its template resolved against `record`; it is no longer variable. `extra_offset`
/// adds to the object's own offset (the copy index of an array).
pub fn resolve_text(
    text: &TextObject,
    record: &ProductionRecord,
    extra_offset: i64,
) -> Result<TextObject, VariableError> {
    let variable = text.variable.ok_or(VariableError::NotVariable)?;
    let offset = variable.offset.checked_add(extra_offset).ok_or(VariableError::SerialExhausted)?;
    let mut resolved = text.clone();
    resolved.content = record.resolve(&text.content, offset)?;
    resolved.variable = None;
    Ok(resolved)
}

/// Bake: the command that replaces the dynamic content by the fixed content. It is a plain text
/// edit, so it is one undo step.
pub fn bake_command(id: TextId, text: &TextObject, record: &ProductionRecord) -> Result<TextCommand, VariableError> {
    Ok(TextCommand::Set { text: id, source: Box::new(resolve_text(text, record, 0)?) })
}
