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
    /// Ellipses whose semi-minor radius the solver moved away from the drawn one.
    pub(crate) minors: BTreeMap<EntityId, f64>,
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
        s.minors.clone_from(&self.minors);
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
        DimensionKind::MinorDiameter { .. } if q.value <= 0.0 => Err(DimensionError::Negative { value: q.value }),
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

/// What the solver is given to see an ellipse beyond its major axis (G7-editor, S01). The solver
/// knows points, lines, arcs and circles only, so the ellipse is described by them: a point at the
/// end of the semi-minor axis (perpendicular to the major axis through the centre), the two foci
/// (on the major axis, as far from the centre as the minor-axis end is from a focus is long), and
/// the mirror image of the major end (so that `2a` is a length the solver can equate to).
/// A point lies on the ellipse when its distances to the foci add up to `2a`, and a line is
/// tangent when the mirror image of one focus in it is `2a` from the other focus.
#[derive(Debug, Clone, Copy)]
struct EllipseAux {
    f1: solver::EntityId,
    f2: solver::EntityId,
    /// Line centre to the end of the semi-minor axis: its length is the semi-minor radius.
    minor_line: solver::EntityId,
    minor_end: solver::EntityId,
    /// Line from the major end to its mirror image about the centre: `2a` long.
    director: solver::EntityId,
}

/// The solver problem under construction, with the auxiliary geometry of the ellipses that need it.
struct Builder<'a> {
    sketch: &'a Sketch,
    params: &'a Values,
    problem: Problem,
    index: Index,
    ellipses: BTreeMap<EntityId, EllipseAux>,
    /// The other branch: which axis of an ellipse carries the foci is decided the other way round
    /// (the retry of an evaluation that did not converge, G8-editor).
    flip: bool,
}

