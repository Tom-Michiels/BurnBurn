//! The solver contract: a numeric constraint problem in sketch-local millimetres, and the report a
//! solver gives back. Everything here is BurnBurn's own type; the third-party solver stays behind
//! [`SketchSolverAdapter`] (ADR 0007 rule 2).

mod adapter;

use std::fmt;

use burnburn_geometry::Point;
use burnburn_types::{CancellationToken, Cancelled};

pub use adapter::SketchSolverAdapter;

/// An entity of a [`Problem`]: its index in [`Problem::entities`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub(crate) u32);

impl EntityId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A constraint of a [`Problem`]: its index in [`Problem::constraints`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstraintIndex(pub usize);

/// A sketch entity. Positions and radii are the *guess*: where the entity is drawn (or was last
/// solved). The solver starts there and keeps the branch they show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Entity {
    Point {
        at: Point,
    },
    Line {
        start: EntityId,
        end: EntityId,
    },
    /// Counter-clockwise from `start` to `end` around `center` (clockwise when `clockwise`).
    Arc {
        center: EntityId,
        start: EntityId,
        end: EntityId,
        clockwise: bool,
    },
    Circle {
        center: EntityId,
        radius: f64,
    },
}

/// How a point-to-point distance is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Along {
    /// Euclidean, never negative.
    #[default]
    Direct,
    /// Signed horizontal offset `b.x - a.x`.
    X,
    /// Signed vertical offset `b.y - a.y`.
    Y,
}

/// One constraint with resolved numeric values (millimetres, degrees).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Constraint {
    Coincident(EntityId, EntityId),
    /// A point lies on a line (its infinite extension), an arc or a circle.
    PointOn {
        point: EntityId,
        curve: EntityId,
    },
    /// A line is horizontal.
    Horizontal(EntityId),
    /// A line is vertical.
    Vertical(EntityId),
    Parallel(EntityId, EntityId),
    Perpendicular(EntityId, EntityId),
    /// Line–arc/circle or arc/circle–arc/circle tangency; side from the guess.
    Tangent(EntityId, EntityId),
    /// Equal lengths (lines) or equal radii (arcs, circles).
    Equal(EntityId, EntityId),
    Midpoint {
        point: EntityId,
        line: EntityId,
    },
    /// `a` and `b` mirror each other about a line or a point.
    Symmetric {
        a: EntityId,
        b: EntityId,
        about: EntityId,
    },
    /// Two arcs/circles share their centre.
    Concentric(EntityId, EntityId),
    /// A point stays at `at`.
    Fix {
        point: EntityId,
        at: Point,
    },
    Distance {
        a: EntityId,
        b: EntityId,
        value: f64,
        along: Along,
    },
    Length {
        line: EntityId,
        value: f64,
    },
    Radius {
        curve: EntityId,
        value: f64,
    },
    Diameter {
        curve: EntityId,
        value: f64,
    },
    /// Signed counter-clockwise angle in degrees from line `from` to line `to`.
    Angle {
        from: EntityId,
        to: EntityId,
        degrees: f64,
    },
}

/// A complete numeric problem. Entity references must point to entities of this problem.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Problem {
    pub entities: Vec<Entity>,
    pub constraints: Vec<Constraint>,
}

impl Problem {
    pub fn new() -> Self {
        Problem::default()
    }

    pub fn add(&mut self, entity: Entity) -> EntityId {
        let id = EntityId(u32::try_from(self.entities.len()).unwrap_or(u32::MAX));
        self.entities.push(entity);
        id
    }

    pub fn point(&mut self, x: f64, y: f64) -> EntityId {
        self.add(Entity::Point { at: Point::new(x, y) })
    }

    pub fn line(&mut self, start: EntityId, end: EntityId) -> EntityId {
        self.add(Entity::Line { start, end })
    }

    pub fn circle(&mut self, center: EntityId, radius: f64) -> EntityId {
        self.add(Entity::Circle { center, radius })
    }

    pub fn constrain(&mut self, c: Constraint) -> ConstraintIndex {
        self.constraints.push(c);
        ConstraintIndex(self.constraints.len() - 1)
    }
}

/// Which coordinate of an entity is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Coordinate {
    X,
    Y,
    Radius,
}

/// A coordinate the constraints leave free to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeCoordinate {
    pub entity: EntityId,
    pub coordinate: Coordinate,
}

