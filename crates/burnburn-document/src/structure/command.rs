use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_types::{GeometryId, IdAllocator};

use super::{
    AssocReference, GeometryRef, Modifier, ModifierChain, ModifierId, ObjectRelation, PartDefinition, PartDefinitionId,
    PartInstance, PartInstanceId, PartShape, Plate, PlateId, RefStatus, ReferenceId, RelationId, StructureLayer,
    resolve_in,
};
use crate::{DerivedKey, DerivedOwner, GeometryObject, GeometrySource};

/// Edits of the document structure. Each is one transaction (one undo step); derived outputs
/// follow in the same transaction.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StructureCommand {
    CreateModifier {
        chain: ModifierChain,
    },
    SetModifier {
        modifier: ModifierId,
        chain: ModifierChain,
    },
    /// Enables or disables one step; nothing else changes.
    SetStepEnabled {
        modifier: ModifierId,
        step: usize,
        enabled: bool,
    },
    /// Removes the chain and its outputs; the source is untouched.
    RemoveModifier {
        modifier: ModifierId,
    },
    /// The target must be bound when the reference is made.
    AddReference {
        reference: AssocReference,
    },
    /// The user's explicit choice of a new target (the only way a reference is rebound).
    RebindReference {
        reference: ReferenceId,
        target: GeometryRef,
    },
    RemoveReference {
        reference: ReferenceId,
    },
    AddRelation {
        relation: ObjectRelation,
    },
    RemoveRelation {
        relation: RelationId,
    },
    DefinePart {
        definition: PartDefinition,
    },
    /// New content for a definition: every instance follows in the same transaction.
    SetPartDefinition {
        definition: PartDefinitionId,
        value: PartDefinition,
    },
    /// Only when no instance uses it.
    RemovePartDefinition {
        definition: PartDefinitionId,
    },
    PlaceInstance {
        instance: PartInstance,
    },
    SetInstance {
        instance: PartInstanceId,
        value: PartInstance,
    },
    RemoveInstance {
        instance: PartInstanceId,
    },
    CreatePlate {
        plate: Plate,
    },
    SetPlate {
        plate: PlateId,
        value: Plate,
    },
    /// Removes the plate; its objects and instances become unassigned.
    RemovePlate {
        plate: PlateId,
    },
    /// Moves objects to `plate` (or off any plate). Part outputs move with their instance.
    AssignToPlate {
        objects: Vec<GeometryId>,
        plate: Option<PlateId>,
    },
    /// A nesting result applied to one plate (IM26, one undo step): every listed instance gets
    /// its new transform (part frame → plate/document frame) and is assigned to `plate`.
    /// Instances that are not listed (unplaced parts) are untouched.
    ApplyNest {
        plate: PlateId,
        placements: Vec<(PartInstanceId, burnburn_geometry::Affine)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StructureLabel {
    CreateModifier,
    EditModifier,
    ToggleModifierStep,
    RemoveModifier,
    AddReference,
    RebindReference,
    RemoveReference,
    AddRelation,
    RemoveRelation,
    DefinePart,
    EditPart,
    RemovePart,
    PlaceInstance,
    EditInstance,
    RemoveInstance,
    CreatePlate,
    EditPlate,
    RemovePlate,
    AssignToPlate,
    ApplyNest,
}

impl StructureCommand {
    pub fn label(&self) -> StructureLabel {
        use StructureCommand as C;
        use StructureLabel as L;
        match self {
            C::CreateModifier { .. } => L::CreateModifier,
            C::SetModifier { .. } => L::EditModifier,
            C::SetStepEnabled { .. } => L::ToggleModifierStep,
            C::RemoveModifier { .. } => L::RemoveModifier,
            C::AddReference { .. } => L::AddReference,
            C::RebindReference { .. } => L::RebindReference,
            C::RemoveReference { .. } => L::RemoveReference,
            C::AddRelation { .. } => L::AddRelation,
            C::RemoveRelation { .. } => L::RemoveRelation,
            C::DefinePart { .. } => L::DefinePart,
            C::SetPartDefinition { .. } => L::EditPart,
            C::RemovePartDefinition { .. } => L::RemovePart,
            C::PlaceInstance { .. } => L::PlaceInstance,
            C::SetInstance { .. } => L::EditInstance,
            C::RemoveInstance { .. } => L::RemoveInstance,
            C::CreatePlate { .. } => L::CreatePlate,
            C::SetPlate { .. } => L::EditPlate,
            C::RemovePlate { .. } => L::RemovePlate,
            C::AssignToPlate { .. } => L::AssignToPlate,
            C::ApplyNest { .. } => L::ApplyNest,
        }
    }
}

/// What a structure command created, changed or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StructureChange {
    Modifier(ModifierId),
    Reference(ReferenceId),
    Relation(RelationId),
    Definition(PartDefinitionId),
    Instance(PartInstanceId),
    Plate(PlateId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StructureError {
    UnknownModifier(ModifierId),
    UnknownStep {
        modifier: ModifierId,
        step: usize,
    },
    UnknownReference(ReferenceId),
    UnknownRelation(RelationId),
    UnknownDefinition(PartDefinitionId),
    UnknownInstance(PartInstanceId),
    UnknownPlate(PlateId),
    UnknownGeometry(GeometryId),
    UnknownOwner(DerivedOwner),
    UnknownParameter(String),
    UnknownVariant(String),
    /// A new reference must point at something that resolves now: broken (with the reason) or
    /// ambiguous (`None`).
    NotBound(Option<super::BreakReason>),
    /// The chain would read its own output.
    SelfReference(GeometryId),
    /// The chain would (indirectly) read its own output through other chains.
    ReferenceCycle(ModifierId),
    DefinitionInUse {
        definition: PartDefinitionId,
        instance: PartInstanceId,
    },
    /// Part outputs belong to their instance's plate; assign the instance instead.
    PartOutputOnPlate(GeometryId),
    InvalidValue {
        field: &'static str,
    },
    /// A stored id is duplicated.
    InvalidId(u64),
    IdsExhausted,
    /// The command would not change anything.
    NothingChanged,
}

impl fmt::Display for StructureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StructureError::UnknownModifier(m) => write!(f, "no modifier {m}"),
            StructureError::UnknownStep { modifier, step } => write!(f, "modifier {modifier} has no step {step}"),
            StructureError::UnknownReference(r) => write!(f, "no reference {r}"),
            StructureError::UnknownRelation(r) => write!(f, "no relation {r}"),
            StructureError::UnknownDefinition(d) => write!(f, "no part definition {d}"),
            StructureError::UnknownInstance(i) => write!(f, "no part instance {i}"),
            StructureError::UnknownPlate(p) => write!(f, "no plate {p}"),
            StructureError::UnknownGeometry(g) => write!(f, "no geometry object {g}"),
            StructureError::UnknownOwner(o) => write!(f, "no derived-geometry owner {o:?}"),
            StructureError::UnknownParameter(p) => write!(f, "no parameter '{p}'"),
            StructureError::UnknownVariant(v) => write!(f, "no variant '{v}'"),
            StructureError::NotBound(s) => write!(f, "the reference does not resolve: {s:?}"),
            StructureError::SelfReference(g) => write!(f, "{g} is this chain's own output"),
            StructureError::ReferenceCycle(m) => write!(f, "modifier {m} would depend on its own output"),
            StructureError::DefinitionInUse { definition, instance } => {
                write!(f, "part definition {definition} is used by {instance}")
            }
            StructureError::PartOutputOnPlate(g) => write!(f, "{g} follows its part instance's plate"),
            StructureError::InvalidValue { field } => write!(f, "invalid {field}"),
            StructureError::InvalidId(raw) => write!(f, "invalid stored id {raw}"),
            StructureError::IdsExhausted => f.write_str("ids exhausted"),
            StructureError::NothingChanged => f.write_str("nothing changed"),
        }
    }
}

