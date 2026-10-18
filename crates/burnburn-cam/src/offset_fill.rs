//! Offset Fill (IM25): concentric rings inside the closed contours of each target, cut as lines.

use burnburn_document::process::{OffsetFillOrder, OffsetFillSettings};
use burnburn_geometry::{OffsetError, OffsetJoin, Path, ProductionTolerance, Subpath, offset};
use burnburn_types::{CancellationToken, GeometryId, OperationId};

use crate::CompileError;
use crate::line::Contour;

/// Upper bound on rings per geometry (keeps one setting from making a compile unbounded).
pub(crate) const MAX_RINGS: u32 = 100_000;

/// Rings of `path` (closed subpaths only; open ones are reported by preflight): ring `k` is the
/// inward offset by `step · (k + ½)`, until the area collapses. Returned in cutting order.
pub(crate) fn rings(
    path: &Path,
    settings: &OffsetFillSettings,
    geometry: GeometryId,
    operation: OperationId,
    tolerance: &ProductionTolerance,
    cancel: &CancellationToken,
) -> Result<Vec<Contour>, CompileError> {
    let closed: Vec<Subpath> = path.subpaths().iter().filter(|sp| sp.is_closed()).cloned().collect();
    let area = Path::from_subpaths(closed);
    let step = settings.step().mm();
    let mut out: Vec<Vec<Contour>> = Vec::new();
    for k in 0..MAX_RINGS {
        cancel.check()?;
        let d = -step * (f64::from(k) + 0.5);
        let ring = match offset(&area, d, OffsetJoin::Miter, tolerance, cancel) {
            Ok(r) if r.is_empty() => break,
            Ok(r) => r,
            Err(OffsetError::Collapsed) => break,
            Err(OffsetError::Cancelled) => return Err(CompileError::Cancelled),
            Err(error) => return Err(CompileError::Offset { operation, geometry, error }),
        };
        out.push(ring.flatten(tolerance).into_iter().map(|pl| Contour::new(geometry, pl.points, pl.closed)).collect());
        if k + 1 == MAX_RINGS {
            return Err(CompileError::TooManyScanRows { operation, rows: u64::from(MAX_RINGS) });
        }
    }
    if settings.order() == OffsetFillOrder::InsideOut {
        out.reverse();
    }
    Ok(out.into_iter().flatten().collect())
}
