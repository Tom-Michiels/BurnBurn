//! Arrangement commands (W2-L1-bewerken): draw order, lock, paste, break apart, auto-group.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use burnburn_geometry::{Affine, ParametricShape, Path, Vector};
use burnburn_types::{GeometryId, GroupId, OperationId};

use crate::process::ProcessLayer;
use crate::{ArrangeLayer, ChangeSet, GeometryObject, GeometrySource, ObjectKind};

/// How a selection moves in the draw order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OrderChange {
    /// One step towards the front, past the next object that is not selected.
    Forward,
    Backward,
    ToFront,
    ToBack,
}

/// One object of a paste: a copy of geometry as taken from a document or the clipboard.
#[derive(Debug, Clone, PartialEq)]
pub struct PasteItem {
    pub name: String,
    pub kind: ObjectKind,
    pub path: Path,
    /// Editable definition of a basic shape; dropped when it does not match `path`.
    pub parametric: Option<ParametricShape>,
    /// Source group chain, outermost first: items with the same key land in one new group.
    pub group: Vec<u32>,
    /// Operations (layers) of the source document the copy joins when they still exist.
    pub operations: Vec<OperationId>,
}

/// One extra copy of a part in a nest (C12): the objects of the part as paste items and the
/// placement of the copy (part coordinates to sheet coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct NestCopy {
    pub items: Vec<PasteItem>,
    pub transform: Affine,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ArrangeCommand {
    Order {
        targets: Vec<GeometryId>,
        change: OrderChange,
    },
    SetLocked {
        targets: Vec<GeometryId>,
        locked: bool,
    },
    /// Display flag: a hidden object is not drawn or picked, but it is still part of the job.
    SetHidden {
        targets: Vec<GeometryId>,
        hidden: bool,
    },
    /// New objects (shifted by `offset`) on top of the draw order; one history entry.
    Paste {
        items: Vec<PasteItem>,
        offset: Vector,
    },
    /// The result of a nest with several copies per part (C12), as one history entry: the
    /// originals in `moves` go to their places (each object once), and every copy becomes new
    /// objects there that keep the groups (a new group per copy) and the layers of their source.
    /// The copies are made from the objects as they were before the moves.
    NestCopies {
        moves: Vec<(GeometryId, Affine)>,
        copies: Vec<NestCopy>,
    },
    /// Every target with several subpaths becomes one object per subpath.
    BreakApart {
        targets: Vec<GeometryId>,
    },
    /// Groups shapes that lie inside each other (outer outline with its holes).
    AutoGroup {
        targets: Vec<GeometryId>,
    },
    /// Takes the outermost group off every target (one level).
    Ungroup {
        targets: Vec<GeometryId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArrangeLabel {
    DrawOrder,
    Lock,
    Unlock,
    Hide,
    Show,
    Paste,
    NestCopies,
    BreakApart,
    AutoGroup,
    Ungroup,
}

impl ArrangeCommand {
    pub fn label(&self) -> ArrangeLabel {
        match self {
            ArrangeCommand::Order { .. } => ArrangeLabel::DrawOrder,
            ArrangeCommand::SetLocked { locked: true, .. } => ArrangeLabel::Lock,
            ArrangeCommand::SetLocked { locked: false, .. } => ArrangeLabel::Unlock,
            ArrangeCommand::SetHidden { hidden: true, .. } => ArrangeLabel::Hide,
            ArrangeCommand::SetHidden { hidden: false, .. } => ArrangeLabel::Show,
            ArrangeCommand::Paste { .. } => ArrangeLabel::Paste,
            ArrangeCommand::NestCopies { .. } => ArrangeLabel::NestCopies,
            ArrangeCommand::BreakApart { .. } => ArrangeLabel::BreakApart,
            ArrangeCommand::AutoGroup { .. } => ArrangeLabel::AutoGroup,
            ArrangeCommand::Ungroup { .. } => ArrangeLabel::Ungroup,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArrangeError {
    UnknownGeometry(GeometryId),
    /// Derived geometry is regenerated from its owner.
    DerivedGeometry(GeometryId),
    /// A pasted item has no geometry.
    EmptyPath,
    /// An object appears twice in the moves of one nest.
    DuplicateTarget(GeometryId),
    /// The command would not change the document (no history entry is made).
    NothingChanged,
    IdsExhausted,
}

impl fmt::Display for ArrangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ArrangeError {}

/// The parts of the session transaction an arrangement command may touch.
pub(crate) struct ArrangeTxn<'a> {
    pub objects: &'a mut BTreeMap<GeometryId, Arc<GeometryObject>>,
    pub ids: &'a mut burnburn_types::IdAllocator,
    pub changes: &'a mut ChangeSet,
    pub process: &'a mut ProcessLayer,
    pub arrange: &'a mut ArrangeLayer,
}

fn unique(targets: &[GeometryId]) -> Vec<GeometryId> {
    let mut seen: Vec<GeometryId> = Vec::with_capacity(targets.len());
    for t in targets {
        if !seen.contains(t) {
            seen.push(*t);
        }
    }
    seen
}

impl ArrangeTxn<'_> {
    fn existing(&self, targets: &[GeometryId]) -> Result<Vec<GeometryId>, ArrangeError> {
        let targets = unique(targets);
        match targets.iter().find(|t| !self.objects.contains_key(t)) {
            Some(missing) => Err(ArrangeError::UnknownGeometry(*missing)),
            None => Ok(targets),
        }
    }

    fn editable(&self, id: GeometryId) -> Result<Arc<GeometryObject>, ArrangeError> {
        let obj = self.objects.get(&id).cloned().ok_or(ArrangeError::UnknownGeometry(id))?;
        if matches!(obj.source(), GeometrySource::Derived(_)) {
            return Err(ArrangeError::DerivedGeometry(id));
        }
        Ok(obj)
    }

    fn draw_order(&self) -> Vec<GeometryId> {
        self.arrange.draw_order(self.objects.keys().copied())
    }

    fn mark_operation(&mut self, operation: OperationId) {
        if !self.changes.operations.contains(&operation) {
            self.changes.operations.push(operation);
        }
    }

    /// Adds `geometry` to the targets of `operation` when that operation exists.
    fn assign(&mut self, operation: OperationId, geometry: GeometryId) {
        let Some(slot) = self.process.operations.iter_mut().find(|o| o.id() == operation) else { return };
        if slot.targets().contains(&geometry) {
            return;
        }
        let mut targets = slot.targets().to_vec();
        targets.push(geometry);
        *slot = Arc::new(slot.with_targets(targets));
        self.mark_operation(operation);
    }
}

pub(crate) fn apply(cmd: ArrangeCommand, mut txn: ArrangeTxn<'_>) -> Result<(), ArrangeError> {
    match cmd {
        ArrangeCommand::Order { targets, change } => order(&mut txn, &targets, change),
        ArrangeCommand::SetLocked { targets, locked } => {
            let targets = txn.existing(&targets)?;
            for t in targets {
                if txn.arrange.set_locked(t, locked) {
                    txn.changes.modified.push(t);
                }
            }
            if txn.changes.modified.is_empty() { Err(ArrangeError::NothingChanged) } else { Ok(()) }
        }
        ArrangeCommand::SetHidden { targets, hidden } => {
            let targets = txn.existing(&targets)?;
            for t in targets {
                if txn.arrange.set_hidden(t, hidden) {
                    txn.changes.modified.push(t);
                }
            }
            if txn.changes.modified.is_empty() { Err(ArrangeError::NothingChanged) } else { Ok(()) }
        }
        ArrangeCommand::Paste { items, offset } => paste(&mut txn, items, offset),
        ArrangeCommand::NestCopies { moves, copies } => nest_copies(&mut txn, moves, copies),
        ArrangeCommand::BreakApart { targets } => break_apart(&mut txn, &targets),
        ArrangeCommand::AutoGroup { targets } => auto_group(&mut txn, &targets),
        ArrangeCommand::Ungroup { targets } => ungroup(&mut txn, &targets),
    }
}

fn order(txn: &mut ArrangeTxn<'_>, targets: &[GeometryId], change: OrderChange) -> Result<(), ArrangeError> {
    let targets = txn.existing(targets)?;
    let before = txn.draw_order();
    let mut after = before.clone();
    let selected: BTreeSet<GeometryId> = targets.iter().copied().collect();
    if !crate::arrange::reorder(&mut after, &selected, change) {
        return Err(ArrangeError::NothingChanged);
    }
    txn.arrange.set_order(after);
    txn.changes.modified.extend(targets);
    Ok(())
}

fn paste(txn: &mut ArrangeTxn<'_>, items: Vec<PasteItem>, offset: Vector) -> Result<(), ArrangeError> {
    if items.is_empty() {
        return Err(ArrangeError::NothingChanged);
    }
    let shift = Affine::translate(offset).map_err(|_| ArrangeError::NothingChanged)?;
    paste_placed(txn, items, &shift)
}

/// The result of a nest with copies: the copies are made first, from the objects as they are, then
/// the originals move. Everything or nothing (the session rolls a failed command back).
fn nest_copies(
    txn: &mut ArrangeTxn<'_>,
    moves: Vec<(GeometryId, Affine)>,
    copies: Vec<NestCopy>,
) -> Result<(), ArrangeError> {
    if moves.is_empty() && copies.is_empty() {
        return Err(ArrangeError::NothingChanged);
    }
    let mut seen: Vec<GeometryId> = Vec::with_capacity(moves.len());
    for (id, _) in &moves {
        if seen.contains(id) {
            return Err(ArrangeError::DuplicateTarget(*id));
        }
        seen.push(*id);
    }
    for (id, _) in &moves {
        txn.editable(*id)?;
    }
    for copy in copies {
        if copy.items.is_empty() {
            return Err(ArrangeError::NothingChanged);
        }
        paste_placed(txn, copy.items, &copy.transform)?;
    }
    for (id, m) in moves {
        let object = txn.editable(id)?;
        txn.objects.insert(id, Arc::new(object.transformed(&m)));
        txn.changes.modified.push(id);
    }
    Ok(())
}

/// New objects from `items`, each placed by `shift`; the items of one source group share a new group.
fn paste_placed(txn: &mut ArrangeTxn<'_>, items: Vec<PasteItem>, shift: &Affine) -> Result<(), ArrangeError> {
    if items.iter().any(|i| i.path.is_empty()) {
        return Err(ArrangeError::EmptyPath);
    }
    let mut groups: BTreeMap<u32, GroupId> = BTreeMap::new();
    for item in items {
        let id: GeometryId = txn.ids.allocate().map_err(|_| ArrangeError::IdsExhausted)?;
        let mut object = GeometryObject::new(id, item.name, item.kind, GeometrySource::Drawn, item.path);
        if let Some(definition) = item.parametric
            && let Ok(defined) = object.clone().with_definition(definition)
        {
            object = defined;
        }
        let mut chain = Vec::with_capacity(item.group.len());
        for key in item.group {
            let group = match groups.get(&key) {
                Some(g) => *g,
                None => {
                    let g: GroupId = txn.ids.allocate().map_err(|_| ArrangeError::IdsExhausted)?;
                    groups.insert(key, g);
                    txn.changes.groups.push(g);
                    g
                }
            };
            chain.push(group);
        }
        let object = object.transformed(shift).with_groups(chain);
        txn.objects.insert(id, Arc::new(object));
        txn.changes.created.push(id);
        for operation in item.operations {
            txn.assign(operation, id);
        }
    }
    Ok(())
}

fn break_apart(txn: &mut ArrangeTxn<'_>, targets: &[GeometryId]) -> Result<(), ArrangeError> {
    let targets = txn.existing(targets)?;
    let mut order = txn.draw_order();
    for target in targets {
        let obj = txn.editable(target)?;
        if obj.path().subpaths().len() < 2 {
            continue;
        }
        let mut pieces = Vec::new();
        for sp in obj.path().subpaths() {
            let id: GeometryId = txn.ids.allocate().map_err(|_| ArrangeError::IdsExhausted)?;
            let piece = GeometryObject::new(
                id,
                obj.name().to_owned(),
                ObjectKind::Path,
                obj.source().clone(),
                Path::from_subpaths(vec![sp.clone()]),
            )
            .with_groups(obj.groups().to_vec());
            txn.objects.insert(id, Arc::new(piece));
            txn.changes.created.push(id);
            pieces.push(id);
        }
        txn.objects.remove(&target);
        txn.changes.deleted.push(target);
        // The pieces join every operation the original belonged to, at its place in the list.
        let affected: Vec<OperationId> = txn.process.operations_using(target);
        for op in affected {
            if let Some(slot) = txn.process.operations.iter_mut().find(|o| o.id() == op) {
                let mut list = Vec::with_capacity(slot.targets().len() + pieces.len());
                for g in slot.targets() {
                    if *g == target {
                        list.extend_from_slice(&pieces);
                    } else {
                        list.push(*g);
                    }
                }
                *slot = Arc::new(slot.with_targets(list));
            }
            txn.mark_operation(op);
        }
        if let Some(at) = order.iter().position(|id| *id == target) {
            order.splice(at..=at, pieces.iter().copied());
        }
    }
    if txn.changes.created.is_empty() {
        return Err(ArrangeError::NothingChanged);
    }
    txn.arrange.set_order(order);
    Ok(())
}

fn auto_group(txn: &mut ArrangeTxn<'_>, targets: &[GeometryId]) -> Result<(), ArrangeError> {
    let targets = txn.existing(targets)?;
    let area = |id: GeometryId| {
        txn.objects.get(&id).and_then(|o| o.bounds()).map_or(f64::INFINITY, |b| b.width() * b.height())
    };
    // Each shape belongs with the smallest other shape that encloses it.
    let mut parent: Vec<usize> = (0..targets.len()).collect();
    for (i, inner) in targets.iter().enumerate() {
        let Some(inner_path) = txn.objects.get(inner).map(|o| Arc::clone(o.path())) else { continue };
        let enclosing = targets
            .iter()
            .enumerate()
            .filter(|(j, outer)| {
                *j != i
                    && txn
                        .objects
                        .get(outer)
                        .is_some_and(|o| burnburn_geometry::encloses(o.path(), &inner_path, ENCLOSE_TOLERANCE_MM))
            })
            .min_by(|a, b| area(*a.1).total_cmp(&area(*b.1)));
        if let Some((j, _)) = enclosing {
            let (a, b) = (find(&parent, i), find(&parent, j));
            parent[a] = b;
        }
    }
    let mut clusters: BTreeMap<usize, Vec<GeometryId>> = BTreeMap::new();
    for (i, id) in targets.iter().enumerate() {
        clusters.entry(find(&parent, i)).or_default().push(*id);
    }
    let mut warnings = Vec::new();
    let mut grouped = false;
    for members in clusters.into_values().filter(|m| m.len() >= 2) {
        crate::edit::apply(
            crate::GeometryCommand::Group { targets: members },
            crate::edit::EditTxn {
                objects: &mut *txn.objects,
                ids: &mut *txn.ids,
                changes: &mut *txn.changes,
                warnings: &mut warnings,
                cache: Default::default(),
            },
        )
        .map_err(|_| ArrangeError::NothingChanged)?;
        grouped = true;
    }
    if grouped { Ok(()) } else { Err(ArrangeError::NothingChanged) }
}

fn ungroup(txn: &mut ArrangeTxn<'_>, targets: &[GeometryId]) -> Result<(), ArrangeError> {
    let targets = txn.existing(targets)?;
    for target in targets {
        let Some(obj) = txn.objects.get(&target).cloned() else { continue };
        let Some((outer, rest)) = obj.groups().split_first() else { continue };
        if !txn.changes.groups.contains(outer) {
            txn.changes.groups.push(*outer);
        }
        txn.objects.insert(target, Arc::new(obj.as_ref().clone().with_groups(rest.to_vec())));
        txn.changes.modified.push(target);
    }
    if txn.changes.modified.is_empty() { Err(ArrangeError::NothingChanged) } else { Ok(()) }
}

/// Distance from the outline of the outer shape that an inner shape must keep (mm).
const ENCLOSE_TOLERANCE_MM: f64 = 1e-6;

fn find(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i {
        i = parent[i];
    }
    i
}
