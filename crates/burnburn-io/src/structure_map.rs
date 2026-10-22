//! Structure layer (IM21) ↔ DTO. Loading validates through the domain constructors
//! (`StructureLayer::from_parts`, `DocumentSnapshot::with_structure`).

use std::collections::BTreeMap;

use burnburn_document::structure::{
    AssocReference, Dim, EdgeName, EdgeOrigin, EvalMode, GeometryRef, Modifier, ModifierChain, ModifierId,
    ModifierStep, NamedShape, ObjectRelation, PartDefinition, PartDefinitionId, PartInstance, PartInstanceId,
    PartShape, Plate, PlateId, ProvenanceEdge, ReferenceId, RelationId, StructureLayer, StructureParts, SubRef,
    VertexEnd,
};
use burnburn_document::{
    DerivedKey, DerivedOwner, DerivedRecord, DocumentSnapshot, EdgeKey, EvalDiagnosis, EvalStatus, ModifierFailure,
};
use burnburn_geometry::array::{CircularArray, GridArray};
use burnburn_geometry::{Affine, BooleanOp, OffsetJoin, ProductionTolerance, Vector};
use burnburn_types::{ContentHash, GeometryId, Length, StableId};

use crate::dto::v1::Num;
use crate::error::{LoadError, SaveError};
use crate::map::{num, path_from_dto, path_to_dto, point, xy};
use crate::structure_dto as d;

fn origin_to(o: &EdgeOrigin) -> d::EdgeOrigin {
    d::EdgeOrigin { source: o.source.raw(), edge: o.edge.0, copy: o.copy }
}

fn name_to(n: &EdgeName) -> d::EdgeName {
    d::EdgeName { origin: origin_to(&n.origin), index: n.index, of: n.of }
}

fn ref_to(r: &GeometryRef) -> d::GeometryRef {
    d::GeometryRef {
        object: r.object.raw(),
        sub: r.sub.map(|s| match s {
            SubRef::Edge(e) => d::Sub::Edge { edge: name_to(&e) },
            SubRef::Vertex { edge, end: VertexEnd::Start } => d::Sub::VertexStart { edge: name_to(&edge) },
            SubRef::Vertex { edge, end: VertexEnd::End } => d::Sub::VertexEnd { edge: name_to(&edge) },
        }),
    }
}

fn dim_to(x: &Dim) -> d::Dim {
    match x {
        Dim::Mm(v) => d::Dim::Mm { value: Num(*v) },
        Dim::Param(p) => d::Dim::Param { name: p.clone() },
        Dim::Expr(text) => d::Dim::Expr { text: text.clone() },
    }
}

fn values_to(m: &BTreeMap<String, Length>) -> BTreeMap<String, Num> {
    m.iter().map(|(k, v)| (k.clone(), Num(v.mm()))).collect()
}

fn owner_to(o: DerivedOwner) -> Option<d::Owner> {
    match o {
        DerivedOwner::Modifier(m) => Some(d::Owner::Modifier { id: m.raw() }),
        DerivedOwner::Part(i) => Some(d::Owner::Part { id: i.raw() }),
        _ => None,
    }
}

fn status_to(s: &EvalStatus) -> d::Status {
    match s {
        EvalStatus::Valid => d::Status::Valid,
        EvalStatus::Failed { diagnosis } => match diagnosis {
            EvalDiagnosis::MissingSources { objects } => {
                d::Status::MissingSources { objects: objects.iter().map(|g| g.raw()).collect() }
            }
            EvalDiagnosis::ModifierStep { step, failure } => d::Status::ModifierStep {
                step: *step,
                failure: match failure {
                    ModifierFailure::OpenContour => d::Failure::OpenContour,
                    ModifierFailure::Empty => d::Failure::Empty,
                    ModifierFailure::OutOfRange => d::Failure::OutOfRange,
                    _ => d::Failure::InvalidValue,
                },
            },
            EvalDiagnosis::InvalidParameters { parameter } => {
                d::Status::InvalidParameters { parameter: parameter.clone() }
            }
            _ => d::Status::InvalidDefinition,
        },
        _ => d::Status::Pending,
    }
}