impl Builder<'_> {
    fn e(&self, id: EntityId) -> Option<solver::EntityId> {
        self.index.of.get(&id).copied()
    }

    /// Where the solver is asked to start for a point entity.
    fn at(&self, id: solver::EntityId) -> Option<Point> {
        match self.problem.entities.get(id.index())? {
            Entity::Point { at } => Some(*at),
            _ => None,
        }
    }

    fn point(&mut self, at: Point) -> solver::EntityId {
        self.problem.add(Entity::Point { at })
    }

    fn line(&mut self, a: solver::EntityId, b: solver::EntityId) -> solver::EntityId {
        self.problem.add(Entity::Line { start: a, end: b })
    }

    /// The driving minor-axis dimension of `ellipse`, as a semi-minor radius, when it has one.
    fn minor_dimension(&self, ellipse: EntityId) -> Option<f64> {
        self.sketch.constraints().find_map(|(_, c)| match c {
            SketchConstraint::Dimension(d)
                if d.role == DimensionRole::Driving
                    && matches!(d.kind, DimensionKind::MinorDiameter { ellipse: e } if e == ellipse) =>
            {
                dim_value(d.kind, &d.text, self.params).ok().map(|v| v / 2.0)
            }
            _ => None,
        })
    }

    /// The driving dimension of the semi-major radius of `ellipse`, when it has one (a radius or a
    /// diameter): the length the major axis is going to have, which decides which axis is longer.
    fn major_dimension(&self, ellipse: EntityId) -> Option<f64> {
        self.sketch.constraints().find_map(|(_, c)| match c {
            SketchConstraint::Dimension(d) if d.role == DimensionRole::Driving => match d.kind {
                DimensionKind::Radius { curve } if curve == ellipse => dim_value(d.kind, &d.text, self.params).ok(),
                DimensionKind::Diameter { curve } if curve == ellipse => {
                    dim_value(d.kind, &d.text, self.params).ok().map(|v| v / 2.0)
                }
                _ => None,
            },
            _ => None,
        })
    }

    /// The ellipse as it is now: centre, unit directions of its axes, semi-axes (drawn or last solved,
    /// or as dimensioned for the semi-minor).
    fn ellipse_geometry(&self, ellipse: EntityId) -> Option<(Point, (f64, f64), (f64, f64), f64, f64)> {
        let DocEntity::Ellipse { center, major, minor } = self.sketch.entity(ellipse)?.entity else { return None };
        let minor = self.sketch.solution().and_then(|s| s.minors.get(&ellipse).copied()).unwrap_or(minor);
        let (pc, pm) = (self.at(self.e(center)?)?, self.at(self.e(major)?)?);
        let a = pc.distance(pm);
        if !(a > 0.0) {
            return None;
        }
        let u = ((pm.x - pc.x) / a, (pm.y - pc.y) / a);
        let b = self.minor_dimension(ellipse).filter(|b| *b > 0.0).unwrap_or(minor);
        Some((pc, u, (-u.1, u.0), a, b))
    }

    /// `n` points on the ellipse in its current shape.
    fn ellipse_samples(&self, ellipse: EntityId, n: usize) -> Option<Vec<Point>> {
        let (c, u, v, a, b) = self.ellipse_geometry(ellipse)?;
        Some(
            (0..n)
                .map(|i| {
                    let t = std::f64::consts::TAU * i as f64 / n as f64;
                    let (s, k) = (t.sin(), t.cos());
                    Point::new(c.x + a * k * u.0 + b * s * v.0, c.y + a * k * u.1 + b * s * v.1)
                })
                .collect(),
        )
    }

    fn has_minor_dimension(&self, ellipse: EntityId) -> bool {
        self.sketch.constraints().any(|(_, c)| {
            matches!(c, SketchConstraint::Dimension(d)
                if d.role == DimensionRole::Driving
                    && matches!(d.kind, DimensionKind::MinorDiameter { ellipse: e } if e == ellipse))
        })
    }

    /// The auxiliary geometry of `ellipse`, made on first use. The constraints that tie it to the
    /// ellipse go to `hidden` (the caller attributes them to the constraint that needed it).
    fn ellipse_aux(&mut self, ellipse: EntityId, hidden: &mut Vec<Constraint>) -> Option<EllipseAux> {
        if let Some(aux) = self.ellipses.get(&ellipse) {
            return Some(*aux);
        }
        let DocEntity::Ellipse { center, major, minor } = self.sketch.entity(ellipse)?.entity else { return None };
        // Where the ellipse was last solved, else as drawn.
        let minor = self.sketch.solution().and_then(|s| s.minors.get(&ellipse).copied()).unwrap_or(minor);
        let (c, m, line) = (self.e(center)?, self.e(major)?, self.e(ellipse)?);
        let (pc, pm) = (self.at(c)?, self.at(m)?);
        let a = pc.distance(pm);
        if !(a > 0.0) {
            return None;
        }
        let u = ((pm.x - pc.x) / a, (pm.y - pc.y) / a);
        let v = (-u.1, u.0);
        let b = self.minor_dimension(ellipse).filter(|b| *b > 0.0).unwrap_or(minor);
        // The foci lie on the longer axis. Normally that is the major axis; an ellipse whose
        // "minor" radius is the larger one has them on the other axis, and the roles swap. A
        // dimension on the major axis says how long it is going to be, which is what counts: an
        // edit that drives it through a = b changes the roles in one solve. Without one the
        // current length is taken, and a solve that does not converge is tried again the other way
        // round (`flip`).
        let a_to_be = self.major_dimension(ellipse).filter(|v| *v > 0.0).unwrap_or(a);
        let swapped = (b > a_to_be) != self.flip;
        let (long, short) = if b > a_to_be { (b, a_to_be) } else { (a_to_be, b) };
        let focal = (long * long - short * short).max(0.0).sqrt().max(1e-3 * long);
        let direction = if swapped { v } else { u };
        let minor_end = self.point(Point::new(pc.x + b * v.0, pc.y + b * v.1));
        let minor_line = self.line(c, minor_end);
        let (long_end, long_line, short_end) = if swapped { (minor_end, minor_line, m) } else { (m, line, minor_end) };
        let (pl, pe) = (self.at(long_end)?, self.at(c)?);
        let long_mirror = self.point(Point::new(2.0 * pe.x - pl.x, 2.0 * pe.y - pl.y));
        let f1 = self.point(Point::new(pc.x + focal * direction.0, pc.y + focal * direction.1));
        let f2 = self.point(Point::new(pc.x - focal * direction.0, pc.y - focal * direction.1));
        let director = self.line(long_end, long_mirror);
        let focal_line = self.line(c, f1);
        let focal_to_short = self.line(f1, short_end);
        hidden.extend([
            // The mirror image of the end of the long axis: |end mirror| = 2 * long.
            Constraint::Symmetric { a: long_end, b: long_mirror, about: c },
            // The end of the semi-minor axis is perpendicular to the major axis through the centre ...
            Constraint::Perpendicular(minor_line, line),
            // ... the foci lie on the long axis, as far from the centre as c = sqrt(long^2 - short^2):
            // the end of the short axis is exactly `long` from a focus ...
            Constraint::Parallel(focal_line, long_line),
            Constraint::Equal(focal_to_short, long_line),
            // ... and the foci are each other's mirror image.
            Constraint::Symmetric { a: f1, b: f2, about: c },
        ]);
        if !self.has_minor_dimension(ellipse) {
            // Without a dimension the drawn semi-minor stays what it is: it was never a freedom.
            hidden.push(Constraint::Length { line: minor_line, value: minor });
        }
        let aux = EllipseAux { f1, f2, minor_line, minor_end, director };
        self.ellipses.insert(ellipse, aux);
        Some(aux)
    }

    /// A point on an ellipse: its distances to the foci add up to `2a`. The auxiliary point `q`
    /// lies on the ray from the first focus through the point, `2a` from that focus, so the point
    /// is as far from `q` as from the second focus exactly when the sum is `2a`.
    fn point_on_ellipse(&mut self, point: EntityId, ellipse: EntityId, out: &mut Vec<Constraint>) -> Option<()> {
        let p = self.e(point)?;
        self.solver_point_on_ellipse(p, ellipse, out)
    }

    fn solver_point_on_ellipse(
        &mut self,
        p: solver::EntityId,
        ellipse: EntityId,
        out: &mut Vec<Constraint>,
    ) -> Option<()> {
        let aux = self.ellipse_aux(ellipse, out)?;
        let (pp, pf1) = (self.at(p)?, self.at(aux.f1)?);
        let two_a = match *self.problem.entities.get(aux.director.index())? {
            Entity::Line { start, end } => self.at(start)?.distance(self.at(end)?),
            _ => return None,
        };
        let d = pf1.distance(pp);
        let (dx, dy) = if d > 0.0 { ((pp.x - pf1.x) / d, (pp.y - pf1.y) / d) } else { (1.0, 0.0) };
        let q = self.point(Point::new(pf1.x + dx * two_a, pf1.y + dy * two_a));
        let (f1_p, f1_q, p_f2, p_q) =
            (self.line(aux.f1, p), self.line(aux.f1, q), self.line(p, aux.f2), self.line(p, q));
        out.extend([
            // q is on the ray from the first focus through the point ...
            Constraint::Parallel(f1_p, f1_q),
            // ... 2a away from it ...
            Constraint::Equal(f1_q, aux.director),
            // ... and the point is as far from q as from the second focus.
            Constraint::Equal(p_f2, p_q),
        ]);
        Some(())
    }

    /// A line tangent to an ellipse: the mirror image of the first focus in the line is `2a` from
    /// the second focus.
    fn tangent_to_ellipse(&mut self, line: EntityId, ellipse: EntityId, out: &mut Vec<Constraint>) -> Option<()> {
        let l = self.e(line)?;
        self.solver_tangent_to_ellipse(l, ellipse, out)
    }

    fn solver_tangent_to_ellipse(
        &mut self,
        l: solver::EntityId,
        ellipse: EntityId,
        out: &mut Vec<Constraint>,
    ) -> Option<()> {
        let aux = self.ellipse_aux(ellipse, out)?;
        let Entity::Line { start, end } = *self.problem.entities.get(l.index())? else { return None };
        let (ps, pe, pf1) = (self.at(start)?, self.at(end)?, self.at(aux.f1)?);
        let (dx, dy) = (pe.x - ps.x, pe.y - ps.y);
        let len2 = dx * dx + dy * dy;
        if !(len2 > 0.0) {
            return None;
        }
        // Reflection of the first focus in the (infinite) line.
        let t = ((pf1.x - ps.x) * dx + (pf1.y - ps.y) * dy) / len2;
        let foot = Point::new(ps.x + t * dx, ps.y + t * dy);
        let mirror = self.point(Point::new(2.0 * foot.x - pf1.x, 2.0 * foot.y - pf1.y));
        let to_f2 = self.line(mirror, aux.f2);
        out.extend([Constraint::Symmetric { a: aux.f1, b: mirror, about: l }, Constraint::Equal(to_f2, aux.director)]);
        Some(())
    }
}

