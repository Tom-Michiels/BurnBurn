//! Variable text building blocks (W1-L5c): serial counters, a job clock in a fixed time zone and
//! the template language (`{serial}`, `{date:%Y-%m-%d}`, `{csv:column}`, `{cut:C01.speed}`).
//!
//! Everything here is pure: the current time, the counter state and the data rows are *inputs*
//! (see `production.rs` for the immutable record a job is bound to).

use std::fmt;

use crate::rows::RowError;

/// Why variable text could not be resolved. A missing value is always an error: a label with a
/// silently empty field is worse than no label.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VariableError {
    /// `set_current` with a value the series never has.
    NotInSeries {
        value: i64,
    },
    /// The series is finished (`OnEnd::Stop`).
    SerialExhausted,
    BadSerial {
        reason: &'static str,
    },
    /// Zone offset of a day or more, or a moment outside years 1–9999.
    BadClock,
    /// Unknown or unfinished `%` specifier at byte `at` of the format.
    BadDateFormat {
        at: usize,
    },
    /// `{` without `}` (or a lone `}`) at byte `at`.
    BadTemplate {
        at: usize,
    },
    UnknownToken {
        name: String,
        at: usize,
    },
    MissingColumn {
        column: String,
    },
    /// The cell is empty: no label is made with a silently blank field.
    MissingValue {
        column: String,
        row: usize,
    },
    NoRow,
    NoSerial,
    NoCutValue {
        layer: String,
        field: CutField,
    },
    Row(RowError),
    /// The record was made before the counters last moved.
    StaleRecord,
    /// The text is literal, there is nothing to resolve.
    NotVariable,
}

impl fmt::Display for VariableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VariableError::NotInSeries { value } => write!(f, "{value} is not a value of the series"),
            VariableError::SerialExhausted => f.write_str("the serial series has reached its end"),
            VariableError::BadSerial { reason } => write!(f, "invalid serial number settings: {reason}"),
            VariableError::BadClock => f.write_str("the job clock is out of range"),
            VariableError::BadDateFormat { at } => write!(f, "unknown date format specifier at {at}"),
            VariableError::BadTemplate { at } => write!(f, "unbalanced brace in the text at {at}"),
            VariableError::UnknownToken { name, at } => write!(f, "unknown field {{{name}}} at {at}"),
            VariableError::MissingColumn { column } => write!(f, "the data has no column '{column}'"),
            VariableError::MissingValue { column, row } => write!(f, "row {row} has no value for '{column}'"),
            VariableError::NoRow => f.write_str("no data row is bound to this job"),
            VariableError::NoSerial => f.write_str("no serial number is set up for this job"),
            VariableError::NoCutValue { layer, field } => write!(f, "no {field:?} value known for layer {layer}"),
            VariableError::Row(e) => write!(f, "{e:?}"),
            VariableError::StaleRecord => f.write_str("the counters moved after this job was prepared"),
            VariableError::NotVariable => f.write_str("the text is not variable text"),
        }
    }
}

impl std::error::Error for VariableError {}

impl From<RowError> for VariableError {
    fn from(e: RowError) -> Self {
        VariableError::Row(e)
    }
}

/// What a counter does when it passes its end value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnEnd {
    /// Continue at the start value.
    Wrap,
    /// Refuse: the series is finished.
    Stop,
}

/// Widest zero padding.
const MAX_WIDTH: usize = 20;

/// Start, end and step of a serial number. `advance_by` may be negative (counting down).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SerialSpec {
    start: i64,
    end: i64,
    advance_by: i64,
    width: usize,
    on_end: OnEnd,
}

impl SerialSpec {
    pub fn new(start: i64, end: i64, advance_by: i64, width: usize, on_end: OnEnd) -> Result<Self, VariableError> {
        let bad = |reason| Err(VariableError::BadSerial { reason });
        if advance_by == 0 {
            return bad("advance by is zero");
        }
        if advance_by > 0 && end < start {
            return bad("end is before start");
        }
        if advance_by < 0 && end > start {
            return bad("end is after start while counting down");
        }
        if width > MAX_WIDTH {
            return bad("width is above 20");
        }
        Ok(SerialSpec { start, end, advance_by, width, on_end })
    }
    pub fn start(&self) -> i64 {
        self.start
    }
    pub fn end(&self) -> i64 {
        self.end
    }
    pub fn advance_by(&self) -> i64 {
        self.advance_by
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn on_end(&self) -> OnEnd {
        self.on_end
    }

    /// Number of values in the series.
    fn len(&self) -> i128 {
        (i128::from(self.end) - i128::from(self.start)) / i128::from(self.advance_by) + 1
    }

    fn value(&self, position: i128) -> i64 {
        // position is in 0..len, so the value lies between start and end.
        i64::try_from(i128::from(self.start) + position * i128::from(self.advance_by)).unwrap_or(self.start)
    }

    /// Position of `value` in the series, if it is one of its values.
    fn position(&self, value: i64) -> Option<i128> {
        let delta = i128::from(value) - i128::from(self.start);
        let by = i128::from(self.advance_by);
        (delta % by == 0 && (0..self.len()).contains(&(delta / by))).then_some(delta / by)
    }
}

/// A serial number series and its current value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SerialCounter {
    spec: SerialSpec,
    current: i64,
}

