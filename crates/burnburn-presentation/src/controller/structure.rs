//! Structure panel (S06, S07, P06): what the document's structure layer knows (parts with their
//! variants and instances, plates, references) as a view model, and the instance edits a panel
//! may make. Every edit is one `StructureCommand::SetInstance`, so it is one undo step and the
//! definition is never written; an invalid variant or override fails that instance only.

use std::collections::BTreeMap;

use burnburn_document::structure::{
    BreakReason, PartInstance, PartInstanceId, PlateId, PlateIssue, RefStatus, StructureCommand, effective_parameters,
};
use burnburn_document::{CommandPayload, DerivedOwner, DocumentSnapshot, EvalDiagnosis, EvalStatus};
use burnburn_geometry::Point;
use burnburn_types::{DocumentId, Length, RequestId, StableId};

use crate::field::{format_mm, parse_length};
use crate::structure_panel::{
    BrokenReason, InstanceRow, InstanceStatus, PartRow, PartValue, PlateIssueRow, PlateRow, ReferenceRow,
    ReferenceStatus, StructureEdit, StructureViewModel,
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
    }
}

fn refused(s: &mut Session, document: DocumentId, message: MessageKey) -> AppUpdate {
    s.problem = Some(ProblemViewModel { message, object: None, at: Point::new(0.0, 0.0) });
    let mut update = error(Some(document), message);
    update.view_changed = true;
    update
}

/// One edit of one instance. Nothing is changed when the instance, the variant or the parameter
/// does not exist, or when the typed value is not a positive length.
pub(super) fn edit(
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