/// What an ellipse touches in the contact relation.
#[derive(Clone, Copy)]
enum Contact {
    Circle(EntityId),
    Ellipse(EntityId),
}

impl Builder<'_> {
    /// Where two curves are closest now, as a starting point for their contact: a point near both
    /// and the unit direction of `ellipse` there.
    fn contact_guess(&self, ellipse: EntityId, other: Contact) -> Option<(Point, (f64, f64))> {
        const N: usize = 360;
        let own = self.ellipse_samples(ellipse, N)?;
        let (at, index) = match other {
            Contact::Circle(circle) => {
                let DocEntity::Circle { center, radius } = self.sketch.entity(circle)?.entity else { return None };
                let radius = self.sketch.solution().and_then(|s| s.radii.get(&circle).copied()).unwrap_or(radius);
                let o = self.at(self.e(center)?)?;
                // The sample whose distance from the circle's centre is nearest to the radius.
                let (i, _) = own
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (i, (p.distance(o) - radius).abs()))
                    .min_by(|x, y| x.1.total_cmp(&y.1))?;
                let d = own[i].distance(o);
                let dir = if d > 0.0 { ((own[i].x - o.x) / d, (own[i].y - o.y) / d) } else { (1.0, 0.0) };
                (Point::new(o.x + dir.0 * radius, o.y + dir.1 * radius), i)
            }
            Contact::Ellipse(second) => {
                let theirs = self.ellipse_samples(second, N)?;
                let mut best = (f64::INFINITY, 0usize, 0usize);
                for (i, p) in own.iter().enumerate() {
                    for (j, q) in theirs.iter().enumerate() {
                        let d = p.distance(*q);
                        if d < best.0 {
                            best = (d, i, j);
                        }
                    }
                }
                (own[best.1].lerp(theirs[best.2], 1.0), best.1)
            }
        };
        let (prev, next) = (own[(index + N - 1) % N], own[(index + 1) % N]);
        let (dx, dy) = (next.x - prev.x, next.y - prev.y);
        let l = (dx * dx + dy * dy).sqrt();
        (l > 0.0).then_some((at, (dx / l, dy / l)))
    }

    /// The normal of `ellipse` at the point `t` on it, as a line through `t`: it runs along the
    /// sum of the unit vectors from `t` to the two foci (the reflection property). Unlike the
    /// mirror-image construction of a free tangent line this is first order in the point, so it
    /// stays well conditioned when the point is pinned to the ellipse.
    fn ellipse_normal(
        &mut self,
        t: solver::EntityId,
        ellipse: EntityId,
        out: &mut Vec<Constraint>,
    ) -> Option<solver::EntityId> {
        let aux = self.ellipse_aux(ellipse, out)?;
        let (pt, p1, p2) = (self.at(t)?, self.at(aux.f1)?, self.at(aux.f2)?);
        let unit = |to: Point| {
            let d = pt.distance(to);
            if d > 0.0 { ((to.x - pt.x) / d, (to.y - pt.y) / d) } else { (1.0, 0.0) }
        };
        let (u1, u2) = (unit(p1), unit(p2));
        let a1 = self.point(Point::new(pt.x + u1.0, pt.y + u1.1));
        let a2 = self.point(Point::new(pt.x + u2.0, pt.y + u2.1));
        // The fourth corner of the parallelogram on the two unit vectors: its diagonal from `t`
        // is their sum, which is never of zero length (the foci are on one side of the point).
        // Both diagonals share their midpoint, a linear condition without a second branch.
        let k = self.point(Point::new(pt.x + u1.0 + u2.0, pt.y + u1.1 + u2.1));
        let centre = self.point(Point::new(pt.x + (u1.0 + u2.0) / 2.0, pt.y + (u1.1 + u2.1) / 2.0));
        let (ta1, tf1, ta2, tf2) = (self.line(t, a1), self.line(t, aux.f1), self.line(t, a2), self.line(t, aux.f2));
        let a1a2 = self.line(a1, a2);
        let normal = self.line(t, k);
        out.extend([
            Constraint::Parallel(ta1, tf1),
            Constraint::Length { line: ta1, value: 1.0 },
            Constraint::Parallel(ta2, tf2),
            Constraint::Length { line: ta2, value: 1.0 },
            Constraint::Midpoint { point: centre, line: a1a2 },
            Constraint::Midpoint { point: centre, line: normal },
        ]);
        Some(normal)
    }

    /// An ellipse touching a circle or another ellipse: a contact point `t` on both and a unit-length
    /// line from it that is perpendicular to the normals of both there. That line exists only when the
    /// curves really touch, so the relation takes exactly one degree of freedom from them.
    fn ellipse_contact(&mut self, ellipse: EntityId, other: Contact, out: &mut Vec<Constraint>) -> Option<()> {
        let (start, direction) = self.contact_guess(ellipse, other)?;
        let t = self.point(start);
        let end = self.point(Point::new(start.x + direction.0, start.y + direction.1));
        let l = self.line(t, end);
        out.push(Constraint::Length { line: l, value: 1.0 });
        self.solver_point_on_ellipse(t, ellipse, out)?;
        let normal = self.ellipse_normal(t, ellipse, out)?;
        out.push(Constraint::Perpendicular(l, normal));
        match other {
            Contact::Circle(circle) => {
                let c = self.e(circle)?;
                let DocEntity::Circle { center, .. } = self.sketch.entity(circle)?.entity else { return None };
                let radius_line = self.line(self.e(center)?, t);
                out.push(Constraint::PointOn { point: t, curve: c });
                out.push(Constraint::Perpendicular(l, radius_line));
            }
            Contact::Ellipse(second) => {
                self.solver_point_on_ellipse(t, second, out)?;
                let second_normal = self.ellipse_normal(t, second, out)?;
                out.push(Constraint::Perpendicular(l, second_normal));
            }
        }
        Some(())
    }
}