impl SerialCounter {
    pub fn new(spec: SerialSpec) -> Self {
        SerialCounter { spec, current: spec.start }
    }
    pub fn spec(&self) -> &SerialSpec {
        &self.spec
    }
    pub fn current(&self) -> i64 {
        self.current
    }

    fn position(&self) -> i128 {
        self.spec.position(self.current).unwrap_or(0)
    }

    /// Sets the current value; it must be one of the series' values.
    pub fn set_current(&mut self, value: i64) -> Result<(), VariableError> {
        self.spec.position(value).ok_or(VariableError::NotInSeries { value })?;
        self.current = value;
        Ok(())
    }

    /// Steps forward; past the end it wraps or, for `OnEnd::Stop`, refuses and stays.
    pub fn advance(&mut self) -> Result<(), VariableError> {
        self.current = self.value_at_offset(1)?;
        Ok(())
    }

    pub fn step_back(&mut self) -> Result<(), VariableError> {
        self.current = self.value_at_offset(-1)?;
        Ok(())
    }

    pub fn reset(&mut self) {
        self.current = self.spec.start;
    }

    /// The value `offset` steps from the current one (an object's own offset); the counter itself
    /// does not move.
    pub fn value_at_offset(&self, offset: i64) -> Result<i64, VariableError> {
        let n = self.spec.len();
        let target = self.position() + i128::from(offset);
        match self.spec.on_end {
            OnEnd::Wrap => Ok(self.spec.value(target.rem_euclid(n))),
            OnEnd::Stop if (0..n).contains(&target) => Ok(self.spec.value(target)),
            OnEnd::Stop => Err(VariableError::SerialExhausted),
        }
    }

    /// The value as text: zero-padded to the spec's width (after the sign), never truncated.
    pub fn format(&self, value: i64) -> String {
        let digits = value.unsigned_abs().to_string();
        let sign = if value < 0 { "-" } else { "" };
        format!("{sign}{digits:0>width$}", width = self.spec.width)
    }
}

/// Range of moments the clock formats: years 1 to 9999.
const MIN_SECONDS: i64 = -62_135_596_800;
const MAX_SECONDS: i64 = 253_402_300_799;

/// The moment a job was prepared and the fixed time zone its date and time fields use. The zone
/// is part of the job: opening the project elsewhere or in another season prints the same text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobClock {
    utc_seconds: i64,
    offset_minutes: i32,
}

struct Civil {
    year: i64,
    month: u32,
    day: u32,
    hour: i64,
    minute: i64,
    second: i64,
    ordinal: u32,
}

impl JobClock {
    pub fn new(utc_seconds: i64, offset_minutes: i32) -> Result<Self, VariableError> {
        if !(MIN_SECONDS..=MAX_SECONDS).contains(&utc_seconds) || offset_minutes.abs() >= 24 * 60 {
            return Err(VariableError::BadClock);
        }
        Ok(JobClock { utc_seconds, offset_minutes })
    }
    pub fn utc_seconds(&self) -> i64 {
        self.utc_seconds
    }
    pub fn offset_minutes(&self) -> i32 {
        self.offset_minutes
    }

    fn civil(&self) -> Civil {
        let local = self.utc_seconds + i64::from(self.offset_minutes) * 60;
        let days = local.div_euclid(86_400);
        let secs = local.rem_euclid(86_400);
        // Civil-from-days (proleptic Gregorian), shifted so that March is the first month.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
        let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
        let year = yoe + era * 400 + i64::from(month <= 2);
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        const BEFORE: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let ordinal = BEFORE[(month - 1) as usize] + day + u32::from(leap && month > 2);
        Civil { year, month, day, hour: secs / 3600, minute: secs % 3600 / 60, second: secs % 60, ordinal }
    }