/// `None` when the document has no structure (the field is then omitted). Fails for a
/// modifier step without a file form (never dropped).
pub(crate) fn structure_to_dto(doc: &DocumentSnapshot) -> Result<Option<d::Structure>, SaveError> {
    let l = doc.structure();
    if l.is_empty() {
        return Ok(None);
    }
    let modifiers = l
        .modifiers()
        .map(|(id, c)| {
            Ok(d::Modifier {
                id: id.raw(),
                name: c.name.clone(),
                source: c.source.raw(),
                tolerance_mm: Num(c.tolerance.mm()),
                background: c.mode == EvalMode::Background,
                steps: c
                    .steps
                    .iter()
                    .map(|s| {
                        let modifier = match &s.modifier {
                            Modifier::Offset { distance, join } => d::ModifierKind::Offset {
                                distance_mm: Num(distance.mm()),
                                join: match join {
                                    OffsetJoin::Miter => d::Join::Miter,
                                    OffsetJoin::Round => d::Join::Round,
                                    OffsetJoin::Bevel => d::Join::Bevel,
                                },
                            },
                            Modifier::Boolean { op, with } => d::ModifierKind::Boolean {
                                op: match op {
                                    BooleanOp::Union => d::BoolOp::Union,
                                    BooleanOp::Difference => d::BoolOp::Difference,
                                    BooleanOp::Intersection => d::BoolOp::Intersection,
                                    BooleanOp::Xor => d::BoolOp::Xor,
                                },
                                with: with.iter().map(|g| g.raw()).collect(),
                            },
                            Modifier::LinearArray { count, step } => {
                                d::ModifierKind::LinearArray { count: *count, step_mm: [Num(step.x), Num(step.y)] }
                            }
                            Modifier::GridArray(g) => d::ModifierKind::GridArray {
                                columns: g.columns,
                                rows: g.rows,
                                pitch_mm: [Num(g.pitch_x.mm()), Num(g.pitch_y.mm())],
                            },
                            Modifier::CircularArray(c) => d::ModifierKind::CircularArray {
                                count: c.count,
                                center: xy(c.center),
                                sweep_deg: Num(c.sweep_deg),
                                rotate_items: c.rotate_items,
                                reference: xy(c.reference),
                            },
                            Modifier::PathArray { count, along, orient } => {
                                d::ModifierKind::PathArray { count: *count, along: along.raw(), orient: *orient }
                            }
                            // A step without a file form is an error, never silently dropped (a
                            // reopened file would compute something else).
                            other => return Err(SaveError::Encode(format!("no file form for modifier {other:?}"))),
                        };
                        Ok(d::Step { enabled: s.enabled, modifier })
                    })
                    .collect::<Result<_, SaveError>>()?,
            })
        })
        .collect::<Result<Vec<_>, SaveError>>()?;
    let references = l
        .references()
        .map(|(id, r)| d::Reference { id: id.raw(), name: r.name.clone(), target: ref_to(&r.target) })
        .collect();
    let relations = l
        .relations()
        .filter_map(|(id, r)| {
            let relation = match *r {
                ObjectRelation::Tangent { a, b } => d::RelationKind::Tangent { a: a.raw(), b: b.raw() },
                ObjectRelation::Symmetric { a, b, axis } => {
                    d::RelationKind::Symmetric { a: a.raw(), b: b.raw(), axis: axis.raw() }
                }
                _ => return None,
            };
            Some(d::Relation { id: id.raw(), relation })
        })
        .collect();
    let definitions = l
        .definitions()
        .map(|(id, def)| d::Definition {
            id: id.raw(),
            name: def.name.clone(),
            parameters_mm: values_to(&def.parameters),
            shapes: def
                .shapes
                .iter()
                .filter_map(|(k, s)| {
                    let shape = match &s.shape {
                        PartShape::Rectangle { x, y, width, height } => d::PartShape::Rectangle {
                            x: dim_to(x),
                            y: dim_to(y),
                            width: dim_to(width),
                            height: dim_to(height),
                        },
                        PartShape::Circle { x, y, radius } => {
                            d::PartShape::Circle { x: dim_to(x), y: dim_to(y), radius: dim_to(radius) }
                        }
                        PartShape::Fixed(p) => d::PartShape::Fixed { subpaths: path_to_dto(p) },
                        PartShape::Fitted { path, x, y, width, height } => d::PartShape::Fitted {
                            subpaths: path_to_dto(path),
                            x: dim_to(x),
                            y: dim_to(y),
                            width: dim_to(width),
                            height: dim_to(height),
                        },
                        _ => return None,
                    };
                    Some(d::Shape { key: *k, name: s.name.clone(), shape })
                })
                .collect(),
            variants_mm: def.variants.iter().map(|(k, v)| (k.clone(), values_to(v))).collect(),
        })
        .collect();
    let instances = l
        .instances()
        .map(|(id, i)| d::Instance {
            id: id.raw(),
            definition: i.definition.raw(),
            variant: i.variant.clone(),
            overrides_mm: values_to(&i.overrides),
            transform: i.transform.coefficients().map(Num),
            plate: i.plate.map(|p| p.raw()),
        })
        .collect();
    let plates = l
        .plates()
        .map(|(id, p)| d::Plate {
            id: id.raw(),
            name: p.name.clone(),
            width_mm: Num(p.width.mm()),
            height_mm: Num(p.height.mm()),
            material: p.material.clone(),
            thickness_mm: p.thickness.map(|t| Num(t.mm())),
        })
        .collect();
    let placement = l.placements().map(|(g, p)| d::Placement { object: g.raw(), plate: p.raw() }).collect();
    let records = l
        .records()
        .filter_map(|(o, r)| {
            Some(d::Record { owner: owner_to(o)?, inputs: r.inputs.map(|h| h.to_hex()), status: status_to(&r.status) })
        })
        .collect();
    let provenance = l
        .provenances()
        .filter_map(|(k, edges)| {
            Some(d::Provenance {
                owner: owner_to(k.owner)?,
                local: k.local,
                edges: edges
                    .iter()
                    .map(|e| d::ProvenanceEdge {
                        subpath: e.subpath,
                        first: e.first,
                        count: e.count,
                        origin: e.origin.as_ref().map(origin_to),
                        also: e.also.as_ref().map(origin_to),
                    })
                    .collect(),
            })
        })
        .collect();
    Ok(Some(d::Structure {
        modifiers,
        references,
        relations,
        definitions,
        instances,
        plates,
        placement,
        records,
        provenance,
    }))
}

