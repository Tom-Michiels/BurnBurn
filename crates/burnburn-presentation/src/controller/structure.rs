//! Structure panel (S06, S07, P06): what the document's structure layer knows (parts with their
//! variants and instances, plates, references) as a view model, and the instance edits a panel
//! may make. Every edit is one `StructureCommand::SetInstance`, so it is one undo step and the
//! definition is never written; an invalid variant or override fails that instance only.

use std::collections::BTreeMap;

use burnburn_document::structure::{
    AssocReference, BreakReason, Dim, EdgeName, EvalMode, GeometryRef, Modifier, ModifierChain, ModifierId,
    ModifierStep, NamedShape, PartDefinition, PartDefinitionId, PartFromObjectsError, PartInstance, PartInstanceId,
    PartShape, Plate, PlateId, PlateIssue, RefStatus, ReferenceId, Resolved, StructureCommand, SubRef,
    effective_parameters, part_from_objects,
};
use burnburn_document::{
    CommandPayload, CompoundCommand, CompoundLabel, CompoundStep, DerivedOwner, DocumentSnapshot, EvalDiagnosis,
    EvalStatus, GeometryObject, GeometrySource,
};
use burnburn_geometry::{Affine, DisplayTolerance, Path, Point, Segment, SegmentRef, Subpath, Vector};
use burnburn_types::{Angle, DocumentId, GeometryId, Length, RequestId, StableId};