/// What became of one constraint.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintState {
    /// Met and independent of the earlier constraints.
    Satisfied,
    /// Implied by earlier constraints (harmless; removing it changes nothing).
    Redundant { with: Vec<ConstraintIndex> },
    /// Contradicts earlier constraints.
    Conflicting { with: Vec<ConstraintIndex> },
    /// Not met, without a proven conflict (for example the solver did not converge).
    Unmet { residual: f64 },
}

/// The solved value of an entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Solved {
    Point(Point),
    /// Lines and arcs are defined by their points.
    Derived,
    Circle {
        radius: f64,
    },
}

/// Overall outcome. "Conflict" is only claimed where the solver proves it; otherwise a failed
/// solve is [`SolveStatus::NotConverged`] (implementatieplan 01, § Geometrie en solver).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveStatus {
    Solved,
    Conflict,
    NotConverged,
}

/// A side or direction of the solution that differs from the guess (a branch flip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flip {
    ArcSweep(EntityId),
    AngleBranch(ConstraintIndex),
    RadiusSign(EntityId),
    TangentSide(ConstraintIndex),
    Corner { point: EntityId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolveReport {
    pub status: SolveStatus,
    /// Unknowns minus the rank of the Jacobian at the solution.
    pub dof: usize,
    pub free: Vec<FreeCoordinate>,
    /// One per [`Problem::constraints`] entry, same order.
    pub constraints: Vec<ConstraintState>,
    /// One per [`Problem::entities`] entry, same order.
    pub entities: Vec<Solved>,
    pub flipped: Vec<Flip>,
    /// Largest residual (mm) and the tolerance it was held to.
    pub residual: f64,
    pub tolerance: f64,
}

impl SolveReport {
    pub fn point(&self, id: EntityId) -> Option<Point> {
        match self.entities.get(id.index())? {
            Solved::Point(p) => Some(*p),
            _ => None,
        }
    }

    pub fn radius(&self, id: EntityId) -> Option<f64> {
        match self.entities.get(id.index())? {
            Solved::Circle { radius } => Some(*radius),
            _ => None,
        }
    }

    pub fn is_solved(&self) -> bool {
        self.status == SolveStatus::Solved
    }

    pub fn redundant(&self) -> impl Iterator<Item = ConstraintIndex> + '_ {
        self.indexed(|s| matches!(s, ConstraintState::Redundant { .. }))
    }

    pub fn conflicting(&self) -> impl Iterator<Item = ConstraintIndex> + '_ {
        self.indexed(|s| matches!(s, ConstraintState::Conflicting { .. }))
    }

    fn indexed(&self, keep: fn(&ConstraintState) -> bool) -> impl Iterator<Item = ConstraintIndex> + '_ {
        self.constraints.iter().enumerate().filter(move |(_, s)| keep(s)).map(|(i, _)| ConstraintIndex(i))
    }
}

/// Why a problem was not solved at all (as opposed to solved with a failing status).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SolveError {
    Cancelled,
    /// An entity reference points nowhere or at the wrong kind of entity.
    InvalidReference {
        entity: EntityId,
    },
    /// An entity's own values are invalid (non-finite position, negative radius, degenerate line).
    InvalidGeometry {
        entity: EntityId,
    },
    /// A value is not finite or out of range (negative length, radius).
    InvalidValue {
        constraint: ConstraintIndex,
        value: f64,
    },
    /// The problem is invalid in another way the solver reported.
    Rejected {
        reason: String,
    },
}

impl From<Cancelled> for SolveError {
    fn from(_: Cancelled) -> Self {
        SolveError::Cancelled
    }
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolveError::Cancelled => f.write_str("solve cancelled"),
            SolveError::InvalidReference { entity } => write!(f, "invalid entity reference {}", entity.index()),
            SolveError::InvalidGeometry { entity } => write!(f, "entity {} has invalid geometry", entity.index()),
            SolveError::InvalidValue { constraint, value } => {
                write!(f, "constraint {} has invalid value {value}", constraint.0)
            }
            SolveError::Rejected { reason } => write!(f, "problem rejected: {reason}"),
        }
    }
}

impl std::error::Error for SolveError {}

/// A deterministic constraint solver. Same problem in, same bits out, independent of earlier
/// solves. Cancellation is checked at least once per iteration.
pub trait Solver {
    fn solve(&self, problem: &Problem, cancel: &CancellationToken) -> Result<SolveReport, SolveError>;
}
