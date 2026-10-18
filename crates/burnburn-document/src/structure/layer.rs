use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_geometry::{Point, Segment, SegmentRef};
use burnburn_types::GeometryId;

use super::reference::{edge_vertex, edges_with, identity_origins, resolve_edge, runs};
use super::{
    AssocReference, BreakReason, EdgeName, GeometryRef, ModifierChain, ModifierId, ObjectRelation, PartDefinition,
    PartDefinitionId, PartInstance, PartInstanceId, Plate, PlateId, ProvenanceEdge, RefStatus, ReferenceId, RelationId,
    RelationState, Resolved, StructureError, SubRef,
};
use crate::{DerivedKey, DerivedOwner, DerivedRecord, GeometryObject, GeometrySource};

/// Modifiers, references, relations, parts and plates of one document, plus the evaluation
/// records and edge provenance of their derived outputs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructureLayer {
    pub(crate) modifiers: BTreeMap<ModifierId, Arc<ModifierChain>>,
    pub(crate) references: BTreeMap<ReferenceId, AssocReference>,
    pub(crate) relations: BTreeMap<RelationId, ObjectRelation>,
    pub(crate) definitions: BTreeMap<PartDefinitionId, Arc<PartDefinition>>,
    pub(crate) instances: BTreeMap<PartInstanceId, PartInstance>,
    pub(crate) plates: BTreeMap<PlateId, Plate>,
    /// Plate of each object that is not a part-instance output (those follow their instance).
    pub(crate) placement: BTreeMap<GeometryId, PlateId>,
    pub(crate) derived: BTreeMap<DerivedOwner, DerivedRecord>,
    pub(crate) provenance: BTreeMap<DerivedKey, Arc<[ProvenanceEdge]>>,
}

/// Stored parts of a structure layer (open/recovery), validated by [`StructureLayer::from_parts`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructureParts {
    pub modifiers: Vec<(ModifierId, ModifierChain)>,
    pub references: Vec<(ReferenceId, AssocReference)>,
    pub relations: Vec<(RelationId, ObjectRelation)>,
    pub definitions: Vec<(PartDefinitionId, PartDefinition)>,
    pub instances: Vec<(PartInstanceId, PartInstance)>,
    pub plates: Vec<(PlateId, Plate)>,
    pub placement: Vec<(GeometryId, PlateId)>,
    pub records: Vec<(DerivedOwner, DerivedRecord)>,
    pub provenance: Vec<(DerivedKey, Vec<ProvenanceEdge>)>,
}

impl StructureLayer {
    pub fn is_empty(&self) -> bool {
        self.modifiers.is_empty()
            && self.references.is_empty()
            && self.relations.is_empty()
            && self.definitions.is_empty()
            && self.instances.is_empty()
            && self.plates.is_empty()
            && self.placement.is_empty()
            && self.derived.is_empty()
            && self.provenance.is_empty()
    }

    /// A layer from stored parts. Checks internal consistency (unique ids, known definitions,
    /// plates, references and owners); ids against the objects are checked by the snapshot.
    pub fn from_parts(parts: StructureParts) -> Result<Self, StructureError> {
        use burnburn_types::StableId;
        let mut seen = std::collections::BTreeSet::new();
        let mut fresh = |raw: u64| if seen.insert(raw) { Ok(()) } else { Err(StructureError::InvalidId(raw)) };
        let mut l = StructureLayer::default();
        for (id, c) in parts.modifiers {
            fresh(id.raw())?;
            l.modifiers.insert(id, Arc::new(c));
        }
        for (id, r) in parts.references {
            fresh(id.raw())?;
            l.references.insert(id, r);
        }
        for (id, d) in parts.definitions {
            fresh(id.raw())?;
            super::command::check_definition(&d)?;
            l.definitions.insert(id, Arc::new(d));
        }
        for (id, p) in parts.plates {
            fresh(id.raw())?;
            l.plates.insert(id, p);
        }
        for (id, i) in parts.instances {
            fresh(id.raw())?;
            l.check_instance(&i)?;
            l.instances.insert(id, i);
        }
        for (id, r) in parts.relations {
            fresh(id.raw())?;
            if let Some(missing) = r.references().into_iter().find(|x| !l.references.contains_key(x)) {
                return Err(StructureError::UnknownReference(missing));
            }
            l.relations.insert(id, r);
        }
        for (g, p) in parts.placement {
            if !l.plates.contains_key(&p) {
                return Err(StructureError::UnknownPlate(p));
            }
            l.placement.insert(g, p);
        }
        for (owner, r) in parts.records {
            l.check_owner(owner)?;
            l.derived.insert(owner, r);
        }
        for (key, edges) in parts.provenance {
            l.check_owner(key.owner)?;
            l.provenance.insert(key, Arc::from(edges));
        }
        Ok(l)
    }

