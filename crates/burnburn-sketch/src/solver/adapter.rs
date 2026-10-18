//! Adapter over the `sketch-solver` crate (0.1.0, MIT OR Apache-2.0, pure Rust; only `libm`).
//! No `sketch_solver` type leaves this module.

use burnburn_geometry::Point;
use burnburn_types::CancellationToken;
use sketch_solver as ss;

use super::{
    Along, Constraint, ConstraintIndex, ConstraintState, Coordinate, Entity, EntityId, Flip, FreeCoordinate, Problem,
    SolveError, SolveReport, SolveStatus, Solved, Solver,
};

/// [`Solver`] backed by `sketch-solver` 0.1.0: Levenberg–Marquardt with continuation to keep the
/// drawn branch, and a QR rank diagnosis (dof, free coordinates, redundant and conflicting
/// equations). Deterministic by construction (no FMA, `libm` trigonometry, fixed loop order).
#[derive(Debug, Clone, Copy, Default)]
pub struct SketchSolverAdapter;

/// Maps between our ids and the solver's.
struct Lowered {
    sketch: ss::Sketch,
    /// Our entity index → solver entity.
    entity: Vec<ss::EntityId>,
    /// Solver entity index → our entity.
    back: Vec<Option<EntityId>>,
    /// Solver constraint index → our constraint.
    constraint: Vec<ConstraintIndex>,
}

impl Solver for SketchSolverAdapter {
    fn solve(&self, problem: &Problem, cancel: &CancellationToken) -> Result<SolveReport, SolveError> {
        cancel.check()?;
        let low = lower(problem)?;
        let interrupt = || cancel.is_cancelled();
        let opt = ss::SolveOptions { interrupt: Some(&interrupt), ..ss::SolveOptions::default() };
        let sol = low.sketch.solve_with(&opt).map_err(|e| match e {
            ss::SolveError::Interrupted => SolveError::Cancelled,
            other => SolveError::Rejected { reason: other.to_string() },
        })?;
        Ok(report(problem, &low, &sol))
    }
}

fn xy(p: Point) -> [f64; 2] {
    [p.x, p.y]
}

fn lower(problem: &Problem) -> Result<Lowered, SolveError> {
    let mut sketch = ss::Sketch::new();
    let n = problem.entities.len();
    let mut entity: Vec<Option<ss::EntityId>> = vec![None; n];
    let mut back = Vec::with_capacity(n);
    let ours = |i: usize| EntityId(u32::try_from(i).unwrap_or(u32::MAX));
    // Points first, so curves can refer to any point regardless of our order.
    for (i, e) in problem.entities.iter().enumerate() {
        if let Entity::Point { at } = *e {
            let id = sketch.point(Some(xy(at))).map_err(|_| SolveError::InvalidGeometry { entity: ours(i) })?;
            entity[i] = Some(id);
            back.push(Some(ours(i)));
        }
    }
    let point = |entity: &[Option<ss::EntityId>], p: EntityId| -> Result<ss::EntityId, SolveError> {
        match problem.entities.get(p.index()) {
            Some(Entity::Point { .. }) => entity[p.index()].ok_or(SolveError::InvalidReference { entity: p }),
            _ => Err(SolveError::InvalidReference { entity: p }),
        }
    };
    for (i, e) in problem.entities.iter().enumerate() {
        let made = match *e {
            Entity::Point { .. } => continue,
            Entity::Line { start, end } => sketch.line(point(&entity, start)?, point(&entity, end)?),
            Entity::Arc { center, start, end, clockwise } => {
                sketch.arc(point(&entity, center)?, point(&entity, start)?, point(&entity, end)?, clockwise)
            }
            Entity::Circle { center, radius } => sketch.circle(point(&entity, center)?, Some(radius)),
        };
        let id = made.map_err(|_| SolveError::InvalidGeometry { entity: ours(i) })?;
        entity[i] = Some(id);
        back.push(Some(ours(i)));
    }
    let entity: Vec<ss::EntityId> = entity.into_iter().flatten().collect();
    if entity.len() != n {
        return Err(SolveError::Rejected { reason: "entity mapping incomplete".into() });
    }
    // `back` was filled in solver creation order, which is exactly the solver's id order.
    let mut constraint = Vec::with_capacity(problem.constraints.len());
    for (ci, c) in problem.constraints.iter().enumerate() {
        let idx = ConstraintIndex(ci);
        let lowered = lower_constraint(problem, &entity, c)?;
        sketch.add(lowered).map_err(|e| model_error(e, idx, c))?;
        constraint.push(idx);
    }
    Ok(Lowered { sketch, entity, back, constraint })
}

