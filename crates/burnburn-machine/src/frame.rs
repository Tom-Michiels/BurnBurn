//! Laser-off framing (M05): a separate plan derived from a frozen [`JobPlan`] that only
//! travels along the plan's bounds.
//!
//! The frame plan has one settings block with minimum and maximum power 0 and contains only
//! `Travel` events, so a backend has nothing to switch the laser on for. Whether a given
//! controller keeps the tube off during such a program is a hardware fact and stays
//! unverified until measured (`docs/hardware/evidence.md`).

use std::fmt;

use burnburn_job::{
    JobPlan, JobPlanBuilder, JobPlanId, MachinePoint, OperationSettings, PlanError, PlanEvent, PlanMode, PlanSource,
    PowerRatio,
};
use burnburn_types::{OperationId, StableId};

/// Recorded as the compiler of every frame plan (part of its id).
pub const FRAME_GENERATOR_VERSION: &str = concat!("burnburn-machine frame ", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameOptions {
    /// Travel speed along the frame (mm/s).
    pub speed_mm_s: f64,
}

/// A frame plan and the plan whose bounds it follows.
#[derive(Debug, Clone, PartialEq)]
pub struct FramePlan {
    source: JobPlanId,
    plan: JobPlan,
}

impl FramePlan {
    /// The production plan this frame was derived from.
    pub fn source(&self) -> JobPlanId {
        self.source
    }

    pub fn plan(&self) -> &JobPlan {
        &self.plan
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The plan has no motion, so there are no bounds to frame.
    NoMotion,
    Plan(PlanError),
}

impl From<PlanError> for FrameError {
    fn from(e: PlanError) -> Self {
        FrameError::Plan(e)
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::NoMotion => f.write_str("plan has no motion to frame"),
            FrameError::Plan(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// The operation id used for the single frame block. Not a document operation.
pub fn frame_operation() -> OperationId {
    OperationId::from_raw(u64::MAX)
}

/// Derives the laser-off frame of `plan`: travel around its bounds (including overscan) and
/// back to the first corner.
pub fn frame_plan(plan: &JobPlan, options: &FrameOptions) -> Result<FramePlan, FrameError> {
    let bounds = plan.bounds().ok_or(FrameError::NoMotion)?;
    let (lo, hi) = (bounds.min(), bounds.max());
    let source = PlanSource { compiler_version: FRAME_GENERATOR_VERSION.to_owned(), ..plan.source().clone() };
    let mut b = JobPlanBuilder::new(source, *plan.frame(), plan.start());
    let off = PowerRatio::new(0.0)?;
    let settings = b.add_settings(OperationSettings {
        mode: PlanMode::Line,
        speed_mm_s: options.speed_mm_s,
        min_power: off,
        max_power: off,
        air_assist: false,
    })?;
    b.push(PlanEvent::OperationBoundary { operation: frame_operation(), settings, pass: 0 })?;
    let corners = [(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y), (lo.x, lo.y)];
    for (x, y) in corners {
        b.push(PlanEvent::Travel { to: MachinePoint::new(x, y)? })?;
    }
    Ok(FramePlan { source: plan.id(), plan: b.finish()? })
}