fn invalid(path: impl Into<String>, reason: impl Into<String>) -> LoadError {
    LoadError::InvalidValue { path: path.into(), reason: reason.into() }
}

fn length(path: &dyn Fn() -> String, n: Num) -> Result<Length, LoadError> {
    let v = num(path, n)?;
    Length::from_mm(v).map_err(|e| invalid(path(), e.to_string()))
}

fn values_from(path: &str, m: &BTreeMap<String, Num>) -> Result<BTreeMap<String, Length>, LoadError> {
    m.iter().map(|(k, v)| Ok((k.clone(), length(&|| format!("{path}.{k}"), *v)?))).collect()
}

fn origin_from(o: &d::EdgeOrigin) -> EdgeOrigin {
    EdgeOrigin { source: GeometryId::from_raw(o.source), edge: EdgeKey(o.edge), copy: o.copy }
}

fn name_from(n: &d::EdgeName) -> EdgeName {
    EdgeName { origin: origin_from(&n.origin), index: n.index, of: n.of }
}

fn ref_from(r: &d::GeometryRef) -> GeometryRef {
    GeometryRef {
        object: GeometryId::from_raw(r.object),
        sub: r.sub.map(|s| match s {
            d::Sub::Edge { edge } => SubRef::Edge(name_from(&edge)),
            d::Sub::VertexStart { edge } => SubRef::Vertex { edge: name_from(&edge), end: VertexEnd::Start },
            d::Sub::VertexEnd { edge } => SubRef::Vertex { edge: name_from(&edge), end: VertexEnd::End },
        }),
    }
}

