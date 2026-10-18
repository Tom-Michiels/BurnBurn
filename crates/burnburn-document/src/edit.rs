//! IM09/IM10 geometry commands: drawing primitives, node edits, booleans/offsets, trim/extend,
//! gap repair, align/distribute, groups and import commits. Each runs on the transaction's
//! working copy; an error drops the copy, so a failed edit changes nothing.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_geometry::{
    Affine, AlignEdge, AlignError, AlignTo, BooleanError, BooleanOp, BooleanOptions, Bounds, BreakError, CornerCut,
    CornerError, CutError, DiagnosticTolerance, Direction, EndpointRef, ExtendError, GeometryError, JoinError,
    NodeEdit, NodeEditError, OffsetError, OffsetJoin, Path, PathOpError, Point, ProductionTolerance, Shape, Spacing,
    TrimError, align_offsets, boolean, close_gaps, distribute_offsets, offset,
};
use burnburn_types::{CancellationToken, ContentHash, GeometryId, GroupId, IdAllocator, Length};

use crate::command::{ChangeSet, CommandWarning};

mod ops;

use crate::object::{GeometryObject, GeometrySource, ObjectKind};
pub use ops::OpsCommand;

/// Reference for [`GeometryCommand::Align`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignTarget {
    /// The combined bounds of all targets.
    Selection,
    /// One of the targets, which stays in place.
    Object(GeometryId),
}

/// One object of an import commit (IM10). The source asset is given once for the whole import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportItem {
    pub name: String,
    /// Entity reference inside the asset (SVG id / element path, DXF handle), kept as provenance.
    pub entity: Option<String>,
    pub path: Path,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GeometryCommand {
    CreateShape {
        shape: Shape,
        name: String,
    },
    /// A free path (for example a pasted or traced outline) drawn in BurnBurn.
    CreatePath {
        path: Path,
        name: String,
    },
    EditNodes {
        target: GeometryId,
        edit: NodeEdit,
    },
    /// `targets[0]` is the subject; the result replaces the targets unless `keep_sources`.
    Boolean {
        op: BooleanOp,
        targets: Vec<GeometryId>,
        options: BooleanOptions,
        keep_sources: bool,
    },
    /// One new object per target.
    Offset {
        targets: Vec<GeometryId>,
        distance: Length,
        join: OffsetJoin,
        tolerance: ProductionTolerance,
        keep_sources: bool,
    },
    TrimLine {
        target: GeometryId,
        pick: Point,
        cutters: Vec<GeometryId>,
    },
    ExtendLine {
        target: GeometryId,
        end: EndpointRef,
        boundaries: Vec<GeometryId>,
    },
    CloseGaps {
        targets: Vec<GeometryId>,
        tolerance: DiagnosticTolerance,
    },
    /// Fillet or chamfer the vertex of `target` nearest to `pick` (within `reach`).
    CutCorner {
        target: GeometryId,
        pick: Point,
        reach: Length,
        cut: CornerCut,
    },
    /// Trim a line, arc or Bézier of `target` at `pick` between the crossings with `cutters`.
    TrimAt {
        target: GeometryId,
        pick: Point,
        cutters: Vec<GeometryId>,
    },
    /// Extend an open line or arc end of `target` to the nearest boundary.
    ExtendEnd {
        target: GeometryId,
        end: EndpointRef,
        boundaries: Vec<GeometryId>,
    },
    /// Split `target` at the point nearest to `pick`.
    BreakAt {
        target: GeometryId,
        pick: Point,
    },
    Align {
        targets: Vec<GeometryId>,
        edge: AlignEdge,
        to: AlignTarget,
    },
    Distribute {
        targets: Vec<GeometryId>,
        direction: Direction,
        spacing: Spacing,
    },
    /// A new innermost-common group around the targets (nested inside their shared groups).
    Group {
        targets: Vec<GeometryId>,
    },
    Ungroup {
        group: GroupId,
    },
    Import {
        asset: ContentHash,
        items: Vec<ImportItem>,
    },
    /// Whole-selection operations: round corners, optimize, cut shapes, join, node sets (W1-L2a).
    Ops(OpsCommand),
}

