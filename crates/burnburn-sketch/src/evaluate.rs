//! Evaluating one sketch definition from the document: parameters → dimension values → solver
//! problem (starting from the last valid solution) → solved geometry, with a diagnosis per
//! constraint. Pure: the result depends only on the sketch (and the solver).

use std::collections::BTreeMap;

use burnburn_document::EvalDiagnosis;
use burnburn_document::sketch::{
    ConstraintId, DimensionKind, DimensionRole, Entity as DocEntity, EntityId, Relation, Sketch, SketchConstraint,
    SketchEntity, SketchSolution,
};
use burnburn_geometry::{GeometryError, Path, Point};
use burnburn_types::{CancellationToken, Cancelled};

use crate::param::{Dim, ExprError, ExprErrorKind, Expression, ParameterError, Parameters, Value, Values};
use crate::solver::{self, Along, Constraint, ConstraintState, Coordinate, Entity, Flip, Problem, SolveError, Solver};

/// A solved sketch: every entity at its solved position (sketch-local mm).
#[derive(Debug, Clone, PartialEq)]
pub struct SolvedSketch {
    pub(crate) entities: BTreeMap<EntityId, SketchEntity>,
}

impl SolvedSketch {
    pub fn entities(&self) -> impl ExactSizeIterator<Item = (EntityId, &SketchEntity)> {
        self.entities.iter().map(|(k, v)| (*k, v))
    }

    pub fn point(&self, id: EntityId) -> Option<Point> {
        match self.entities.get(&id)?.entity {
            DocEntity::Point { at } => Some(at),
            _ => None,
        }
    }

    /// Radius of a circle or arc; of an ellipse its semi-major radius (centre to major end).
    pub fn radius(&self, id: EntityId) -> Option<f64> {
        match self.entities.get(&id)?.entity {
            DocEntity::Circle { radius, .. } => Some(radius),
            DocEntity::Ellipse { center, major, .. } => Some(self.point(center)?.distance(self.point(major)?)),
            DocEntity::Arc { center, start, .. } => Some(self.point(center)?.distance(self.point(start)?)),
            _ => None,
        }
    }

    /// Production outputs: one closed or open contour per chain of non-construction lines and
    /// arcs, one per circle; keyed by the lowest entity id in it (stable across value edits).
    pub fn outputs(&self) -> Result<Vec<(u64, Path)>, GeometryError> {
        crate::output::outputs(self)
    }

    /// The solution to store as the next starting point.
    pub fn solution(&self, dof: usize) -> SketchSolution {
        let mut s = SketchSolution { dof: u32::try_from(dof).unwrap_or(u32::MAX), ..SketchSolution::default() };
        for (id, e) in &self.entities {
            match e.entity {
                DocEntity::Point { at } => {
                    s.points.insert(*id, at);
                }
                DocEntity::Circle { radius, .. } => {
                    s.radii.insert(*id, radius);
                }
                _ => {}
            }
        }
        s
    }
}

/// Overall state of an evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    FullyConstrained,
    /// Solved with remaining freedom. Producible: "fully constrained" is a design aid only.
    UnderConstrained {
        dof: usize,
    },
    Conflict,
    NotConverged,
    /// A dimension, the parameters or the definition is invalid; nothing was solved.
    Invalid,
}

impl Health {
    pub fn is_valid(self) -> bool {
        matches!(self, Health::FullyConstrained | Health::UnderConstrained { .. })
    }
}

/// Why a dimension has no usable value.
#[derive(Debug, Clone, PartialEq)]
pub enum DimensionError {
    Expression(ExprError),
    Parameters(ParameterError),
    /// A distance, length, radius or diameter below zero.
    Negative {
        value: f64,
    },
}

/// What became of one sketch constraint (constraint manager, S03/S04).
#[derive(Debug, Clone, PartialEq)]
pub enum Diagnosis {
    Satisfied,
    Redundant {
        with: Vec<ConstraintId>,
    },
    Conflicting {
        with: Vec<ConstraintId>,
    },
    Unmet {
        residual: f64,
    },
    /// A driven dimension's measured value.
    Measured(Value),
    Invalid(DimensionError),
    /// Not evaluated because the sketch could not be solved at all.
    NotEvaluated,
}

