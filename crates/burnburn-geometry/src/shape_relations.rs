//! Relations between two outlines used by selection and arrangement commands (W2-L1-bewerken):
//! whether one lies inside the other and whether two are the same drawing.

use burnburn_types::Length;

use crate::query::HitKind;
use crate::{HitTestTolerance, Path, Subpath};

/// True when `inner` lies strictly inside a closed outline of `outer`: it has geometry, its
/// bounds lie within the bounds of `outer`, and every subpath start of `inner` is in the
/// interior of `outer` (even-odd), clear of the outline by more than `tol_mm`.
pub fn encloses(outer: &Path, inner: &Path, tol_mm: f64) -> bool {
    let (Some(ob), Some(ib)) = (outer.bounds(), inner.bounds()) else { return false };
    if !outer.subpaths().iter().any(Subpath::is_closed) || !(ob.contains(ib.min()) && ob.contains(ib.max())) {
        return false;
    }
    let Some(tol) = Length::from_mm(tol_mm).ok().and_then(|l| HitTestTolerance::new(l).ok()) else { return false };
    inner.subpaths().iter().all(|sp| matches!(outer.hit(sp.start(), &tol), Some(HitKind::Interior)))
}

/// True when `a` and `b` are the same outline: the same subpaths in the same order, each with
/// the same closure and segment kinds, and every segment end within `tol_mm`.
pub fn coincident(a: &Path, b: &Path, tol_mm: f64) -> bool {
    a.subpaths().len() == b.subpaths().len()
        && a.subpaths().iter().zip(b.subpaths()).all(|(x, y)| {
            x.is_closed() == y.is_closed()
                && x.segments().len() == y.segments().len()
                && x.start().distance(y.start()) <= tol_mm
                && x.segments().iter().zip(y.segments()).all(|(p, q)| {
                    std::mem::discriminant(p) == std::mem::discriminant(q) && p.end().distance(q.end()) <= tol_mm
                })
        })
}
