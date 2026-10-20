use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_types::{GeometryId, IdAllocator, RevisionId, StableId};

use crate::engine::{EngineError, EngineLayer};
use crate::generator::{GeneratorDefinition, GeneratorId};
use crate::process::{ProcessError, ProcessLayer};
use crate::sketch::{Sketch, SketchError, SketchId, SketchLayer};
use crate::structure::{
    GeometryRef, ObjectRelation, PlateId, PlateIssue, RefStatus, ReferenceId, RelationId, RelationState,
    StructureError, StructureLayer, relation_state_in, resolve_in,
};
use crate::text::{TextError, TextId, TextLayer, TextObject};
use crate::trace::{TraceDefinition, TraceId};
use crate::{ArrangeLayer, GeometryObject};
use crate::{DerivedOwner, DerivedRecord, EvalStatus, GeometrySource};

pub(crate) type Objects = Arc<BTreeMap<GeometryId, Arc<GeometryObject>>>;

/// An immutable document state. Cloning is cheap (shared `Arc`s); safe to send to workers.
#[derive(Debug, Clone)]
pub struct DocumentSnapshot {
    revision: RevisionId,
    pub(crate) objects: Objects,
    pub(crate) next_id: u64,
    /// Operations, order constraints and recipes; refers to `objects` by id only.
    pub(crate) process: Arc<ProcessLayer>,
    /// Sketch definitions and per-owner derived-output state (IM11).
    pub(crate) sketches: Arc<SketchLayer>,
    /// Modifiers, references, parts and plates (IM21).
    pub(crate) structure: Arc<StructureLayer>,
    /// Text sources and their evaluation records (IM18, DR-22).
    pub(crate) texts: Arc<TextLayer>,
    /// Trace and generator definitions and their evaluation records (C-9, IM22).
    pub(crate) engines: Arc<EngineLayer>,
    /// Draw order and locks (W2-L1-bewerken).
    pub(crate) arrange: Arc<ArrangeLayer>,
}

/// The part of a document that one plate compiles (P06): only the plate's objects, operations
/// restricted to them, and what had to be left out or must block production.
#[derive(Debug, Clone)]
pub struct PlateView {
    pub plate: PlateId,
    pub snapshot: DocumentSnapshot,
    pub issues: Vec<PlateIssue>,
}