fn get(entity: &[ss::EntityId], id: EntityId) -> Result<ss::EntityId, SolveError> {
    entity.get(id.index()).copied().ok_or(SolveError::InvalidReference { entity: id })
}

fn center_of(problem: &Problem, entity: &[ss::EntityId], curve: EntityId) -> Result<ss::EntityId, SolveError> {
    match problem.entities.get(curve.index()) {
        Some(Entity::Arc { center, .. } | Entity::Circle { center, .. }) => get(entity, *center),
        _ => Err(SolveError::InvalidReference { entity: curve }),
    }
}

fn lower_constraint(problem: &Problem, e: &[ss::EntityId], c: &Constraint) -> Result<ss::Constraint, SolveError> {
    use ss::Constraint as S;
    let g = |id| get(e, id);
    Ok(match *c {
        Constraint::Coincident(a, b) => S::Coincident(g(a)?, g(b)?),
        Constraint::PointOn { point, curve } => S::On { point: g(point)?, curve: g(curve)? },
        Constraint::Horizontal(l) => S::Horizontal(ss::Pair::Line(g(l)?)),
        Constraint::Vertical(l) => S::Vertical(ss::Pair::Line(g(l)?)),
        Constraint::Parallel(a, b) => S::Parallel(g(a)?, g(b)?),
        Constraint::Perpendicular(a, b) => S::Perpendicular(g(a)?, g(b)?),
        Constraint::Tangent(a, b) => S::Tangent(g(a)?, g(b)?),
        Constraint::Equal(a, b) => S::Equal(g(a)?, g(b)?),
        Constraint::Midpoint { point, line } => S::Midpoint { point: g(point)?, line: g(line)? },
        Constraint::Symmetric { a, b, about } => S::Symmetric { a: g(a)?, b: g(b)?, about: g(about)? },
        Constraint::Concentric(a, b) => S::Coincident(center_of(problem, e, a)?, center_of(problem, e, b)?),
        Constraint::Fix { point, at } => S::Fix { entity: g(point)?, at: Some(xy(at)) },
        Constraint::Distance { a, b, value, along } => S::Distance {
            a: g(a)?,
            b: g(b)?,
            value,
            along: match along {
                Along::Direct => ss::Along::Direct,
                Along::X => ss::Along::X,
                Along::Y => ss::Along::Y,
            },
        },
        Constraint::Length { line, value } => S::Length { line: g(line)?, value },
        Constraint::Radius { curve, value } => S::Radius { curve: g(curve)?, value },
        Constraint::Diameter { curve, value } => S::Diameter { curve: g(curve)?, value },
        Constraint::Angle { from, to, degrees } => S::Angle { from: g(from)?, to: g(to)?, degrees },
    })
}

/// The entity arguments of a constraint, in the order the solver numbers them.
fn args(c: &Constraint) -> Vec<EntityId> {
    match *c {
        Constraint::Coincident(a, b)
        | Constraint::Parallel(a, b)
        | Constraint::Perpendicular(a, b)
        | Constraint::Tangent(a, b)
        | Constraint::Equal(a, b)
        | Constraint::Concentric(a, b) => vec![a, b],
        Constraint::PointOn { point, curve } => vec![point, curve],
        Constraint::Horizontal(l) | Constraint::Vertical(l) => vec![l],
        Constraint::Midpoint { point, line } => vec![point, line],
        Constraint::Symmetric { a, b, about } => vec![a, b, about],
        Constraint::Fix { point, .. } => vec![point],
        Constraint::Distance { a, b, .. } => vec![a, b],
        Constraint::Length { line, .. } => vec![line],
        Constraint::Radius { curve, .. } | Constraint::Diameter { curve, .. } => vec![curve],
        Constraint::Angle { from, to, .. } => vec![from, to],
    }
}

