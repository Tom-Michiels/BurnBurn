//! Sketch DTO ↔ document sketch layer. Loading goes through `Sketch::from_parts` and
//! `SketchLayer::from_parts`, which apply the same reference checks as editing.

use burnburn_document::sketch::{
    Along, ConstraintId, Dimension, DimensionKind, DimensionRole, Entity, EntityId, ParameterTable, Relation, Sketch,
    SketchConstraint, SketchEntity, SketchId, SketchLayer, SketchSolution,
};
use burnburn_document::{DerivedOwner, DerivedRecord, DocumentSnapshot, EvalDiagnosis, EvalStatus};
use burnburn_types::{ContentHash, StableId};

use crate::dto::v1::Num;
use crate::error::LoadError;
use crate::map::{num, point, xy};
use crate::sketch_dto as d;

fn ids(v: &[ConstraintId]) -> Vec<u32> {
    v.iter().map(|c| c.raw()).collect()
}

fn along_to(a: Along) -> d::Along {
    match a {
        Along::Direct => d::Along::Direct,
        Along::X => d::Along::X,
        Along::Y => d::Along::Y,
    }
}

fn role_to(r: DimensionRole) -> d::Role {
    match r {
        DimensionRole::Driving => d::Role::Driving,
        DimensionRole::Driven => d::Role::Driven,
    }
}

fn status_to(s: &EvalStatus) -> d::Status {
    match s {
        EvalStatus::Valid => d::Status::Valid,
        EvalStatus::Failed { diagnosis } => match diagnosis {
            EvalDiagnosis::Conflict { constraints } => d::Status::Conflict { constraints: ids(constraints) },
            EvalDiagnosis::NotConverged { unmet } => d::Status::NotConverged { unmet: ids(unmet) },
            EvalDiagnosis::InvalidDimensions { constraints } => {
                d::Status::InvalidDimensions { constraints: ids(constraints) }
            }
            EvalDiagnosis::InvalidParameters { parameter } => {
                d::Status::InvalidParameters { parameter: parameter.clone() }
            }
            // Unknown future diagnoses are stored as "definition invalid": still blocked.
            _ => d::Status::InvalidDefinition,
        },
        // Pending and unknown future states are stored as pending: still blocked, re-evaluated.
        _ => d::Status::Pending,
    }
}

fn entity_to(id: EntityId, e: &SketchEntity) -> Option<d::Entity> {
    let shape = match e.entity {
        Entity::Point { at } => d::Shape::Point { at: xy(at) },
        Entity::Line { start, end } => d::Shape::Line { start: start.raw(), end: end.raw() },
        Entity::Arc { center, start, end, clockwise } => {
            d::Shape::Arc { center: center.raw(), start: start.raw(), end: end.raw(), clockwise }
        }
        Entity::Circle { center, radius } => d::Shape::Circle { center: center.raw(), radius: Num(radius) },
        Entity::Ellipse { center, major, minor } => {
            d::Shape::Ellipse { center: center.raw(), major: major.raw(), minor: Num(minor) }
        }
        _ => return None,
    };
    Some(d::Entity { id: id.raw(), construction: e.construction, shape })
}

fn constraint_to(id: ConstraintId, c: &SketchConstraint) -> Option<d::Constraint> {
    let kind = match c {
        SketchConstraint::Relation(r) => match *r {
            Relation::Coincident(a, b) => d::Kind::Coincident { a: a.raw(), b: b.raw() },
            Relation::PointOn { point, curve } => d::Kind::PointOn { point: point.raw(), curve: curve.raw() },
            Relation::Horizontal(l) => d::Kind::Horizontal { line: l.raw() },
            Relation::Vertical(l) => d::Kind::Vertical { line: l.raw() },
            Relation::Parallel(a, b) => d::Kind::Parallel { a: a.raw(), b: b.raw() },
            Relation::Perpendicular(a, b) => d::Kind::Perpendicular { a: a.raw(), b: b.raw() },
            Relation::Tangent(a, b) => d::Kind::Tangent { a: a.raw(), b: b.raw() },
            Relation::Equal(a, b) => d::Kind::Equal { a: a.raw(), b: b.raw() },
            Relation::Midpoint { point, line } => d::Kind::Midpoint { point: point.raw(), line: line.raw() },
            Relation::Symmetric { a, b, about } => d::Kind::Symmetric { a: a.raw(), b: b.raw(), about: about.raw() },
            Relation::Concentric(a, b) => d::Kind::Concentric { a: a.raw(), b: b.raw() },
            Relation::Fix { point, at } => d::Kind::Fix { point: point.raw(), at: xy(at) },
            _ => return None,
        },
        SketchConstraint::Dimension(Dimension { kind, text, role }) => {
            let (text, role) = (text.clone(), role_to(*role));
            match *kind {
                DimensionKind::Distance { a, b, along } => {
                    d::Kind::Distance { a: a.raw(), b: b.raw(), along: along_to(along), text, role }
                }
                DimensionKind::Length { line } => d::Kind::Length { line: line.raw(), text, role },
                DimensionKind::Radius { curve } => d::Kind::Radius { curve: curve.raw(), text, role },
                DimensionKind::Diameter { curve } => d::Kind::Diameter { curve: curve.raw(), text, role },
                DimensionKind::Angle { from, to } => d::Kind::Angle { from: from.raw(), to: to.raw(), text, role },
                _ => return None,
            }
        }
    };
    Some(d::Constraint { id: id.raw(), kind })
}

