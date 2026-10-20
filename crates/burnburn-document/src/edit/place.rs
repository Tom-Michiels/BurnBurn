//! Placement commands (W2-L2): dock objects against a reference and close the gaps between them.
//! Both only translate, so identity, size and parametric form of every object are kept.

use std::collections::BTreeMap;

use burnburn_geometry::{
    Bounds, Direction, DockContact, DockDirection, ProductionTolerance, TogetherAnchor, dock_offset,
    move_together_offsets,
};
use burnburn_types::{GeometryId, GroupId, Length};

use super::{EditError, EditLabel, EditTxn, unique};

/// What moves as one body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceUnits {
    /// Every target moves on its own.
    Individual,
    /// Targets that share their outermost group move together; the other members of that group
    /// stay where they are unless they are targets themselves.
    ByGroup,
}

/// Which object stays put when closing gaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveAnchor {
    Start,
    End,
    /// The unit holding this target.
    Object(GeometryId),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PlaceCommand {
    /// Slides every unit of `targets` along `direction` until it touches the union of `reference`
    /// (which never moves), leaving `gap` of clearance. Any unit that cannot dock fails the command.
    Dock {
        targets: Vec<GeometryId>,
        reference: Vec<GeometryId>,
        direction: DockDirection,
        contact: DockContact,
        gap: Length,
        units: PlaceUnits,
        tolerance: ProductionTolerance,
    },
    /// Closes the gaps between the units of `targets` along one axis, towards `anchor`.
    MoveTogether { targets: Vec<GeometryId>, direction: Direction, anchor: MoveAnchor, gap: Length, units: PlaceUnits },
    /// The result of a nest: every object gets its own placement (rotation and translation, in the
    /// object's own coordinates), all in one transaction. An object may appear only once.
    Nest { moves: Vec<(GeometryId, burnburn_geometry::Affine)> },
}

impl PlaceCommand {
    pub fn label(&self) -> EditLabel {
        match self {
            PlaceCommand::Dock { .. } => EditLabel::Dock,
            PlaceCommand::MoveTogether { .. } => EditLabel::MoveTogether,
            PlaceCommand::Nest { .. } => EditLabel::Nest,
        }
    }
}

pub(super) fn apply(cmd: PlaceCommand, txn: &mut EditTxn<'_>) -> Result<(), EditError> {
    match cmd {
        PlaceCommand::Dock { targets, reference, direction, contact, gap, units, tolerance } => {
            dock(txn, &targets, &reference, direction, contact, gap, units, &tolerance)
        }
        PlaceCommand::MoveTogether { targets, direction, anchor, gap, units } => {
            move_together(txn, &targets, direction, anchor, gap, units)
        }
        PlaceCommand::Nest { moves } => nest(txn, &moves),
    }
}

/// The bodies that move, in the order their first member was selected.
fn bodies(txn: &EditTxn<'_>, targets: &[GeometryId], mode: PlaceUnits) -> Result<Vec<Vec<GeometryId>>, EditError> {
    let mut out: Vec<Vec<GeometryId>> = Vec::new();
    let mut by_group: BTreeMap<GroupId, usize> = BTreeMap::new();
    for &t in targets {
        let obj = txn.editable(t)?;
        match (mode, obj.groups().first()) {
            (PlaceUnits::ByGroup, Some(g)) => match by_group.get(g) {
                Some(&k) => out[k].push(t),
                None => {
                    by_group.insert(*g, out.len());
                    out.push(vec![t]);
                }
            },
            _ => out.push(vec![t]),
        }
    }
    Ok(out)
}

fn body_bounds(txn: &EditTxn<'_>, body: &[GeometryId]) -> Result<Bounds, EditError> {
    let mut union: Option<Bounds> = None;
    for id in body {
        let b = txn.get(*id)?.bounds().ok_or(EditError::Align(burnburn_geometry::AlignError::NothingToAlign))?;
        union = Some(union.map_or(b, |u| u.union(b)));
    }
    union.ok_or(EditError::NotEnoughTargets)
}

#[allow(clippy::too_many_arguments)] // the command's fields, unpacked
fn dock(
    txn: &mut EditTxn<'_>,
    targets: &[GeometryId],
    reference: &[GeometryId],
    direction: DockDirection,
    contact: DockContact,
    gap: Length,
    units: PlaceUnits,
    tolerance: &ProductionTolerance,
) -> Result<(), EditError> {
    let targets = unique(targets);
    let reference = unique(reference);
    if targets.is_empty() || reference.is_empty() {
        return Err(EditError::NotEnoughTargets);
    }
    if let Some(r) = reference.iter().find(|r| targets.contains(r)) {
        return Err(EditError::ReferenceInTargets(*r));
    }
    let wall = txn.paths(&reference)?;
    let wall: Vec<&burnburn_geometry::Path> = wall.iter().map(|p| p.as_ref()).collect();
    let mut moves = Vec::new();
    for body in bodies(txn, &targets, units)? {
        let paths = txn.paths(&body)?;
        let moving: Vec<&burnburn_geometry::Path> = paths.iter().map(|p| p.as_ref()).collect();
        let v = dock_offset(&moving, &wall, direction, contact, gap.mm(), tolerance).map_err(EditError::Dock)?;
        moves.push((body, v));
    }
    for (body, v) in moves {
        for id in body {
            txn.translate(id, v)?;
        }
    }
    Ok(())
}

fn move_together(
    txn: &mut EditTxn<'_>,
    targets: &[GeometryId],
    direction: Direction,
    anchor: MoveAnchor,
    gap: Length,
    units: PlaceUnits,
) -> Result<(), EditError> {
    let targets = unique(targets);
    let bodies = bodies(txn, &targets, units)?;
    let anchor = match anchor {
        MoveAnchor::Start => TogetherAnchor::Start,
        MoveAnchor::End => TogetherAnchor::End,
        MoveAnchor::Object(id) => {
            let k = bodies.iter().position(|b| b.contains(&id)).ok_or(EditError::ReferenceNotInTargets(id))?;
            TogetherAnchor::Object(k)
        }
    };
    let bounds = bodies.iter().map(|b| body_bounds(txn, b)).collect::<Result<Vec<_>, _>>()?;
    let offsets = move_together_offsets(&bounds, direction, anchor, gap.mm()).map_err(EditError::Together)?;
    for (body, v) in bodies.into_iter().zip(offsets) {
        for id in body {
            txn.translate(id, v)?;
        }
    }
    Ok(())
}

fn nest(txn: &mut EditTxn<'_>, moves: &[(GeometryId, burnburn_geometry::Affine)]) -> Result<(), EditError> {
    if moves.is_empty() {
        return Err(EditError::NotEnoughTargets);
    }
    let mut seen: Vec<GeometryId> = Vec::with_capacity(moves.len());
    for (id, _) in moves {
        if seen.contains(id) {
            return Err(EditError::DuplicateTarget(*id));
        }
        seen.push(*id);
    }
    for (id, m) in moves {
        let obj = txn.editable(*id)?;
        txn.objects.insert(*id, std::sync::Arc::new(obj.transformed(m)));
        txn.mark_modified(*id);
    }
    Ok(())
}
