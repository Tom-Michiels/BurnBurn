//! Advanced process options (IM25): tabs, leads, overcut, perforation, Z steps between passes,
//! sub-operations and the shared-edge policy (IM32). Every option is validated here; whether a
//! machine may use it is the profile's capability matrix (`burnburn_job::MachineCapabilities`),
//! checked by the compiler. Defaults mean "off".

use burnburn_types::Length;

use super::{OperationMode, ProcessError, ProcessSettings};

fn positive(l: Length, field: &'static str) -> Result<f64, ProcessError> {
    if l.mm() > 0.0 { Ok(l.mm()) } else { Err(ProcessError::NotPositive { field }) }
}

/// How many tabs a contour gets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabPlacement {
    /// Exactly this many tabs per contour.
    Count(u32),
    /// One tab per `spacing` mm of contour length (rounded, at least one), evenly spread.
    Spacing(f64),
}

/// Uncut bridges on every closed contour of a Line operation: `n` gaps of `width`, spaced
/// evenly by arc length; the first gap is centred `L / (2·n)` after the entry point. `n` is the
/// fixed count or derived from the contour length and the spacing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabSpec {
    placement: TabPlacement,
    width_mm: f64,
    /// Percentage of the operation's power the tabs are cut with; `None` leaves them uncut.
    cut_power_percent: Option<f64>,
}

impl TabSpec {
    pub const MAX_COUNT: u32 = 64;

    pub fn new(count: u32, width: Length) -> Result<Self, ProcessError> {
        if !(1..=Self::MAX_COUNT).contains(&count) {
            return Err(ProcessError::OutOfRange { field: "tab count" });
        }
        Ok(TabSpec { placement: TabPlacement::Count(count), width_mm: positive(width, "tab width")?, cut_power_percent: None })
    }

    /// Tabs every `spacing` of contour length. The spacing must exceed the tab width.
    pub fn spaced(spacing: Length, width: Length) -> Result<Self, ProcessError> {
        let spacing_mm = positive(spacing, "tab spacing")?;
        let width_mm = positive(width, "tab width")?;
        if spacing_mm <= width_mm {
            return Err(ProcessError::OutOfRange { field: "tab spacing" });
        }
        Ok(TabSpec { placement: TabPlacement::Spacing(spacing_mm), width_mm, cut_power_percent: None })
    }

    pub fn placement(&self) -> TabPlacement {
        self.placement
    }

    /// The fixed tab count, or 0 for spacing-based tabs (see [`TabSpec::placement`]).
    pub fn count(&self) -> u32 {
        match self.placement {
            TabPlacement::Count(n) => n,
            TabPlacement::Spacing(_) => 0,
        }
    }
    pub fn width_mm(&self) -> f64 {
        self.width_mm
    }

    /// The same tabs, cut at `percent` of the operation's power instead of skipped (C04).
    pub fn with_cut_power(mut self, percent: f64) -> Result<Self, ProcessError> {
        // 0 is "skip" (leave it unset) and 100 would be no tab at all.
        if !(percent.is_finite() && percent > 0.0 && percent < 100.0) {
            return Err(ProcessError::OutOfRange { field: "tab cut power" });
        }
        self.cut_power_percent = Some(percent);
        Ok(self)
    }

    /// The same tabs, skipped again.
    pub fn without_cut_power(mut self) -> Self {
        self.cut_power_percent = None;
        self
    }

    /// The share of the operation's power the tabs are cut with, or `None` when they are skipped.
    pub fn cut_power_percent(&self) -> Option<f64> {
        self.cut_power_percent
    }
}

/// Common-line cutting (C12): where the toolpaths of different shapes of a Line operation coincide
/// within `tolerance` over at least `min_overlap`, the stretch is cut once. Parts that touch (or
/// lie a kerf apart) then share one cut. Needs the `CommonLine` capability of the machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommonLine {
    tolerance_mm: f64,
    min_overlap_mm: f64,
}

impl CommonLine {
    /// Largest tolerance (mm): beyond it two lines are no longer "the same line".
    pub const MAX_TOLERANCE_MM: f64 = 5.0;

    pub fn new(tolerance: Length, min_overlap: Length) -> Result<Self, ProcessError> {
        let (tolerance_mm, min_overlap_mm) = (tolerance.mm(), min_overlap.mm());
        if !(tolerance_mm > 0.0 && tolerance_mm <= Self::MAX_TOLERANCE_MM) {
            return Err(ProcessError::OutOfRange { field: "common-line tolerance" });
        }
        if !(min_overlap_mm >= 0.0) {
            return Err(ProcessError::OutOfRange { field: "common-line overlap" });
        }
        Ok(CommonLine { tolerance_mm, min_overlap_mm })
    }