pub(crate) fn sketches_to_dto(doc: &DocumentSnapshot) -> Vec<d::Sketch> {
    let layer = doc.sketch_layer();
    layer
        .sketches()
        .map(|(id, s)| d::Sketch {
            id: id.raw(),
            name: s.name().to_owned(),
            next_entity: s.next_entity_id().raw(),
            next_constraint: s.next_constraint_id().raw(),
            entities: s.entities().filter_map(|(i, e)| entity_to(i, e)).collect(),
            constraints: s.constraints().filter_map(|(i, c)| constraint_to(i, c)).collect(),
            parameters: s.parameters().iter().map(|(n, t)| [n.to_owned(), t.to_owned()]).collect(),
            solution: s.solution().map(|sol| d::Solution {
                points: sol.points.iter().map(|(i, p)| (i.raw(), xy(*p))).collect(),
                radii: sol.radii.iter().map(|(i, r)| (i.raw(), Num(*r))).collect(),
                dof: sol.dof,
            }),
            evaluation: layer
                .record(DerivedOwner::Sketch(id))
                .map(|r| d::Evaluation { inputs: r.inputs.map(|h| h.to_hex()), status: status_to(&r.status) }),
        })
        .collect()
}

fn invalid(path: String, reason: impl Into<String>) -> LoadError {
    LoadError::InvalidValue { path, reason: reason.into() }
}

fn along_from(a: d::Along) -> Along {
    match a {
        d::Along::Direct => Along::Direct,
        d::Along::X => Along::X,
        d::Along::Y => Along::Y,
    }
}

fn role_from(r: d::Role) -> DimensionRole {
    match r {
        d::Role::Driving => DimensionRole::Driving,
        d::Role::Driven => DimensionRole::Driven,
    }
}

fn cids(v: &[u32]) -> Vec<ConstraintId> {
    v.iter().map(|c| ConstraintId::from_raw(*c)).collect()
}

fn status_from(s: &d::Status) -> EvalStatus {
    let failed = |diagnosis| EvalStatus::Failed { diagnosis };
    match s {
        d::Status::Valid => EvalStatus::Valid,
        d::Status::Pending => EvalStatus::Pending,
        d::Status::Conflict { constraints } => failed(EvalDiagnosis::Conflict { constraints: cids(constraints) }),
        d::Status::NotConverged { unmet } => failed(EvalDiagnosis::NotConverged { unmet: cids(unmet) }),
        d::Status::InvalidDimensions { constraints } => {
            failed(EvalDiagnosis::InvalidDimensions { constraints: cids(constraints) })
        }
        d::Status::InvalidParameters { parameter } => {
            failed(EvalDiagnosis::InvalidParameters { parameter: parameter.clone() })
        }
        d::Status::InvalidDefinition => failed(EvalDiagnosis::InvalidDefinition),
    }
}