/// Why parts (for example from a loaded file) do not form a valid snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    DuplicateId(GeometryId),
    /// An object id is not below the persisted next-id counter, so it could be handed out again.
    IdNotBelowNext {
        id: GeometryId,
        next_id: u64,
    },
    /// The process layer is inconsistent with itself or with the objects.
    Process(ProcessError),
    /// The sketch layer is inconsistent with itself or with the objects.
    Sketch(SketchError),
    /// The structure layer is inconsistent with itself or with the objects (IM21).
    Structure(StructureError),
    /// The text layer is inconsistent with itself or with the objects (IM18).
    Text(TextError),
    /// The trace/generator layer is inconsistent with itself or with the objects.
    Engine(EngineError),
    /// The arrangement layer names an object that does not exist.
    Arrange(GeometryId),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::DuplicateId(id) => write!(f, "duplicate geometry id {id}"),
            SnapshotError::IdNotBelowNext { id, next_id } => {
                write!(f, "geometry id {id} is not below the next id {next_id}")
            }
            SnapshotError::Process(e) => write!(f, "process layer: {e}"),
            SnapshotError::Sketch(e) => write!(f, "sketch layer: {e}"),
            SnapshotError::Structure(e) => write!(f, "structure layer: {e}"),
            SnapshotError::Text(e) => write!(f, "text layer: {e}"),
            SnapshotError::Engine(e) => write!(f, "trace/generator layer: {e}"),
            SnapshotError::Arrange(id) => write!(f, "arrangement names unknown object {id}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl DocumentSnapshot {
    pub(crate) fn empty() -> Self {
        DocumentSnapshot {
            revision: RevisionId::INITIAL,
            objects: Arc::default(),
            next_id: IdAllocator::new().peek_next(),
            process: Arc::default(),
            sketches: Arc::default(),
            structure: Arc::default(),
            texts: Arc::default(),
            engines: Arc::default(),
            arrange: Arc::default(),
        }
    }

    /// Rebuilds a snapshot from stored parts (open/recovery). `next_id` is the persisted
    /// allocator position; every id must be below it so ids are never reused.
    pub fn from_parts(objects: Vec<GeometryObject>, next_id: u64) -> Result<Self, SnapshotError> {
        let mut map = BTreeMap::new();
        for obj in objects {
            let id = obj.id();
            if id.raw() >= next_id {
                return Err(SnapshotError::IdNotBelowNext { id, next_id });
            }
            if map.insert(id, Arc::new(obj)).is_some() {
                return Err(SnapshotError::DuplicateId(id));
            }
        }
        Ok(DocumentSnapshot {
            revision: RevisionId::INITIAL,
            objects: Arc::new(map),
            next_id,
            process: Arc::default(),
            sketches: Arc::default(),
            structure: Arc::default(),
            texts: Arc::default(),
            engines: Arc::default(),
            arrange: Arc::default(),
        })
    }

    /// The same objects with a stored process layer (open/recovery). Every operation/recipe id
    /// must be unique, below `next_id` and distinct from geometry ids; every target must exist.
    pub fn with_process(self, process: ProcessLayer) -> Result<Self, SnapshotError> {
        let invalid = |raw| SnapshotError::Process(ProcessError::InvalidId { raw });
        for raw in process.ids() {
            if raw >= self.next_id || self.objects.contains_key(&GeometryId::from_raw(raw)) {
                return Err(invalid(raw));
            }
        }
        for op in process.operations() {
            if let Some(g) = op.targets().iter().find(|g| !self.objects.contains_key(g)) {
                return Err(SnapshotError::Process(ProcessError::UnknownGeometry(*g)));
            }
        }
        Ok(DocumentSnapshot { process: Arc::new(process), ..self })
    }

    /// The same snapshot with a stored sketch layer (open/recovery). Sketch ids must be below
    /// `next_id` and distinct from other ids; derived objects must belong to a stored sketch.
    pub fn with_sketches(self, sketches: SketchLayer) -> Result<Self, SnapshotError> {
        for (id, _) in sketches.sketches() {
            let raw = id.raw();
            let taken = self.objects.contains_key(&GeometryId::from_raw(raw)) || self.process.ids().any(|p| p == raw);
            if raw >= self.next_id || taken {
                return Err(SnapshotError::Sketch(SketchError::UnknownSketch(id)));
            }
        }
        for obj in self.objects.values() {
            if let GeometrySource::Derived(key) = obj.source()
                && let DerivedOwner::Sketch(id) = key.owner
                && sketches.sketch(id).is_none()
            {
                return Err(SnapshotError::Sketch(SketchError::UnknownSketch(id)));
            }
        }
        Ok(DocumentSnapshot { sketches: Arc::new(sketches), ..self })
    }

    /// The same snapshot with a stored structure layer (open/recovery; after `with_sketches`).
    /// Ids must be below `next_id` and unused by other entities; derived objects of modifiers
    /// and parts must belong to a stored chain/instance; placed objects must exist.
    pub fn with_structure(self, structure: StructureLayer) -> Result<Self, SnapshotError> {
        let invalid = |raw| SnapshotError::Structure(StructureError::InvalidId(raw));
        for raw in structure.ids() {
            let taken = self.objects.contains_key(&GeometryId::from_raw(raw))
                || self.process.ids().any(|p| p == raw)
                || self.sketches.sketches().any(|(s, _)| s.raw() == raw);
            if raw >= self.next_id || taken {
                return Err(invalid(raw));
            }
        }
        for obj in self.objects.values() {
            if let GeometrySource::Derived(key) = obj.source() {
                let known = match key.owner {
                    DerivedOwner::Modifier(m) => structure.modifier(m).is_some(),
                    DerivedOwner::Part(i) => structure.instance(i).is_some(),
                    _ => true,
                };
                if !known {
                    return Err(SnapshotError::Structure(StructureError::UnknownOwner(key.owner)));
                }
            }
        }
        for (g, _) in structure.placements() {
            if !self.objects.contains_key(&g) {
                return Err(SnapshotError::Structure(StructureError::UnknownGeometry(g)));
            }
        }
        Ok(DocumentSnapshot { structure: Arc::new(structure), ..self })
    }

    /// The same snapshot with a stored text layer (open/recovery). Text ids must be below
    /// `next_id` and unused by other entities; derived objects of texts must belong to a stored text.
    pub fn with_texts(self, texts: TextLayer) -> Result<Self, SnapshotError> {
        for (id, _) in texts.texts() {
            let raw = id.raw();
            let taken = self.objects.contains_key(&GeometryId::from_raw(raw))
                || self.process.ids().any(|p| p == raw)
                || self.sketches.sketches().any(|(s, _)| s.raw() == raw)
                || self.structure.ids().any(|s| s == raw);
            if raw >= self.next_id || taken {
                return Err(SnapshotError::Text(TextError::UnknownText(id)));
            }
        }
        for obj in self.objects.values() {
            if let GeometrySource::Derived(key) = obj.source()
                && let DerivedOwner::Text(id) = key.owner
                && texts.text(id).is_none()
            {
                return Err(SnapshotError::Text(TextError::UnknownText(id)));
            }
        }
        Ok(DocumentSnapshot { texts: Arc::new(texts), ..self })
    }

    /// The same snapshot with a stored trace/generator layer (open/recovery; after the other
    /// layers). Ids must be below `next_id` and unused by other entities; derived objects of
    /// traces and generators must belong to a stored definition.
    pub fn with_engines(self, engines: EngineLayer) -> Result<Self, SnapshotError> {
        for raw in engines.ids() {
            let taken = self.objects.contains_key(&GeometryId::from_raw(raw))
                || self.process.ids().any(|p| p == raw)
                || self.sketches.sketches().any(|(s, _)| s.raw() == raw)
                || self.structure.ids().any(|s| s == raw)
                || self.texts.texts().any(|(t, _)| t.raw() == raw);
            if raw >= self.next_id || taken {
                return Err(SnapshotError::Engine(EngineError::InvalidRecord(raw)));
            }
        }
        for obj in self.objects.values() {
            if let GeometrySource::Derived(key) = obj.source() {
                let missing = match key.owner {
                    DerivedOwner::Trace(t) => engines.trace(t).is_none(),
                    DerivedOwner::Generator(g) => engines.generator(g).is_none(),
                    _ => false,
                };
                if missing {
                    return Err(SnapshotError::Engine(EngineError::InvalidRecord(obj.id().raw())));
                }
            }
        }
        Ok(DocumentSnapshot { engines: Arc::new(engines), ..self })
    }

    pub(crate) fn from_txn(
        revision: RevisionId,
        objects: Objects,
        next_id: u64,
        process: Arc<ProcessLayer>,
        sketches: Arc<SketchLayer>,
        structure: Arc<StructureLayer>,
        texts: Arc<TextLayer>,
        engines: Arc<EngineLayer>,
    ) -> Self {
        DocumentSnapshot {
            revision,
            objects,
            next_id,
            process,
            sketches,
            structure,
            texts,
            engines,
            arrange: Arc::default(),
        }
    }

    /// The same snapshot with another arrangement layer (transactions and undo).
    pub(crate) fn with_arrange_layer(self, arrange: Arc<ArrangeLayer>) -> Self {
        DocumentSnapshot { arrange, ..self }
    }

    /// The same snapshot with a stored arrangement layer (open/recovery). Every object it names
    /// must exist.
    pub fn with_arrange(self, arrange: ArrangeLayer) -> Result<Self, SnapshotError> {
        for id in arrange.order().iter().copied().chain(arrange.locked()).chain(arrange.hidden()) {
            if !self.objects.contains_key(&id) {
                return Err(SnapshotError::Arrange(id));
            }
        }
        Ok(DocumentSnapshot { arrange: Arc::new(arrange), ..self })
    }

    /// Draw order and locks.
    pub fn arrange(&self) -> &ArrangeLayer {
        &self.arrange
    }

    /// All objects back to front: the order they are drawn in (not the cut order).
    pub fn draw_order(&self) -> Vec<GeometryId> {
        self.arrange.draw_order(self.objects.keys().copied())
    }

    /// Locked objects are not picked on the canvas and are not moved by it.
    pub fn is_locked(&self, id: GeometryId) -> bool {
        self.arrange.is_locked(id)
    }

    /// Hidden objects are not drawn or picked on the canvas; they stay part of the job.
    pub fn is_hidden(&self, id: GeometryId) -> bool {
        self.arrange.is_hidden(id)
    }

    /// Visible and unlocked: what a canvas click, a rubber band or Select All may pick.
    pub fn is_selectable(&self, id: GeometryId) -> bool {
        !self.is_locked(id) && !self.is_hidden(id)
    }

    /// Trace and generator definitions with their evaluation records (C-9, IM22).
    pub fn engine_layer(&self) -> &EngineLayer {
        &self.engines
    }

    pub fn trace(&self, id: TraceId) -> Option<&TraceDefinition> {
        self.engines.trace(id)
    }

    pub fn generator(&self, id: GeneratorId) -> Option<&GeneratorDefinition> {
        self.engines.generator(id)
    }

    /// Text sources, evaluation records and missing glyphs (IM18).
    pub fn text_layer(&self) -> &TextLayer {
        &self.texts
    }

    pub fn text(&self, id: TextId) -> Option<&TextObject> {
        self.texts.text(id)
    }

    /// Modifiers, references, relations, parts and plates (IM21).
    pub fn structure(&self) -> &StructureLayer {
        &self.structure
    }

    /// Current status of a stored reference (`None`: no such reference).
    pub fn reference_status(&self, id: ReferenceId) -> Option<RefStatus> {
        let r = self.structure.reference(id)?;
        Some(self.resolve(&r.target))
    }

    /// Resolves any associative reference against this snapshot.
    pub fn resolve(&self, target: &GeometryRef) -> RefStatus {
        resolve_in(&self.structure, &|g| self.object(g), target)
    }

    /// The associative name of the edge of `object` that contains `segment` (what a pick on the
    /// canvas turns into a reference). `None` when the edge has no origin (generated geometry).
    pub fn edge_ref(&self, object: GeometryId, segment: burnburn_geometry::SegmentRef) -> Option<GeometryRef> {
        let obj = self.object(object)?;
        let name = self.structure.edge_name(obj, segment)?;
        Some(GeometryRef { object, sub: Some(crate::structure::SubRef::Edge(name)) })
    }

    /// Checked state of a stored relation.
    pub fn relation_state(&self, id: RelationId) -> Option<RelationState> {
        let r: &ObjectRelation = self.structure.relation(id)?;
        Some(relation_state_in(&self.structure, &|g| self.object(g), r))
    }

    /// The plate an object belongs to.
    pub fn plate_of(&self, id: GeometryId) -> Option<PlateId> {
        self.object(id).and_then(|o| self.structure.plate_of(o))
    }

    /// The document as plate `plate` compiles it (P06).
    pub fn plate_view(&self, plate: PlateId) -> Result<PlateView, StructureError> {
        use std::collections::BTreeSet;
        if self.structure.plate(plate).is_none() {
            return Err(StructureError::UnknownPlate(plate));
        }
        let on: BTreeMap<GeometryId, Arc<GeometryObject>> = self
            .objects
            .iter()
            .filter(|(_, o)| self.structure.plate_of(o) == Some(plate))
            .map(|(id, o)| (*id, Arc::clone(o)))
            .collect();
        let mut issues = Vec::new();
        let mut operations = Vec::new();
        let mut kept = BTreeSet::new();
        for op in self.process.operations() {
            let (inside, outside): (Vec<GeometryId>, Vec<GeometryId>) =
                op.targets().iter().partition(|g| on.contains_key(g));
            if inside.is_empty() {
                continue;
            }
            issues.extend(outside.into_iter().map(|g| PlateIssue::TargetOffPlate { operation: op.id(), geometry: g }));
            let restricted = crate::process::Operation::new(op.id(), op.draft(), inside, op.recipe())
                .map_err(|_| StructureError::InvalidValue { field: "operation" })?;
            kept.insert(op.id());
            operations.push(restricted);
        }
        let constraints = self
            .process
            .constraints()
            .iter()
            .filter(|c| kept.contains(&c.before) && kept.contains(&c.after))
            .copied()
            .collect();
        let recipes = self.process.recipes().cloned().collect();
        let process = ProcessLayer::from_parts(operations, constraints, recipes)
            .map_err(|_| StructureError::InvalidValue { field: "process" })?;
        let view = DocumentSnapshot::from_txn(
            self.revision,
            Arc::new(on),
            self.next_id,
            Arc::new(process),
            Arc::clone(&self.sketches),
            Arc::clone(&self.structure),
            Arc::clone(&self.texts),
            Arc::clone(&self.engines),
        )
        .with_arrange_layer(Arc::clone(&self.arrange));
        issues.extend(
            view.objects().filter(|(id, _)| view.production_blocked(*id)).map(|(id, _)| PlateIssue::Blocked(id)),
        );
        Ok(PlateView { plate, snapshot: view, issues })
    }

    /// Sketch definitions and derived-output records.
    pub fn sketch_layer(&self) -> &SketchLayer {
        &self.sketches
    }

    pub fn sketch(&self, id: SketchId) -> Option<&Sketch> {
        self.sketches.sketch(id)
    }

    /// Evaluation state of a derived-geometry owner.
    pub fn derived_record(&self, owner: DerivedOwner) -> Option<&DerivedRecord> {
        match owner {
            DerivedOwner::Sketch(_) => self.sketches.record(owner),
            DerivedOwner::Text(_) => self.texts.record(owner),
            DerivedOwner::Trace(_) | DerivedOwner::Generator(_) => self.engines.record(owner),
            _ => self.structure.record(owner),
        }
    }

    /// True when `id` is derived geometry whose owner's last evaluation is not valid (failed,
    /// pending or never evaluated): its geometry is the last valid result, not the current
    /// definition. CAM must refuse to export it (preview may show it).
    pub fn production_blocked(&self, id: GeometryId) -> bool {
        match self.object(id).map(|o| o.source()) {
            Some(GeometrySource::Derived(key)) => {
                !matches!(self.derived_record(key.owner), Some(DerivedRecord { status: EvalStatus::Valid, .. }))
            }
            _ => false,
        }
    }

    /// The process layer (operations reference objects of this snapshot by id).
    pub fn process(&self) -> &ProcessLayer {
        &self.process
    }

    pub fn revision(&self) -> RevisionId {
        self.revision
    }

    pub fn object(&self, id: GeometryId) -> Option<&GeometryObject> {
        self.objects.get(&id).map(|o| o.as_ref())
    }

    /// All objects, ordered by id (creation order).
    pub fn objects(&self) -> impl ExactSizeIterator<Item = (GeometryId, &GeometryObject)> {
        self.objects.iter().map(|(id, o)| (*id, o.as_ref()))
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Objects in `group` (at any nesting depth), ordered by id.
    pub fn group_members(&self, group: burnburn_types::GroupId) -> Vec<GeometryId> {
        self.objects.iter().filter(|(_, o)| o.groups().contains(&group)).map(|(id, _)| *id).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The id allocator position to persist with this snapshot.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Same objects and process layer with the same content (revision is ignored).
    pub fn same_content(&self, other: &DocumentSnapshot) -> bool {
        (Arc::ptr_eq(&self.objects, &other.objects) || self.objects == other.objects)
            && (Arc::ptr_eq(&self.process, &other.process) || self.process == other.process)
            && (Arc::ptr_eq(&self.sketches, &other.sketches) || self.sketches == other.sketches)
            && (Arc::ptr_eq(&self.structure, &other.structure) || self.structure == other.structure)
            && (Arc::ptr_eq(&self.texts, &other.texts) || self.texts == other.texts)
            && (Arc::ptr_eq(&self.engines, &other.engines) || self.engines == other.engines)
            && (Arc::ptr_eq(&self.arrange, &other.arrange) || self.arrange == other.arrange)
    }
}
