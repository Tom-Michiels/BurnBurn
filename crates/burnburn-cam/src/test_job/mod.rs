//! Material Test, Interval Test and Focus Test (W2-L3): small test documents generated *beside*
//! the running document and compiled by the same compiler.
//!
//! A generator never touches the user's document. It builds a private [`TestJob`] — its own
//! session with a grid of shapes and one operation per cell — and [`TestJob::compile`] runs the
//! ordinary [`compile`](crate::compile) on it, so a test job is a normal immutable `JobPlan` for
//! preview, RD export and upload, subject to the same capability checks. The temporary settings
//! live only in that private document. Nothing here talks to a machine.

use burnburn_document::process::{
    AdvancedOptions, FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange,
    ProcessCommand, ProcessError, ProcessSettings, ScanAxis, ScanDirection, Speed, ZOffset,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, DocumentSnapshot, GeometryCommand};
use burnburn_geometry::{Path, PathBuilder, Point, Rect};
use burnburn_job::{Feature, JobPlan, MachineProfile, Support};
use burnburn_types::{CancellationToken, DocumentId, GeometryId, Length, OperationId, RequestId, StableId};

use crate::{CompileError, CompileOptions};

mod strokes;

/// The setting a test axis varies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestParameter {
    /// Head speed, mm/s.
    Speed,
    /// Maximum power, percent (the minimum power stays at or below it).
    Power,
    /// Number of passes; every value must be a whole number.
    Passes,
    /// Scan line interval, mm (Fill cells only).
    Interval,
    /// Focus offset, mm (needs the Z capability).
    ZOffset,
}

/// A parameter and the `count` values it takes, evenly spread from `min` to `max` inclusive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestAxis {
    pub parameter: TestParameter,
    pub min: f64,
    pub max: f64,
    pub count: u32,
}

/// What one cell of a material test burns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestCellMode {
    /// The outline of the cell is cut.
    Line,
    /// The cell is filled at `interval` with `overscan`.
    Fill { interval: Length, overscan: Length },
}

/// Digit labels next to the grid, cut with their own settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestLabels {
    pub height: Length,
    pub settings: ProcessSettings,
}

/// A rectangle around everything, `margin` outside the content, cut last.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestBorder {
    pub margin: Length,
    pub settings: ProcessSettings,
}

/// A grid of cells with one parameter per column and (optionally) one per row. The first row
/// is at the top, the first column on the left.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialTestSpec {
    pub mode: TestCellMode,
    pub columns: TestAxis,
    pub rows: Option<TestAxis>,
    /// Settings of every cell before its axis values replace theirs.
    pub base: ProcessSettings,
    pub cell_width: Length,
    pub cell_height: Length,
    pub gap: Length,
    pub labels: Option<TestLabels>,
    pub border: Option<TestBorder>,
}

/// A row of fill cells with a different scan interval each. Dithered images need a raster
/// source and are not covered; the cells are plain fills.
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalTestSpec {
    pub min_interval: Length,
    pub max_interval: Length,
    pub count: u32,
    pub overscan: Length,
    pub base: ProcessSettings,
    pub cell_width: Length,
    pub cell_height: Length,
    pub gap: Length,
    pub labels: Option<TestLabels>,
    pub border: Option<TestBorder>,
}