/// Undo/redo label (localisation key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EditLabel {
    CreateShape,
    CreatePath,
    EditNodes,
    Boolean,
    Offset,
    Trim,
    Extend,
    CloseGaps,
    CutCorner,
    Break,
    Align,
    Distribute,
    Group,
    Ungroup,
    Import,
    RoundCorners,
    OptimizeShapes,
    CutShapes,
    JoinShapes,
    /// Reverse or set the direction of contours.
    Direction,
}

impl GeometryCommand {
    pub fn label(&self) -> EditLabel {
        match self {
            GeometryCommand::CreateShape { .. } => EditLabel::CreateShape,
            GeometryCommand::CreatePath { .. } => EditLabel::CreatePath,
            GeometryCommand::EditNodes { .. } => EditLabel::EditNodes,
            GeometryCommand::Boolean { .. } => EditLabel::Boolean,
            GeometryCommand::Offset { .. } => EditLabel::Offset,
            GeometryCommand::TrimLine { .. } => EditLabel::Trim,
            GeometryCommand::ExtendLine { .. } => EditLabel::Extend,
            GeometryCommand::CloseGaps { .. } => EditLabel::CloseGaps,
            GeometryCommand::CutCorner { .. } => EditLabel::CutCorner,
            GeometryCommand::TrimAt { .. } => EditLabel::Trim,
            GeometryCommand::ExtendEnd { .. } => EditLabel::Extend,
            GeometryCommand::BreakAt { .. } => EditLabel::Break,
            GeometryCommand::Align { .. } => EditLabel::Align,
            GeometryCommand::Distribute { .. } => EditLabel::Distribute,
            GeometryCommand::Group { .. } => EditLabel::Group,
            GeometryCommand::Ungroup { .. } => EditLabel::Ungroup,
            GeometryCommand::Import { .. } => EditLabel::Import,
            GeometryCommand::Ops(ops) => ops.label(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    UnknownGeometry(GeometryId),
    /// Derived (sketch) geometry is regenerated from its owner; edit the owner instead.
    DerivedGeometry(GeometryId),
    Shape(GeometryError),
    Node(NodeEditError),
    Boolean(BooleanError),
    Offset(OffsetError),
    Trim(TrimError),
    Extend(ExtendError),
    Corner(CornerError),
    Break(BreakError),
    Align(AlignError),
    PathOp(PathOpError),
    Cut(CutError),
    Join(JoinError),
    NotEnoughTargets,
    /// The align reference must be one of the targets.
    ReferenceNotInTargets(GeometryId),
    NoSuchGroup(GroupId),
    /// The edit would not change the document (no history entry is made).
    NothingChanged,
    IdsExhausted,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EditError {}

/// The parts of the session transaction an edit may touch.
pub(crate) struct EditTxn<'a> {
    pub objects: &'a mut BTreeMap<GeometryId, Arc<GeometryObject>>,
    pub ids: &'a mut IdAllocator,
    pub changes: &'a mut ChangeSet,
    pub warnings: &'a mut Vec<CommandWarning>,
}

impl EditTxn<'_> {
    fn get(&self, id: GeometryId) -> Result<Arc<GeometryObject>, EditError> {
        self.objects.get(&id).cloned().ok_or(EditError::UnknownGeometry(id))
    }

    /// An object this edit may change: it must exist and must not be evaluator output.
    fn editable(&self, id: GeometryId) -> Result<Arc<GeometryObject>, EditError> {
        let obj = self.get(id)?;
        if matches!(obj.source(), GeometrySource::Derived(_)) {
            return Err(EditError::DerivedGeometry(id));
        }
        Ok(obj)
    }

    fn create(
        &mut self,
        name: String,
        kind: ObjectKind,
        source: GeometrySource,
        path: Path,
    ) -> Result<GeometryId, EditError> {
        let id: GeometryId = self.ids.allocate().map_err(|_| EditError::IdsExhausted)?;
        if !name.is_empty()
            && let Some(other) = self.objects.values().find(|o| o.name() == name)
        {
            self.warnings.push(CommandWarning::DuplicateName { name: name.clone(), other: other.id() });
        }
        self.objects.insert(id, Arc::new(GeometryObject::new(id, name, kind, source, path)));
        self.changes.created.push(id);
        Ok(id)
    }

    fn replace(&mut self, id: GeometryId, path: Path) -> Result<(), EditError> {
        let obj = self.editable(id)?;
        if obj.path().as_ref() == &path {
            return Ok(());
        }
        self.objects.insert(id, Arc::new(obj.with_path(path)));
        self.mark_modified(id);
        Ok(())
    }

    fn mark_modified(&mut self, id: GeometryId) {
        if !self.changes.modified.contains(&id) {
            self.changes.modified.push(id);
        }
    }

    fn delete(&mut self, id: GeometryId) -> Result<(), EditError> {
        self.objects.remove(&id).ok_or(EditError::UnknownGeometry(id))?;
        self.changes.deleted.push(id);
        Ok(())
    }

    fn paths(&self, ids: &[GeometryId]) -> Result<Vec<Arc<Path>>, EditError> {
        ids.iter().map(|id| self.get(*id).map(|o| Arc::clone(o.path()))).collect()
    }

    fn translate(&mut self, id: GeometryId, v: burnburn_geometry::Vector) -> Result<(), EditError> {
        if v == burnburn_geometry::Vector::ZERO {
            return Ok(());
        }
        let m = Affine::translate(v).map_err(EditError::Shape)?;
        let obj = self.editable(id)?;
        self.objects.insert(id, Arc::new(obj.transformed(&m)));
        self.mark_modified(id);
        Ok(())
    }

    fn set_groups(&mut self, id: GeometryId, groups: Vec<GroupId>) -> Result<(), EditError> {
        let obj = self.get(id)?;
        if obj.groups() == groups.as_slice() {
            return Ok(());
        }
        self.objects.insert(id, Arc::new(obj.as_ref().clone().with_groups(groups)));
        self.mark_modified(id);
        Ok(())
    }
}

fn shape_kind(shape: &Shape) -> ObjectKind {
    match shape {
        Shape::Line { .. } => ObjectKind::Line,
        Shape::Rectangle(_) => ObjectKind::Rectangle,
        Shape::Circle { .. } => ObjectKind::Circle,
        Shape::Ellipse { .. } => ObjectKind::Ellipse,
        Shape::Polygon { .. } => ObjectKind::Polygon,
        Shape::Star { .. } => ObjectKind::Star,
        Shape::Polyline { .. } => ObjectKind::Polyline,
        _ => ObjectKind::Path,
    }
}

fn unique(targets: &[GeometryId]) -> Vec<GeometryId> {
    let mut seen = Vec::with_capacity(targets.len());
    for t in targets {
        if !seen.contains(t) {
            seen.push(*t);
        }
    }
    seen
}

pub(crate) fn apply(cmd: GeometryCommand, mut txn: EditTxn<'_>) -> Result<(), EditError> {
    let never = CancellationToken::never();
    match cmd {
        GeometryCommand::CreateShape { shape, name } => {
            let path = shape.to_path().map_err(EditError::Shape)?;
            txn.create(name, shape_kind(&shape), GeometrySource::Drawn, path)?;
        }
        GeometryCommand::CreatePath { path, name } => {
            if path.is_empty() {
                return Err(EditError::NothingChanged);
            }
            txn.create(name, ObjectKind::Path, GeometrySource::Drawn, path)?;
        }
        GeometryCommand::EditNodes { target, edit } => {
            let obj = txn.editable(target)?;
            let path = obj.path().edit_nodes(&edit).map_err(EditError::Node)?;
            // Edge keys (DR-20): edits that know their mapping keep keys exactly; the rest use
            // the conservative remap of `with_path`.
            let old = obj.path();
            let keys = obj.edge_keys();
            let mapped = match edit {
                NodeEdit::InsertNode { subpath, segment, .. } => keys.split(subpath, segment),
                NodeEdit::ReverseSubpath { subpath } => keys.reversed(old, &path, subpath),
                NodeEdit::ConvertToCubic { subpath } => keys.converted(old, &path, subpath),
                _ => None,
            };
            match mapped {
                Some(keys) if old.as_ref() != &path => {
                    txn.objects.insert(target, Arc::new(obj.with_path_and_keys(path, keys)));
                    txn.mark_modified(target);
                }
                _ => txn.replace(target, path)?,
            }
        }
        GeometryCommand::Boolean { op, targets, options, keep_sources } => {
            let targets = unique(&targets);
            if targets.len() < 2 {
                return Err(EditError::NotEnoughTargets);
            }
            let paths = txn.paths(&targets)?;
            let subject_name = txn.get(targets[0])?.name().to_owned();
            let result = match op {
                // Subject minus the union of the others.
                BooleanOp::Difference => {
                    let mut clip = Path::default();
                    for p in &paths[1..] {
                        clip = boolean(&clip, p, BooleanOp::Union, &options, &never).map_err(EditError::Boolean)?;
                    }
                    boolean(&paths[0], &clip, BooleanOp::Difference, &options, &never)
                }
                _ => {
                    let mut acc = paths[0].as_ref().clone();
                    for p in &paths[1..] {
                        acc = boolean(&acc, p, op, &options, &never).map_err(EditError::Boolean)?;
                    }
                    Ok(acc)
                }
            }
            .map_err(EditError::Boolean)?;
            if !keep_sources {
                for t in &targets {
                    txn.editable(*t)?;
                    txn.delete(*t)?;
                }
            }
            txn.create(subject_name, ObjectKind::Path, GeometrySource::Drawn, result)?;
        }
        GeometryCommand::Offset { targets, distance, join, tolerance, keep_sources } => {
            let targets = unique(&targets);
            if targets.is_empty() {
                return Err(EditError::NotEnoughTargets);
            }
            for t in targets {
                let obj = txn.get(t)?;
                let result = offset(obj.path(), distance.mm(), join, &tolerance, &never).map_err(EditError::Offset)?;
                if !keep_sources {
                    txn.editable(t)?;
                    txn.delete(t)?;
                }
                txn.create(obj.name().to_owned(), ObjectKind::Path, GeometrySource::Drawn, result)?;
            }
        }
        GeometryCommand::TrimLine { target, pick, cutters } => {
            let obj = txn.editable(target)?;
            let cut = txn.paths(&cutters)?;
            let refs: Vec<&Path> = cut.iter().map(|p| p.as_ref()).collect();
            let path = obj.path().trim_line(pick, &refs).map_err(EditError::Trim)?;
            if path.is_empty() { txn.delete(target)? } else { txn.replace(target, path)? }
        }
        GeometryCommand::ExtendLine { target, end, boundaries } => {
            let obj = txn.editable(target)?;
            let b = txn.paths(&boundaries)?;
            let refs: Vec<&Path> = b.iter().map(|p| p.as_ref()).collect();
            let path = obj.path().extend_line(end, &refs).map_err(EditError::Extend)?;
            txn.replace(target, path)?;
        }
        GeometryCommand::CutCorner { target, pick, reach, cut } => {
            let obj = txn.editable(target)?;
            let path = obj.path().cut_corner(pick, reach.mm(), cut).map_err(EditError::Corner)?;
            txn.replace(target, path)?;
        }
        GeometryCommand::TrimAt { target, pick, cutters } => {
            let obj = txn.editable(target)?;
            let cut = txn.paths(&cutters)?;
            let refs: Vec<&Path> = cut.iter().map(|p| p.as_ref()).collect();
            let path = obj.path().trim_at(pick, &refs).map_err(EditError::Trim)?;
            if path.is_empty() { txn.delete(target)? } else { txn.replace(target, path)? }
        }
        GeometryCommand::ExtendEnd { target, end, boundaries } => {
            let obj = txn.editable(target)?;
            let b = txn.paths(&boundaries)?;
            let refs: Vec<&Path> = b.iter().map(|p| p.as_ref()).collect();
            let path = obj.path().extend_end(end, &refs).map_err(EditError::Extend)?;
            txn.replace(target, path)?;
        }
        GeometryCommand::BreakAt { target, pick } => {
            let obj = txn.editable(target)?;
            let path = obj.path().break_at(pick).map_err(EditError::Break)?;
            txn.replace(target, path)?;
        }
        GeometryCommand::CloseGaps { targets, tolerance } => {
            for t in unique(&targets) {
                let obj = txn.editable(t)?;
                let (path, closed) = close_gaps(obj.path(), &tolerance);
                if !closed.is_empty() || path.subpaths().len() != obj.path().subpaths().len() {
                    txn.replace(t, path)?;
                }
            }
        }
        GeometryCommand::Align { targets, edge, to } => {
            let targets = unique(&targets);
            let bounds = bounds_of(&txn, &targets)?;
            let to = match to {
                AlignTarget::Selection => AlignTo::Selection,
                AlignTarget::Object(r) => {
                    AlignTo::Object(targets.iter().position(|t| *t == r).ok_or(EditError::ReferenceNotInTargets(r))?)
                }
            };
            let shifts = align_offsets(&bounds, edge, to).map_err(EditError::Align)?;
            for (t, v) in targets.iter().zip(shifts) {
                txn.translate(*t, v)?;
            }
        }
        GeometryCommand::Distribute { targets, direction, spacing } => {
            let targets = unique(&targets);
            let bounds = bounds_of(&txn, &targets)?;
            let shifts = distribute_offsets(&bounds, direction, spacing).map_err(EditError::Align)?;
            for (t, v) in targets.iter().zip(shifts) {
                txn.translate(*t, v)?;
            }
        }
        GeometryCommand::Group { targets } => {
            let targets = unique(&targets);
            if targets.is_empty() {
                return Err(EditError::NotEnoughTargets);
            }
            let chains: Vec<Vec<GroupId>> =
                targets.iter().map(|t| txn.get(*t).map(|o| o.groups().to_vec())).collect::<Result<_, _>>()?;
            // The new group nests inside the groups all targets share.
            let mut common = chains[0].len();
            for c in &chains[1..] {
                common = common.min(c.iter().zip(&chains[0]).take_while(|(a, b)| a == b).count());
            }
            let g: GroupId = txn.ids.allocate().map_err(|_| EditError::IdsExhausted)?;
            for (t, chain) in targets.iter().zip(chains) {
                let mut next = chain[..common].to_vec();
                next.push(g);
                next.extend_from_slice(&chain[common..]);
                txn.set_groups(*t, next)?;
            }
            txn.changes.groups.push(g);
        }
        GeometryCommand::Ungroup { group } => {
            let members: Vec<GeometryId> =
                txn.objects.iter().filter(|(_, o)| o.groups().contains(&group)).map(|(id, _)| *id).collect();
            if members.is_empty() {
                return Err(EditError::NoSuchGroup(group));
            }
            for m in members {
                let chain: Vec<GroupId> = txn.get(m)?.groups().iter().copied().filter(|g| *g != group).collect();
                txn.set_groups(m, chain)?;
            }
            txn.changes.groups.push(group);
        }
        GeometryCommand::Ops(ops) => ops::apply(ops, &mut txn)?,
        GeometryCommand::Import { asset, items } => {
            if items.is_empty() {
                return Err(EditError::NothingChanged);
            }
            for item in items {
                let source = GeometrySource::Imported { asset, entity: item.entity };
                txn.create(item.name, ObjectKind::Path, source, item.path)?;
            }
        }
    }
    if txn.changes.is_empty() {
        return Err(EditError::NothingChanged);
    }
    Ok(())
}

fn bounds_of(txn: &EditTxn<'_>, targets: &[GeometryId]) -> Result<Vec<Bounds>, EditError> {
    targets
        .iter()
        .map(|t| txn.get(*t).and_then(|o| o.bounds().ok_or(EditError::Align(AlignError::NothingToAlign))))
        .collect()
}