impl std::error::Error for StructureError {}

/// What a structure command may touch.
pub(crate) struct StructureTxn<'a> {
    pub layer: &'a mut StructureLayer,
    pub ids: &'a mut IdAllocator,
    pub objects: &'a BTreeMap<GeometryId, Arc<GeometryObject>>,
    pub changed: &'a mut Vec<StructureChange>,
}

/// What the session must do after a structure command.
pub(crate) enum StructureEffect {
    None,
    /// The owner is gone: remove its derived outputs and records.
    RemoveOwner(DerivedOwner),
}

fn positive(v: f64, field: &'static str) -> Result<(), StructureError> {
    if v.is_finite() && v > 0.0 { Ok(()) } else { Err(StructureError::InvalidValue { field }) }
}

pub(crate) fn check_definition(d: &PartDefinition) -> Result<(), StructureError> {
    let param = |p: &str| {
        if d.parameters.contains_key(p) { Ok(()) } else { Err(StructureError::UnknownParameter(p.to_owned())) }
    };
    if d.parameters.keys().any(|k| k.trim().is_empty()) {
        return Err(StructureError::InvalidValue { field: "parameter name" });
    }
    let check = |dim: &super::Dim, field: &'static str, must_be_positive: bool| match dim {
        super::Dim::Mm(v) if must_be_positive => positive(*v, field),
        super::Dim::Mm(v) if v.is_finite() => Ok(()),
        super::Dim::Mm(_) => Err(StructureError::InvalidValue { field }),
        super::Dim::Param(p) => param(p),
    };
    for s in d.shapes.values() {
        match &s.shape {
            PartShape::Rectangle { x, y, width, height } => {
                check(x, "x", false)?;
                check(y, "y", false)?;
                check(width, "width", true)?;
                check(height, "height", true)?;
            }
            PartShape::Circle { x, y, radius } => {
                check(x, "x", false)?;
                check(y, "y", false)?;
                check(radius, "radius", true)?;
            }
            PartShape::Fixed(_) => {}
        }
    }
    for v in d.variants.values() {
        if let Some(p) = v.keys().find(|p| !d.parameters.contains_key(*p)) {
            return Err(StructureError::UnknownParameter(p.clone()));
        }
    }
    Ok(())
}

