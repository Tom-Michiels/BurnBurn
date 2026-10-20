//! Rotary attachment as a profile setting (W2-L3, M08): a chuck or roller turns the object, so
//! the document's Y (millimetres along the object surface) becomes a rotation. The plan stays in
//! surface millimetres — preview, bounds and preflight read the object as the user sees it — and
//! a backend applies [`RotaryConfig::to_controller_y`] when it quantises.

use crate::PlanError;

/// What turns the object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RotaryKind {
    /// The object is clamped in a chuck: one rotation moves its surface by `π · object diameter`.
    Chuck { object_diameter_mm: f64 },
    /// The object rests on rollers: one roller rotation moves the surface by `π · roller diameter`,
    /// whatever the object's own diameter.
    Roller { roller_diameter_mm: f64 },
}

/// Rotary settings of a machine profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotaryConfig {
    kind: RotaryKind,
    steps_per_rotation: u32,
    y_steps_per_mm: f64,
    mirror: bool,
}

impl RotaryConfig {
    pub const MAX_DIAMETER_MM: f64 = 2000.0;
    pub const MAX_STEPS_PER_ROTATION: u32 = 100_000_000;

    /// `y_steps_per_mm` is the flat-bed calibration of the controller's Y axis: the rotary reuses
    /// that axis, so the controller still counts in "flat-bed millimetres".
    pub fn new(
        kind: RotaryKind,
        steps_per_rotation: u32,
        y_steps_per_mm: f64,
        mirror: bool,
    ) -> Result<Self, PlanError> {
        let diameter = match kind {
            RotaryKind::Chuck { object_diameter_mm } => object_diameter_mm,
            RotaryKind::Roller { roller_diameter_mm } => roller_diameter_mm,
        };
        if !(diameter.is_finite() && diameter > 0.0 && diameter <= Self::MAX_DIAMETER_MM) {
            return Err(PlanError::OutOfRange("rotary diameter"));
        }
        if !(1..=Self::MAX_STEPS_PER_ROTATION).contains(&steps_per_rotation) {
            return Err(PlanError::OutOfRange("steps per rotation"));
        }
        if !(y_steps_per_mm.is_finite() && y_steps_per_mm > 0.0) {
            return Err(PlanError::OutOfRange("y steps per mm"));
        }
        Ok(RotaryConfig { kind, steps_per_rotation, y_steps_per_mm, mirror })
    }

    pub fn kind(&self) -> RotaryKind {
        self.kind
    }
    pub fn steps_per_rotation(&self) -> u32 {
        self.steps_per_rotation
    }
    pub fn y_steps_per_mm(&self) -> f64 {
        self.y_steps_per_mm
    }
    pub fn mirror(&self) -> bool {
        self.mirror
    }

    /// Surface travelled per full rotation, mm.
    pub fn surface_per_rotation_mm(&self) -> f64 {
        let diameter = match self.kind {
            RotaryKind::Chuck { object_diameter_mm } => object_diameter_mm,
            RotaryKind::Roller { roller_diameter_mm } => roller_diameter_mm,
        };
        std::f64::consts::PI * diameter
    }

    /// Controller millimetres per surface millimetre.
    pub fn y_scale(&self) -> f64 {
        f64::from(self.steps_per_rotation) / (self.surface_per_rotation_mm() * self.y_steps_per_mm)
    }

    /// A surface position `y_mm` (bed view, frame height `frame_height_mm`) as the controller's
    /// Y in flat-bed millimetres.
    pub fn to_controller_y(&self, y_mm: f64, frame_height_mm: f64) -> f64 {
        let along = if self.mirror { frame_height_mm - y_mm } else { y_mm };
        along * self.y_scale()
    }
}
