//! Sketch definition data: entities, relations, dimensions (with their expression *text*),
//! parameters (name → text) and the last valid solution. Parsing and solving live in
//! `burnburn-sketch`; this module only stores and validates references.

use std::collections::BTreeMap;
use std::fmt;

use burnburn_geometry::Point;
use burnburn_types::StableId;

use super::SketchError;

/// Identity of a sketch in a document. Allocated from the project's shared id counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SketchId(u64);

impl StableId for SketchId {
    fn from_raw(raw: u64) -> Self {
        SketchId(raw)
    }
    fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for SketchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sk{}", self.0)
    }
}

macro_rules! local_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub const fn from_raw(raw: u32) -> Self {
                $name(raw)
            }
            pub const fn raw(self) -> u32 {
                self.0
            }
        }
    };
}

local_id!(
    /// An entity within one sketch; never reused within that sketch.
    EntityId
);
local_id!(
    /// A constraint within one sketch; never reused within that sketch.
    ConstraintId
);

/// A sketch entity in sketch-local millimetres. Positions are the drawing; the solver starts
/// from the last valid [`SketchSolution`] when there is one.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
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
    /// An ellipse around `center`. `major` is the point at the end of the semi-major axis: the
    /// axis direction and the semi-major radius follow from the two points, so relations on the
    /// points (and on the ellipse as its major axis) move it. `minor` is the semi-minor radius
    /// (mm, > 0). It is a plain value: only the two points move under the solver.
    Ellipse {
        center: EntityId,
        major: EntityId,
        minor: f64,
    },
}

impl Entity {
    /// The point entities this entity is built on.
    pub fn points(&self) -> Vec<EntityId> {
        match *self {
            Entity::Point { .. } => Vec::new(),
            Entity::Line { start, end } => vec![start, end],
            Entity::Arc { center, start, end, .. } => vec![center, start, end],
            Entity::Circle { center, .. } => vec![center],
            Entity::Ellipse { center, major, .. } => vec![center, major],
        }
    }
}

/// An entity with its construction flag. Construction geometry never becomes output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SketchEntity {
    pub entity: Entity,
    pub construction: bool,
}

/// How a point-to-point distance is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Along {
    /// Euclidean, never negative.
    #[default]
    Direct,
    /// Signed horizontal offset `b.x - a.x`.
    X,
    /// Signed vertical offset `b.y - a.y`.
    Y,
}

/// A geometric relation without a value.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Relation {
    Coincident(EntityId, EntityId),
    PointOn { point: EntityId, curve: EntityId },
    Horizontal(EntityId),
    Vertical(EntityId),
    Parallel(EntityId, EntityId),
    Perpendicular(EntityId, EntityId),
    Tangent(EntityId, EntityId),
    Equal(EntityId, EntityId),
    Midpoint { point: EntityId, line: EntityId },
    Symmetric { a: EntityId, b: EntityId, about: EntityId },
    Concentric(EntityId, EntityId),
    Fix { point: EntityId, at: Point },
}

/// What a dimension measures.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum DimensionKind {
    Distance {
        a: EntityId,
        b: EntityId,
        along: Along,
    },
    Length {
        line: EntityId,
    },
    Radius {
        curve: EntityId,
    },
    Diameter {
        curve: EntityId,
    },
    /// Counter-clockwise from `from` to `to`.
    Angle {
        from: EntityId,
        to: EntityId,
    },
}

/// Driving dimensions constrain; driven ones only measure (shown as "(120)").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DimensionRole {
    Driving,
    Driven,
}

/// A dimension. `text` is the expression as typed (kept even when invalid, so it can be fixed).
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    pub kind: DimensionKind,
    pub text: String,
    pub role: DimensionRole,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SketchConstraint {
    Relation(Relation),
    Dimension(Dimension),
}