    fn check_owner(&self, owner: DerivedOwner) -> Result<(), StructureError> {
        let known = match owner {
            DerivedOwner::Modifier(m) => self.modifiers.contains_key(&m),
            DerivedOwner::Part(i) => self.instances.contains_key(&i),
            _ => false,
        };
        if known { Ok(()) } else { Err(StructureError::UnknownOwner(owner)) }
    }

    pub(crate) fn check_instance(&self, i: &PartInstance) -> Result<(), StructureError> {
        let def = self.definitions.get(&i.definition).ok_or(StructureError::UnknownDefinition(i.definition))?;
        if let Some(v) = &i.variant
            && !def.variants.contains_key(v)
        {
            return Err(StructureError::UnknownVariant(v.clone()));
        }
        if let Some(p) = i.overrides.keys().find(|p| !def.parameters.contains_key(*p)) {
            return Err(StructureError::UnknownParameter(p.clone()));
        }
        if let Some(p) = i.plate
            && !self.plates.contains_key(&p)
        {
            return Err(StructureError::UnknownPlate(p));
        }
        Ok(())
    }

    /// Every raw id this layer allocated (for id-space checks).
    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        use burnburn_types::StableId;
        self.modifiers
            .keys()
            .map(|k| k.raw())
            .chain(self.references.keys().map(|k| k.raw()))
            .chain(self.relations.keys().map(|k| k.raw()))
            .chain(self.definitions.keys().map(|k| k.raw()))
            .chain(self.instances.keys().map(|k| k.raw()))
            .chain(self.plates.keys().map(|k| k.raw()))
    }

    pub fn modifier(&self, id: ModifierId) -> Option<&ModifierChain> {
        self.modifiers.get(&id).map(|c| c.as_ref())
    }
    pub fn modifiers(&self) -> impl ExactSizeIterator<Item = (ModifierId, &ModifierChain)> {
        self.modifiers.iter().map(|(k, v)| (*k, v.as_ref()))
    }
    pub fn reference(&self, id: ReferenceId) -> Option<&AssocReference> {
        self.references.get(&id)
    }
    pub fn references(&self) -> impl ExactSizeIterator<Item = (ReferenceId, &AssocReference)> {
        self.references.iter().map(|(k, v)| (*k, v))
    }
    pub fn relation(&self, id: RelationId) -> Option<&ObjectRelation> {
        self.relations.get(&id)
    }
    pub fn relations(&self) -> impl ExactSizeIterator<Item = (RelationId, &ObjectRelation)> {
        self.relations.iter().map(|(k, v)| (*k, v))
    }
    pub fn definition(&self, id: PartDefinitionId) -> Option<&PartDefinition> {
        self.definitions.get(&id).map(|d| d.as_ref())
    }
    pub fn definitions(&self) -> impl ExactSizeIterator<Item = (PartDefinitionId, &PartDefinition)> {
        self.definitions.iter().map(|(k, v)| (*k, v.as_ref()))
    }
    pub fn instance(&self, id: PartInstanceId) -> Option<&PartInstance> {
        self.instances.get(&id)
    }
    pub fn instances(&self) -> impl ExactSizeIterator<Item = (PartInstanceId, &PartInstance)> {
        self.instances.iter().map(|(k, v)| (*k, v))
    }
    pub fn plate(&self, id: PlateId) -> Option<&Plate> {
        self.plates.get(&id)
    }
    pub fn plates(&self) -> impl ExactSizeIterator<Item = (PlateId, &Plate)> {
        self.plates.iter().map(|(k, v)| (*k, v))
    }
    /// Stored plate assignments of objects that are not part-instance outputs.
    pub fn placements(&self) -> impl ExactSizeIterator<Item = (GeometryId, PlateId)> + '_ {
        self.placement.iter().map(|(k, v)| (*k, *v))
    }
    pub fn record(&self, owner: DerivedOwner) -> Option<&DerivedRecord> {
        self.derived.get(&owner)
    }
    pub fn records(&self) -> impl ExactSizeIterator<Item = (DerivedOwner, &DerivedRecord)> {
        self.derived.iter().map(|(k, v)| (*k, v))
    }
    /// Edge provenance of one derived output.
    pub fn provenance(&self, key: DerivedKey) -> Option<&[ProvenanceEdge]> {
        self.provenance.get(&key).map(|p| p.as_ref())
    }
    pub fn provenances(&self) -> impl ExactSizeIterator<Item = (DerivedKey, &[ProvenanceEdge])> {
        self.provenance.iter().map(|(k, v)| (*k, v.as_ref()))
    }

    /// The plate an object belongs to (part outputs follow their instance).
    pub(crate) fn plate_of(&self, obj: &GeometryObject) -> Option<PlateId> {
        match obj.source() {
            GeometrySource::Derived(DerivedKey { owner: DerivedOwner::Part(i), .. }) => {
                self.instances.get(i).and_then(|i| i.plate)
            }
            _ => self.placement.get(&obj.id()).copied(),
        }
    }

    /// The edges of an object: stored provenance for modifier outputs, otherwise every segment
    /// is its own edge.
    pub(crate) fn edges_of(&self, obj: &GeometryObject) -> Vec<ProvenanceEdge> {
        if let GeometrySource::Derived(key) = obj.source()
            && let Some(p) = self.provenance.get(key)
        {
            return p.to_vec();
        }
        runs(obj.path(), &identity_origins(obj))
    }

    /// The name of the edge of `obj` containing `segment`, if that edge has an origin.
    pub(crate) fn edge_name(&self, obj: &GeometryObject, segment: SegmentRef) -> Option<EdgeName> {
        let edges = self.edges_of(obj);
        let n = obj.path().subpaths().get(segment.subpath).map(|sp| super::reference::drawn_segments(sp).len())?;
        let hit = edges.iter().find(|e| {
            e.subpath as usize == segment.subpath
                && (0..e.count).any(|k| (e.first + k) as usize % n.max(1) == segment.segment)
        })?;
        let origin = hit.origin?;
        let same = edges_with(&edges, origin);
        let index = same.iter().position(|e| e == hit)? as u32;
        Some(EdgeName { origin, index, of: same.len() as u32 })
    }
}