/// The solver entity of the centre of a circle, arc or ellipse.
fn centre_of(
    sketch: &Sketch,
    id: EntityId,
    e: &dyn Fn(EntityId) -> Option<solver::EntityId>,
) -> Option<solver::EntityId> {
    match sketch.entity(id)?.entity {
        DocEntity::Circle { center, .. } | DocEntity::Arc { center, .. } | DocEntity::Ellipse { center, .. } => {
            e(center)
        }
        _ => None,
    }
}

fn relation(b: &mut Builder<'_>, r: Relation, out: &mut Vec<Constraint>) -> Option<()> {
    let sketch = b.sketch;
    let e = |id: EntityId| b.index.of.get(&id).copied();
    let is_ellipse =
        |id: EntityId| matches!(sketch.entity(id), Some(SketchEntity { entity: DocEntity::Ellipse { .. }, .. }));
    let is_circle =
        |id: EntityId| matches!(sketch.entity(id), Some(SketchEntity { entity: DocEntity::Circle { .. }, .. }));
    match r {
        Relation::Tangent(a, c) if is_ellipse(a) && is_ellipse(c) => {
            return b.ellipse_contact(a, Contact::Ellipse(c), out);
        }
        Relation::Tangent(a, c) if is_ellipse(a) && is_circle(c) => {
            return b.ellipse_contact(a, Contact::Circle(c), out);
        }
        Relation::Tangent(a, c) if is_ellipse(c) && is_circle(a) => {
            return b.ellipse_contact(c, Contact::Circle(a), out);
        }
        _ => {}
    }
    let one = match r {
        // The solver's concentricity wants round curves; an ellipse shares its centre point.
        Relation::Concentric(a, c) if is_ellipse(a) || is_ellipse(c) => {
            Constraint::Coincident(centre_of(sketch, a, &e)?, centre_of(sketch, c, &e)?)
        }
        Relation::PointOn { point, curve } if is_ellipse(curve) => return b.point_on_ellipse(point, curve, out),
        Relation::Tangent(a, c) if is_ellipse(a) => return b.tangent_to_ellipse(c, a, out),
        Relation::Tangent(a, c) if is_ellipse(c) => return b.tangent_to_ellipse(a, c, out),
        Relation::Coincident(a, c) => Constraint::Coincident(e(a)?, e(c)?),
        Relation::PointOn { point, curve } => Constraint::PointOn { point: e(point)?, curve: e(curve)? },
        Relation::Horizontal(l) => Constraint::Horizontal(e(l)?),
        Relation::Vertical(l) => Constraint::Vertical(e(l)?),
        Relation::Parallel(a, c) => Constraint::Parallel(e(a)?, e(c)?),
        Relation::Perpendicular(a, c) => Constraint::Perpendicular(e(a)?, e(c)?),
        Relation::Tangent(a, c) => Constraint::Tangent(e(a)?, e(c)?),
        Relation::Equal(a, c) => Constraint::Equal(e(a)?, e(c)?),
        Relation::Midpoint { point, line } => Constraint::Midpoint { point: e(point)?, line: e(line)? },
        Relation::Symmetric { a, b: c, about } => Constraint::Symmetric { a: e(a)?, b: e(c)?, about: e(about)? },
        Relation::Concentric(a, c) => Constraint::Concentric(e(a)?, e(c)?),
        Relation::Fix { point, at } => Constraint::Fix { point: e(point)?, at },
        _ => return None,
    };
    out.push(one);
    Some(())
}

