//! Structure panel (S06, S07, P06): what the document's structure layer knows (parts with their
//! variants and instances, plates, references) as a view model, and the instance edits a panel
//! may make. Every edit is one `StructureCommand::SetInstance`, so it is one undo step and the
//! definition is never written; an invalid variant or override fails that instance only.

use std::collections::BTreeMap;

use burnburn_document::structure::{
    AssocReference, BreakReason, EdgeName, GeometryRef, PartDefinitionId, PartFromObjectsError, PartInstance,
    PartInstanceId, PlateId, PlateIssue, RefStatus, ReferenceId, Resolved, StructureCommand, SubRef,
    effective_parameters, part_from_objects,
};
use burnburn_document::{CommandPayload, DerivedOwner, DocumentSnapshot, EvalDiagnosis, EvalStatus, GeometryObject};
use burnburn_geometry::{Affine, DisplayTolerance, Path, Point, Segment, SegmentRef, Subpath, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

use crate::field::{format_mm, parse_length, parse_position};
use crate::structure_panel::{
    BrokenReason, EdgeRow, InstanceRow, InstanceStatus, PartRow, PartValue, PlateIssueRow, PlateRow, ReferenceRow,
    ReferenceStatus, StructureEdit, StructureProblem, StructureViewModel, VariantRow,
};
use crate::view_model::{MessageKey, ProblemViewModel};

use super::{AppUpdate, Session, apply_edit, error};

fn values(map: &BTreeMap<String, Length>) -> Vec<PartValue> {
    map.iter().map(|(name, v)| PartValue { name: name.clone(), value: format_mm(v.mm()) }).collect()
}

fn status_of(snapshot: &DocumentSnapshot, id: PartInstanceId) -> InstanceStatus {
    match snapshot.derived_record(DerivedOwner::Part(id)).map(|r| &r.status) {
        Some(EvalStatus::Valid) => InstanceStatus::Valid,
        Some(EvalStatus::Failed { diagnosis }) => InstanceStatus::Failed {
            parameter: match diagnosis {
                EvalDiagnosis::InvalidParameters { parameter } => parameter.clone(),
                _ => None,
            },
        },
        Some(EvalStatus::Pending) | None => InstanceStatus::Pending,
        Some(_) => InstanceStatus::Pending,
    }
}

/// The length of segment `index` of `sp` in millimetres; the implicit closing segment of a closed
/// subpath comes after the last stored one.
fn segment_length(sp: &Subpath, index: usize) -> f64 {
    let flat = |from: Point, seg: Segment| {
        let Ok(sub) = Subpath::new(from, vec![seg], false) else { return 0.0 };
        let Ok(tol) = Length::from_mm(0.005).map_err(|_| ()).and_then(|l| DisplayTolerance::new(l).map_err(|_| ())) else {
            return 0.0;
        };
        Path::from_subpaths(vec![sub])
            .flatten(&tol)
            .iter()
            .map(|poly| poly.points.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>())
            .sum()
    };
    let mut from = sp.start();
    for (i, seg) in sp.segments().iter().enumerate() {
        if i == index {
            return flat(from, *seg);
        }
        from = seg.end();
    }
    if sp.is_closed() && index == sp.segments().len() { from.distance(sp.start()) } else { 0.0 }
}

/// The edges of `object`: one row per named edge, with the length of all its segments.
fn edges_view(snapshot: &DocumentSnapshot, object: Option<GeometryId>) -> Vec<EdgeRow> {
    let Some(id) = object else { return Vec::new() };
    let Some(obj) = snapshot.object(id) else { return Vec::new() };
    let mut rows = Vec::new();
    let mut seen: Vec<GeometryRef> = Vec::new();
    for (subpath, sp) in obj.path().subpaths().iter().enumerate() {
        let drawn = sp.segments().len() + usize::from(sp.is_closed());
        for segment in 0..drawn {
            let Some(edge) = snapshot.edge_ref(id, SegmentRef { subpath, segment }) else { continue };
            if seen.contains(&edge) {
                continue;
            }
            seen.push(edge);
            let length = match snapshot.resolve(&edge) {
                RefStatus::Bound(Resolved::Edge { segments, .. }) => {
                    segments.iter().map(|i| segment_length(sp, *i)).sum()
                }
                _ => segment_length(sp, segment),
            };
            rows.push(EdgeRow { subpath, segment, length: format_mm(length) });
        }
    }
    rows
}

/// What an ambiguous reference may mean, in words that need no language: where the edge starts and
/// how long it is.
fn candidate_label(snapshot: &DocumentSnapshot, object: GeometryId, name: &EdgeName) -> String {
    let target = GeometryRef { object, sub: Some(SubRef::Edge(*name)) };
    match (snapshot.resolve(&target), snapshot.object(object)) {
        (RefStatus::Bound(Resolved::Edge { subpath, segments, .. }), Some(obj)) => {
            let Some(sp) = obj.path().subpaths().get(subpath) else { return format!("{}/{}", name.index + 1, name.of) };
            let first = segments.first().copied().unwrap_or(0);
            let mut at = sp.start();
            for seg in sp.segments().iter().take(first) {
                at = seg.end();
            }
            let length: f64 = segments.iter().map(|i| segment_length(sp, *i)).sum();
            format!("{} mm ({}, {})", format_mm(length), format_mm(at.x), format_mm(at.y))
        }
        _ => format!("{}/{}", name.index + 1, name.of),
    }
}

pub(super) fn view(s: &Session) -> StructureViewModel {
    let snapshot = s.doc.snapshot();
    let layer = snapshot.structure();
    let plate_name = |id| layer.plate(id).map(|p| p.name.clone());
    let parts = layer
        .definitions()
        .map(|(id, def)| PartRow {
            id: id.raw(),
            name: def.name.clone(),
            parameters: values(&def.parameters),
            variants: def.variants.keys().cloned().collect(),
            variant_values: def
                .variants
                .iter()
                .map(|(name, v)| VariantRow { name: name.clone(), values: values(v) })
                .collect(),
            instances: layer
                .instances()
                .filter(|(_, i)| i.definition == id)
                .map(|(iid, inst)| InstanceRow {
                    id: iid.raw(),
                    variant: inst.variant.clone(),
                    overrides: values(&inst.overrides),
                    effective: values(&effective_parameters(def, inst)),
                    plate: inst.plate.and_then(plate_name),
                    status: status_of(&snapshot, iid),
                })
                .collect(),
        })
        .collect();
    let plates = layer
        .plates()
        .map(|(id, p)| PlateRow {
            id: id.raw(),
            name: p.name.clone(),
            width: format_mm(p.width.mm()),
            height: format_mm(p.height.mm()),
            material: p.material.clone(),
            thickness: p.thickness.map(|t| format_mm(t.mm())),
            instances: layer.instances().filter(|(_, i)| i.plate == Some(id)).count(),
            objects: layer.placements().filter(|(_, pid)| *pid == id).count(),
        })
        .collect();
    let references = layer
        .references()
        .map(|(id, r)| ReferenceRow {
            id: id.raw(),
            name: r.name.clone(),
            object: r.target.object.raw(),
            source: snapshot.object(r.target.object).map(|o| o.name().to_owned()),
            candidates: match snapshot.resolve(&r.target) {
                RefStatus::Ambiguous { candidates } => candidates
                    .iter()
                    .map(|name| candidate_label(&snapshot, r.target.object, name))
                    .collect(),
                _ => Vec::new(),
            },
            status: match snapshot.resolve(&r.target) {
                RefStatus::Bound(_) => ReferenceStatus::Bound,
                RefStatus::Ambiguous { candidates } => ReferenceStatus::Ambiguous { candidates: candidates.len() },
                RefStatus::Broken { reason } => ReferenceStatus::Broken {
                    reason: match reason {
                        BreakReason::ObjectMissing => BrokenReason::ObjectMissing,
                        _ => BrokenReason::OriginGone,
                    },
                },
            },
        })
        .collect();
    let compile_plate = s.compile_plate.filter(|p| layer.plate(*p).is_some());
    let plate_issues = compile_plate
        .and_then(|p| snapshot.plate_view(p).ok())
        .map(|view| {
            view.issues
                .iter()
                .filter_map(|issue| match issue {
                    PlateIssue::TargetOffPlate { operation, geometry } => Some(PlateIssueRow::TargetOffPlate {
                        operation: snapshot.process().operation(*operation).map(|o| o.name().to_owned())?,
                        object: geometry.raw(),
                    }),
                    PlateIssue::Blocked(geometry) => Some(PlateIssueRow::Blocked { object: geometry.raw() }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    StructureViewModel {
        parts,
        plates,
        references,
        compile_plate: compile_plate.map(StableId::raw),
        plan_plate: s.plan_plate.map(StableId::raw),
        plate_issues,
        selected: s.selection.ids().iter().filter(|id| snapshot.object(**id).is_some()).count(),
        edges: edges_view(&snapshot, s.selection.single()),
        edge_object: s.selection.single().filter(|id| snapshot.object(*id).is_some()).map(StableId::raw),
    }
}

fn refused(s: &mut Session, document: DocumentId, message: MessageKey) -> AppUpdate {
    s.problem = Some(ProblemViewModel { message, object: None, at: Point::new(0.0, 0.0) });
    let mut update = error(Some(document), message);
    update.view_changed = true;
    update
}

/// One edit of the structure panel: of an instance (variant, local override), of a part (define,
/// place, parameters, variants) or of a reference. Each is one undo step; nothing is changed when
/// what it names does not exist or the typed value does not fit.
pub(super) fn edit(
    s: &mut Session,
    document: DocumentId,
    edit: StructureEdit,
    request: Option<RequestId>,
) -> AppUpdate {
    match edit {
        StructureEdit::SetVariant { .. } | StructureEdit::SetOverride { .. } => {
            instance_edit(s, document, edit, request)
        }
        other => other_edit(s, document, other, request),
    }
}

fn problem(p: StructureProblem) -> MessageKey {
    MessageKey::Structure(p)
}

/// The length of the typed text, or the refusal to show.
fn typed_length(text: &str) -> Result<Length, MessageKey> {
    parse_length(text).map_err(MessageKey::InvalidLength)
}

/// A position typed in a field: empty is 0.
fn typed_position(text: &str) -> Result<f64, MessageKey> {
    if text.trim().is_empty() { Ok(0.0) } else { parse_position(text).map(Length::mm).map_err(MessageKey::InvalidLength) }
}

fn other_edit(s: &mut Session, document: DocumentId, edit: StructureEdit, request: Option<RequestId>) -> AppUpdate {
    match build(s, edit) {
        Ok(command) => {
            s.problem = None;
            apply_edit(s, document, CommandPayload::Structure(command), request)
        }
        Err(message) => refused(s, document, message),
    }
}

/// The document command an edit stands for, or why there is none.
fn build(s: &Session, edit: StructureEdit) -> Result<StructureCommand, MessageKey> {
    let snapshot = s.doc.snapshot();
    let layer = snapshot.structure();
    let definition_of = |raw: u64| {
        let id = PartDefinitionId::from_raw(raw);
        layer.definition(id).map(|d| (id, d.clone())).ok_or(problem(StructureProblem::UnknownPart))
    };
    match edit {
        StructureEdit::SetVariant { .. } | StructureEdit::SetOverride { .. } => Err(MessageKey::CommandRejected),
        StructureEdit::DefinePart { name } => {
            let objects: Vec<&GeometryObject> =
                s.selection.ids().iter().filter_map(|id| snapshot.object(*id)).collect();
            match part_from_objects(&name, &objects) {
                Ok(made) => Ok(StructureCommand::DefinePart { definition: made.definition }),
                Err(PartFromObjectsError::NoObjects) => Err(problem(StructureProblem::NeedsSelection)),
                Err(PartFromObjectsError::EmptyName) => Err(problem(StructureProblem::NeedsName)),
                Err(_) => Err(MessageKey::CommandRejected),
            }
        }
        StructureEdit::PlaceInstance { definition, x, y } => {
            let (id, _) = definition_of(definition)?;
            let (x, y) = (typed_position(&x)?, typed_position(&y)?);
            let transform = Affine::translate(Vector::new(x, y)).map_err(|_| MessageKey::CommandRejected)?;
            let instance = PartInstance { definition: id, variant: None, overrides: BTreeMap::new(), transform, plate: None };
            Ok(StructureCommand::PlaceInstance { instance })
        }
        StructureEdit::RemoveInstance { instance } => {
            let id = PartInstanceId::from_raw(instance);
            layer.instance(id).ok_or(problem(StructureProblem::UnknownInstance))?;
            Ok(StructureCommand::RemoveInstance { instance: id })
        }
        StructureEdit::RemovePart { definition } => {
            let (id, _) = definition_of(definition)?;
            if layer.instances().any(|(_, i)| i.definition == id) {
                return Err(problem(StructureProblem::PartInUse));
            }
            Ok(StructureCommand::RemovePartDefinition { definition: id })
        }
        StructureEdit::SetParameter { definition, parameter, text } => {
            let (id, mut value) = definition_of(definition)?;
            if !value.parameters.contains_key(&parameter) {
                return Err(problem(StructureProblem::UnknownParameter));
            }
            value.parameters.insert(parameter, typed_length(&text)?);
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::AddVariant { definition, name } => {
            let (id, mut value) = definition_of(definition)?;
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err(problem(StructureProblem::NeedsName));
            }
            if value.variants.contains_key(&name) {
                return Err(problem(StructureProblem::VariantExists));
            }
            value.variants.insert(name, BTreeMap::new());
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::SetVariantValue { definition, variant, parameter, text } => {
            let (id, mut value) = definition_of(definition)?;
            if !value.parameters.contains_key(&parameter) {
                return Err(problem(StructureProblem::UnknownParameter));
            }
            let set = value.variants.get_mut(&variant).ok_or(problem(StructureProblem::UnknownVariant))?;
            if text.trim().is_empty() {
                set.remove(&parameter);
            } else {
                set.insert(parameter, typed_length(&text)?);
            }
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::AddReference { name, object, subpath, segment } => {
            let object = GeometryId::from_raw(object);
            snapshot.object(object).ok_or(problem(StructureProblem::UnknownObject))?;
            let target = snapshot
                .edge_ref(object, SegmentRef { subpath, segment })
                .ok_or(problem(StructureProblem::NotAnEdge))?;
            let name = if name.trim().is_empty() { format!("Rand {}", segment + 1) } else { name.trim().to_owned() };
            Ok(StructureCommand::AddReference { reference: AssocReference { name, target } })
        }
        StructureEdit::RebindReference { reference, candidate } => {
            let id = ReferenceId::from_raw(reference);
            let stored = layer.reference(id).ok_or(problem(StructureProblem::UnknownReference))?;
            let RefStatus::Ambiguous { candidates } = snapshot.resolve(&stored.target) else {
                return Err(problem(StructureProblem::NotAmbiguous));
            };
            let name = candidates.get(candidate).ok_or(problem(StructureProblem::NoCandidate))?;
            let target = GeometryRef { object: stored.target.object, sub: Some(SubRef::Edge(*name)) };
            Ok(StructureCommand::RebindReference { reference: id, target })
        }
        StructureEdit::RemoveReference { reference } => {
            let id = ReferenceId::from_raw(reference);
            layer.reference(id).ok_or(problem(StructureProblem::UnknownReference))?;
            Ok(StructureCommand::RemoveReference { reference: id })
        }
    }
}

/// One edit of one instance. Nothing is changed when the instance, the variant or the parameter
/// does not exist, or when the typed value is not a positive length.
fn instance_edit(
    s: &mut Session,
    document: DocumentId,
    edit: StructureEdit,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let layer = snapshot.structure();
    let (instance, new): (PartInstanceId, PartInstance) = match &edit {
        StructureEdit::SetVariant { instance, variant } => {
            let id = PartInstanceId::from_raw(*instance);
            let Some(current) = layer.instance(id) else { return refused(s, document, MessageKey::CommandRejected) };
            let known = variant
                .as_ref()
                .is_none_or(|v| layer.definition(current.definition).is_some_and(|d| d.variants.contains_key(v)));
            if !known {
                return refused(s, document, MessageKey::CommandRejected);
            }
            let mut next = current.clone();
            next.variant.clone_from(variant);
            (id, next)
        }
        StructureEdit::SetOverride { instance, parameter, text } => {
            let id = PartInstanceId::from_raw(*instance);
            let Some(current) = layer.instance(id) else { return refused(s, document, MessageKey::CommandRejected) };
            if !layer.definition(current.definition).is_some_and(|d| d.parameters.contains_key(parameter)) {
                return refused(s, document, MessageKey::CommandRejected);
            }
            let mut next = current.clone();
            if text.trim().is_empty() {
                next.overrides.remove(parameter);
            } else {
                match parse_length(text) {
                    Ok(v) => {
                        next.overrides.insert(parameter.clone(), v);
                    }
                    Err(e) => return refused(s, document, MessageKey::InvalidLength(e)),
                }
            }
            (id, next)
        }
        // The other edits have their own route.
        _ => return AppUpdate::redraw(document),
    };
    if layer.instance(instance) == Some(&new) {
        return AppUpdate::redraw(document);
    }
    s.problem = None;
    apply_edit(s, document, CommandPayload::Structure(StructureCommand::SetInstance { instance, value: new }), request)
}

/// Chooses the plate Compile uses. The plan shown so far belonged to the other choice and goes;
/// compiles in flight for it are cancelled. A plate that does not exist is refused.
pub(super) fn choose_plate(s: &mut Session, document: DocumentId, plate: Option<u64>) -> AppUpdate {
    let chosen = match plate {
        None => None,
        Some(raw) => {
            let id = PlateId::from_raw(raw);
            if s.doc.snapshot().structure().plate(id).is_none() {
                return refused(s, document, MessageKey::CommandRejected);
            }
            Some(id)
        }
    };
    if s.compile_plate == chosen {
        return AppUpdate::redraw(document);
    }
    s.compile_plate = chosen;
    for (kind, older) in s.pending.values() {
        if *kind == crate::task::TaskKind::Compile {
            older.cancel();
        }
    }
    s.compile_plates.clear();
    s.compiled = None;
    s.plan_plate = None;
    s.problem = None;
    AppUpdate { view_changed: true, scenes_changed: vec![document], ..AppUpdate::default() }
}