/// Parallel lines, each cut at its own focus offset `z_start + i · z_step`. Needs a profile
/// that supports Z.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusTestSpec {
    pub z_start: f64,
    pub z_step: f64,
    pub count: u32,
    pub line_length: Length,
    pub spacing: Length,
    pub base: ProcessSettings,
    pub labels: Option<TestLabels>,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TestError {
    /// `count` must be `1..=MAX_AXIS_COUNT`.
    Count {
        axis: &'static str,
    },
    /// A value is not finite, `min > max`, or a value is outside the parameter's range.
    Range {
        axis: &'static str,
        parameter: TestParameter,
    },
    /// A value that must be whole (passes) is not.
    NotWhole {
        axis: &'static str,
        value: f64,
    },
    /// The parameter does not apply to the cell mode (interval on a line cell).
    ParameterNotForMode {
        parameter: TestParameter,
    },
    /// Columns and rows vary the same parameter.
    SameParameter(TestParameter),
    /// A size is not positive.
    Size {
        field: &'static str,
    },
    TooManyCells {
        cells: u32,
    },
    /// The profile does not support the feature the test needs.
    Unsupported {
        feature: Feature,
        support: Support,
    },
    Process(ProcessError),
    /// The private test document refused a command.
    Document(String),
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Count { axis } => write!(f, "{axis}: the value count is out of range"),
            TestError::Range { axis, parameter } => write!(f, "{axis}: {parameter:?} values are out of range"),
            TestError::NotWhole { axis, value } => write!(f, "{axis}: {value} is not a whole number"),
            TestError::ParameterNotForMode { parameter } => write!(f, "{parameter:?} does not apply to this mode"),
            TestError::SameParameter(p) => write!(f, "columns and rows both vary {p:?}"),
            TestError::Size { field } => write!(f, "{field} must be positive"),
            TestError::TooManyCells { cells } => write!(f, "{cells} cells is more than {MAX_CELLS}"),
            TestError::Unsupported { feature, .. } => write!(f, "the machine profile does not support {feature:?}"),
            TestError::Process(e) => write!(f, "process: {e}"),
            TestError::Document(e) => write!(f, "test document: {e}"),
        }
    }
}

impl std::error::Error for TestError {}

impl From<ProcessError> for TestError {
    fn from(e: ProcessError) -> Self {
        TestError::Process(e)
    }
}

/// Most values on one axis.
pub const MAX_AXIS_COUNT: u32 = 25;
/// Most cells of one test: every cell is a settings block of the plan (RD allows 128 parts).
pub const MAX_CELLS: u32 = 100;

/// One generated cell (or focus line).
#[derive(Debug, Clone, PartialEq)]
pub struct TestCell {
    pub column: u32,
    pub row: u32,
    pub column_value: f64,
    pub row_value: Option<f64>,
    pub geometry: GeometryId,
    pub operation: OperationId,
}

/// A generated test: a private document plus the cells in it.
#[derive(Debug, Clone)]
pub struct TestJob {
    snapshot: std::sync::Arc<DocumentSnapshot>,
    cells: Vec<TestCell>,
}

impl TestJob {
    /// The private test document; unrelated to any open document.
    pub fn snapshot(&self) -> &DocumentSnapshot {
        &self.snapshot
    }

    pub fn cells(&self) -> &[TestCell] {
        &self.cells
    }

    /// Compiles the test with the ordinary compiler: same capability checks, same plan type.
    pub fn compile(
        &self,
        profile: &MachineProfile,
        options: &CompileOptions,
        cancel: &CancellationToken,
    ) -> Result<JobPlan, CompileError> {
        crate::compile(&self.snapshot, profile, options, cancel)
    }
}

/// Space between a label and what it names, and between labels and the grid (mm).
const LABEL_GAP_MM: f64 = 1.0;

fn mm(v: f64) -> Result<Length, TestError> {
    Length::from_mm(v).map_err(|_| TestError::Size { field: "length" })
}

fn positive(l: Length, field: &'static str) -> Result<f64, TestError> {
    if l.mm() > 0.0 { Ok(l.mm()) } else { Err(TestError::Size { field }) }
}

/// The private document a test is built in.
struct Scratch {
    session: DocumentSession,
    cells: Vec<TestCell>,
    request: u64,
}

impl Scratch {
    fn new() -> Self {
        Scratch { session: DocumentSession::new(DocumentId::from_raw(1)), cells: Vec::new(), request: 0 }
    }

    fn apply(&mut self, payload: CommandPayload) -> Result<burnburn_document::CommandOutcome, TestError> {
        self.request += 1;
        let cmd = DocumentCommand {
            document: self.session.id(),
            expected: self.session.revision(),
            request: RequestId::from_raw(self.request),
            payload,
        };
        self.session.apply(cmd).map_err(|e| TestError::Document(e.to_string()))
    }

    fn rectangle(&mut self, x: f64, y: f64, w: f64, h: f64) -> Result<GeometryId, TestError> {
        let rect = Rect::new(Point::new(x, y), mm(w)?, mm(h)?).map_err(|e| TestError::Document(e.to_string()))?;
        Ok(self.apply(CommandPayload::CreateRectangle { rect, name: String::new() })?.changes.created[0])
    }