impl SketchConstraint {
    /// Entities referenced, in argument order.
    pub fn entities(&self) -> Vec<EntityId> {
        match self {
            SketchConstraint::Relation(r) => match *r {
                Relation::Coincident(a, b)
                | Relation::Parallel(a, b)
                | Relation::Perpendicular(a, b)
                | Relation::Tangent(a, b)
                | Relation::Equal(a, b)
                | Relation::Concentric(a, b) => vec![a, b],
                Relation::PointOn { point, curve } => vec![point, curve],
                Relation::Horizontal(l) | Relation::Vertical(l) => vec![l],
                Relation::Midpoint { point, line } => vec![point, line],
                Relation::Symmetric { a, b, about } => vec![a, b, about],
                Relation::Fix { point, .. } => vec![point],
            },
            SketchConstraint::Dimension(d) => match d.kind {
                DimensionKind::Distance { a, b, .. } => vec![a, b],
                DimensionKind::Length { line } => vec![line],
                DimensionKind::Radius { curve } | DimensionKind::Diameter { curve } => vec![curve],
                DimensionKind::Angle { from, to } => vec![from, to],
            },
        }
    }
}

/// Parameter definitions, name → expression text, in definition order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParameterTable {
    entries: Vec<(String, String)>,
}

impl ParameterTable {
    pub fn new() -> Self {
        ParameterTable::default()
    }

    /// Defines or replaces `name`. Only the name's shape is checked here; the expression is
    /// stored as typed and checked by the evaluator.
    pub fn set(&mut self, name: &str, text: &str) -> Result<(), SketchError> {
        let mut chars = name.chars();
        let ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            return Err(SketchError::InvalidName(name.to_owned()));
        }
        match self.entries.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = text.to_owned(),
            None => self.entries.push((name.to_owned(), text.to_owned())),
        }
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<(), SketchError> {
        let at = self
            .entries
            .iter()
            .position(|(n, _)| n == name)
            .ok_or_else(|| SketchError::UnknownParameter(name.to_owned()))?;
        self.entries.remove(at);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.iter().find(|(n, _)| n == name).map(|(_, t)| t.as_str())
    }

    /// `(name, text)` in definition order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &str)> {
        self.entries.iter().map(|(n, t)| (n.as_str(), t.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The last valid solution: solved point positions and circle radii (sketch-local mm).
/// Part of the derived state, persisted so reopening shows the same geometry without a solve.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SketchSolution {
    pub points: BTreeMap<EntityId, Point>,
    pub radii: BTreeMap<EntityId, f64>,
    /// Remaining degrees of freedom at the solution.
    pub dof: u32,
}

/// A parametric sketch definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Sketch {
    name: String,
    entities: BTreeMap<EntityId, SketchEntity>,
    constraints: BTreeMap<ConstraintId, SketchConstraint>,
    parameters: ParameterTable,
    next_entity: u32,
    next_constraint: u32,
    solution: Option<SketchSolution>,
}

impl Sketch {
    pub fn new(name: impl Into<String>) -> Self {
        Sketch {
            name: name.into(),
            entities: BTreeMap::new(),
            constraints: BTreeMap::new(),
            parameters: ParameterTable::new(),
            next_entity: 0,
            next_constraint: 0,
            solution: None,
        }
    }

