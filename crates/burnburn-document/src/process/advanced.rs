//! Advanced process options (IM25): tabs, leads, overcut, perforation, Z steps between passes,
//! sub-operations and the shared-edge policy (IM32). Every option is validated here; whether a
//! machine may use it is the profile's capability matrix (`burnburn_job::MachineCapabilities`),
//! checked by the compiler. Defaults mean "off".

use burnburn_types::Length;

use super::{OperationMode, ProcessError, ProcessSettings};

fn positive(l: Length, field: &'static str) -> Result<f64, ProcessError> {
    if l.mm() > 0.0 { Ok(l.mm()) } else { Err(ProcessError::NotPositive { field }) }
}

/// Uncut bridges on every closed contour of a Line operation: `count` gaps of `width`, spaced
/// evenly by arc length; the first gap is centred `L / (2·count)` after the entry point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabSpec {
    count: u32,
    width_mm: f64,
}

impl TabSpec {
    pub const MAX_COUNT: u32 = 64;

    pub fn new(count: u32, width: Length) -> Result<Self, ProcessError> {
        if !(1..=Self::MAX_COUNT).contains(&count) {
            return Err(ProcessError::OutOfRange { field: "tab count" });
        }
        Ok(TabSpec { count, width_mm: positive(width, "tab width")? })
    }

    pub fn count(&self) -> u32 {
        self.count
    }
    pub fn width_mm(&self) -> f64 {
        self.width_mm
    }
}

/// A straight lead of `length`, perpendicular to the contour at the entry, on the waste side
/// (outside an outer contour, inside a hole).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lead {
    length_mm: f64,
}

impl Lead {
    pub fn new(length: Length) -> Result<Self, ProcessError> {
        Ok(Lead { length_mm: positive(length, "lead length")? })
    }

    pub fn length_mm(&self) -> f64 {
        self.length_mm
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

/// Line-only options. All `None` = a plain closed or open cut.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LineOptions {
    pub tabs: Option<TabSpec>,
    pub lead_in: Option<Lead>,
    pub lead_out: Option<Lead>,
    pub overcut: Option<Overcut>,
    pub perforation: Option<Perforation>,
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

/// Everything beyond mode and basic settings. `Default` is "nothing extra".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AdvancedOptions {
    pub line: LineOptions,
    pub z_step: Option<ZStep>,
    pub sub_operations: Vec<SubOperation>,
    pub shared_edges: SharedEdgePolicy,
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
        Ok(())
    }
}