    pub fn tolerance_mm(&self) -> f64 {
        self.tolerance_mm
    }

    pub fn min_overlap_mm(&self) -> f64 {
        self.min_overlap_mm
    }
}

/// Shape of a lead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LeadStyle {
    /// A straight line at [`Lead::angle_deg`] to the contour.
    #[default]
    Line,
    /// A quarter circle of radius `length`, tangent to the contour at the entry or exit.
    Arc,
}

/// A lead of `length` on the waste side of a closed contour (outside an outer contour, inside a
/// hole). A line lead makes `angle_deg` with the contour tangent at the entry or exit (90° =
/// perpendicular, the default); an arc lead is tangent and has `length` as its radius. On an
/// open contour a line lead continues the first or last segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lead {
    length_mm: f64,
    style: LeadStyle,
    angle_deg: f64,
}

impl Lead {
    /// A perpendicular line lead.
    pub fn new(length: Length) -> Result<Self, ProcessError> {
        Ok(Lead { length_mm: positive(length, "lead length")?, style: LeadStyle::Line, angle_deg: 90.0 })
    }

    /// The same lead with another shape.
    pub fn with_style(mut self, style: LeadStyle) -> Self {
        self.style = style;
        self
    }

    /// The same lead at `degrees` to the contour tangent, in `(0, 90]`.
    pub fn with_angle(mut self, degrees: f64) -> Result<Self, ProcessError> {
        if !(degrees.is_finite() && degrees > 0.0 && degrees <= 90.0) {
            return Err(ProcessError::OutOfRange { field: "lead angle" });
        }
        self.angle_deg = degrees;
        Ok(self)
    }

    pub fn length_mm(&self) -> f64 {
        self.length_mm
    }
    pub fn style(&self) -> LeadStyle {
        self.style
    }
    pub fn angle_deg(&self) -> f64 {
        self.angle_deg
    }
}

/// Dot (dwell) mode: instead of a continuous cut the laser fires a pulse of `dwell_ms` every
/// `spacing` of contour length, starting at the entry point. Excludes the other line options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DotMode {
    spacing_mm: f64,
    dwell_ms: f64,
}

impl DotMode {
    /// Longest pulse accepted (10 s).
    pub const MAX_DWELL_MS: f64 = 10_000.0;

    pub fn new(spacing: Length, dwell_ms: f64) -> Result<Self, ProcessError> {
        if !(dwell_ms.is_finite() && dwell_ms > 0.0 && dwell_ms <= Self::MAX_DWELL_MS) {
            return Err(ProcessError::OutOfRange { field: "dot dwell" });
        }
        Ok(DotMode { spacing_mm: positive(spacing, "dot spacing")?, dwell_ms })
    }

    pub fn spacing_mm(&self) -> f64 {
        self.spacing_mm
    }
    pub fn dwell_ms(&self) -> f64 {
        self.dwell_ms
    }
}

/// Cutting `length` past the entry point of a closed contour along its own path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overcut {
    length_mm: f64,
}

impl Overcut {
    pub fn new(length: Length) -> Result<Self, ProcessError> {
        Ok(Overcut { length_mm: positive(length, "overcut")? })
    }

    pub fn length_mm(&self) -> f64 {
        self.length_mm
    }
}

/// Alternating `on` (laser on) and `off` (laser off) lengths along the cut path, starting with
/// `on` at the entry point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Perforation {
    on_mm: f64,
    off_mm: f64,
}

impl Perforation {
    pub fn new(on: Length, off: Length) -> Result<Self, ProcessError> {
        Ok(Perforation { on_mm: positive(on, "perforation on")?, off_mm: positive(off, "perforation off")? })
    }

    pub fn on_mm(&self) -> f64 {
        self.on_mm
    }
    pub fn off_mm(&self) -> f64 {
        self.off_mm
    }
}

/// Focus change per pass after the first (positive = deeper into the material), `|z| <= 50` mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZStep {
    mm: f64,
}

impl ZStep {
    pub fn new(step: Length) -> Result<Self, ProcessError> {
        let mm = step.mm();
        if mm == 0.0 || mm.abs() > 50.0 {
            return Err(ProcessError::OutOfRange { field: "z step" });
        }
        Ok(ZStep { mm })
    }

    pub fn mm(&self) -> f64 {
        self.mm
    }
}

