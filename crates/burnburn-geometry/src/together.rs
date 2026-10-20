//! Move Together (E04): remove the gaps between objects along one axis.

use std::fmt;

use crate::{Bounds, Direction, Vector};

/// Which object keeps its place; the others close in on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TogetherAnchor {
    /// The object with the lowest position along the axis.
    Start,
    /// The object with the highest position along the axis.
    End,
    /// One of the input boxes (index into the slice).
    Object(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TogetherError {
    /// Moving together needs at least two objects.
    TooFew,
    NoSuchReference(usize),
    /// The remaining gap must be finite and not negative.
    InvalidGap,
}

impl fmt::Display for TogetherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot move together: {self:?}")
    }
}

impl std::error::Error for TogetherError {}

/// Translation per input box. Objects close in on the anchor but never move apart: an overlap stays
/// as it is. `gap` is the space left between neighbours. Neighbours are ordered by their centres
/// (ties by input index), so the result does not depend on the order of the selection.
pub fn move_together_offsets(
    bounds: &[Bounds],
    direction: Direction,
    anchor: TogetherAnchor,
    gap: f64,
) -> Result<Vec<Vector>, TogetherError> {
    if bounds.len() < 2 {
        return Err(TogetherError::TooFew);
    }
    if !gap.is_finite() || gap < 0.0 {
        return Err(TogetherError::InvalidGap);
    }
    let horizontal = direction == Direction::Horizontal;
    let lo = |b: &Bounds| if horizontal { b.min().x } else { b.min().y };
    let hi = |b: &Bounds| if horizontal { b.max().x } else { b.max().y };
    let centre = |b: &Bounds| if horizontal { b.center().x } else { b.center().y };
    let n = bounds.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| centre(&bounds[a]).total_cmp(&centre(&bounds[b])).then(a.cmp(&b)));
    let reference = match anchor {
        TogetherAnchor::Start => order[0],
        TogetherAnchor::End => order[n - 1],
        TogetherAnchor::Object(i) if i < n => i,
        TogetherAnchor::Object(i) => return Err(TogetherError::NoSuchReference(i)),
    };
    let k = order.iter().position(|&i| i == reference).unwrap_or(0);
    let mut shift = vec![0.0; n];
    let mut cursor = hi(&bounds[reference]);
    for &i in &order[k + 1..] {
        let b = &bounds[i];
        let wanted = cursor + gap;
        if lo(b) > wanted {
            shift[i] = wanted - lo(b);
        }
        cursor = cursor.max(hi(b) + shift[i]);
    }
    let mut cursor = lo(&bounds[reference]);
    for &i in order[..k].iter().rev() {
        let b = &bounds[i];
        let wanted = cursor - gap;
        if hi(b) < wanted {
            shift[i] = wanted - hi(b);
        }
        cursor = cursor.min(lo(b) + shift[i]);
    }
    Ok(shift.into_iter().map(|d| if horizontal { Vector::new(d, 0.0) } else { Vector::new(0.0, d) }).collect())
}