    fn path(&mut self, path: Path) -> Result<GeometryId, TestError> {
        Ok(self.apply(CommandPayload::Edit(GeometryCommand::CreatePath { path, name: String::new() }))?.changes.created
            [0])
    }

    fn operation(
        &mut self,
        draft: OperationDraft,
        targets: Vec<GeometryId>,
        z_offset: Option<ZOffset>,
    ) -> Result<OperationId, TestError> {
        let operation =
            self.apply(CommandPayload::Process(ProcessCommand::AddOperation { draft, targets }))?.changes.operations[0];
        if z_offset.is_some() {
            let advanced = AdvancedOptions { z_offset, ..Default::default() };
            self.apply(CommandPayload::Process(ProcessCommand::SetAdvanced { operation, advanced }))?;
        }
        Ok(operation)
    }

    /// One Line operation cutting every label stroke.
    fn labels(&mut self, name: &str, paths: Vec<Path>, settings: ProcessSettings) -> Result<(), TestError> {
        let mut ids = Vec::with_capacity(paths.len());
        for p in paths {
            ids.push(self.path(p)?);
        }
        if ids.is_empty() {
            return Ok(());
        }
        let draft = line_draft(name, settings);
        self.operation(draft, ids, None)?;
        Ok(())
    }

    fn finish(self) -> TestJob {
        TestJob { snapshot: self.session.snapshot(), cells: self.cells }
    }
}

fn line_draft(name: &str, settings: ProcessSettings) -> OperationDraft {
    OperationDraft { name: name.into(), mode: OperationMode::Line { kerf: Kerf::NONE }, settings, output_enabled: true }
}

/// Rounds away float noise of `min + k · step` (nine decimals are far below any machine step).
fn clean(v: f64) -> f64 {
    let r = (v * 1e9).round() / 1e9;
    if r == 0.0 { 0.0 } else { r }
}

/// The values of one axis, validated for the parameter and the cell mode.
fn axis_values(axis: &TestAxis, name: &'static str, fill: bool) -> Result<Vec<f64>, TestError> {
    if !(1..=MAX_AXIS_COUNT).contains(&axis.count) {
        return Err(TestError::Count { axis: name });
    }
    let range = TestError::Range { axis: name, parameter: axis.parameter };
    if !(axis.min.is_finite() && axis.max.is_finite()) || (axis.count > 1 && axis.min >= axis.max) {
        return Err(range);
    }
    if axis.parameter == TestParameter::Interval && !fill {
        return Err(TestError::ParameterNotForMode { parameter: axis.parameter });
    }
    let n = axis.count;
    let values: Vec<f64> = (0..n)
        .map(|k| match k {
            0 => axis.min,
            k if k + 1 == n => axis.max,
            k => clean(axis.min + (axis.max - axis.min) * f64::from(k) / f64::from(n - 1)),
        })
        .collect();
    for &v in &values {
        let ok = match axis.parameter {
            TestParameter::Speed | TestParameter::Interval => v > 0.0,
            TestParameter::Power => (0.0..=100.0).contains(&v),
            TestParameter::Passes => v >= 1.0 && v <= f64::from(Passes::MAX),
            TestParameter::ZOffset => v.abs() <= 50.0,
        };
        if !ok {
            return Err(range);
        }
        if axis.parameter == TestParameter::Passes && (v - v.round()).abs() > 1e-9 {
            return Err(TestError::NotWhole { axis: name, value: v });
        }
    }
    Ok(values)
}

/// One cell's settings: the base with up to two parameter values applied.
#[derive(Clone, Copy)]
struct CellSettings {
    process: ProcessSettings,
    interval_mm: Option<f64>,
    z_offset_mm: f64,
}

impl CellSettings {
    fn with(mut self, parameter: TestParameter, v: f64) -> Result<Self, TestError> {
        match parameter {
            TestParameter::Speed => self.process.speed = Speed::mm_per_s(v)?,
            TestParameter::Power => {
                let max = PowerPercent::new(v)?;
                let min = PowerPercent::new(self.process.power.min().get().min(v))?;
                self.process.power = PowerRange::new(min, max)?;
            }
            TestParameter::Passes => self.process.passes = Passes::new(v.round() as u32)?,
            TestParameter::Interval => self.interval_mm = Some(v),
            TestParameter::ZOffset => self.z_offset_mm = v,
        }
        Ok(self)
    }
}