    /// strftime-like: `%Y %y %m %d %H %M %S %j %z %%`.
    pub fn format(&self, format: &str) -> Result<String, VariableError> {
        validate_format(format)?;
        let c = self.civil();
        let mut out = String::new();
        let mut chars = format.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                out.push(ch);
                continue;
            }
            match chars.next() {
                Some('Y') => out.push_str(&format!("{:04}", c.year)),
                Some('y') => out.push_str(&format!("{:02}", c.year.rem_euclid(100))),
                Some('m') => out.push_str(&format!("{:02}", c.month)),
                Some('d') => out.push_str(&format!("{:02}", c.day)),
                Some('H') => out.push_str(&format!("{:02}", c.hour)),
                Some('M') => out.push_str(&format!("{:02}", c.minute)),
                Some('S') => out.push_str(&format!("{:02}", c.second)),
                Some('j') => out.push_str(&format!("{:03}", c.ordinal)),
                Some('z') => {
                    let sign = if self.offset_minutes < 0 { '-' } else { '+' };
                    let m = self.offset_minutes.abs();
                    out.push_str(&format!("{sign}{:02}{:02}", m / 60, m % 60));
                }
                _ => out.push('%'),
            }
        }
        Ok(out)
    }
}

fn validate_format(format: &str) -> Result<(), VariableError> {
    let mut it = format.char_indices();
    while let Some((at, ch)) = it.next() {
        if ch == '%' {
            match it.next() {
                Some((_, 'Y' | 'y' | 'm' | 'd' | 'H' | 'M' | 'S' | 'j' | 'z' | '%')) => {}
                _ => return Err(VariableError::BadDateFormat { at }),
            }
        }
    }
    Ok(())
}

/// Which cut-settings value a template refers to (for example "this layer's speed").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CutField {
    Speed,
    Power,
    MinPower,
    Passes,
    Interval,
    Name,
}

impl CutField {
    fn parse(s: &str) -> Option<CutField> {
        Some(match s {
            "speed" => CutField::Speed,
            "power" => CutField::Power,
            "min_power" => CutField::MinPower,
            "passes" => CutField::Passes,
            "interval" => CutField::Interval,
            "name" => CutField::Name,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Literal(String),
    Serial,
    Date(String),
    Csv(String),
    Cut { layer: String, field: CutField },
}

/// Default for `{date}`.
const DEFAULT_DATE: &str = "%Y-%m-%d";

/// A parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    tokens: Vec<Token>,
}

impl Template {
    /// `{{` and `}}` are literal braces; a field is `{serial}`, `{date}`, `{date:FORMAT}`,
    /// `{csv:COLUMN}` or `{cut:LAYER.FIELD}` (`speed`, `power`, `min_power`, `passes`, `interval`, `name`).
    pub fn parse(text: &str) -> Result<Template, VariableError> {
        let mut tokens = Vec::new();
        let mut literal = String::new();
        let mut it = text.char_indices().peekable();
        while let Some((at, ch)) = it.next() {
            match ch {
                '{' if it.peek().map(|p| p.1) == Some('{') => {
                    it.next();
                    literal.push('{');
                }
                '}' if it.peek().map(|p| p.1) == Some('}') => {
                    it.next();
                    literal.push('}');
                }
                '}' => return Err(VariableError::BadTemplate { at }),
                '{' => {
                    let rest = text.get(at + 1..).unwrap_or("");
                    let end = rest.find('}').ok_or(VariableError::BadTemplate { at })?;
                    let name = rest.get(..end).unwrap_or("");
                    if name.contains('{') {
                        return Err(VariableError::BadTemplate { at });
                    }
                    for _ in 0..name.chars().count() + 1 {
                        it.next();
                    }
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    tokens.push(parse_field(name, at)?);
                }
                c => literal.push(c),
            }
        }
        if !literal.is_empty() {
            tokens.push(Token::Literal(literal));
        }
        Ok(Template { tokens })
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// Whether anything in the template changes between jobs.
    pub fn is_dynamic(&self) -> bool {
        self.tokens.iter().any(|t| !matches!(t, Token::Literal(_)))
    }
}

fn parse_field(name: &str, at: usize) -> Result<Token, VariableError> {
    let unknown = || VariableError::UnknownToken { name: name.to_owned(), at };
    if name == "serial" {
        return Ok(Token::Serial);
    }
    if name == "date" {
        return Ok(Token::Date(DEFAULT_DATE.to_owned()));
    }
    if let Some(format) = name.strip_prefix("date:") {
        validate_format(format)?;
        return Ok(Token::Date(format.to_owned()));
    }
    if let Some(column) = name.strip_prefix("csv:") {
        return if column.is_empty() { Err(unknown()) } else { Ok(Token::Csv(column.to_owned())) };
    }
    if let Some(reference) = name.strip_prefix("cut:") {
        let (layer, field) = reference.rsplit_once('.').ok_or_else(unknown)?;
        let field = CutField::parse(field).ok_or_else(unknown)?;
        return if layer.is_empty() { Err(unknown()) } else { Ok(Token::Cut { layer: layer.to_owned(), field }) };
    }
    Err(unknown())
}
