//! Join / Close selected shapes with an explicit tolerance (W1-L2a).

use std::fmt;

use burnburn_types::CancellationToken;

use crate::repair::close_gaps_cancellable;
use crate::{DiagnosticTolerance, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum JoinError {
    Cancelled,
}

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("join cancelled")
    }
}

impl std::error::Error for JoinError {}

/// The joined outlines and what the join did.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinOutcome {
    pub path: Path,
    /// Open pieces fused end to end into a longer piece.
    pub merged: usize,
    /// Open pieces whose two ends met and which became closed contours.
    pub closed: usize,
}

/// Fuses the open pieces of all `paths` whose ends lie within `tolerance` of each other.
///
/// The globally nearest pair of ends joins first (the moved end snaps exactly onto its partner,
/// no bridging line), then the search repeats, so at a meeting point of three ends only the
/// closest two fuse. Ends further apart than the tolerance and all closed contours are left as
/// they are; the result lists the subpaths of the inputs in order, joined ones at the position
/// of their first piece.
pub fn join_shapes(
    paths: &[&Path],
    tolerance: &DiagnosticTolerance,
    cancel: &CancellationToken,
) -> Result<JoinOutcome, JoinError> {
    cancel.check().map_err(|_| JoinError::Cancelled)?;
    let subs: Vec<_> = paths.iter().flat_map(|p| p.subpaths().iter().cloned()).collect();
    let before = subs.len();
    let (path, closed) =
        close_gaps_cancellable(&Path::from_subpaths(subs), tolerance, cancel).map_err(|_| JoinError::Cancelled)?;
    Ok(JoinOutcome { merged: before - path.subpaths().len(), closed: closed.len(), path })
}