fn label_path(text: &str, x: f64, y: f64, height: f64) -> Option<Path> {
    strokes::text_path(text, x, y, height)
}

/// Builds a material test: see [`MaterialTestSpec`].
pub fn generate_material_test(spec: &MaterialTestSpec) -> Result<TestJob, TestError> {
    let fill = matches!(spec.mode, TestCellMode::Fill { .. });
    let columns = axis_values(&spec.columns, "columns", fill)?;
    let rows = match &spec.rows {
        Some(a) => Some(axis_values(a, "rows", fill)?),
        None => None,
    };
    if let Some(r) = &spec.rows
        && r.parameter == spec.columns.parameter
    {
        return Err(TestError::SameParameter(r.parameter));
    }
    let (w, h, gap) = (
        positive(spec.cell_width, "cell width")?,
        positive(spec.cell_height, "cell height")?,
        positive(spec.gap, "gap")?,
    );
    let row_count = rows.as_ref().map_or(1, Vec::len);
    let cells = columns.len() * row_count;
    if cells > MAX_CELLS as usize {
        return Err(TestError::TooManyCells { cells: cells as u32 });
    }
    let label_height = spec.labels.map(|l| positive(l.height, "label height")).transpose()?;
    let margin = spec.border.map(|b| positive(b.margin, "border margin")).transpose()?.unwrap_or(0.0);

    let row_texts: Vec<String> = rows.iter().flatten().map(|v| strokes::format_value(*v)).collect();
    let column_texts: Vec<String> = columns.iter().map(|v| strokes::format_value(*v)).collect();
    let row_label_width = label_height
        .map(|lh| row_texts.iter().map(|t| strokes::text_width(t, lh)).fold(0.0, f64::max))
        .filter(|w| *w > 0.0)
        .map_or(0.0, |w| w + LABEL_GAP_MM);
    let (gx, gy) = (margin + row_label_width, margin);
    let grid_w = columns.len() as f64 * w + (columns.len() - 1) as f64 * gap;
    let grid_h = row_count as f64 * h + (row_count - 1) as f64 * gap;

    let mut scratch = Scratch::new();
    let base = CellSettings { process: spec.base, interval_mm: None, z_offset_mm: 0.0 };
    for r in 0..row_count {
        let y = gy + (row_count - 1 - r) as f64 * (h + gap);
        for (c, &column_value) in columns.iter().enumerate() {
            let x = gx + c as f64 * (w + gap);
            let mut cell = base.with(spec.columns.parameter, column_value)?;
            let row_value = rows.as_ref().map(|v| v[r]);
            if let (Some(a), Some(v)) = (&spec.rows, row_value) {
                cell = cell.with(a.parameter, v)?;
            }
            let mode = match spec.mode {
                TestCellMode::Line => OperationMode::Line { kerf: Kerf::NONE },
                TestCellMode::Fill { interval, overscan } => {
                    let interval = match cell.interval_mm {
                        Some(v) => mm(v)?,
                        None => interval,
                    };
                    OperationMode::Fill(FillSettings::new(
                        interval,
                        ScanAxis::X,
                        ScanDirection::Bidirectional,
                        overscan,
                    )?)
                }
            };
            let geometry = scratch.rectangle(x, y, w, h)?;
            let draft = OperationDraft {
                name: format!("Test {}/{}", c + 1, r + 1),
                mode,
                settings: cell.process,
                output_enabled: true,
            };
            let z = if cell.z_offset_mm == 0.0 { None } else { Some(ZOffset::new(mm(cell.z_offset_mm)?)?) };
            let operation = scratch.operation(draft, vec![geometry], z)?;
            scratch.cells.push(TestCell {
                column: c as u32,
                row: r as u32,
                column_value,
                row_value,
                geometry,
                operation,
            });
        }
    }

    let mut top = gy + grid_h;
    if let (Some(labels), Some(lh)) = (spec.labels, label_height) {
        let mut paths = Vec::new();
        let label_y = gy + grid_h + LABEL_GAP_MM;
        for (c, text) in column_texts.iter().enumerate() {
            let centre = gx + c as f64 * (w + gap) + w / 2.0;
            let x = (centre - strokes::text_width(text, lh) / 2.0).max(margin);
            paths.extend(label_path(text, x, label_y, lh));
        }
        for (r, text) in row_texts.iter().enumerate() {
            let y = gy + (row_count - 1 - r) as f64 * (h + gap) + (h - lh) / 2.0;
            paths.extend(label_path(text, gx - LABEL_GAP_MM - strokes::text_width(text, lh), y, lh));
        }
        scratch.labels("Labels", paths, labels.settings)?;
        top = label_y + lh;
    }
    if let Some(border) = spec.border {
        let right = gx + grid_w + margin;
        let geometry = scratch.rectangle(0.0, 0.0, right, top + margin)?;
        scratch.operation(line_draft("Border", border.settings), vec![geometry], None)?;
    }
    Ok(scratch.finish())
}

