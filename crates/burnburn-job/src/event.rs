use std::fmt;

use burnburn_types::{GeometryId, OperationId};

/// A position in the machine frame, in millimetres. Always finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MachinePoint {
    x: f64,
    y: f64,
}

impl MachinePoint {
    pub fn new(x_mm: f64, y_mm: f64) -> Result<Self, PlanError> {
        if x_mm.is_finite() && y_mm.is_finite() {
            Ok(MachinePoint { x: x_mm, y: y_mm })
        } else {
            Err(PlanError::NotFinite)
        }
    }

    pub fn x_mm(self) -> f64 {
        self.x
    }

    pub fn y_mm(self) -> f64 {
        self.y
    }

    pub fn distance(self, other: MachinePoint) -> f64 {
        (other.x - self.x).hypot(other.y - self.y)
    }
}

/// Fraction of the rated laser power, `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct PowerRatio(f64);

impl PowerRatio {
    pub fn new(r: f64) -> Result<Self, PlanError> {
        if r.is_finite() && (0.0..=1.0).contains(&r) { Ok(PowerRatio(r)) } else { Err(PlanError::OutOfRange("power")) }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// Machine axis a scan runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanAxis {
    X,
    Y,
}

/// How the backend must execute the motion of one settings block.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PlanMode {
    /// Vector cutting/scoring: `Cut` events.
    Line,
    /// Line-by-line scanning: `ScanRun` events with this interval (mm) along `axis`.
    Scan { interval_mm: f64, axis: ScanAxis, bidirectional: bool },
    /// Scanning with lines rotated by `angle_deg ∈ (0°, 180°) \ {90°}` from the X axis:
    /// `ScanRun` events. Needs the `ScanAngle` capability (diagonal runs are not RD-confirmed).
    AngledScan { interval_mm: f64, angle_deg: f64, bidirectional: bool },
    /// Power-modulated scanning along X: `PowerRun` events, each at its own power (grayscale).
    /// Needs the `GrayscaleModulation` capability; no backend lowers it yet.
    GrayscaleScan { interval_mm: f64, bidirectional: bool },
    /// Discrete laser pulses: `Dot` events, each firing for `dwell_ms` at the head position.
    /// Needs the `DotMode` capability; no backend lowers it yet.
    Dots { dwell_ms: f64 },
}

impl PlanMode {
    /// Whether `ScanRun` events belong to this mode.
    pub fn is_binary_scan(&self) -> bool {
        matches!(self, PlanMode::Scan { .. } | PlanMode::AngledScan { .. })
    }
}

/// Concrete settings of one block of motion. Referenced by index from `OperationBoundary`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OperationSettings {
    pub mode: PlanMode,
    pub speed_mm_s: f64,
    pub min_power: PowerRatio,
    pub max_power: PowerRatio,
    pub air_assist: bool,
}

impl OperationSettings {
    pub(crate) fn validate(&self) -> Result<(), PlanError> {
        if !(self.speed_mm_s.is_finite() && self.speed_mm_s > 0.0) {
            return Err(PlanError::OutOfRange("speed"));
        }
        if self.min_power.get() > self.max_power.get() {
            return Err(PlanError::OutOfRange("power"));
        }
        let interval = match self.mode {
            PlanMode::Line => None,
            PlanMode::Dots { dwell_ms } => {
                if !(dwell_ms.is_finite() && dwell_ms > 0.0) {
                    return Err(PlanError::OutOfRange("dwell"));
                }
                None
            }
            PlanMode::Scan { interval_mm, .. } | PlanMode::GrayscaleScan { interval_mm, .. } => Some(interval_mm),
            PlanMode::AngledScan { interval_mm, angle_deg, .. } => {
                if !(angle_deg > 0.0 && angle_deg < 180.0 && angle_deg != 90.0) {
                    return Err(PlanError::OutOfRange("angle"));
                }
                Some(interval_mm)
            }
        };
        if let Some(i) = interval
            && !(i.is_finite() && i > 0.0)
        {
            return Err(PlanError::OutOfRange("interval"));
        }
        Ok(())
    }
}

/// Index into [`crate::JobPlan::settings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SettingsIndex(pub(crate) u32);