use crate::field::{format_mm, parse_length, parse_position};
use crate::structure_panel::{
    BooleanKind, BrokenReason, EdgeRow, InstanceRow, InstanceStatus, ModifierKind, ModifierRow, ModifierStatus,
    ModifierStepRow, ModifierValueKind, NewModifierStep, PartRow, PartShapeKind, PartShapeRow, PartValue, PlateField,
    PlateIssueRow, PlateRow, ReferenceRow, ReferenceStatus, ShapeField, StructureEdit, StructureProblem,
    StructureViewModel, VariantRow,
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
        let Ok(tol) = Length::from_mm(0.005).map_err(|_| ()).and_then(|l| DisplayTolerance::new(l).map_err(|_| ()))
        else {
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

// ---------------------------------------------------------------------------------------------
// The modifier stack (G7-editor, E06)
// ---------------------------------------------------------------------------------------------

fn kind_of(modifier: &Modifier) -> ModifierKind {
    match modifier {
        Modifier::Offset { .. } => ModifierKind::Offset,
        Modifier::Boolean { .. } => ModifierKind::Boolean,
        Modifier::LinearArray { .. } => ModifierKind::LinearArray,
        Modifier::GridArray(_) => ModifierKind::GridArray,
        Modifier::CircularArray(_) => ModifierKind::CircularArray,
        Modifier::PathArray { .. } => ModifierKind::PathArray,
        // A step kind this panel has no words for yet is shown as the nearest known one.
        _ => ModifierKind::Boolean,
    }
}

/// The one number of a step the panel can edit.
fn value_of(modifier: &Modifier) -> Option<(ModifierValueKind, String)> {
    match modifier {
        Modifier::Offset { distance, .. } => Some((ModifierValueKind::Distance, format_mm(distance.mm()))),
        Modifier::LinearArray { count, .. } | Modifier::PathArray { count, .. } => {
            Some((ModifierValueKind::Count, count.to_string()))
        }
        Modifier::CircularArray(c) => Some((ModifierValueKind::Count, c.count.to_string())),
        _ => None,
    }
}

/// The selected objects a boolean step combines with: everything selected except the chain's
/// source and the chain's own outputs.
fn operands_of(
    s: &Session,
    snapshot: &DocumentSnapshot,
    source: GeometryId,
    own: Option<DerivedOwner>,
) -> Vec<GeometryId> {
    s.selection
        .ids()
        .iter()
        .copied()
        .filter(|id| *id != source)
        .filter(|id| {
            snapshot.object(*id).is_some_and(
                |o| !matches!((o.source(), own), (GeometrySource::Derived(k), Some(owner)) if k.owner == owner),
            )
        })
        .collect()
}

/// An array the geometry refuses (zero or too many copies) is an invalid count.
fn array_error(_: burnburn_geometry::array::ArrayError) -> MessageKey {
    problem(StructureProblem::InvalidCount)
}

/// The modifier step `step` stands for, with its numbers parsed. `operands` are the objects a
/// boolean step combines with.
fn new_step(
    snapshot: &DocumentSnapshot,
    source: GeometryId,
    operands: &[GeometryId],
    step: NewModifierStep,
) -> Result<Modifier, MessageKey> {
    match step {
        NewModifierStep::Offset { distance } => {
            let distance = Length::from_mm(typed_position(&distance)?).map_err(|_| MessageKey::CommandRejected)?;
            Ok(Modifier::Offset { distance, join: burnburn_geometry::OffsetJoin::Round })
        }
        NewModifierStep::Boolean { op } => {
            if operands.is_empty() {
                return Err(problem(StructureProblem::NeedsOperands));
            }
            let op = match op {
                BooleanKind::Union => burnburn_geometry::BooleanOp::Union,
                BooleanKind::Difference => burnburn_geometry::BooleanOp::Difference,
                BooleanKind::Intersection => burnburn_geometry::BooleanOp::Intersection,
                BooleanKind::Xor => burnburn_geometry::BooleanOp::Xor,
            };
            Ok(Modifier::Boolean { op, with: operands.to_vec() })
        }
        NewModifierStep::Grid { columns, rows, pitch_x, pitch_y } => {
            let (columns, rows) = (typed_count(&columns)?, typed_count(&rows)?);
            let pitch_x = Length::from_mm(typed_position(&pitch_x)?).map_err(|_| MessageKey::CommandRejected)?;
            let pitch_y = Length::from_mm(typed_position(&pitch_y)?).map_err(|_| MessageKey::CommandRejected)?;
            let grid = burnburn_geometry::array::GridArray { columns, rows, pitch_x, pitch_y };
            burnburn_geometry::array::grid_array(&grid).map_err(array_error)?;
            Ok(Modifier::GridArray(grid))
        }
        NewModifierStep::Circle { count, center_x, center_y, sweep } => {
            let count = typed_count(&count)?;
            let center = Point::new(typed_position(&center_x)?, typed_position(&center_y)?);
            let sweep_deg = crate::field::parse_number(&sweep, &["°", "deg"])
                .ok()
                .filter(|v| v.is_finite() && *v != 0.0 && v.abs() <= 360.0)
                .ok_or(problem(StructureProblem::InvalidSweep))?;
            let reference =
                snapshot.object(source).and_then(|o| o.path().bounds()).map_or(Point::new(0.0, 0.0), |b| b.center());
            let circular =
                burnburn_geometry::array::CircularArray { count, center, sweep_deg, rotate_items: true, reference };
            burnburn_geometry::array::circular_array(&circular).map_err(array_error)?;
            Ok(Modifier::CircularArray(circular))
        }
    }
}

fn modifiers_view(s: &Session, snapshot: &DocumentSnapshot) -> Vec<ModifierRow> {
    snapshot
        .structure()
        .modifiers()
        .map(|(id, chain)| {
            let owner = DerivedOwner::Modifier(id);
            let record = snapshot.derived_record(owner);
            let failed_step = match record.map(|r| &r.status) {
                Some(EvalStatus::Failed { diagnosis: EvalDiagnosis::ModifierStep { step, .. } }) => Some(*step),
                _ => None,
            };
            let status = match record.map(|r| &r.status) {
                Some(EvalStatus::Valid) => ModifierStatus::Valid,
                Some(EvalStatus::Failed { .. }) => ModifierStatus::Failed { step: failed_step },
                _ => ModifierStatus::Pending,
            };
            ModifierRow {
                id: id.raw(),
                name: chain.name.clone(),
                source: chain.source.raw(),
                source_name: snapshot.object(chain.source).map(|o| o.name().to_owned()),
                steps: chain
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(index, step)| ModifierStepRow {
                        index,
                        kind: kind_of(&step.modifier),
                        value: value_of(&step.modifier),
                        enabled: step.enabled,
                        failed: failed_step == Some(index),
                    })
                    .collect(),
                status,
                outputs: snapshot
                    .objects()
                    .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == owner))
                    .count(),
                in_selection: s.selection.ids().iter().any(|sel| {
                    *sel == chain.source
                        || snapshot
                            .object(*sel)
                            .is_some_and(|o| matches!(o.source(), GeometrySource::Derived(k) if k.owner == owner))
                }),
            }
        })
        .collect()
}