/// The result of evaluating a sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub health: Health,
    pub dof: usize,
    pub free: Vec<(EntityId, Coordinate)>,
    pub diagnostics: BTreeMap<ConstraintId, Diagnosis>,
    pub flipped: Vec<Flip>,
    /// Largest residual (mm) and the tolerance it was held to (0 when not solved).
    pub residual: f64,
    pub tolerance: f64,
    pub parameter_error: Option<ParameterError>,
    pub problem_error: Option<SolveError>,
    /// The solved geometry, only when [`Health::is_valid`].
    pub solved: Option<SolvedSketch>,
}

impl Evaluation {
    pub fn is_valid(&self) -> bool {
        self.health.is_valid() && self.solved.is_some()
    }

    /// Conflicting and unmet constraints, with what they conflict with, sorted: what to
    /// select when explaining a failure.
    pub fn conflict_set(&self) -> Vec<ConstraintId> {
        let mut out = Vec::new();
        for (id, d) in &self.diagnostics {
            match d {
                Diagnosis::Conflicting { with } => {
                    out.push(*id);
                    out.extend(with.iter().copied());
                }
                Diagnosis::Unmet { .. } => out.push(*id),
                _ => {}
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The document-level diagnosis of a failed evaluation (`None` when valid).
    pub fn failure(&self) -> Option<EvalDiagnosis> {
        let of = |keep: fn(&Diagnosis) -> bool| -> Vec<ConstraintId> {
            self.diagnostics.iter().filter(|(_, d)| keep(d)).map(|(id, _)| *id).collect()
        };
        match self.health {
            Health::FullyConstrained | Health::UnderConstrained { .. } if self.solved.is_some() => None,
            Health::Conflict => Some(EvalDiagnosis::Conflict { constraints: self.conflict_set() }),
            Health::NotConverged => {
                Some(EvalDiagnosis::NotConverged { unmet: of(|d| matches!(d, Diagnosis::Unmet { .. })) })
            }
            _ => {
                if let Some(e) = &self.parameter_error {
                    let parameter = match e {
                        ParameterError::Expression { parameter, .. } => Some(parameter.clone()),
                        ParameterError::Cycle { cycle } => cycle.first().cloned(),
                        ParameterError::InvalidName { name, .. } => Some(name.clone()),
                        _ => None,
                    };
                    return Some(EvalDiagnosis::InvalidParameters { parameter });
                }
                let invalid = of(|d| matches!(d, Diagnosis::Invalid(_)));
                if invalid.is_empty() {
                    Some(EvalDiagnosis::InvalidDefinition)
                } else {
                    Some(EvalDiagnosis::InvalidDimensions { constraints: invalid })
                }
            }
        }
    }
}

fn dim_value(kind: DimensionKind, text: &str, params: &Values) -> Result<f64, DimensionError> {
    let expr = Expression::parse(text).map_err(DimensionError::Expression)?;
    let q = expr.evaluate(&|n| params.quantity(n)).map_err(DimensionError::Expression)?;
    let expected = if matches!(kind, DimensionKind::Angle { .. }) { Dim::ANGLE } else { Dim::LENGTH };
    if q.dim != expected {
        return Err(DimensionError::Expression(ExprError {
            kind: ExprErrorKind::WrongDimension { expected, found: q.dim },
            span: expr.span(),
        }));
    }
    match kind {
        DimensionKind::Angle { .. } => Ok(q.value * (180.0 / std::f64::consts::PI)),
        DimensionKind::Distance {
            along: burnburn_document::sketch::Along::X | burnburn_document::sketch::Along::Y,
            ..
        } => Ok(q.value),
        _ if q.value < 0.0 => Err(DimensionError::Negative { value: q.value }),
        _ => Ok(q.value),
    }
}

/// Our problem index for each document entity.
struct Index {
    of: BTreeMap<EntityId, solver::EntityId>,
    back: Vec<EntityId>,
}

fn along(a: burnburn_document::sketch::Along) -> Along {
    match a {
        burnburn_document::sketch::Along::X => Along::X,
        burnburn_document::sketch::Along::Y => Along::Y,
        _ => Along::Direct,
    }
}

fn lower(sketch: &Sketch) -> Option<(Problem, Index)> {
    let mut problem = Problem::new();
    let mut index = Index { of: BTreeMap::new(), back: Vec::new() };
    let sol = sketch.solution();
    // Entities are added in id order; references always point to lower ids (points first).
    for (id, e) in sketch.entities() {
        let get = |p: EntityId, index: &Index| index.of.get(&p).copied();
        let entity = match e.entity {
            DocEntity::Point { at } => Entity::Point { at: sol.and_then(|s| s.points.get(&id).copied()).unwrap_or(at) },
            DocEntity::Line { start, end } => Entity::Line { start: get(start, &index)?, end: get(end, &index)? },
            DocEntity::Arc { center, start, end, clockwise } => Entity::Arc {
                center: get(center, &index)?,
                start: get(start, &index)?,
                end: get(end, &index)?,
                clockwise,
            },
            DocEntity::Circle { center, radius } => Entity::Circle {
                center: get(center, &index)?,
                radius: sol.and_then(|s| s.radii.get(&id).copied()).unwrap_or(radius),
            },
            // The solver sees an ellipse as its major axis: a line from the centre to the major end.
            DocEntity::Ellipse { center, major, .. } => {
                Entity::Line { start: get(center, &index)?, end: get(major, &index)? }
            }
            _ => return None,
        };
        let pid = problem.add(entity);
        index.of.insert(id, pid);
        index.back.push(id);
    }
    Some((problem, index))
}

/// The solver entity of the centre of a circle, arc or ellipse.
fn centre_of(sketch: &Sketch, id: EntityId, e: &dyn Fn(EntityId) -> Option<solver::EntityId>) -> Option<solver::EntityId> {
    match sketch.entity(id)?.entity {
        DocEntity::Circle { center, .. } | DocEntity::Arc { center, .. } | DocEntity::Ellipse { center, .. } => {
            e(center)
        }
        _ => None,
    }
}

fn relation(
    sketch: &Sketch,
    r: Relation,
    e: &dyn Fn(EntityId) -> Option<solver::EntityId>,
) -> Option<Constraint> {
    let is_ellipse = |id: EntityId| matches!(sketch.entity(id), Some(SketchEntity { entity: DocEntity::Ellipse { .. }, .. }));
    Some(match r {
        // The solver's concentricity wants round curves; an ellipse shares its centre point.
        Relation::Concentric(a, b) if is_ellipse(a) || is_ellipse(b) => {
            Constraint::Coincident(centre_of(sketch, a, e)?, centre_of(sketch, b, e)?)
        }
        Relation::Coincident(a, b) => Constraint::Coincident(e(a)?, e(b)?),
        Relation::PointOn { point, curve } => Constraint::PointOn { point: e(point)?, curve: e(curve)? },
        Relation::Horizontal(l) => Constraint::Horizontal(e(l)?),
        Relation::Vertical(l) => Constraint::Vertical(e(l)?),
        Relation::Parallel(a, b) => Constraint::Parallel(e(a)?, e(b)?),
        Relation::Perpendicular(a, b) => Constraint::Perpendicular(e(a)?, e(b)?),
        Relation::Tangent(a, b) => Constraint::Tangent(e(a)?, e(b)?),
        Relation::Equal(a, b) => Constraint::Equal(e(a)?, e(b)?),
        Relation::Midpoint { point, line } => Constraint::Midpoint { point: e(point)?, line: e(line)? },
        Relation::Symmetric { a, b, about } => Constraint::Symmetric { a: e(a)?, b: e(b)?, about: e(about)? },
        Relation::Concentric(a, b) => Constraint::Concentric(e(a)?, e(b)?),
        Relation::Fix { point, at } => Constraint::Fix { point: e(point)?, at },
        _ => return None,
    })
}

fn dimension(
    sketch: &Sketch,
    kind: DimensionKind,
    value: f64,
    e: &dyn Fn(EntityId) -> Option<solver::EntityId>,
) -> Option<Constraint> {
    let is_ellipse = |id: EntityId| matches!(sketch.entity(id), Some(SketchEntity { entity: DocEntity::Ellipse { .. }, .. }));
    Some(match kind {
        // An ellipse is its major axis for the solver: radius = semi-major, diameter = full axis.
        DimensionKind::Radius { curve } if is_ellipse(curve) => Constraint::Length { line: e(curve)?, value },
        DimensionKind::Diameter { curve } if is_ellipse(curve) => {
            Constraint::Length { line: e(curve)?, value: value / 2.0 }
        }
        DimensionKind::Distance { a, b, along: al } => {
            Constraint::Distance { a: e(a)?, b: e(b)?, value, along: along(al) }
        }
        DimensionKind::Length { line } => Constraint::Length { line: e(line)?, value },
        DimensionKind::Radius { curve } => Constraint::Radius { curve: e(curve)?, value },
        DimensionKind::Diameter { curve } => Constraint::Diameter { curve: e(curve)?, value },
        DimensionKind::Angle { from, to } => Constraint::Angle { from: e(from)?, to: e(to)?, degrees: value },
        _ => return None,
    })
}

/// Evaluates a sketch definition. Cancellation is checked before and during the solve.
pub fn evaluate_sketch(
    sketch: &Sketch,
    solver: &dyn Solver,
    cancel: &CancellationToken,
) -> Result<Evaluation, Cancelled> {
    cancel.check()?;
    let mut diagnostics: BTreeMap<ConstraintId, Diagnosis> =
        sketch.constraints().map(|(id, _)| (id, Diagnosis::NotEvaluated)).collect();
    let failed = |diagnostics, parameter_error, problem_error| Evaluation {
        health: Health::Invalid,
        dof: 0,
        free: Vec::new(),
        diagnostics,
        flipped: Vec::new(),
        residual: 0.0,
        tolerance: 0.0,
        parameter_error,
        problem_error,
        solved: None,
    };
    let params = match Parameters::from_definitions(sketch.parameters().iter()).and_then(|p| p.evaluate()) {
        Ok(v) => v,
        Err(e) => {
            for (id, c) in sketch.constraints() {
                if let SketchConstraint::Dimension(d) = c
                    && d.role == DimensionRole::Driving
                {
                    diagnostics.insert(id, Diagnosis::Invalid(DimensionError::Parameters(e.clone())));
                }
            }
            return Ok(failed(diagnostics, Some(e), None));
        }
    };
    let Some((mut problem, index)) = lower(sketch) else {
        return Ok(failed(diagnostics, None, None));
    };
    let e = |id: EntityId| index.of.get(&id).copied();
    let mut to_sketch: Vec<ConstraintId> = Vec::new();
    let mut to_problem: BTreeMap<ConstraintId, usize> = BTreeMap::new();
    let mut any_invalid = false;
    for (id, c) in sketch.constraints() {
        let lowered = match c {
            SketchConstraint::Relation(r) => relation(sketch, *r, &e),
            SketchConstraint::Dimension(d) if d.role == DimensionRole::Driven => continue,
            SketchConstraint::Dimension(d) => match dim_value(d.kind, &d.text, &params) {
                Ok(v) => dimension(sketch, d.kind, v, &e),
                Err(err) => {
                    diagnostics.insert(id, Diagnosis::Invalid(err));
                    any_invalid = true;
                    continue;
                }
            },
        };
        let Some(lowered) = lowered else {
            return Ok(failed(diagnostics, None, None));
        };
        to_problem.insert(id, problem.constraints.len());
        problem.constraints.push(lowered);
        to_sketch.push(id);
    }
    if any_invalid {
        return Ok(failed(diagnostics, None, None));
    }
    let report = match solver.solve(&problem, cancel) {
        Ok(r) => r,
        Err(SolveError::Cancelled) => return Err(Cancelled),
        Err(err) => return Ok(failed(diagnostics, None, Some(err))),
    };
    let map = |v: &[solver::ConstraintIndex]| -> Vec<ConstraintId> {
        v.iter().filter_map(|c| to_sketch.get(c.0).copied()).collect()
    };
    for (id, p) in &to_problem {
        let d = match report.constraints.get(*p) {
            Some(ConstraintState::Satisfied) => Diagnosis::Satisfied,
            Some(ConstraintState::Redundant { with }) => Diagnosis::Redundant { with: map(with) },
            Some(ConstraintState::Conflicting { with }) => Diagnosis::Conflicting { with: map(with) },
            Some(ConstraintState::Unmet { residual }) => Diagnosis::Unmet { residual: *residual },
            None => Diagnosis::NotEvaluated,
        };
        diagnostics.insert(*id, d);
    }
    let health = match report.status {
        solver::SolveStatus::Solved if report.dof == 0 => Health::FullyConstrained,
        solver::SolveStatus::Solved => Health::UnderConstrained { dof: report.dof },
        solver::SolveStatus::Conflict => Health::Conflict,
        solver::SolveStatus::NotConverged => Health::NotConverged,
    };
    let solved = health.is_valid().then(|| {
        let entities = sketch
            .entities()
            .zip(&report.entities)
            .map(|((id, e), s)| {
                let entity = match (e.entity, s) {
                    (DocEntity::Point { .. }, solver::Solved::Point(p)) => DocEntity::Point { at: *p },
                    // A negative solved radius is the same circle seen from the other side of a
                    // tangent (a reported flip): the geometry has the positive radius.
                    (DocEntity::Circle { center, .. }, solver::Solved::Circle { radius }) => {
                        DocEntity::Circle { center, radius: radius.abs() }
                    }
                    (other, _) => other,
                };
                (id, SketchEntity { entity, construction: e.construction })
            })
            .collect();
        SolvedSketch { entities }
    });
    if let Some(s) = &solved {
        for (id, c) in sketch.constraints() {
            if let SketchConstraint::Dimension(d) = c
                && d.role == DimensionRole::Driven
            {
                diagnostics.insert(id, measure(s, d.kind).map_or(Diagnosis::NotEvaluated, Diagnosis::Measured));
            }
        }
    }
    let free = report.free.iter().filter_map(|f| Some((*index.back.get(f.entity.index())?, f.coordinate))).collect();
    Ok(Evaluation {
        health,
        dof: report.dof,
        free,
        diagnostics,
        flipped: report.flipped,
        residual: report.residual,
        tolerance: report.tolerance,
        parameter_error: None,
        problem_error: None,
        solved,
    })
}

/// Measures a dimension on solved geometry (driven dimensions; display values).
pub fn measure(s: &SolvedSketch, kind: DimensionKind) -> Option<Value> {
    let len = |mm: f64| burnburn_types::Length::from_mm(mm).ok().map(Value::Length);
    let line = |id: EntityId| match s.entities.get(&id)?.entity {
        DocEntity::Line { start, end } => Some((s.point(start)?, s.point(end)?)),
        _ => None,
    };
    match kind {
        DimensionKind::Distance { a, b, along: al } => {
            let pa = s.point(a)?;
            match (s.point(b), along(al)) {
                (Some(pb), Along::Direct) => len(pa.distance(pb)),
                (Some(pb), Along::X) => len(pb.x - pa.x),
                (Some(pb), Along::Y) => len(pb.y - pa.y),
                (None, Along::Direct) => {
                    let (p, q) = line(b)?;
                    let (dx, dy) = (q.x - p.x, q.y - p.y);
                    let l = (dx * dx + dy * dy).sqrt();
                    if l > 0.0 { len(((pa.x - p.x) * dy - (pa.y - p.y) * dx).abs() / l) } else { None }
                }
                (None, _) => None,
            }
        }
        DimensionKind::Length { line: l } => {
            let (p, q) = line(l)?;
            len(p.distance(q))
        }
        DimensionKind::Radius { curve } => len(s.radius(curve)?),
        DimensionKind::Diameter { curve } => len(2.0 * s.radius(curve)?),
        DimensionKind::Angle { from, to } => {
            let ((a0, a1), (b0, b1)) = (line(from)?, line(to)?);
            // libm: platform-independent bits (DR-13).
            let ta = libm::atan2(a1.y - a0.y, a1.x - a0.x);
            let tb = libm::atan2(b1.y - b0.y, b1.x - b0.x);
            let mut d = tb - ta;
            if d <= -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            } else if d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            burnburn_types::Angle::from_radians(d).ok().map(Value::Angle)
        }
        _ => None,
    }
}
