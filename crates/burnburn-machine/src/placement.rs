//! Job placement for the laser window: "Start From" (absolute coordinates, current position,
//! user origin), the 3×3 job origin, and the compile scope ("Cut Selected Graphics", "Use
//! Selection Origin"). Pure planning over a frozen [`JobPlan`]; no controller is involved.
//!
//! Three separate reference systems (`research/lightburn/02-features.md` §8):
//!
//! - *Absolute coordinates*: the job stays where it was designed on the bed.
//! - *Current position*: the chosen anchor of the job is put on the laser head's position. An
//!   unknown position is never read as zero ([`PlacementError::PositionUnknown`]).
//! - *User origin*: the anchor is put on a user-set point that is independent of the head.
//!
//! A [`JobAnchor`] is one of nine points on the **job bounds as the user sees them** (bed view,
//! y up), not an axis of the machine and not the bed origin. Which bounds those are is decided
//! first by [`resolve_scope`]: filters and origin are composed before any bounds are used.

use std::fmt;

use burnburn_job::{JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OriginCorner, PlanError, PlanEvent, ScanLine};

/// Axis-aligned rectangle in bed-view millimetres (y up). Always `min <= max` and finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Rect {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Option<Rect> {
        let finite = [min_x, min_y, max_x, max_y].iter().all(|v| v.is_finite());
        (finite && min_x <= max_x && min_y <= max_y).then_some(Rect { min_x, min_y, max_x, max_y })
    }

    pub fn min(&self) -> (f64, f64) {
        (self.min_x, self.min_y)
    }

    pub fn max(&self) -> (f64, f64) {
        (self.max_x, self.max_y)
    }

    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    /// The anchor point of the 3×3 grid on this rectangle.
    pub fn anchor_point(&self, anchor: JobAnchor) -> (f64, f64) {
        let (fx, fy) = anchor.fractions();
        (self.min_x + fx * (self.max_x - self.min_x), self.min_y + fy * (self.max_y - self.min_y))
    }
}