/// Whether `text` is more than a word or a number: an operator, a bracket, a space or another sign.
fn looks_like_formula(text: &str) -> bool {
    text.trim().chars().any(|c| !(c.is_alphanumeric() || matches!(c, '_' | '.' | ',')))
}

/// A formula over the parameters of the part as a shape number, or why it is none.
fn formula_dim(text: &str, parameters: &BTreeMap<String, Length>) -> Result<Dim, MessageKey> {
    let formula =
        burnburn_document::structure::parse_formula(text).map_err(|_| problem(StructureProblem::BadFormula))?;
    if formula.names().iter().any(|n| !parameters.contains_key(n)) {
        return Err(problem(StructureProblem::BadFormula));
    }
    Ok(Dim::Expr(text.trim().to_owned()))
}

/// The fixed outline `path` with one number of its box changed: the corner moves (X, Y) or the
/// outline is stretched from its lower left corner (width, height).
fn fixed_shape_edit(path: &Path, field: ShapeField, text: &str) -> Result<Path, MessageKey> {
    let bounds = path.bounds().ok_or(problem(StructureProblem::NoSuchField))?;
    let (min, width, height) = (bounds.min(), bounds.width(), bounds.height());
    let transform = match field {
        ShapeField::X => {
            let x = parse_position(text).map_err(MessageKey::InvalidLength)?.mm();
            Affine::translate(Vector::new(x - min.x, 0.0))
        }
        ShapeField::Y => {
            let y = parse_position(text).map_err(MessageKey::InvalidLength)?.mm();
            Affine::translate(Vector::new(0.0, y - min.y))
        }
        ShapeField::Width => {
            let w = typed_length(text)?.mm();
            if !(width > 0.0) {
                return Err(problem(StructureProblem::NoSuchField));
            }
            Affine::scale_about(min, w / width, 1.0)
        }
        ShapeField::Height => {
            let h = typed_length(text)?.mm();
            if !(height > 0.0) {
                return Err(problem(StructureProblem::NoSuchField));
            }
            Affine::scale_about(min, 1.0, h / height)
        }
        _ => return Err(problem(StructureProblem::NoSuchField)),
    };
    Ok(path.transform(&transform.map_err(|_| MessageKey::CommandRejected)?))
}

/// The turn of a transform in degrees (counter-clockwise, -180 to 180]; a mirror shows the turn of
/// its x axis.
fn rotation_degrees(t: &Affine) -> f64 {
    let [a, b, ..] = t.coefficients();
    let degrees = b.atan2(a).to_degrees();
    // A turn that is zero up to rounding shows as 0.
    if degrees.abs() < 1e-9 { 0.0 } else { degrees }
}

fn dim_text(d: &Dim) -> String {
    match d {
        Dim::Mm(v) => format_mm(*v),
        Dim::Param(p) => p.clone(),
        Dim::Expr(text) => text.clone(),
    }
}