impl SettingsIndex {
    pub fn get(self) -> usize {
        self.0 as usize
    }
}

/// Where a burning move came from: preflight errors and preview picks lead back to it (IM15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRef {
    pub geometry: GeometryId,
    pub operation: OperationId,
}

/// One scan run with the laser on from `from` to `to`. `overscan_mm` is laser-off run-up and
/// run-out along the same line before `from` and after `to`; it is part of the plan's bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanLine {
    pub from: MachinePoint,
    pub to: MachinePoint,
    pub overscan_mm: f64,
}

impl ScanLine {
    /// The laser-off run-up start and run-out end (equal to `from`/`to` without overscan).
    pub fn overscan_ends(&self) -> (MachinePoint, MachinePoint) {
        let (dx, dy) = (self.to.x - self.from.x, self.to.y - self.from.y);
        let len = dx.hypot(dy);
        if !(len > 0.0) || self.overscan_mm == 0.0 {
            return (self.from, self.to);
        }
        let (ux, uy) = (dx / len * self.overscan_mm, dy / len * self.overscan_mm);
        (
            MachinePoint { x: self.from.x - ux, y: self.from.y - uy },
            MachinePoint { x: self.to.x + ux, y: self.to.y + uy },
        )
    }
}

/// One step of the plan, in execution order.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PlanEvent {
    /// Starts a block of motion with `settings`; `pass` counts from 0.
    OperationBoundary { operation: OperationId, settings: SettingsIndex, pass: u32 },
    /// Laser off.
    Travel { to: MachinePoint },
    /// Laser on, straight line from the current position (Line mode only).
    Cut { to: MachinePoint, origin: Option<SourceRef> },
    /// One scan run (Scan/AngledScan modes). The head travels to `line.from` first via a `Travel`.
    ScanRun { line: ScanLine, origin: Option<SourceRef> },
    /// One scan run at its own `power` (GrayscaleScan mode only).
    PowerRun { line: ScanLine, power: PowerRatio, origin: Option<SourceRef> },
    /// Focus (Z) offset from the start focus for the following motion, mm (positive = deeper).
    FocusOffset { z_mm: f64 },
    /// One laser pulse at the current head position (Dots mode only).
    Dot { origin: Option<SourceRef> },
    /// A scan run whose power ramps linearly from the settings' minimum to maximum power over
    /// the first and the last `ramp_mm` of the run (each at most half the run), Scan/AngledScan
    /// modes only. Needs the `RampLength` capability; no backend lowers it yet.
    RampRun { line: ScanLine, ramp_mm: f64, origin: Option<SourceRef> },
    /// Feeder (FE01): advance the material by `mm` at `speed_mm_s` while the laser is off. The
    /// head does not move. No backend lowers it yet (the Ruida form is not proven).
    Feed { mm: f64, speed_mm_s: f64 },
    /// Feeder (FE01): wait `seconds` before the next event, for example for the operator to
    /// change the workpiece. The head does not move.
    Pause { seconds: f64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanError {
    NotFinite,
    OutOfRange(&'static str),
    UnknownSettings(SettingsIndex),
    /// A `Cut`/`ScanRun` before any `OperationBoundary`.
    MotionOutsideOperation,
    /// A `Cut` under scan settings or a `ScanRun` under line settings.
    EventDoesNotMatchMode,
    /// A `ScanRun` that does not start at the current head position.
    ScanRunNotAtCurrentPosition,
    TooManySettings,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::NotFinite => f.write_str("non-finite value in plan"),
            PlanError::OutOfRange(field) => write!(f, "{field} out of range"),
            PlanError::UnknownSettings(i) => write!(f, "unknown settings index {}", i.0),
            PlanError::MotionOutsideOperation => f.write_str("burning motion outside an operation"),
            PlanError::EventDoesNotMatchMode => f.write_str("event does not match the settings mode"),
            PlanError::ScanRunNotAtCurrentPosition => f.write_str("scan run does not start at the head position"),
            PlanError::TooManySettings => f.write_str("too many settings blocks"),
        }
    }
}

impl std::error::Error for PlanError {}