fn dimension(b: &mut Builder<'_>, kind: DimensionKind, value: f64, out: &mut Vec<Constraint>) -> Option<()> {
    if let DimensionKind::MinorDiameter { ellipse } = kind {
        // The semi-minor radius is the length of the auxiliary line to the end of the minor axis.
        let aux = b.ellipse_aux(ellipse, out)?;
        out.push(Constraint::Length { line: aux.minor_line, value: value / 2.0 });
        return Some(());
    }
    let sketch = b.sketch;
    let e = |id: EntityId| b.index.of.get(&id).copied();
    let is_ellipse =
        |id: EntityId| matches!(sketch.entity(id), Some(SketchEntity { entity: DocEntity::Ellipse { .. }, .. }));
    out.push(match kind {
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
    });
    Some(())
}

/// Evaluates a sketch definition. Cancellation is checked before and during the solve.
pub fn evaluate_sketch(
    sketch: &Sketch,
    solver: &dyn Solver,
    cancel: &CancellationToken,
) -> Result<Evaluation, Cancelled> {
    let first = evaluate_branch(sketch, solver, cancel, false)?;
    let has_ellipse = sketch.entities().any(|(_, e)| matches!(e.entity, DocEntity::Ellipse { .. }));
    if has_ellipse && matches!(first.health, Health::NotConverged | Health::Conflict) {
        // Which axis of an ellipse is the longer one is a branch the solver cannot jump (G8-editor,
        // S01): when this one failed, the same sketch with the roles decided the other way round
        // gets a go, and is taken when it holds.
        let second = evaluate_branch(sketch, solver, cancel, true)?;
        if second.health.is_valid() {
            return Ok(second);
        }
    }
    Ok(first)
}