fn sketch_from(si: usize, s: &d::Sketch) -> Result<Sketch, LoadError> {
    let base = move || format!("sketches[{si}]");
    let e = |i: u32| EntityId::from_raw(i);
    let mut entities = Vec::with_capacity(s.entities.len());
    for (ei, en) in s.entities.iter().enumerate() {
        let at = move || format!("{}.entities[{ei}]", base());
        let entity = match en.shape {
            d::Shape::Point { at: p } => Entity::Point { at: point(&|| format!("{}.at", at()), p)? },
            d::Shape::Line { start, end } => Entity::Line { start: e(start), end: e(end) },
            d::Shape::Arc { center, start, end, clockwise } => {
                Entity::Arc { center: e(center), start: e(start), end: e(end), clockwise }
            }
            d::Shape::Circle { center, radius } => {
                Entity::Circle { center: e(center), radius: num(&|| format!("{}.radius", at()), radius)? }
            }
            d::Shape::Ellipse { center, major, minor } => Entity::Ellipse {
                center: e(center),
                major: e(major),
                minor: num(&|| format!("{}.minor", at()), minor)?,
            },
        };
        entities.push((e(en.id), SketchEntity { entity, construction: en.construction }));
    }
    let mut constraints = Vec::with_capacity(s.constraints.len());
    for (ci, c) in s.constraints.iter().enumerate() {
        let at = move || format!("{}.constraints[{ci}]", base());
        let dim = |kind, text: &String, role| {
            SketchConstraint::Dimension(Dimension { kind, text: text.clone(), role: role_from(role) })
        };
        let rel = SketchConstraint::Relation;
        let k = match &c.kind {
            d::Kind::Coincident { a, b } => rel(Relation::Coincident(e(*a), e(*b))),
            d::Kind::PointOn { point: p, curve } => rel(Relation::PointOn { point: e(*p), curve: e(*curve) }),
            d::Kind::Horizontal { line } => rel(Relation::Horizontal(e(*line))),
            d::Kind::Vertical { line } => rel(Relation::Vertical(e(*line))),
            d::Kind::Parallel { a, b } => rel(Relation::Parallel(e(*a), e(*b))),
            d::Kind::Perpendicular { a, b } => rel(Relation::Perpendicular(e(*a), e(*b))),
            d::Kind::Tangent { a, b } => rel(Relation::Tangent(e(*a), e(*b))),
            d::Kind::Equal { a, b } => rel(Relation::Equal(e(*a), e(*b))),
            d::Kind::Midpoint { point: p, line } => rel(Relation::Midpoint { point: e(*p), line: e(*line) }),
            d::Kind::Symmetric { a, b, about } => rel(Relation::Symmetric { a: e(*a), b: e(*b), about: e(*about) }),
            d::Kind::Concentric { a, b } => rel(Relation::Concentric(e(*a), e(*b))),
            d::Kind::Fix { point: p, at: q } => {
                rel(Relation::Fix { point: e(*p), at: point(&|| format!("{}.at", at()), *q)? })
            }
            d::Kind::Distance { a, b, along, text, role } => {
                dim(DimensionKind::Distance { a: e(*a), b: e(*b), along: along_from(*along) }, text, *role)
            }
            d::Kind::Length { line, text, role } => dim(DimensionKind::Length { line: e(*line) }, text, *role),
            d::Kind::Radius { curve, text, role } => dim(DimensionKind::Radius { curve: e(*curve) }, text, *role),
            d::Kind::Diameter { curve, text, role } => dim(DimensionKind::Diameter { curve: e(*curve) }, text, *role),
            d::Kind::Angle { from, to, text, role } => {
                dim(DimensionKind::Angle { from: e(*from), to: e(*to) }, text, *role)
            }
        };
        constraints.push((ConstraintId::from_raw(c.id), k));
    }
    let mut parameters = ParameterTable::new();
    for (pi, [name, text]) in s.parameters.iter().enumerate() {
        parameters.set(name, text).map_err(|err| invalid(format!("{}.parameters[{pi}]", base()), err.to_string()))?;
    }
    let solution = match &s.solution {
        None => None,
        Some(sol) => {
            let mut out = SketchSolution { dof: sol.dof, ..SketchSolution::default() };
            for (k, (id, p)) in sol.points.iter().enumerate() {
                out.points.insert(e(*id), point(&|| format!("{}.solution.points[{k}]", base()), *p)?);
            }
            for (k, (id, r)) in sol.radii.iter().enumerate() {
                out.radii.insert(e(*id), num(&|| format!("{}.solution.radii[{k}]", base()), *r)?);
            }
            Some(out)
        }
    };
    Sketch::from_parts(s.name.clone(), entities, constraints, parameters, s.next_entity, s.next_constraint, solution)
        .map_err(|err| invalid(base(), err.to_string()))
}

pub(crate) fn sketches_from_dto(dto: &[d::Sketch]) -> Result<SketchLayer, LoadError> {
    let mut sketches = Vec::with_capacity(dto.len());
    let mut records = Vec::new();
    for (si, s) in dto.iter().enumerate() {
        let id = SketchId::from_raw(s.id);
        sketches.push((id, sketch_from(si, s)?));
        if let Some(ev) = &s.evaluation {
            let inputs = match &ev.inputs {
                None => None,
                Some(hex) => Some(
                    ContentHash::from_hex(hex)
                        .map_err(|err| invalid(format!("sketches[{si}].evaluation.inputs"), err.to_string()))?,
                ),
            };
            records.push((DerivedOwner::Sketch(id), DerivedRecord { inputs, status: status_from(&ev.status) }));
        }
    }
    SketchLayer::from_parts(sketches, records).map_err(|err| invalid("sketches".into(), err.to_string()))
}
