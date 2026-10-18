//! What is known about the head position. Unknown is its own state and is never a number.

use burnburn_job::MachinePoint;
use burnburn_types::MonotonicTime;

/// One axis in controller-native micrometres, or unknown. There is deliberately no `Default`
/// and no conversion from a plain number that could turn "unknown" into 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisPosition {
    Unknown,
    Known { um: i64 },
}

impl AxisPosition {
    /// Millimetres for display; `None` = show "unknown", never 0.
    pub fn mm(self) -> Option<f64> {
        match self {
            AxisPosition::Unknown => None,
            AxisPosition::Known { um } => Some(um as f64 / 1000.0),
        }
    }

    pub fn um(self) -> Option<i64> {
        match self {
            AxisPosition::Unknown => None,
            AxisPosition::Known { um } => Some(um),
        }
    }

    pub fn from_reading(um: Option<i64>) -> Self {
        um.map_or(AxisPosition::Unknown, |um| AxisPosition::Known { um })
    }
}

/// The last position the controller reported, per axis. Any motion command invalidates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionReading {
    pub x: AxisPosition,
    pub y: AxisPosition,
    /// Monotonic time of the reading that produced the known axes.
    pub read_at: Option<MonotonicTime>,
}

impl PositionReading {
    pub const UNKNOWN: PositionReading =
        PositionReading { x: AxisPosition::Unknown, y: AxisPosition::Unknown, read_at: None };

    /// The head position as a machine-frame point; `None` unless both axes are known.
    pub fn machine_point(&self) -> Option<MachinePoint> {
        let (x, y) = (self.x.mm()?, self.y.mm()?);
        MachinePoint::new(x, y).ok()
    }
}