/// One attempt of [`evaluate_sketch`]; `flip` decides the roles of the axes of the ellipses the
/// other way round.
fn evaluate_branch(
    sketch: &Sketch,
    solver: &dyn Solver,
    cancel: &CancellationToken,
    flip: bool,
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
    let Some((problem, index)) = lower(sketch) else {
        return Ok(failed(diagnostics, None, None));
    };
    let mut builder = Builder { sketch, params: &params, problem, index, ellipses: BTreeMap::new(), flip };
    // The document constraint each solver constraint belongs to. One constraint of the document can
    // be several in the solver (an ellipse needs its auxiliary geometry tied down as well).
    let mut to_sketch: Vec<ConstraintId> = Vec::new();
    let mut any_invalid = false;
    for (id, c) in sketch.constraints() {
        let mut lowered: Vec<Constraint> = Vec::new();
        let ok = match c {
            SketchConstraint::Relation(r) => relation(&mut builder, *r, &mut lowered),
            SketchConstraint::Dimension(d) if d.role == DimensionRole::Driven => continue,
            SketchConstraint::Dimension(d) => match dim_value(d.kind, &d.text, &params) {
                Ok(v) => dimension(&mut builder, d.kind, v, &mut lowered),
                Err(err) => {
                    diagnostics.insert(id, Diagnosis::Invalid(err));
                    any_invalid = true;
                    continue;
                }
            },
        };
        if ok.is_none() {
            return Ok(failed(diagnostics, None, None));
        }
        to_sketch.extend(std::iter::repeat_n(id, lowered.len()));
        builder.problem.constraints.extend(lowered);
    }
    if any_invalid {
        return Ok(failed(diagnostics, None, None));
    }
    let Builder { problem, index, ellipses, .. } = builder;
    let report = match solver.solve(&problem, cancel) {
        Ok(r) => r,
        Err(SolveError::Cancelled) => return Err(Cancelled),
        Err(err) => return Ok(failed(diagnostics, None, Some(err))),
    };
    let map = |own: ConstraintId, v: &[solver::ConstraintIndex]| -> Vec<ConstraintId> {
        let mut out: Vec<ConstraintId> =
            v.iter().filter_map(|c| to_sketch.get(c.0).copied()).filter(|c| *c != own).collect();
        out.sort();
        out.dedup();
        out
    };
    let mut ids: Vec<ConstraintId> = to_sketch.clone();
    ids.dedup();
    ids.sort();
    ids.dedup();
    for id in ids {
        // The worst state of the solver constraints the document constraint became.
        let mut state = Diagnosis::Satisfied;
        let rank = |d: &Diagnosis| match d {
            Diagnosis::Conflicting { .. } => 3,
            Diagnosis::Unmet { .. } => 2,
            Diagnosis::Redundant { .. } => 1,
            _ => 0,
        };
        for (i, owner) in to_sketch.iter().enumerate() {
            if *owner != id {
                continue;
            }
            let d = match report.constraints.get(i) {
                Some(ConstraintState::Satisfied) => Diagnosis::Satisfied,
                Some(ConstraintState::Redundant { with }) => Diagnosis::Redundant { with: map(id, with) },
                Some(ConstraintState::Conflicting { with }) => Diagnosis::Conflicting { with: map(id, with) },
                Some(ConstraintState::Unmet { residual }) => Diagnosis::Unmet { residual: *residual },
                None => Diagnosis::NotEvaluated,
            };
            if rank(&d) > rank(&state) {
                state = d;
            }
        }
        diagnostics.insert(id, state);
    }
    let health = match report.status {
        solver::SolveStatus::Solved if report.dof == 0 => Health::FullyConstrained,
        solver::SolveStatus::Solved => Health::UnderConstrained { dof: report.dof },
        solver::SolveStatus::Conflict => Health::Conflict,
        solver::SolveStatus::NotConverged => Health::NotConverged,
    };
    let solved = health.is_valid().then(|| {
        let mut minors = BTreeMap::new();
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
                    // An ellipse whose minor axis the solver saw: the distance from the centre to
                    // the end of the semi-minor axis is the solved semi-minor radius.
                    (DocEntity::Ellipse { center, major, minor }, _) => {
                        let solved = ellipses.get(&id).and_then(|aux| {
                            let c = report.point(index.of.get(&center).copied()?)?;
                            let end = report.point(aux.minor_end)?;
                            Some(c.distance(end)).filter(|m| *m > 0.0)
                        });
                        if let Some(m) = solved.filter(|m| (*m - minor).abs() > 1e-12) {
                            minors.insert(id, m);
                        }
                        DocEntity::Ellipse { center, major, minor: solved.unwrap_or(minor) }
                    }
                    (other, _) => other,
                };
                (id, SketchEntity { entity, construction: e.construction })
            })
            .collect();
        SolvedSketch { entities, minors }
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
        DimensionKind::MinorDiameter { ellipse } => match s.entities.get(&ellipse)?.entity {
            DocEntity::Ellipse { minor, .. } => len(2.0 * minor),
            _ => None,
        },
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