fn shapes_view(def: &PartDefinition) -> Vec<PartShapeRow> {
    def.shapes
        .iter()
        .map(|(key, named)| {
            let (kind, fields) = match &named.shape {
                PartShape::Rectangle { x, y, width, height } => (
                    PartShapeKind::Rectangle,
                    vec![
                        (ShapeField::X, dim_text(x)),
                        (ShapeField::Y, dim_text(y)),
                        (ShapeField::Width, dim_text(width)),
                        (ShapeField::Height, dim_text(height)),
                    ],
                ),
                PartShape::Circle { x, y, radius } => (
                    PartShapeKind::Circle,
                    vec![
                        (ShapeField::X, dim_text(x)),
                        (ShapeField::Y, dim_text(y)),
                        (ShapeField::Radius, dim_text(radius)),
                    ],
                ),
                PartShape::Fixed(path) => (
                    PartShapeKind::Fixed,
                    // The box of the fixed outline: it can be moved and resized from here (G8-editor).
                    path.bounds().map_or_else(Vec::new, |b| {
                        vec![
                            (ShapeField::X, format_mm(b.min().x)),
                            (ShapeField::Y, format_mm(b.min().y)),
                            (ShapeField::Width, format_mm(b.width())),
                            (ShapeField::Height, format_mm(b.height())),
                        ]
                    }),
                ),
                PartShape::Fitted { x, y, width, height, .. } => (
                    PartShapeKind::Fixed,
                    vec![
                        (ShapeField::X, dim_text(x)),
                        (ShapeField::Y, dim_text(y)),
                        (ShapeField::Width, dim_text(width)),
                        (ShapeField::Height, dim_text(height)),
                    ],
                ),
                _ => (PartShapeKind::Fixed, Vec::new()),
            };
            PartShapeRow { key: *key, name: named.name.clone(), kind, fields }
        })
        .collect()
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
            shapes: shapes_view(def),
            instances: layer
                .instances()
                .filter(|(_, i)| i.definition == id)
                .map(|(iid, inst)| InstanceRow {
                    id: iid.raw(),
                    variant: inst.variant.clone(),
                    overrides: values(&inst.overrides),
                    effective: values(&effective_parameters(def, inst)),
                    plate: inst.plate.and_then(plate_name),
                    plate_id: inst.plate.map(StableId::raw),
                    rotation: format_mm(rotation_degrees(&inst.transform)),
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
                RefStatus::Ambiguous { candidates } => {
                    candidates.iter().map(|name| candidate_label(&snapshot, r.target.object, name)).collect()
                }
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
        modifiers: modifiers_view(s, &snapshot),
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
        StructureEdit::RotateInstance { instance, degrees, relative } => {
            rotate_instance(s, document, instance, &degrees, relative, request)
        }
        StructureEdit::DistributeOverPlates { gap } => distribute(s, document, &gap, request),
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

/// The plate `raw` names, or the refusal to show.
fn plate_of(layer: &burnburn_document::structure::StructureLayer, raw: u64) -> Result<(PlateId, Plate), MessageKey> {
    let id = PlateId::from_raw(raw);
    layer.plate(id).map(|p| (id, p.clone())).ok_or(problem(StructureProblem::UnknownPlate))
}

/// The chain `raw` names, or the refusal to show.
fn chain_of(
    layer: &burnburn_document::structure::StructureLayer,
    raw: u64,
) -> Result<(ModifierId, ModifierChain), MessageKey> {
    let id = ModifierId::from_raw(raw);
    layer.modifier(id).map(|c| (id, c.clone())).ok_or(problem(StructureProblem::UnknownModifier))
}

/// A number of copies typed in a field: a whole number of at least one.
fn typed_count(text: &str) -> Result<u32, MessageKey> {
    let v = crate::field::parse_number(text, &[]).map_err(MessageKey::InvalidLength)?;
    if v.fract() == 0.0 && (1.0..=f64::from(u32::MAX)).contains(&v) {
        Ok(v as u32)
    } else {
        Err(problem(StructureProblem::InvalidCount))
    }
}

/// A position typed in a field: empty is 0.
fn typed_position(text: &str) -> Result<f64, MessageKey> {
    if text.trim().is_empty() {
        Ok(0.0)
    } else {
        parse_position(text).map(Length::mm).map_err(MessageKey::InvalidLength)
    }
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
            let instance =
                PartInstance { definition: id, variant: None, overrides: BTreeMap::new(), transform, plate: None };
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
        StructureEdit::CreatePlate { name, width, height, material, thickness } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err(problem(StructureProblem::NeedsName));
            }
            let thickness = if thickness.trim().is_empty() { None } else { Some(typed_length(&thickness)?) };
            let plate = Plate {
                name,
                width: typed_length(&width)?,
                height: typed_length(&height)?,
                material: material.trim().to_owned(),
                thickness,
            };
            Ok(StructureCommand::CreatePlate { plate })
        }
        StructureEdit::SetPlateField { plate, field, text } => {
            let (id, mut value) = plate_of(layer, plate)?;
            match field {
                PlateField::Name => {
                    value.name = text.trim().to_owned();
                    if value.name.is_empty() {
                        return Err(problem(StructureProblem::NeedsName));
                    }
                }
                PlateField::Width => value.width = typed_length(&text)?,
                PlateField::Height => value.height = typed_length(&text)?,
                PlateField::Material => value.material = text.trim().to_owned(),
                PlateField::Thickness => {
                    value.thickness = if text.trim().is_empty() { None } else { Some(typed_length(&text)?) };
                }
            }
            Ok(StructureCommand::SetPlate { plate: id, value })
        }
        StructureEdit::RemovePlate { plate } => {
            let (id, _) = plate_of(layer, plate)?;
            Ok(StructureCommand::RemovePlate { plate: id })
        }
        StructureEdit::AssignSelectionToPlate { plate } => {
            let objects: Vec<GeometryId> =
                s.selection.ids().iter().copied().filter(|id| snapshot.object(*id).is_some()).collect();
            if objects.is_empty() {
                return Err(problem(StructureProblem::NeedsSelection));
            }
            let plate = plate.map(|raw| plate_of(layer, raw).map(|(id, _)| id)).transpose()?;
            let part_output = |id: &GeometryId| {
                snapshot.object(*id).is_some_and(
                    |o| matches!(o.source(), GeometrySource::Derived(k) if matches!(k.owner, DerivedOwner::Part(_))),
                )
            };
            if objects.iter().any(part_output) {
                return Err(problem(StructureProblem::PartOutputSelected));
            }
            Ok(StructureCommand::AssignToPlate { objects, plate })
        }
        StructureEdit::SetInstancePlate { instance, plate } => {
            let id = PartInstanceId::from_raw(instance);
            let mut value = layer.instance(id).cloned().ok_or(problem(StructureProblem::UnknownInstance))?;
            value.plate = plate.map(|raw| plate_of(layer, raw).map(|(id, _)| id)).transpose()?;
            Ok(StructureCommand::SetInstance { instance: id, value })
        }
        StructureEdit::RenameVariant { definition, from, to } => {
            let (id, def) = definition_of(definition)?;
            if !def.variants.contains_key(&from) {
                return Err(problem(StructureProblem::UnknownVariant));
            }
            let to = to.trim().to_owned();
            if to.is_empty() {
                return Err(problem(StructureProblem::NeedsName));
            }
            if to != from && def.variants.contains_key(&to) {
                return Err(problem(StructureProblem::VariantExists));
            }
            Ok(StructureCommand::RenameVariant { definition: id, from, to })
        }
        StructureEdit::RemoveVariant { definition, name } => {
            let (id, mut value) = definition_of(definition)?;
            if value.variants.remove(&name).is_none() {
                return Err(problem(StructureProblem::UnknownVariant));
            }
            if layer.instances().any(|(_, i)| i.definition == id && i.variant.as_deref() == Some(name.as_str())) {
                return Err(problem(StructureProblem::VariantInUse));
            }
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::AddPartShape { definition, kind, name } => {
            let (id, mut value) = definition_of(definition)?;
            let shape = match kind {
                PartShapeKind::Rectangle => PartShape::Rectangle {
                    x: Dim::Mm(0.0),
                    y: Dim::Mm(0.0),
                    width: Dim::Mm(10.0),
                    height: Dim::Mm(10.0),
                },
                PartShapeKind::Circle => PartShape::Circle { x: Dim::Mm(5.0), y: Dim::Mm(5.0), radius: Dim::Mm(5.0) },
                _ => return Err(problem(StructureProblem::CannotAddFixed)),
            };
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err(problem(StructureProblem::NeedsName));
            }
            let key = value.shapes.keys().next_back().map_or(1, |k| k.saturating_add(1));
            value.shapes.insert(key, NamedShape { name, shape });
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::RemovePartShape { definition, shape } => {
            let (id, mut value) = definition_of(definition)?;
            value.shapes.remove(&shape).ok_or(problem(StructureProblem::UnknownShape))?;
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::RenamePartShape { definition, shape, name } => {
            let (id, mut value) = definition_of(definition)?;
            let slot = value.shapes.get_mut(&shape).ok_or(problem(StructureProblem::UnknownShape))?;
            slot.name = name.trim().to_owned();
            if slot.name.is_empty() {
                return Err(problem(StructureProblem::NeedsName));
            }
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::SetPartShapeField { definition, shape, field, text } => {
            let (id, mut value) = definition_of(definition)?;
            let known = value.parameters.keys().any(|p| *p == text.trim());
            if let Some(NamedShape { shape: PartShape::Fixed(path), .. }) = value.shapes.get(&shape) {
                // A parameter name or a formula makes the box of the outline follow the parameters
                // (G9-editor): the fixed outline becomes a fitted one and the code below sets the
                // number. A plain number moves or stretches the outline itself.
                let box_field = matches!(field, ShapeField::X | ShapeField::Y | ShapeField::Width | ShapeField::Height);
                let parametric = known || (looks_like_formula(&text) && parse_length(&text).is_err());
                if parametric && box_field {
                    let b = path.bounds().ok_or(problem(StructureProblem::NoSuchField))?;
                    if !(b.width() > 0.0 && b.height() > 0.0) {
                        return Err(problem(StructureProblem::NoSuchField));
                    }
                    let fitted = PartShape::Fitted {
                        path: path.clone(),
                        x: Dim::Mm(b.min().x),
                        y: Dim::Mm(b.min().y),
                        width: Dim::Mm(b.width()),
                        height: Dim::Mm(b.height()),
                    };
                    if let Some(slot) = value.shapes.get_mut(&shape) {
                        slot.shape = fitted;
                    }
                } else {
                    let moved = fixed_shape_edit(path, field, &text)?;
                    if let Some(slot) = value.shapes.get_mut(&shape) {
                        slot.shape = PartShape::Fixed(moved);
                    }
                    return Ok(StructureCommand::SetPartDefinition { definition: id, value });
                }
            }
            let positioned = matches!(field, ShapeField::X | ShapeField::Y);
            let dim = if known {
                Dim::Param(text.trim().to_owned())
            } else {
                let number = if positioned { parse_position(&text) } else { parse_length(&text) };
                match number {
                    Ok(v) => Dim::Mm(v.mm()),
                    // Not a number, but built like a formula: a formula over the parameters or nothing.
                    Err(crate::field::FieldError::NotANumber) if looks_like_formula(&text) => {
                        formula_dim(&text, &value.parameters)?
                    }
                    Err(e) => return Err(MessageKey::InvalidLength(e)),
                }
            };
            let slot = value.shapes.get_mut(&shape).ok_or(problem(StructureProblem::UnknownShape))?;
            let target = match (&mut slot.shape, field) {
                (PartShape::Rectangle { x, .. } | PartShape::Circle { x, .. } | PartShape::Fitted { x, .. }, ShapeField::X) => x,
                (PartShape::Rectangle { y, .. } | PartShape::Circle { y, .. } | PartShape::Fitted { y, .. }, ShapeField::Y) => y,
                (PartShape::Rectangle { width, .. } | PartShape::Fitted { width, .. }, ShapeField::Width) => width,
                (PartShape::Rectangle { height, .. } | PartShape::Fitted { height, .. }, ShapeField::Height) => height,
                (PartShape::Circle { radius, .. }, ShapeField::Radius) => radius,
                _ => return Err(problem(StructureProblem::NoSuchField)),
            };
            *target = dim;
            Ok(StructureCommand::SetPartDefinition { definition: id, value })
        }
        StructureEdit::ToggleModifierStep { modifier, step, enabled } => {
            let (id, chain) = chain_of(layer, modifier)?;
            chain.steps.get(step).ok_or(problem(StructureProblem::UnknownStep))?;
            Ok(StructureCommand::SetStepEnabled { modifier: id, step, enabled })
        }
        StructureEdit::MoveModifierStep { modifier, step, up } => {
            let (id, mut chain) = chain_of(layer, modifier)?;
            if step >= chain.steps.len() {
                return Err(problem(StructureProblem::UnknownStep));
            }
            let other = if up { step.checked_sub(1) } else { Some(step + 1).filter(|n| *n < chain.steps.len()) };
            let other = other.ok_or(problem(StructureProblem::CannotMoveStep))?;
            chain.steps.swap(step, other);
            Ok(StructureCommand::SetModifier { modifier: id, chain })
        }
        StructureEdit::RemoveModifierStep { modifier, step } => {
            let (id, mut chain) = chain_of(layer, modifier)?;
            if step >= chain.steps.len() {
                return Err(problem(StructureProblem::UnknownStep));
            }
            if chain.steps.len() == 1 {
                return Err(problem(StructureProblem::LastStep));
            }
            chain.steps.remove(step);
            Ok(StructureCommand::SetModifier { modifier: id, chain })
        }
        StructureEdit::SetModifierStepValue { modifier, step, text } => {
            let (id, mut chain) = chain_of(layer, modifier)?;
            let slot = chain.steps.get_mut(step).ok_or(problem(StructureProblem::UnknownStep))?;
            match &mut slot.modifier {
                Modifier::Offset { distance, .. } => {
                    *distance = Length::from_mm(typed_position(&text)?).map_err(|_| MessageKey::CommandRejected)?;
                }
                Modifier::LinearArray { count, .. } | Modifier::PathArray { count, .. } => {
                    *count = typed_count(&text)?;
                }
                Modifier::CircularArray(c) => c.count = typed_count(&text)?,
                _ => return Err(problem(StructureProblem::StepHasNoValue)),
            }
            Ok(StructureCommand::SetModifier { modifier: id, chain })
        }
        StructureEdit::RemoveModifier { modifier } => {
            let (id, _) = chain_of(layer, modifier)?;
            Ok(StructureCommand::RemoveModifier { modifier: id })
        }
        StructureEdit::BakeModifier { modifier } => {
            let (id, _) = chain_of(layer, modifier)?;
            Ok(StructureCommand::BakeModifier { modifier: id })
        }
        StructureEdit::RotateInstance { .. } | StructureEdit::DistributeOverPlates { .. } => {
            Err(MessageKey::CommandRejected)
        }
        StructureEdit::AddModifierStep { modifier, step } => {
            let (id, mut chain) = chain_of(layer, modifier)?;
            let own = DerivedOwner::Modifier(id);
            let operands = operands_of(s, &snapshot, chain.source, Some(own));
            let made = new_step(&snapshot, chain.source, &operands, step)?;
            chain.steps.push(ModifierStep { modifier: made, enabled: true });
            Ok(StructureCommand::SetModifier { modifier: id, chain })
        }
        StructureEdit::CreateModifierChain { step } => {
            let selected: Vec<GeometryId> =
                s.selection.ids().iter().copied().filter(|id| snapshot.object(*id).is_some()).collect();
            let source = *selected.first().ok_or(problem(StructureProblem::NeedsSelection))?;
            let name = match &step {
                NewModifierStep::Offset { .. } => "Offset",
                NewModifierStep::Boolean { .. } => "Boolean",
                NewModifierStep::Grid { .. } => "Raster",
                NewModifierStep::Circle { .. } => "Cirkel",
            };
            let made = new_step(&snapshot, source, &selected[1..], step)?;
            let tolerance = crate::commit::boolean_options().map(|o| o.tolerance).ok_or(MessageKey::CommandRejected)?;
            let chain = ModifierChain {
                name: name.to_owned(),
                source,
                steps: vec![ModifierStep { modifier: made, enabled: true }],
                tolerance,
                mode: EvalMode::Inline,
            };
            Ok(StructureCommand::CreateModifier { chain })
        }
        StructureEdit::CreateOffsetModifier { distance } => {
            let source = s.selection.single().filter(|id| snapshot.object(*id).is_some());
            let source = source.ok_or(problem(StructureProblem::NeedsSelection))?;
            let distance = Length::from_mm(typed_position(&distance)?).map_err(|_| MessageKey::CommandRejected)?;
            let tolerance = crate::commit::boolean_options().map(|o| o.tolerance).ok_or(MessageKey::CommandRejected)?;
            let chain = ModifierChain {
                name: "Offset".to_owned(),
                source,
                steps: vec![ModifierStep {
                    modifier: Modifier::Offset { distance, join: burnburn_geometry::OffsetJoin::Round },
                    enabled: true,
                }],
                tolerance,
                mode: EvalMode::Inline,
            };
            Ok(StructureCommand::CreateModifier { chain })
        }
    }
}

/// The union of the bounds of what instance `id` outputs (None: nothing was ever evaluated).
fn instance_bounds(
    snapshot: &burnburn_document::DocumentSnapshot,
    id: PartInstanceId,
) -> Option<burnburn_geometry::Bounds> {
    let owner = DerivedOwner::Part(id);
    snapshot
        .objects()
        .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == owner))
        .filter_map(|(_, o)| o.bounds())
        .reduce(|a, b| a.union(b))
}

/// Turns an instance about the middle of its outline (G8-editor): to the angle typed, or by it.
/// An angle it has already is no command.
fn rotate_instance(
    s: &mut Session,
    document: DocumentId,
    instance: u64,
    degrees: &str,
    relative: bool,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let id = PartInstanceId::from_raw(instance);
    let Some(current) = snapshot.structure().instance(id) else {
        return refused(s, document, problem(StructureProblem::UnknownInstance));
    };
    let typed = match crate::field::parse_number(degrees, &["°", "deg"]) {
        Ok(v) => v,
        Err(e) => return refused(s, document, MessageKey::InvalidLength(e)),
    };
    let now = rotation_degrees(&current.transform);
    let delta = if relative { typed } else { typed - now };
    if delta.rem_euclid(360.0).min(360.0 - delta.rem_euclid(360.0)) < 1e-9 {
        s.problem = None;
        return AppUpdate::redraw(document);
    }
    let middle =
        instance_bounds(&snapshot, id).map_or_else(|| current.transform.apply(Point::new(0.0, 0.0)), |b| b.center());
    let turn = Angle::from_degrees(delta).map(Affine::rotate).ok();
    let about = turn.and_then(|r| {
        let there = Affine::translate(Vector::new(middle.x, middle.y)).ok()?;
        let back = Affine::translate(Vector::new(-middle.x, -middle.y)).ok()?;
        Some(back.then(&r).then(&there))
    });
    let Some(about) = about else { return refused(s, document, MessageKey::CommandRejected) };
    let mut next = current.clone();
    next.transform = current.transform.then(&about);
    s.problem = None;
    apply_edit(
        s,
        document,
        CommandPayload::Structure(StructureCommand::SetInstance { instance: id, value: next }),
        request,
    )
}

/// Puts every instance that is on no plate on a plate where it fits (G8-editor, P06): one undo
/// step for all of them, or a refusal that changes nothing.
fn distribute(s: &mut Session, document: DocumentId, gap: &str, request: Option<RequestId>) -> AppUpdate {
    use burnburn_document::structure::{Footprint, PlateRoom, distribute_over_plates};
    let gap = if gap.trim().is_empty() {
        0.0
    } else {
        match parse_position(gap) {
            Ok(v) if v.mm() >= 0.0 => v.mm(),
            Ok(_) => return refused(s, document, MessageKey::InvalidLength(crate::field::FieldError::NotPositive)),
            Err(e) => return refused(s, document, MessageKey::InvalidLength(e)),
        }
    };
    let snapshot = s.doc.snapshot();
    let layer = snapshot.structure();
    let plates: Vec<PlateRoom> =
        layer.plates().map(|(id, p)| PlateRoom { id, width: p.width.mm(), height: p.height.mm() }).collect();
    if plates.is_empty() {
        return refused(s, document, problem(StructureProblem::NoPlates));
    }
    if layer.instances().all(|(_, i)| i.plate.is_some()) {
        return refused(s, document, problem(StructureProblem::NothingToDistribute));
    }
    let items: Vec<Footprint> = layer
        .instances()
        .map(|(id, inst)| {
            let (width, height) = instance_bounds(&snapshot, id).map_or((0.0, 0.0), |b| (b.width(), b.height()));
            Footprint { id, width, height, plate: inst.plate }
        })
        .collect();
    let plan = distribute_over_plates(&plates, &items, gap);
    if !plan.unplaced.is_empty() {
        return refused(s, document, problem(StructureProblem::NoRoomOnPlates));
    }
    let steps: Vec<CompoundStep> = plan
        .assigned
        .iter()
        .filter_map(|(instance, plate)| {
            let mut value = layer.instance(*instance)?.clone();
            value.plate = Some(*plate);
            Some(CompoundStep::Run(CommandPayload::Structure(StructureCommand::SetInstance {
                instance: *instance,
                value,
            })))
        })
        .collect();
    s.problem = None;
    let payload = CommandPayload::Compound(CompoundCommand { label: CompoundLabel::DistributeOverPlates, steps });
    apply_edit(s, document, payload, request)
}

/// One edit of one instance. Nothing is changed when the instance, the variant or the parameter
/// does not exist, or when the typed value is not a positive length.
fn instance_edit(s: &mut Session, document: DocumentId, edit: StructureEdit, request: Option<RequestId>) -> AppUpdate {
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