/// Resolves a reference against `lookup` and the layer's provenance. Never by index or
/// proximity: an edge is found only through its origin.
pub(crate) fn resolve_in<'a>(
    layer: &StructureLayer,
    lookup: &dyn Fn(GeometryId) -> Option<&'a GeometryObject>,
    target: &GeometryRef,
) -> RefStatus {
    let Some(obj) = lookup(target.object) else {
        return RefStatus::Broken { reason: BreakReason::ObjectMissing };
    };
    match target.sub {
        None => RefStatus::Bound(Resolved::Object(target.object)),
        Some(SubRef::Edge(name)) => resolve_edge(obj.id(), obj.path(), &layer.edges_of(obj), &name),
        Some(SubRef::Vertex { edge, end }) => match resolve_edge(obj.id(), obj.path(), &layer.edges_of(obj), &edge) {
            RefStatus::Bound(Resolved::Edge { object, subpath, segments }) => {
                match edge_vertex(obj.path(), subpath, &segments, end) {
                    Some(point) => RefStatus::Bound(Resolved::Vertex { object, point }),
                    None => RefStatus::Broken { reason: BreakReason::OriginGone },
                }
            }
            other => other,
        },
    }
}

/// A referenced edge as an analytic curve.
enum Curve {
    Line { from: Point, to: Point },
    Circle { center: Point, radius: f64 },
}

