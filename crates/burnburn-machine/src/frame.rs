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
use burnburn_ruida::RuidaProfile;
use burnburn_types::{OperationId, StableId};

use crate::artifact::RdArtifact;

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

    /// The scrambled `.rd` bytes of the frame, lowered by the same path as a production job.
    pub fn rd_file(&self, profile: &RuidaProfile) -> Result<Vec<u8>, FrameError> {
        let program = burnburn_ruida::lower(&self.plan, profile).map_err(|e| FrameError::Rd(format!("{e:?}")))?;
        program.to_rd_file(profile.magic).map_err(|e| FrameError::Rd(format!("{e:?}")))
    }

    /// The frame as an uploadable artifact. It names the frame plan, never the source plan.
    pub fn artifact(&self, profile: &RuidaProfile, max_chunk: usize) -> Result<RdArtifact, FrameError> {
        let rd = self.rd_file(profile)?;
        RdArtifact::from_rd_file(self.plan.id(), &rd, max_chunk).map_err(|e| FrameError::Rd(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The plan has no motion, so there are no bounds to frame.
    NoMotion,
    Plan(PlanError),
    /// The frame plan could not be lowered to RD bytes.
    Rd(String),
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
            FrameError::Rd(e) => write!(f, "frame RD: {e}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// The operation id used for the single frame block. Not a document operation.
pub fn frame_operation() -> OperationId {
    OperationId::from_raw(u64::MAX)
}

/// Which outline the head follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameShape {
    /// The bounding rectangle of everything the head reaches.
    BoundingRectangle,
    /// The convex hull of everything the head reaches ("rubber band").
    RubberBand,
}

/// Derives the laser-off frame of `plan`: travel around its bounds (including overscan) and
/// back to the first corner.
pub fn frame_plan(plan: &JobPlan, options: &FrameOptions) -> Result<FramePlan, FrameError> {
    frame_plan_shaped(plan, options, FrameShape::BoundingRectangle)
}

pub fn frame_plan_shaped(plan: &JobPlan, options: &FrameOptions, shape: FrameShape) -> Result<FramePlan, FrameError> {
    frame_plan_aimed(plan, options, shape, None)
}

/// Like [`frame_plan_shaped`], but for a red-dot pointer that sits `dot_offset_mm` (machine frame,
/// dot minus beam) away from the beam: the head travels the outline shifted by minus the offset, so
/// that the dot, not the beam, runs along the outline of the job. `None` is the plain frame. An
/// offset that is not a number, or an outline the head cannot reach on the bed, is an error: the
/// frame is never clamped.
pub fn frame_plan_aimed(
    plan: &JobPlan,
    options: &FrameOptions,
    shape: FrameShape,
    dot_offset_mm: Option<(f64, f64)>,
) -> Result<FramePlan, FrameError> {
    let (dx, dy) = match dot_offset_mm {
        Some((dx, dy)) if dx.is_finite() && dy.is_finite() => (dx, dy),
        Some(_) => return Err(PlanError::OutOfRange("pointer offset").into()),
        None => (0.0, 0.0),
    };
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
    let corners = match shape {
        FrameShape::BoundingRectangle => {
            vec![(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y), (lo.x, lo.y)]
        }
        FrameShape::RubberBand => closed_loop(convex_hull(reached_points(plan))),
    };
    for (x, y) in corners {
        let to = MachinePoint::new(x - dx, y - dy)?;
        if dot_offset_mm.is_some() && !plan.frame().contains(to) {
            return Err(PlanError::OutOfRange("frame outside the bed").into());
        }
        b.push(PlanEvent::Travel { to })?;
    }
    Ok(FramePlan { source: plan.id(), plan: b.finish()? })
}

/// Every position the head reaches, with the same rules as [`JobPlan::bounds`] (overscan ends
/// count; the start position counts only when a cut begins there).
fn reached_points(plan: &JobPlan) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let xy = |p: MachinePoint| (p.x_mm(), p.y_mm());
    let mut at = plan.start();
    for e in plan.events() {
        match *e {
            PlanEvent::Travel { to } => {
                out.push(xy(to));
                at = to;
            }
            PlanEvent::Cut { to, .. } | PlanEvent::CutPower { to, .. } => {
                out.push(xy(at));
                out.push(xy(to));
                at = to;
            }
            PlanEvent::ScanRun { line, .. } | PlanEvent::PowerRun { line, .. } => {
                let (a, z) = line.overscan_ends();
                out.push(xy(a));
                out.push(xy(z));
                at = line.to;
            }
            _ => {}
        }
    }
    out
}

fn cross(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

/// Andrew's monotone chain: counter-clockwise from the lowest-left point, strictly convex (no
/// collinear or duplicate vertices). One point stays one point, collinear points give two.
fn convex_hull(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let mut hull: Vec<(f64, f64)> = Vec::with_capacity(pts.len() + 1);
    extend_chain(&mut hull, 1, pts.iter());
    let lower = hull.len();
    extend_chain(&mut hull, lower, pts.iter().rev().skip(1));
    hull.pop(); // the first point again
    hull
}

/// One monotone-chain pass: keep only left turns above `floor` points.
fn extend_chain<'a>(hull: &mut Vec<(f64, f64)>, floor: usize, points: impl Iterator<Item = &'a (f64, f64)>) {
    for &p in points {
        while hull.len() > floor && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
}

/// Hull vertices as a closed travel loop; two vertices go there and back, one stays one.
fn closed_loop(hull: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    match hull.as_slice() {
        [] | [_] => hull,
        [a, b] => vec![*a, *b, *a],
        [first, ..] => {
            let mut out = hull.clone();
            out.push(*first);
            out
        }
    }
}