fn dim_from(path: &dyn Fn() -> String, x: &d::Dim) -> Result<Dim, LoadError> {
    Ok(match x {
        d::Dim::Mm { value } => Dim::Mm(num(path, *value)?),
        d::Dim::Param { name } => Dim::Param(name.clone()),
        d::Dim::Expr { text } => Dim::Expr(text.clone()),
    })
}

fn owner_from(o: d::Owner) -> DerivedOwner {
    match o {
        d::Owner::Modifier { id } => DerivedOwner::Modifier(ModifierId::from_raw(id)),
        d::Owner::Part { id } => DerivedOwner::Part(PartInstanceId::from_raw(id)),
    }
}

fn status_from(s: &d::Status) -> EvalStatus {
    let failed = |diagnosis| EvalStatus::Failed { diagnosis };
    match s {
        d::Status::Valid => EvalStatus::Valid,
        d::Status::Pending => EvalStatus::Pending,
        d::Status::MissingSources { objects } => failed(EvalDiagnosis::MissingSources {
            objects: objects.iter().map(|g| GeometryId::from_raw(*g)).collect(),
        }),
        d::Status::ModifierStep { step, failure } => failed(EvalDiagnosis::ModifierStep {
            step: *step,
            failure: match failure {
                d::Failure::OpenContour => ModifierFailure::OpenContour,
                d::Failure::Empty => ModifierFailure::Empty,
                d::Failure::OutOfRange => ModifierFailure::OutOfRange,
                d::Failure::InvalidValue => ModifierFailure::InvalidValue,
            },
        }),
        d::Status::InvalidParameters { parameter } => {
            failed(EvalDiagnosis::InvalidParameters { parameter: parameter.clone() })
        }
        d::Status::InvalidDefinition => failed(EvalDiagnosis::InvalidDefinition),
    }
}

