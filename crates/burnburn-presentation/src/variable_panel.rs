//! The variable-text panel (W3-UI-tekst, E11): Normal/Variable, the serial counter, the data
//! rows (CSV), Test and Bake. Counters, the chosen data row and the series settings are window
//! state until the project file carries them (queue W2-L5c); the text's own type and offset are
//! document data (one undo step). Raw field text goes to the controller unparsed (DR-05).

pub use burnburn_generators::AdvancePolicy;

/// A field of the serial-number settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SerialField {
    /// The value the next job prints; must be a value of the series.
    Current,
    Start,
    End,
    AdvanceBy,
    /// Zero-padded width (0 = no padding).
    Digits,
}

/// What went wrong with variable text; the host shows it in words (with [`VariableProblem::detail`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VariableIssue {
    /// The field does not hold a whole number.
    InvalidNumber(SerialField),
    /// Start, End and Advance by do not form a series (zero step, end on the wrong side).
    InvalidSerial,
    /// The current value is not a value of the series.
    NotInSeries,
    /// The series is finished (it stops at its end).
    SerialExhausted,
    /// The offset of the text is not a whole number.
    InvalidOffset,
    /// The chosen data row is not a number from 1 to the number of rows.
    RowOutOfRange,
    /// The file is not a CSV this app can read (not UTF-8, ragged rows, empty).
    CsvUnreadable,
    /// The template names a column the data does not have.
    MissingColumn,
    /// The cell of the chosen row is empty: no label is made with a blank field.
    MissingValue,
    /// The template needs a data row but no CSV is chosen.
    NoData,
    /// The template needs a serial number but none is set up.
    NoSerial,
    /// The template is wrong (unbalanced brace, unknown field or date format).
    BadTemplate,
    /// A `{cut:..}` reference names a layer or field with no value.
    NoCutValue,
    /// No clock is attached, so `{date}` cannot be resolved.
    NoClock,
    /// No font source is attached, so the resolved text cannot be drawn.
    NoFonts,
    /// The resolved text could not be drawn (its font is missing).
    FontMissing,
    /// The document refused the edit.
    Refused,
    /// The counters moved after this job was prepared.
    StaleJob,
}

/// A problem with the place it was found: the column and row of a missing value, the template
/// position, the name of the text. Empty when there is nothing more to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableProblem {
    pub issue: VariableIssue,
    pub detail: String,
}

/// The serial-number settings and the live counter, as the fields show them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialViewModel {
    pub current: String,
    pub start: String,
    pub end: String,
    pub advance_by: String,
    pub digits: String,
    /// After the end the series starts again (otherwise it stops and refuses).
    pub wrap: bool,
}

/// The chosen CSV file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvViewModel {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: usize,
    /// The row the next job uses, counted from 1.
    pub row: String,
}

/// One text of the document with the result of a Test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestLine {
    /// The template as typed.
    pub template: String,
    pub result: Result<String, VariableProblem>,
}

/// The panel for the one selected text object.
#[derive(Debug, Clone, PartialEq)]
pub struct VariablePanelViewModel {
    /// Type: Variable (the content is a template) or Normal (literal).
    pub variable: bool,
    /// The text's own serial offset in series steps (Variable only).
    pub offset: String,
    pub serial: SerialViewModel,
    pub policy: AdvancePolicy,
    pub csv: Option<CsvViewModel>,
    /// What the selected text prints now, with the live counters (Variable only).
    pub preview: Option<Result<String, VariableProblem>>,
    /// The latest Test of all variable texts of the document; cleared by the next change.
    pub test: Option<Vec<TestLine>>,
    /// Why the last input was refused, or why the last compile was.
    pub problem: Option<VariableProblem>,
    /// The record id (hex) of the plan the last compile was bound to, if it used variable text.
    pub bound_record: Option<String>,
}

/// One edit of the variable-text panel.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum VariableEdit {
    /// Normal ⇄ Variable: one `TextCommand::Set`.
    SetVariable(bool),
    /// The text's offset (raw text): one `TextCommand::Set`.
    Offset(String),
    /// A series setting or the current value (raw text).
    Serial(SerialField, String),
    /// After the end the series starts again (`true`) or stops (`false`).
    Wrap(bool),
    /// Auto-Advance: the job event the counters move on.
    Policy(AdvancePolicy),
    Previous,
    Next,
    Reset,
    /// The row the next job uses, counted from 1 (raw text).
    Row(String),
    /// A CSV chosen by the host: its display name and bytes (read by the host).
    LoadCsv {
        name: String,
        bytes: Vec<u8>,
    },
    ClearCsv,
    /// Resolve every variable text with the live counters; shows the result or the problem.
    Test,
    /// Replace the template by the resolved text: one `TextCommand::Set`.
    Bake,
}