/// Builds an interval test: one fill cell per interval, from `min_interval` to `max_interval`.
pub fn generate_interval_test(spec: &IntervalTestSpec) -> Result<TestJob, TestError> {
    let (lo, hi) = (positive(spec.min_interval, "minimum interval")?, positive(spec.max_interval, "maximum interval")?);
    generate_material_test(&MaterialTestSpec {
        mode: TestCellMode::Fill { interval: spec.min_interval, overscan: spec.overscan },
        columns: TestAxis { parameter: TestParameter::Interval, min: lo, max: hi, count: spec.count },
        rows: None,
        base: spec.base,
        cell_width: spec.cell_width,
        cell_height: spec.cell_height,
        gap: spec.gap,
        labels: spec.labels,
        border: spec.border,
    })
}

/// Builds a focus test for a profile with Z support; any other profile is refused up front.
pub fn generate_focus_test(spec: &FocusTestSpec, profile: &MachineProfile) -> Result<TestJob, TestError> {
    let support = profile.capabilities().support(Feature::ZStep);
    if support != Support::Supported {
        return Err(TestError::Unsupported { feature: Feature::ZStep, support });
    }
    if !(1..=MAX_AXIS_COUNT).contains(&spec.count) {
        return Err(TestError::Count { axis: "focus lines" });
    }
    if !(spec.z_start.is_finite() && spec.z_step.is_finite() && spec.z_step != 0.0) {
        return Err(TestError::Range { axis: "focus lines", parameter: TestParameter::ZOffset });
    }
    let (length, spacing) = (positive(spec.line_length, "line length")?, positive(spec.spacing, "spacing")?);
    let offsets: Vec<f64> = (0..spec.count).map(|i| clean(spec.z_start + spec.z_step * f64::from(i))).collect();
    if offsets.iter().any(|z| z.abs() > 50.0) {
        return Err(TestError::Range { axis: "focus lines", parameter: TestParameter::ZOffset });
    }
    let label_height = spec.labels.map(|l| positive(l.height, "label height")).transpose()?;
    let texts: Vec<String> = offsets.iter().map(|z| strokes::format_value(*z)).collect();
    let label_width = label_height
        .map(|lh| texts.iter().map(|t| strokes::text_width(t, lh)).fold(0.0, f64::max))
        .filter(|w| *w > 0.0)
        .map_or(0.0, |w| w + LABEL_GAP_MM);

    let mut scratch = Scratch::new();
    let n = offsets.len();
    for (i, &z) in offsets.iter().enumerate() {
        let y = (n - 1 - i) as f64 * spacing + label_height.unwrap_or(0.0) / 2.0;
        let line = PathBuilder::new()
            .move_to(Point::new(label_width, y))
            .line_to(Point::new(label_width + length, y))
            .build()
            .map_err(|e| TestError::Document(e.to_string()))?;
        let geometry = scratch.path(line)?;
        let draft = line_draft(&format!("Focus {}", texts[i]), spec.base);
        let z_offset = if z == 0.0 { None } else { Some(ZOffset::new(mm(z)?)?) };
        let operation = scratch.operation(draft, vec![geometry], z_offset)?;
        scratch.cells.push(TestCell {
            column: i as u32,
            row: 0,
            column_value: z,
            row_value: None,
            geometry,
            operation,
        });
    }
    if let (Some(labels), Some(lh)) = (spec.labels, label_height) {
        let mut paths = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            let y = (n - 1 - i) as f64 * spacing;
            paths.extend(label_path(text, label_width - LABEL_GAP_MM - strokes::text_width(text, lh), y, lh));
        }
        scratch.labels("Labels", paths, labels.settings)?;
    }
    Ok(scratch.finish())
}