    /// Rebuilds a stored sketch (open/recovery) with the same checks as editing.
    pub fn from_parts(
        name: String,
        entities: Vec<(EntityId, SketchEntity)>,
        constraints: Vec<(ConstraintId, SketchConstraint)>,
        parameters: ParameterTable,
        next_entity: u32,
        next_constraint: u32,
        solution: Option<SketchSolution>,
    ) -> Result<Self, SketchError> {
        let mut s = Sketch::new(name);
        for (id, e) in entities {
            if id.raw() >= next_entity || s.entities.contains_key(&id) {
                return Err(SketchError::InvalidId(id.raw()));
            }
            s.check_entity(&e.entity)?;
            s.entities.insert(id, e);
        }
        for (id, c) in constraints {
            if id.raw() >= next_constraint || s.constraints.contains_key(&id) {
                return Err(SketchError::InvalidId(id.raw()));
            }
            s.check_constraint(&c)?;
            s.constraints.insert(id, c);
        }
        if let Some(sol) = &solution {
            s.check_solution(sol)?;
        }
        s.parameters = parameters;
        s.next_entity = next_entity;
        s.next_constraint = next_constraint;
        s.solution = solution;
        Ok(s)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn entities(&self) -> impl ExactSizeIterator<Item = (EntityId, &SketchEntity)> {
        self.entities.iter().map(|(k, v)| (*k, v))
    }

    pub fn entity(&self, id: EntityId) -> Option<&SketchEntity> {
        self.entities.get(&id)
    }

    pub fn constraints(&self) -> impl ExactSizeIterator<Item = (ConstraintId, &SketchConstraint)> {
        self.constraints.iter().map(|(k, v)| (*k, v))
    }

    pub fn constraint(&self, id: ConstraintId) -> Option<&SketchConstraint> {
        self.constraints.get(&id)
    }

    pub fn parameters(&self) -> &ParameterTable {
        &self.parameters
    }

    /// The last valid solution, if the sketch was ever solved.
    pub fn solution(&self) -> Option<&SketchSolution> {
        self.solution.as_ref()
    }

    /// The ids the next added entity / constraint will get.
    pub fn next_entity_id(&self) -> EntityId {
        EntityId(self.next_entity)
    }

    pub fn next_constraint_id(&self) -> ConstraintId {
        ConstraintId(self.next_constraint)
    }

    fn check_entity(&self, e: &Entity) -> Result<(), SketchError> {
        for p in e.points() {
            match self.entities.get(&p) {
                Some(SketchEntity { entity: Entity::Point { .. }, .. }) => {}
                Some(_) => return Err(SketchError::WrongEntityKind(p)),
                None => return Err(SketchError::UnknownEntity(p)),
            }
        }
        let finite = match *e {
            Entity::Point { at } => at.is_finite(),
            Entity::Circle { radius, .. } => radius.is_finite() && radius >= 0.0,
            Entity::Ellipse { minor, .. } => minor.is_finite() && minor > 0.0,
            _ => true,
        };
        if finite { Ok(()) } else { Err(SketchError::InvalidValue) }
    }

    fn check_constraint(&self, c: &SketchConstraint) -> Result<(), SketchError> {
        for e in c.entities() {
            if !self.entities.contains_key(&e) {
                return Err(SketchError::UnknownEntity(e));
            }
        }
        if let SketchConstraint::Relation(Relation::Fix { at, .. }) = c
            && !at.is_finite()
        {
            return Err(SketchError::InvalidValue);
        }
        // An ellipse is no circular curve: nothing can lie on it, be tangent to it or equal to a
        // circle (its major axis and centre take part in the other relations).
        let is_ellipse = |e: &EntityId| matches!(self.entities.get(e), Some(SketchEntity { entity: Entity::Ellipse { .. }, .. }));
        if let SketchConstraint::Relation(r) = c {
            let bad = match *r {
                Relation::PointOn { curve, .. } if is_ellipse(&curve) => Some(curve),
                Relation::Tangent(a, b) => [a, b].into_iter().find(|e| is_ellipse(e)),
                Relation::Equal(a, b) if is_ellipse(&a) || is_ellipse(&b) => Some(if is_ellipse(&a) { a } else { b }),
                _ => None,
            };
            if let Some(e) = bad {
                return Err(SketchError::WrongEntityKind(e));
            }
        }
        Ok(())
    }

    fn check_solution(&self, sol: &SketchSolution) -> Result<(), SketchError> {
        let ok = sol.points.iter().all(|(id, p)| {
            p.is_finite() && matches!(self.entities.get(id), Some(SketchEntity { entity: Entity::Point { .. }, .. }))
        }) && sol.radii.iter().all(|(id, r)| {
            r.is_finite() && matches!(self.entities.get(id), Some(SketchEntity { entity: Entity::Circle { .. }, .. }))
        });
        if ok { Ok(()) } else { Err(SketchError::InvalidSolution) }
    }

    pub fn add_entity(&mut self, entity: SketchEntity) -> Result<EntityId, SketchError> {
        self.check_entity(&entity.entity)?;
        let id = EntityId(self.next_entity);
        self.next_entity = self.next_entity.checked_add(1).ok_or(SketchError::IdsExhausted)?;
        self.entities.insert(id, entity);
        Ok(id)
    }

    pub fn add_constraint(&mut self, constraint: SketchConstraint) -> Result<ConstraintId, SketchError> {
        self.check_constraint(&constraint)?;
        let id = ConstraintId(self.next_constraint);
        self.next_constraint = self.next_constraint.checked_add(1).ok_or(SketchError::IdsExhausted)?;
        self.constraints.insert(id, constraint);
        Ok(id)
    }

    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<SketchConstraint, SketchError> {
        self.constraints.remove(&id).ok_or(SketchError::UnknownConstraint(id))
    }

    pub fn set_parameter(&mut self, name: &str, text: &str) -> Result<(), SketchError> {
        self.parameters.set(name, text)
    }

    pub fn remove_parameter(&mut self, name: &str) -> Result<(), SketchError> {
        self.parameters.remove(name)
    }

    fn dimension_mut(&mut self, id: ConstraintId) -> Result<&mut Dimension, SketchError> {
        match self.constraints.get_mut(&id) {
            Some(SketchConstraint::Dimension(d)) => Ok(d),
            Some(_) => Err(SketchError::NotADimension(id)),
            None => Err(SketchError::UnknownConstraint(id)),
        }
    }

    pub fn set_dimension_text(&mut self, id: ConstraintId, text: &str) -> Result<(), SketchError> {
        self.dimension_mut(id)?.text = text.to_owned();
        Ok(())
    }

    pub fn set_dimension_role(&mut self, id: ConstraintId, role: DimensionRole) -> Result<(), SketchError> {
        self.dimension_mut(id)?.role = role;
        Ok(())
    }

    pub fn set_construction(&mut self, id: EntityId, construction: bool) -> Result<(), SketchError> {
        self.entities.get_mut(&id).ok_or(SketchError::UnknownEntity(id))?.construction = construction;
        Ok(())
    }

    /// Stores a new last valid solution (from an evaluator patch).
    pub(crate) fn set_solution(&mut self, solution: SketchSolution) -> Result<(), SketchError> {
        self.check_solution(&solution)?;
        self.solution = Some(solution);
        Ok(())
    }

    // Convenience builders (definitions are built before `SketchCommand::Create`).

    pub fn point(&mut self, x: f64, y: f64) -> Result<EntityId, SketchError> {
        self.add_entity(SketchEntity { entity: Entity::Point { at: Point::new(x, y) }, construction: false })
    }

    pub fn line(&mut self, start: EntityId, end: EntityId) -> Result<EntityId, SketchError> {
        self.add_entity(SketchEntity { entity: Entity::Line { start, end }, construction: false })
    }

    pub fn construction_line(&mut self, start: EntityId, end: EntityId) -> Result<EntityId, SketchError> {
        self.add_entity(SketchEntity { entity: Entity::Line { start, end }, construction: true })
    }

    pub fn arc(
        &mut self,
        center: EntityId,
        start: EntityId,
        end: EntityId,
        clockwise: bool,
    ) -> Result<EntityId, SketchError> {
        self.add_entity(SketchEntity { entity: Entity::Arc { center, start, end, clockwise }, construction: false })
    }

    pub fn circle(&mut self, center: EntityId, radius: f64) -> Result<EntityId, SketchError> {
        self.add_entity(SketchEntity { entity: Entity::Circle { center, radius }, construction: false })
    }

    pub fn ellipse(&mut self, center: EntityId, major: EntityId, minor: f64) -> Result<EntityId, SketchError> {
        self.add_entity(SketchEntity { entity: Entity::Ellipse { center, major, minor }, construction: false })
    }

    pub fn relate(&mut self, relation: Relation) -> Result<ConstraintId, SketchError> {
        self.add_constraint(SketchConstraint::Relation(relation))
    }

    pub fn dimension(
        &mut self,
        kind: DimensionKind,
        text: &str,
        role: DimensionRole,
    ) -> Result<ConstraintId, SketchError> {
        self.add_constraint(SketchConstraint::Dimension(Dimension { kind, text: text.to_owned(), role }))
    }
}