/// Focus (Z) offset of the whole operation from the start focus (positive = deeper into the
/// material), `0 < |z| <= 50` mm; no offset is `None`. The Z step per pass adds to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZOffset {
    mm: f64,
}

impl ZOffset {
    pub fn new(offset: Length) -> Result<Self, ProcessError> {
        let mm = offset.mm();
        if mm == 0.0 || mm.abs() > 50.0 {
            return Err(ProcessError::OutOfRange { field: "z offset" });
        }
        Ok(ZOffset { mm })
    }

    pub fn mm(&self) -> f64 {
        self.mm
    }
}

/// Line-only options. All `None` = a plain closed or open cut.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LineOptions {
    pub tabs: Option<TabSpec>,
    pub lead_in: Option<Lead>,
    pub lead_out: Option<Lead>,
    pub overcut: Option<Overcut>,
    pub perforation: Option<Perforation>,
    pub dot: Option<DotMode>,
}

/// What happens to a boundary segment that two target geometries of one Line operation share
/// exactly (for example two colour regions of a trace, IM32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SharedEdgePolicy {
    /// Cut it once; the second occurrence becomes laser-off travel (preflight warns).
    #[default]
    CutOnce,
    /// Cut it for every region (explicit choice).
    CutPerRegion,
}

/// A child operation on the parent's targets with its own mode and settings, executed right
/// after the parent (LightBurn sub-layers).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubOperation {
    pub mode: OperationMode,
    pub settings: ProcessSettings,
}

/// A tool layer (LightBurn T1/T2): geometry kept in the document for guides and registration.
/// It is drawn and linked like any layer but is never part of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolLayer {
    T1,
    T2,
}

impl ToolLayer {
    pub const ALL: [ToolLayer; 2] = [ToolLayer::T1, ToolLayer::T2];

    /// The name LightBurn gives the layer.
    pub fn name(self) -> &'static str {
        match self {
            ToolLayer::T1 => "T1",
            ToolLayer::T2 => "T2",
        }
    }
}

/// Everything beyond mode and basic settings. `Default` is "nothing extra".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AdvancedOptions {
    pub line: LineOptions,
    pub z_step: Option<ZStep>,
    /// Focus offset of the operation (W2-L3); needs the same Z capability as the step.
    pub z_offset: Option<ZOffset>,
    pub sub_operations: Vec<SubOperation>,
    pub shared_edges: SharedEdgePolicy,
    /// Set on a tool layer (T1/T2): no output, no cut options.
    pub tool: Option<ToolLayer>,
    /// Common-line cutting of the shapes of a Line operation (C12); off by default.
    pub common_line: Option<CommonLine>,
}

impl AdvancedOptions {
    /// Upper bound on sub-operations per operation.
    pub const MAX_SUB_OPERATIONS: usize = 16;

    pub fn is_default(&self) -> bool {
        *self == AdvancedOptions::default()
    }

    pub(crate) fn validate(&self) -> Result<(), ProcessError> {
        if self.sub_operations.len() > Self::MAX_SUB_OPERATIONS {
            return Err(ProcessError::OutOfRange { field: "sub-operations" });
        }
        let l = &self.line;
        if self.tool.is_some() {
            // A tool layer is never cut: nothing that shapes a cut can be set on it.
            let cut_options = [
                (l.tabs.is_some(), "tabs"),
                (l.lead_in.is_some(), "lead-in"),
                (l.lead_out.is_some(), "lead-out"),
                (l.overcut.is_some(), "overcut"),
                (l.perforation.is_some(), "perforation"),
                (l.dot.is_some(), "dot mode"),
                (self.z_step.is_some(), "z step"),
                (self.z_offset.is_some(), "z offset"),
                (!self.sub_operations.is_empty(), "sub-operations"),
                (self.common_line.is_some(), "common line"),
            ];
            if let Some((_, second)) = cut_options.into_iter().find(|(on, _)| *on) {
                return Err(ProcessError::ConflictingOptions { first: "tool layer", second });
            }
        }
        if l.dot.is_some() {
            let others = [
                (l.tabs.is_some(), "tabs"),
                (l.lead_in.is_some(), "lead-in"),
                (l.lead_out.is_some(), "lead-out"),
                (l.overcut.is_some(), "overcut"),
                (l.perforation.is_some(), "perforation"),
                (self.common_line.is_some(), "common line"),
            ];
            if let Some((_, second)) = others.into_iter().find(|(on, _)| *on) {
                return Err(ProcessError::ConflictingOptions { first: "dot mode", second });
            }
        }
        Ok(())
    }
}