/// Adds the stored structure layer to `snapshot` (after objects, sketches, texts and process).
pub(crate) fn structure_from_dto(
    dto: Option<&d::Structure>,
    snapshot: DocumentSnapshot,
) -> Result<DocumentSnapshot, LoadError> {
    let Some(s) = dto else { return Ok(snapshot) };
    let mut parts = StructureParts::default();
    for (i, m) in s.modifiers.iter().enumerate() {
        let p = move || format!("structure.modifiers[{i}]");
        let tol = num(&|| format!("{}.tolerance_mm", p()), m.tolerance_mm)?;
        let tolerance = Length::from_mm(tol)
            .ok()
            .and_then(|l| ProductionTolerance::new(l).ok())
            .ok_or_else(|| invalid(format!("{}.tolerance_mm", p()), "not a positive tolerance"))?;
        let mut steps = Vec::with_capacity(m.steps.len());
        for (si, st) in m.steps.iter().enumerate() {
            let sp = move || format!("{}.steps[{si}]", p());
            let modifier = match &st.modifier {
                d::ModifierKind::Offset { distance_mm, join } => Modifier::Offset {
                    distance: length(&|| format!("{}.distance_mm", sp()), *distance_mm)?,
                    join: match join {
                        d::Join::Miter => OffsetJoin::Miter,
                        d::Join::Round => OffsetJoin::Round,
                        d::Join::Bevel => OffsetJoin::Bevel,
                    },
                },
                d::ModifierKind::Boolean { op, with } => Modifier::Boolean {
                    op: match op {
                        d::BoolOp::Union => BooleanOp::Union,
                        d::BoolOp::Difference => BooleanOp::Difference,
                        d::BoolOp::Intersection => BooleanOp::Intersection,
                        d::BoolOp::Xor => BooleanOp::Xor,
                    },
                    with: with.iter().map(|g| GeometryId::from_raw(*g)).collect(),
                },
                d::ModifierKind::LinearArray { count, step_mm } => {
                    if *count == 0 {
                        return Err(invalid(sp(), "array count 0"));
                    }
                    let x = num(&|| format!("{}.step_mm[0]", sp()), step_mm[0])?;
                    let y = num(&|| format!("{}.step_mm[1]", sp()), step_mm[1])?;
                    Modifier::LinearArray { count: *count, step: Vector::new(x, y) }
                }
                d::ModifierKind::GridArray { columns, rows, pitch_mm } => Modifier::GridArray(GridArray {
                    columns: *columns,
                    rows: *rows,
                    pitch_x: length(&|| format!("{}.pitch_mm[0]", sp()), pitch_mm[0])?,
                    pitch_y: length(&|| format!("{}.pitch_mm[1]", sp()), pitch_mm[1])?,
                }),
                d::ModifierKind::PathArray { count, along, orient } => {
                    if *count == 0 {
                        return Err(invalid(sp(), "array count 0"));
                    }
                    Modifier::PathArray { count: *count, along: GeometryId::from_raw(*along), orient: *orient }
                }
                d::ModifierKind::CircularArray { count, center, sweep_deg, rotate_items, reference } => {
                    Modifier::CircularArray(CircularArray {
                        count: *count,
                        center: point(&|| format!("{}.center", sp()), *center)?,
                        sweep_deg: num(&|| format!("{}.sweep_deg", sp()), *sweep_deg)?,
                        rotate_items: *rotate_items,
                        reference: point(&|| format!("{}.reference", sp()), *reference)?,
                    })
                }
            };
            steps.push(ModifierStep { modifier, enabled: st.enabled });
        }
        let chain = ModifierChain {
            name: m.name.clone(),
            source: GeometryId::from_raw(m.source),
            steps,
            tolerance,
            mode: if m.background { EvalMode::Background } else { EvalMode::Inline },
        };
        parts.modifiers.push((ModifierId::from_raw(m.id), chain));
    }
    for r in &s.references {
        parts
            .references
            .push((ReferenceId::from_raw(r.id), AssocReference { name: r.name.clone(), target: ref_from(&r.target) }));
    }
    for r in &s.relations {
        let rel = match r.relation {
            d::RelationKind::Tangent { a, b } => {
                ObjectRelation::Tangent { a: ReferenceId::from_raw(a), b: ReferenceId::from_raw(b) }
            }
            d::RelationKind::Symmetric { a, b, axis } => ObjectRelation::Symmetric {
                a: ReferenceId::from_raw(a),
                b: ReferenceId::from_raw(b),
                axis: ReferenceId::from_raw(axis),
            },
        };
        parts.relations.push((RelationId::from_raw(r.id), rel));
    }
    for (di, def) in s.definitions.iter().enumerate() {
        let p = format!("structure.definitions[{di}]");
        let mut shapes = BTreeMap::new();
        for (si, sh) in def.shapes.iter().enumerate() {
            let sp = || format!("{p}.shapes[{si}]");
            let shape = match &sh.shape {
                d::PartShape::Rectangle { x, y, width, height } => PartShape::Rectangle {
                    x: dim_from(&sp, x)?,
                    y: dim_from(&sp, y)?,
                    width: dim_from(&sp, width)?,
                    height: dim_from(&sp, height)?,
                },
                d::PartShape::Circle { x, y, radius } => {
                    PartShape::Circle { x: dim_from(&sp, x)?, y: dim_from(&sp, y)?, radius: dim_from(&sp, radius)? }
                }
                d::PartShape::Fixed { subpaths } => PartShape::Fixed(path_from_dto(&sp, subpaths)?),
                d::PartShape::Fitted { subpaths, x, y, width, height } => PartShape::Fitted {
                    path: path_from_dto(&sp, subpaths)?,
                    x: dim_from(&sp, x)?,
                    y: dim_from(&sp, y)?,
                    width: dim_from(&sp, width)?,
                    height: dim_from(&sp, height)?,
                },
            };
            if shapes.insert(sh.key, NamedShape { name: sh.name.clone(), shape }).is_some() {
                return Err(invalid(sp(), format!("duplicate shape key {}", sh.key)));
            }
        }
        let mut variants = BTreeMap::new();
        for (k, v) in &def.variants_mm {
            variants.insert(k.clone(), values_from(&format!("{p}.variants_mm.{k}"), v)?);
        }
        let definition = PartDefinition {
            name: def.name.clone(),
            parameters: values_from(&format!("{p}.parameters_mm"), &def.parameters_mm)?,
            shapes,
            variants,
        };
        parts.definitions.push((PartDefinitionId::from_raw(def.id), definition));
    }
    for (ii, inst) in s.instances.iter().enumerate() {
        let p = move || format!("structure.instances[{ii}]");
        let mut c = [0.0; 6];
        for (k, v) in inst.transform.iter().enumerate() {
            c[k] = num(&|| format!("{}.transform[{k}]", p()), *v)?;
        }
        let transform = Affine::new(c).map_err(|e| invalid(format!("{}.transform", p()), e.to_string()))?;
        let instance = PartInstance {
            definition: PartDefinitionId::from_raw(inst.definition),
            variant: inst.variant.clone(),
            overrides: values_from(&format!("{}.overrides_mm", p()), &inst.overrides_mm)?,
            transform,
            plate: inst.plate.map(PlateId::from_raw),
        };
        parts.instances.push((PartInstanceId::from_raw(inst.id), instance));
    }
    for (pi, pl) in s.plates.iter().enumerate() {
        let p = move || format!("structure.plates[{pi}]");
        let thickness = match pl.thickness_mm {
            Some(t) => Some(length(&|| format!("{}.thickness_mm", p()), t)?),
            None => None,
        };
        let plate = Plate {
            name: pl.name.clone(),
            width: length(&|| format!("{}.width_mm", p()), pl.width_mm)?,
            height: length(&|| format!("{}.height_mm", p()), pl.height_mm)?,
            material: pl.material.clone(),
            thickness,
        };
        parts.plates.push((PlateId::from_raw(pl.id), plate));
    }
    parts.placement =
        s.placement.iter().map(|x| (GeometryId::from_raw(x.object), PlateId::from_raw(x.plate))).collect();
    for (ri, r) in s.records.iter().enumerate() {
        let inputs = match &r.inputs {
            Some(h) => Some(
                ContentHash::from_hex(h)
                    .map_err(|e| invalid(format!("structure.records[{ri}].inputs"), e.to_string()))?,
            ),
            None => None,
        };
        parts.records.push((owner_from(r.owner), DerivedRecord { inputs, status: status_from(&r.status) }));
    }
    for p in &s.provenance {
        let edges = p
            .edges
            .iter()
            .map(|e| ProvenanceEdge {
                subpath: e.subpath,
                first: e.first,
                count: e.count,
                origin: e.origin.as_ref().map(origin_from),
                also: e.also.as_ref().map(origin_from),
            })
            .collect();
        parts.provenance.push((DerivedKey { owner: owner_from(p.owner), local: p.local }, edges));
    }
    let layer = StructureLayer::from_parts(parts).map_err(|e| invalid("structure", e.to_string()))?;
    snapshot.with_structure(layer).map_err(|e| invalid("structure", e.to_string()))
}