/// Classifies a resolved edge: collinear lines → a line, arcs of one circle → that circle.
fn curve_of(obj: &GeometryObject, subpath: usize, segments: &[usize]) -> Option<Curve> {
    let sp = obj.path().subpaths().get(subpath)?;
    let segs = super::reference::drawn_segments(sp);
    let picked: Vec<(Point, Segment)> = segments.iter().filter_map(|i| segs.get(*i).copied()).collect();
    let (first_from, first) = *picked.first()?;
    match first {
        Segment::Line { .. } => {
            let to = picked.last()?.1.end();
            let d = to - first_from;
            let len = d.length();
            let on_line = picked.iter().all(|(f, s)| {
                matches!(s, Segment::Line { .. })
                    && [*f, s.end()].iter().all(|p| {
                        (d.x * (p.y - first_from.y) - d.y * (p.x - first_from.x)).abs()
                            <= 1e-9 * len.max(1.0) * len.max(1.0)
                    })
            });
            (len > 0.0 && on_line).then_some(Curve::Line { from: first_from, to })
        }
        Segment::Arc(arc) => {
            let (u, v) = arc.semi_diameters();
            let (center, radius) = (arc.center(), u.length());
            let circular = |a: &burnburn_geometry::EllipticalArc| {
                let (u, v) = a.semi_diameters();
                (u.length() - radius).abs() <= 1e-9 * radius.max(1.0)
                    && (v.length() - radius).abs() <= 1e-9 * radius.max(1.0)
                    && u.dot(v).abs() <= 1e-9 * radius.max(1.0) * radius.max(1.0)
                    && a.center().distance(center) <= 1e-9 * radius.max(1.0)
            };
            let _ = v;
            let all = picked.iter().all(|(_, s)| matches!(s, Segment::Arc(a) if circular(a)));
            (radius > 0.0 && all).then_some(Curve::Circle { center, radius })
        }
        Segment::Cubic { .. } => None,
    }
}

fn line_distance(from: Point, to: Point, p: Point) -> f64 {
    let d = to - from;
    (d.x * (p.y - from.y) - d.y * (p.x - from.x)).abs() / d.length()
}

/// Checks one relation against the current geometry. Never changes anything.
pub(crate) fn relation_state_in<'a>(
    layer: &StructureLayer,
    lookup: &dyn Fn(GeometryId) -> Option<&'a GeometryObject>,
    relation: &ObjectRelation,
) -> RelationState {
    use super::RELATION_TOLERANCE_MM as TOL;
    let resolve = |r: ReferenceId| -> Result<Resolved, RelationState> {
        let target = layer.references.get(&r).ok_or(RelationState::Broken { reference: r })?;
        match resolve_in(layer, lookup, &target.target) {
            RefStatus::Bound(res) => Ok(res),
            _ => Err(RelationState::Broken { reference: r }),
        }
    };
    let curve = |r: ReferenceId| -> Result<Curve, RelationState> {
        match resolve(r)? {
            Resolved::Edge { object, subpath, segments } => {
                lookup(object).and_then(|o| curve_of(o, subpath, &segments)).ok_or(RelationState::Unsupported)
            }
            _ => Err(RelationState::Unsupported),
        }
    };
    let verdict = |residual_mm: f64| {
        if residual_mm <= TOL {
            RelationState::Satisfied { residual_mm }
        } else {
            RelationState::Violated { residual_mm }
        }
    };
    let result = (|| -> Result<RelationState, RelationState> {
        match *relation {
            ObjectRelation::Tangent { a, b } => {
                let (ca, cb) = (curve(a)?, curve(b)?);
                let r = match (ca, cb) {
                    (Curve::Line { from, to }, Curve::Circle { center, radius })
                    | (Curve::Circle { center, radius }, Curve::Line { from, to }) => {
                        (line_distance(from, to, center) - radius).abs()
                    }
                    (Curve::Circle { center: c1, radius: r1 }, Curve::Circle { center: c2, radius: r2 }) => {
                        let d = c1.distance(c2);
                        (d - (r1 + r2)).abs().min((d - (r1 - r2).abs()).abs())
                    }
                    _ => return Err(RelationState::Unsupported),
                };
                Ok(verdict(r))
            }
            ObjectRelation::Symmetric { a, b, axis } => {
                let point = |r| match resolve(r)? {
                    Resolved::Vertex { point, .. } => Ok(point),
                    _ => Err(RelationState::Unsupported),
                };
                let (pa, pb) = (point(a)?, point(b)?);
                let Curve::Line { from, to } = curve(axis)? else { return Err(RelationState::Unsupported) };
                let d = to - from;
                let t = (pa - from).dot(d) / d.dot(d);
                let foot = from + d * t;
                let mirrored = foot + (foot - pa);
                Ok(verdict(mirrored.distance(pb)))
            }
        }
    })();
    result.unwrap_or_else(|state| state)
}
