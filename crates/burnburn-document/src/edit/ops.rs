//! Whole-selection geometry operations (W1-L2a). Each runs on the transaction's working copy,
//! so an error anywhere drops every change of the command.

use burnburn_geometry::{
    BooleanOptions, DiagnosticTolerance, Path, PathOp, PathOpError, RoundError, cut_shapes, join_shapes,
};
use burnburn_types::{CancellationToken, GeometryId};

use super::{EditError, EditLabel, EditTxn, unique};
use crate::object::{GeometrySource, ObjectKind};

/// One selection-wide operation; always exactly one history entry.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum OpsCommand {
    /// Applies `op` to each target. A target the operation would leave as it is is skipped; when
    /// no target changes, the command is `NothingChanged`. Identity and names are kept.
    PathOp { targets: Vec<GeometryId>, op: PathOp },
    /// Cuts every subject along the outline of `cutter`; each subject that is crossed is replaced
    /// by its pieces (new objects, named like the subject). The cutter is removed unless `keep_cutter`.
    CutShapes { subjects: Vec<GeometryId>, cutter: GeometryId, keep_cutter: bool, options: BooleanOptions },
    /// Joins the open pieces of the targets whose ends lie within `tolerance`. One target is
    /// closed or joined in place; several are replaced by one object (named after the first).
    JoinShapes { targets: Vec<GeometryId>, tolerance: DiagnosticTolerance },
}

impl OpsCommand {
    pub fn label(&self) -> EditLabel {
        match self {
            OpsCommand::PathOp { op, .. } => match op {
                PathOp::RoundCorners { .. } => EditLabel::RoundCorners,
                PathOp::Optimize { .. } => EditLabel::OptimizeShapes,
                PathOp::SetWinding { .. } | PathOp::Reverse { .. } => EditLabel::Direction,
                _ => EditLabel::EditNodes,
            },
            OpsCommand::CutShapes { .. } => EditLabel::CutShapes,
            OpsCommand::JoinShapes { .. } => EditLabel::JoinShapes,
        }
    }
}

pub(super) fn apply(ops: OpsCommand, txn: &mut EditTxn<'_>) -> Result<(), EditError> {
    match ops {
        OpsCommand::PathOp { targets, op } => path_op(txn, &targets, &op),
        OpsCommand::CutShapes { subjects, cutter, keep_cutter, options } => {
            cut_shapes_command(txn, &subjects, cutter, keep_cutter, &options)
        }
        OpsCommand::JoinShapes { targets, tolerance } => join_command(txn, &targets, &tolerance),
    }
}

fn path_op(txn: &mut EditTxn<'_>, targets: &[GeometryId], op: &PathOp) -> Result<(), EditError> {
    let targets = unique(targets);
    if targets.is_empty() {
        return Err(EditError::NotEnoughTargets);
    }
    let cancel = CancellationToken::never();
    let mut last_skip = None;
    let mut changed = false;
    for &t in &targets {
        let obj = txn.editable(t)?;
        match obj.path().apply_op(op, &cancel) {
            Ok(path) => {
                txn.replace(t, path)?;
                changed = true;
            }
            // A target the operation has nothing to do for is skipped, not an error of the command.
            Err(e @ (PathOpError::NoChange | PathOpError::Round(RoundError::NothingToRound))) => last_skip = Some(e),
            Err(e) => return Err(EditError::PathOp(e)),
        }
    }
    match (changed, last_skip, targets.len()) {
        (true, ..) => Ok(()),
        (false, Some(PathOpError::NoChange) | None, _) | (false, Some(_), 2..) => Err(EditError::NothingChanged),
        (false, Some(e), _) => Err(EditError::PathOp(e)),
    }
}

fn cut_shapes_command(
    txn: &mut EditTxn<'_>,
    subjects: &[GeometryId],
    cutter: GeometryId,
    keep_cutter: bool,
    options: &BooleanOptions,
) -> Result<(), EditError> {
    let subjects: Vec<GeometryId> = unique(subjects).into_iter().filter(|s| *s != cutter).collect();
    if subjects.is_empty() {
        return Err(EditError::NotEnoughTargets);
    }
    let cutter_path = txn.get(cutter)?.path().clone();
    let paths = txn.paths(&subjects)?;
    let refs: Vec<&Path> = paths.iter().map(|p| p.as_ref()).collect();
    let results = cut_shapes(&refs, &cutter_path, options, &CancellationToken::never()).map_err(EditError::Cut)?;
    let mut any = false;
    for (id, result) in subjects.iter().zip(results) {
        if !result.changed {
            continue;
        }
        any = true;
        let name = txn.editable(*id)?.name().to_owned();
        txn.delete(*id)?;
        for (i, piece) in result.pieces.into_iter().enumerate() {
            let piece_name = if i == 0 { name.clone() } else { format!("{name} {}", i + 1) };
            txn.create(piece_name, ObjectKind::Path, GeometrySource::Drawn, piece.path)?;
        }
    }
    if !any {
        return Err(EditError::NothingChanged);
    }
    if !keep_cutter {
        txn.editable(cutter)?;
        txn.delete(cutter)?;
    }
    Ok(())
}

fn join_command(
    txn: &mut EditTxn<'_>,
    targets: &[GeometryId],
    tolerance: &DiagnosticTolerance,
) -> Result<(), EditError> {
    let targets = unique(targets);
    if targets.is_empty() {
        return Err(EditError::NotEnoughTargets);
    }
    let paths = txn.paths(&targets)?;
    let refs: Vec<&Path> = paths.iter().map(|p| p.as_ref()).collect();
    let outcome = join_shapes(&refs, tolerance, &CancellationToken::never()).map_err(EditError::Join)?;
    if outcome.merged == 0 && outcome.closed == 0 {
        return Err(EditError::NothingChanged);
    }
    if let [only] = targets[..] {
        return txn.replace(only, outcome.path);
    }
    let name = txn.get(targets[0])?.name().to_owned();
    for t in &targets {
        txn.editable(*t)?;
        txn.delete(*t)?;
    }
    txn.create(name, ObjectKind::Path, GeometrySource::Drawn, outcome.path)?;
    Ok(())
}