fn value(c: &Constraint) -> f64 {
    match *c {
        Constraint::Distance { value, .. }
        | Constraint::Length { value, .. }
        | Constraint::Radius { value, .. }
        | Constraint::Diameter { value, .. } => value,
        Constraint::Angle { degrees, .. } => degrees,
        Constraint::Fix { at, .. } if !at.is_finite() => f64::NAN,
        _ => 0.0,
    }
}

fn model_error(e: ss::ModelError, idx: ConstraintIndex, c: &Constraint) -> SolveError {
    let arg = |argument: usize| {
        args(c)
            .get(argument)
            .copied()
            .map_or(SolveError::Rejected { reason: e.to_string() }, |entity| SolveError::InvalidReference { entity })
    };
    match e {
        ss::ModelError::BadValue { .. } | ss::ModelError::NothingToFixAt => {
            SolveError::InvalidValue { constraint: idx, value: value(c) }
        }
        ss::ModelError::WrongKind { argument, .. } | ss::ModelError::SameEntity { argument } => arg(argument),
        ss::ModelError::UnknownEntity(_) => arg(0),
    }
}

fn report(problem: &Problem, low: &Lowered, sol: &ss::Solution) -> SolveReport {
    let ent = |id: ss::EntityId| low.back.get(id.index()).copied().flatten();
    let con = |src: &ss::Source| match src {
        ss::Source::Constraint(id) => low.constraint.get(id.index()).copied(),
        ss::Source::Arc(_) => None,
    };
    let mut constraints = vec![ConstraintState::Satisfied; problem.constraints.len()];
    for d in &sol.redundant {
        if let Some(i) = con(&d.source) {
            constraints[i.0] = ConstraintState::Redundant { with: d.with.iter().filter_map(con).collect() };
        }
    }
    for d in &sol.conflicts {
        if let Some(i) = con(&d.source) {
            constraints[i.0] = ConstraintState::Conflicting { with: d.with.iter().filter_map(con).collect() };
        }
    }
    for (src, residual) in &sol.unmet {
        if let Some(i) = con(src)
            && constraints[i.0] == ConstraintState::Satisfied
        {
            constraints[i.0] = ConstraintState::Unmet { residual: *residual };
        }
    }
    let status = match sol.status {
        ss::Status::Solved => SolveStatus::Solved,
        ss::Status::NotConverged if !sol.conflicts.is_empty() => SolveStatus::Conflict,
        ss::Status::NotConverged => SolveStatus::NotConverged,
    };
    let free = sol
        .free
        .iter()
        .filter_map(|f| {
            Some(FreeCoordinate {
                entity: ent(f.entity)?,
                coordinate: match f.coordinate {
                    ss::Coordinate::X => Coordinate::X,
                    ss::Coordinate::Y => Coordinate::Y,
                    ss::Coordinate::Radius => Coordinate::Radius,
                },
            })
        })
        .collect();
    let entities = problem
        .entities
        .iter()
        .zip(&low.entity)
        .map(|(e, id)| match e {
            Entity::Point { at } => Solved::Point(sol.point(*id).map_or(*at, |[x, y]| Point::new(x, y))),
            Entity::Circle { radius, .. } => Solved::Circle { radius: sol.radius(*id).unwrap_or(*radius) },
            Entity::Line { .. } | Entity::Arc { .. } => Solved::Derived,
        })
        .collect();
    let flipped = sol
        .flipped
        .iter()
        .filter_map(|o| {
            Some(match *o {
                ss::Orientation::ArcSweep(e) => Flip::ArcSweep(ent(e)?),
                ss::Orientation::AngleBranch(c) => Flip::AngleBranch(con(&ss::Source::Constraint(c))?),
                ss::Orientation::RadiusSign(e) => Flip::RadiusSign(ent(e)?),
                ss::Orientation::TangentSide(c) => Flip::TangentSide(con(&ss::Source::Constraint(c))?),
                ss::Orientation::Corner { point, .. } => Flip::Corner { point: ent(point)? },
            })
        })
        .collect();
    SolveReport {
        status,
        dof: sol.dof,
        free,
        constraints,
        entities,
        flipped,
        residual: sol.residual,
        tolerance: sol.tolerance,
    }
}
