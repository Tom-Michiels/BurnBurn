//! Reshaping commands (W2-L2): Warp 4 points, Deform 16 points and Resize Slots. Each target keeps
//! its identity and name; the outline is replaced, so a parametric shape becomes a plain path.

use burnburn_geometry::{
    Bounds, DeformGrid, Point, ProductionTolerance, QuadWarp, ResizeSlots, SlotsError, WarpKind, warp_path,
};
use burnburn_types::{CancellationToken, GeometryId};

use super::{EditError, EditLabel, EditTxn, unique};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ReshapeCommand {
    /// Maps the common bounding box of `targets` onto the quadrilateral `corners`
    /// (`[bottom-left, bottom-right, top-right, top-left]` of that box) as one warp.
    Warp4 { targets: Vec<GeometryId>, corners: [Point; 4], kind: WarpKind, tolerance: ProductionTolerance },
    /// Maps the common bounding box of `targets` through a 4 × 4 control net (`net[row][column]`, row 0
    /// at the bottom). `DeformGrid::identity(bounds)` is the starting net.
    Deform16 { targets: Vec<GeometryId>, net: Box<[[Point; 4]; 4]>, tolerance: ProductionTolerance },
    /// Resizes the matching slots/tabs of every target; targets without any are skipped.
    ResizeSlots { targets: Vec<GeometryId>, spec: ResizeSlots },
}

impl ReshapeCommand {
    pub fn label(&self) -> EditLabel {
        match self {
            ReshapeCommand::Warp4 { .. } => EditLabel::Warp,
            ReshapeCommand::Deform16 { .. } => EditLabel::Deform,
            ReshapeCommand::ResizeSlots { .. } => EditLabel::ResizeSlots,
        }
    }
}

pub(super) fn apply(cmd: ReshapeCommand, txn: &mut EditTxn<'_>) -> Result<(), EditError> {
    match cmd {
        ReshapeCommand::Warp4 { targets, corners, kind, tolerance } => {
            let targets = unique(&targets);
            let source = common_bounds(txn, &targets)?;
            let map = QuadWarp::new(source, corners, kind).map_err(EditError::Warp)?;
            warp_all(txn, &targets, &map, &tolerance)
        }
        ReshapeCommand::Deform16 { targets, net, tolerance } => {
            let targets = unique(&targets);
            let source = common_bounds(txn, &targets)?;
            let map = DeformGrid::new(source, *net).map_err(EditError::Warp)?;
            warp_all(txn, &targets, &map, &tolerance)
        }
        ReshapeCommand::ResizeSlots { targets, spec } => resize_slots(txn, &targets, &spec),
    }
}

fn common_bounds(txn: &EditTxn<'_>, targets: &[GeometryId]) -> Result<Bounds, EditError> {
    if targets.is_empty() {
        return Err(EditError::NotEnoughTargets);
    }
    let mut union: Option<Bounds> = None;
    for &t in targets {
        let b = txn.editable(t)?.bounds().ok_or(EditError::NothingChanged)?;
        union = Some(union.map_or(b, |u| u.union(b)));
    }
    union.ok_or(EditError::NotEnoughTargets)
}

fn warp_all(
    txn: &mut EditTxn<'_>,
    targets: &[GeometryId],
    map: &impl burnburn_geometry::PointMap,
    tolerance: &ProductionTolerance,
) -> Result<(), EditError> {
    let cancel = CancellationToken::never();
    for &t in targets {
        let path = warp_path(txn.editable(t)?.path(), map, tolerance, &cancel).map_err(EditError::Warp)?;
        txn.replace(t, path)?;
    }
    Ok(())
}

fn resize_slots(txn: &mut EditTxn<'_>, targets: &[GeometryId], spec: &ResizeSlots) -> Result<(), EditError> {
    let targets = unique(targets);
    if targets.is_empty() {
        return Err(EditError::NotEnoughTargets);
    }
    let mut changed = false;
    for &t in &targets {
        match txn.editable(t)?.path().resize_slots(spec) {
            Ok(outcome) => {
                txn.replace(t, outcome.path)?;
                changed = true;
            }
            // A target without matching features is skipped, not an error of the command.
            Err(SlotsError::NoSlotsFound) => {}
            Err(e) => return Err(EditError::Slots(e)),
        }
    }
    match (changed, targets.len()) {
        (true, _) => Ok(()),
        (false, 1) => Err(EditError::Slots(SlotsError::NoSlotsFound)),
        (false, _) => Err(EditError::NothingChanged),
    }
}