/// The nine points of the 3×3 job origin, named as the user sees the job (top = larger y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl JobAnchor {
    pub const ALL: [JobAnchor; 9] = [
        JobAnchor::TopLeft,
        JobAnchor::TopCenter,
        JobAnchor::TopRight,
        JobAnchor::MiddleLeft,
        JobAnchor::Center,
        JobAnchor::MiddleRight,
        JobAnchor::BottomLeft,
        JobAnchor::BottomCenter,
        JobAnchor::BottomRight,
    ];

    /// Fractions from the left and from the bottom of the bounds.
    fn fractions(self) -> (f64, f64) {
        match self {
            JobAnchor::TopLeft => (0.0, 1.0),
            JobAnchor::TopCenter => (0.5, 1.0),
            JobAnchor::TopRight => (1.0, 1.0),
            JobAnchor::MiddleLeft => (0.0, 0.5),
            JobAnchor::Center => (0.5, 0.5),
            JobAnchor::MiddleRight => (1.0, 0.5),
            JobAnchor::BottomLeft => (0.0, 0.0),
            JobAnchor::BottomCenter => (0.5, 0.0),
            JobAnchor::BottomRight => (1.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartFrom {
    AbsoluteCoordinates,
    CurrentPosition,
    UserOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementSpec {
    pub start_from: StartFrom,
    pub anchor: JobAnchor,
}

/// Machine-side facts a placement needs. Positions are controller (machine-frame) points;
/// `None` means unknown, never zero.
pub struct PlacementInput<'a> {
    pub frame: &'a MachineFrame,
    pub current_position: Option<MachinePoint>,
    pub user_origin: Option<MachinePoint>,
}

/// A plan moved as a whole, and by how much (machine frame, mm).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedPlan {
    pub plan: JobPlan,
    pub delta_mm: (f64, f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlacementError {
    /// "Current position" was chosen but the head position is not known.
    PositionUnknown,
    UserOriginNotSet,
    /// The moved job would leave the bed; the resulting machine-frame bounds are given.
    OutsideBed { min: (f64, f64), max: (f64, f64) },
    /// A plan event this build cannot translate (the event enum is non-exhaustive).
    UnsupportedEvent,
    Plan(PlanError),
}

impl From<PlanError> for PlacementError {
    fn from(e: PlanError) -> Self {
        PlacementError::Plan(e)
    }
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PlacementError {}

/// Sign of machine axes relative to the bed view (y up, origin bottom-left).
fn axis_signs(origin: OriginCorner) -> (f64, f64) {
    match origin {
        OriginCorner::BottomLeft => (1.0, 1.0),
        OriginCorner::TopLeft => (1.0, -1.0),
        OriginCorner::TopRight => (-1.0, -1.0),
        OriginCorner::BottomRight => (-1.0, 1.0),
    }
}

/// Moves `plan` so that the `spec.anchor` of `origin_bounds` (bed view) lands on the reference
/// of `spec.start_from`. `origin_bounds` come from [`resolve_scope`]; with absolute coordinates
/// the plan is returned unchanged.
pub fn place_plan(
    plan: &JobPlan,
    spec: &PlacementSpec,
    origin_bounds: Rect,
    input: &PlacementInput<'_>,
) -> Result<PlacedPlan, PlacementError> {
    let target = match spec.start_from {
        StartFrom::AbsoluteCoordinates => return Ok(PlacedPlan { plan: plan.clone(), delta_mm: (0.0, 0.0) }),
        StartFrom::CurrentPosition => input.current_position.ok_or(PlacementError::PositionUnknown)?,
        StartFrom::UserOrigin => input.user_origin.ok_or(PlacementError::UserOriginNotSet)?,
    };
    let target_view = input.frame.to_bed(target);
    let anchor = origin_bounds.anchor_point(spec.anchor);
    let (sx, sy) = axis_signs(input.frame.origin());
    let delta = (sx * (target_view.x - anchor.0), sy * (target_view.y - anchor.1));
    if delta == (0.0, 0.0) {
        return Ok(PlacedPlan { plan: plan.clone(), delta_mm: delta });
    }
    if let Some(b) = plan.bounds() {
        let (min, max) = ((b.min().x + delta.0, b.min().y + delta.1), (b.max().x + delta.0, b.max().y + delta.1));
        let inside = |p: (f64, f64)| MachinePoint::new(p.0, p.1).is_ok_and(|p| input.frame.contains(p));
        if !(inside(min) && inside(max)) {
            return Err(PlacementError::OutsideBed { min, max });
        }
    }
    Ok(PlacedPlan { plan: translated(plan, delta)?, delta_mm: delta })
}

fn translated(plan: &JobPlan, (dx, dy): (f64, f64)) -> Result<JobPlan, PlacementError> {
    let mv = |p: MachinePoint| MachinePoint::new(p.x_mm() + dx, p.y_mm() + dy);
    let mut b = JobPlanBuilder::new(plan.source().clone(), *plan.frame(), plan.start());
    for s in plan.settings() {
        b.add_settings(*s)?;
    }
    for e in plan.events() {
        let moved = match *e {
            PlanEvent::Travel { to } => PlanEvent::Travel { to: mv(to)? },
            PlanEvent::Cut { to, origin } => PlanEvent::Cut { to: mv(to)?, origin },
            PlanEvent::ScanRun { line, origin } => PlanEvent::ScanRun { line: moved_line(line, &mv)?, origin },
            PlanEvent::PowerRun { line, power, origin } => {
                PlanEvent::PowerRun { line: moved_line(line, &mv)?, power, origin }
            }
            PlanEvent::OperationBoundary { .. } | PlanEvent::FocusOffset { .. } => *e,
            _ => return Err(PlacementError::UnsupportedEvent),
        };
        b.push(moved)?;
    }
    Ok(b.finish()?)
}

fn moved_line(
    line: ScanLine,
    mv: &impl Fn(MachinePoint) -> Result<MachinePoint, PlanError>,
) -> Result<ScanLine, PlanError> {
    Ok(ScanLine { from: mv(line.from)?, to: mv(line.to)?, overscan_mm: line.overscan_mm })
}

/// The two compile-time filters of the laser window. They are decided together, before any
/// bounds are computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileScope {
    /// "Cut Selected Graphics": only the selected objects are output.
    pub cut_selected: bool,
    /// "Use Selection Origin": the 3×3 anchor is taken from the selection's bounds, even when
    /// everything is output.
    pub use_selection_origin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScopeObject {
    pub id: u64,
    /// Bed-view bounds of the object as it will be output.
    pub bounds: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScopeResolution {
    /// Objects to compile, in document order.
    pub included: Vec<u64>,
    /// The bounds the 3×3 anchor refers to.
    pub origin_bounds: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    /// A selection-based option is on but nothing is selected.
    NothingSelected,
    NoObjects,
    /// A selected id is not in the document.
    UnknownObject(u64),
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ScopeError {}

/// Decides which objects are output and which bounds the job origin refers to.
pub fn resolve_scope(
    scope: &CompileScope,
    objects: &[ScopeObject],
    selected: &[u64],
) -> Result<ScopeResolution, ScopeError> {
    if objects.is_empty() {
        return Err(ScopeError::NoObjects);
    }
    if let Some(&unknown) = selected.iter().find(|id| !objects.iter().any(|o| o.id == **id)) {
        return Err(ScopeError::UnknownObject(unknown));
    }
    if (scope.cut_selected || scope.use_selection_origin) && selected.is_empty() {
        return Err(ScopeError::NothingSelected);
    }
    let is_selected = |o: &&ScopeObject| selected.contains(&o.id);
    let included: Vec<&ScopeObject> =
        if scope.cut_selected { objects.iter().filter(is_selected).collect() } else { objects.iter().collect() };
    let origin: Vec<&ScopeObject> =
        if scope.use_selection_origin { objects.iter().filter(is_selected).collect() } else { included.clone() };
    let origin_bounds = origin
        .iter()
        .map(|o| o.bounds)
        .reduce(|a, b| a.union(&b))
        .ok_or(ScopeError::NothingSelected)?;
    Ok(ScopeResolution { included: included.iter().map(|o| o.id).collect(), origin_bounds })
}