fn check_chain(
    layer: &StructureLayer,
    objects: &BTreeMap<GeometryId, Arc<GeometryObject>>,
    own: Option<ModifierId>,
    chain: &ModifierChain,
) -> Result<(), StructureError> {
    for g in chain.inputs() {
        let obj = objects.get(&g).ok_or(StructureError::UnknownGeometry(g))?;
        if let GeometrySource::Derived(DerivedKey { owner: DerivedOwner::Modifier(m), .. }) = obj.source()
            && let Some(own) = own
        {
            if *m == own {
                return Err(StructureError::SelfReference(g));
            }
            if depends_on(layer, objects, *m, own) {
                return Err(StructureError::ReferenceCycle(own));
            }
        }
    }
    for s in &chain.steps {
        let is_array =
            matches!(s.modifier, Modifier::LinearArray { .. } | Modifier::GridArray(_) | Modifier::CircularArray(_));
        if is_array && super::modifier::array_transforms(&s.modifier).is_none() {
            return Err(StructureError::InvalidValue { field: "array" });
        }
    }
    Ok(())
}

/// The chains whose outputs chain `m` reads.
pub(crate) fn chain_deps(
    layer: &StructureLayer,
    objects: &BTreeMap<GeometryId, Arc<GeometryObject>>,
    m: ModifierId,
) -> Vec<ModifierId> {
    let Some(chain) = layer.modifiers.get(&m) else { return Vec::new() };
    let mut out: Vec<ModifierId> = chain
        .inputs()
        .into_iter()
        .filter_map(|g| match objects.get(&g)?.source() {
            GeometrySource::Derived(DerivedKey { owner: DerivedOwner::Modifier(d), .. }) => Some(*d),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Whether chain `from` (transitively) reads the output of chain `target`.
fn depends_on(
    layer: &StructureLayer,
    objects: &BTreeMap<GeometryId, Arc<GeometryObject>>,
    from: ModifierId,
    target: ModifierId,
) -> bool {
    let mut stack = vec![from];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(m) = stack.pop() {
        if m == target {
            return true;
        }
        if seen.insert(m) {
            stack.extend(chain_deps(layer, objects, m));
        }
    }
    false
}

/// Evaluation order of all chains: dependencies first, otherwise by id (deterministic).
/// Cycles cannot be stored (refused on edit); a cycle in loaded data is evaluated in id order.
pub(crate) fn chain_order(
    layer: &StructureLayer,
    objects: &BTreeMap<GeometryId, Arc<GeometryObject>>,
) -> Vec<ModifierId> {
    fn visit(
        m: ModifierId,
        layer: &StructureLayer,
        objects: &BTreeMap<GeometryId, Arc<GeometryObject>>,
        state: &mut BTreeMap<ModifierId, bool>,
        out: &mut Vec<ModifierId>,
    ) {
        if state.contains_key(&m) {
            return;
        }
        state.insert(m, false);
        for d in chain_deps(layer, objects, m) {
            visit(d, layer, objects, state, out);
        }
        state.insert(m, true);
        out.push(m);
    }
    let mut state = BTreeMap::new();
    let mut out = Vec::new();
    for m in layer.modifiers.keys() {
        visit(*m, layer, objects, &mut state, &mut out);
    }
    out
}

fn not_bound(status: RefStatus) -> Result<(), StructureError> {
    match status {
        RefStatus::Bound(_) => Ok(()),
        RefStatus::Broken { reason } => Err(StructureError::NotBound(Some(reason))),
        RefStatus::Ambiguous { .. } => Err(StructureError::NotBound(None)),
    }
}

fn allocate<I: burnburn_types::StableId>(ids: &mut IdAllocator) -> Result<I, StructureError> {
    ids.allocate().map_err(|_| StructureError::IdsExhausted)
}

pub(crate) fn apply(cmd: StructureCommand, t: StructureTxn<'_>) -> Result<StructureEffect, StructureError> {
    use StructureCommand as C;
    let StructureTxn { layer, ids, objects, changed } = t;
    let lookup = |g: GeometryId| objects.get(&g).map(|o| o.as_ref());
    match cmd {
        C::CreateModifier { chain } => {
            check_chain(layer, objects, None, &chain)?;
            let id: ModifierId = allocate(ids)?;
            layer.modifiers.insert(id, Arc::new(chain));
            changed.push(StructureChange::Modifier(id));
        }
        C::SetModifier { modifier, chain } => {
            let slot = layer.modifiers.get(&modifier).ok_or(StructureError::UnknownModifier(modifier))?;
            if **slot == chain {
                return Err(StructureError::NothingChanged);
            }
            check_chain(layer, objects, Some(modifier), &chain)?;
            layer.modifiers.insert(modifier, Arc::new(chain));
            changed.push(StructureChange::Modifier(modifier));
        }
        C::SetStepEnabled { modifier, step, enabled } => {
            let slot = layer.modifiers.get(&modifier).ok_or(StructureError::UnknownModifier(modifier))?;
            let mut next = (**slot).clone();
            let s = next.steps.get_mut(step).ok_or(StructureError::UnknownStep { modifier, step })?;
            if s.enabled == enabled {
                return Err(StructureError::NothingChanged);
            }
            s.enabled = enabled;
            layer.modifiers.insert(modifier, Arc::new(next));
            changed.push(StructureChange::Modifier(modifier));
        }
        C::RemoveModifier { modifier } => {
            layer.modifiers.remove(&modifier).ok_or(StructureError::UnknownModifier(modifier))?;
            changed.push(StructureChange::Modifier(modifier));
            return Ok(StructureEffect::RemoveOwner(DerivedOwner::Modifier(modifier)));
        }
        C::AddReference { reference } => {
            let status = resolve_in(layer, &lookup, &reference.target);
            not_bound(status)?;
            let id: ReferenceId = allocate(ids)?;
            layer.references.insert(id, reference);
            changed.push(StructureChange::Reference(id));
        }
        C::RebindReference { reference, target } => {
            let status = resolve_in(layer, &lookup, &target);
            not_bound(status)?;
            let r = layer.references.get_mut(&reference).ok_or(StructureError::UnknownReference(reference))?;
            if r.target == target {
                return Err(StructureError::NothingChanged);
            }
            r.target = target;
            changed.push(StructureChange::Reference(reference));
        }
        C::RemoveReference { reference } => {
            layer.references.remove(&reference).ok_or(StructureError::UnknownReference(reference))?;
            // A relation without its reference is meaningless: it goes in the same step.
            let gone: Vec<RelationId> = layer
                .relations
                .iter()
                .filter(|(_, r)| r.references().contains(&reference))
                .map(|(id, _)| *id)
                .collect();
            for r in gone {
                layer.relations.remove(&r);
                changed.push(StructureChange::Relation(r));
            }
            changed.push(StructureChange::Reference(reference));
        }
        C::AddRelation { relation } => {
            if let Some(missing) = relation.references().into_iter().find(|r| !layer.references.contains_key(r)) {
                return Err(StructureError::UnknownReference(missing));
            }
            let id: RelationId = allocate(ids)?;
            layer.relations.insert(id, relation);
            changed.push(StructureChange::Relation(id));
        }
        C::RemoveRelation { relation } => {
            layer.relations.remove(&relation).ok_or(StructureError::UnknownRelation(relation))?;
            changed.push(StructureChange::Relation(relation));
        }
        C::DefinePart { definition } => {
            check_definition(&definition)?;
            let id: PartDefinitionId = allocate(ids)?;
            layer.definitions.insert(id, Arc::new(definition));
            changed.push(StructureChange::Definition(id));
        }
        C::SetPartDefinition { definition, value } => {
            let slot = layer.definitions.get(&definition).ok_or(StructureError::UnknownDefinition(definition))?;
            if **slot == value {
                return Err(StructureError::NothingChanged);
            }
            check_definition(&value)?;
            // Instances must stay valid against the new definition (variants, overrides).
            let old = layer.definitions.insert(definition, Arc::new(value));
            let users: Vec<PartInstance> =
                layer.instances.values().filter(|i| i.definition == definition).cloned().collect();
            if let Err(e) = users.iter().try_for_each(|i| layer.check_instance(i)) {
                if let Some(old) = old {
                    layer.definitions.insert(definition, old);
                }
                return Err(e);
            }
            changed.push(StructureChange::Definition(definition));
        }
        C::RemovePartDefinition { definition } => {
            if let Some((instance, _)) = layer.instances.iter().find(|(_, i)| i.definition == definition) {
                return Err(StructureError::DefinitionInUse { definition, instance: *instance });
            }
            layer.definitions.remove(&definition).ok_or(StructureError::UnknownDefinition(definition))?;
            changed.push(StructureChange::Definition(definition));
        }
        C::PlaceInstance { instance } => {
            layer.check_instance(&instance)?;
            let id: PartInstanceId = allocate(ids)?;
            layer.instances.insert(id, instance);
            changed.push(StructureChange::Instance(id));
        }
        C::SetInstance { instance, value } => {
            let current = layer.instances.get(&instance).ok_or(StructureError::UnknownInstance(instance))?;
            if *current == value {
                return Err(StructureError::NothingChanged);
            }
            layer.check_instance(&value)?;
            layer.instances.insert(instance, value);
            changed.push(StructureChange::Instance(instance));
        }
        C::RemoveInstance { instance } => {
            layer.instances.remove(&instance).ok_or(StructureError::UnknownInstance(instance))?;
            changed.push(StructureChange::Instance(instance));
            return Ok(StructureEffect::RemoveOwner(DerivedOwner::Part(instance)));
        }
        C::CreatePlate { plate } => {
            positive(plate.width.mm(), "plate width")?;
            positive(plate.height.mm(), "plate height")?;
            let id: PlateId = allocate(ids)?;
            layer.plates.insert(id, plate);
            changed.push(StructureChange::Plate(id));
        }
        C::SetPlate { plate, value } => {
            positive(value.width.mm(), "plate width")?;
            positive(value.height.mm(), "plate height")?;
            let slot = layer.plates.get_mut(&plate).ok_or(StructureError::UnknownPlate(plate))?;
            if *slot == value {
                return Err(StructureError::NothingChanged);
            }
            *slot = value;
            changed.push(StructureChange::Plate(plate));
        }
        C::RemovePlate { plate } => {
            layer.plates.remove(&plate).ok_or(StructureError::UnknownPlate(plate))?;
            layer.placement.retain(|_, p| *p != plate);
            for (id, inst) in layer.instances.iter_mut() {
                if inst.plate == Some(plate) {
                    inst.plate = None;
                    changed.push(StructureChange::Instance(*id));
                }
            }
            changed.push(StructureChange::Plate(plate));
        }
        C::AssignToPlate { objects: targets, plate } => {
            if let Some(p) = plate
                && !layer.plates.contains_key(&p)
            {
                return Err(StructureError::UnknownPlate(p));
            }
            let mut any = false;
            for g in targets {
                let obj = objects.get(&g).ok_or(StructureError::UnknownGeometry(g))?;
                if matches!(obj.source(), GeometrySource::Derived(DerivedKey { owner: DerivedOwner::Part(_), .. })) {
                    return Err(StructureError::PartOutputOnPlate(g));
                }
                let before = match plate {
                    Some(p) => layer.placement.insert(g, p),
                    None => layer.placement.remove(&g),
                };
                any |= before != plate;
            }
            if !any {
                return Err(StructureError::NothingChanged);
            }
            if let Some(p) = plate {
                changed.push(StructureChange::Plate(p));
            }
        }
        C::ApplyNest { plate, placements } => {
            if !layer.plates.contains_key(&plate) {
                return Err(StructureError::UnknownPlate(plate));
            }
            if placements.is_empty() {
                return Err(StructureError::NothingChanged);
            }
            let mut seen = std::collections::BTreeSet::new();
            for (i, _) in &placements {
                if !layer.instances.contains_key(i) {
                    return Err(StructureError::UnknownInstance(*i));
                }
                if !seen.insert(*i) {
                    return Err(StructureError::InvalidValue { field: "placements" });
                }
            }
            let mut any = false;
            for (i, transform) in placements {
                if let Some(inst) = layer.instances.get_mut(&i)
                    && (inst.transform != transform || inst.plate != Some(plate))
                {
                    inst.transform = transform;
                    inst.plate = Some(plate);
                    changed.push(StructureChange::Instance(i));
                    any = true;
                }
            }
            if !any {
                return Err(StructureError::NothingChanged);
            }
            changed.push(StructureChange::Plate(plate));
        }
    }
    Ok(StructureEffect::None)
}
