//! Align and distribute (E04): pure translations, so sizes are preserved exactly.

use std::fmt;

use crate::{Bounds, Vector};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignEdge {
    Left,
    CenterX,
    Right,
    Bottom,
    CenterY,
    Top,
}

/// What the edge is aligned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignTo {
    /// The combined bounds of all objects.
    Selection,
    /// The bounds of one of them (index into the slice), which does not move.
    Object(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Spacing {
    /// Equal distance between centres.
    Centers,
    /// Equal free space between neighbouring boxes.
    Gaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AlignError {
    NothingToAlign,
    NoSuchReference(usize),
    /// Distribution needs at least three objects.
    TooFew,
}

impl fmt::Display for AlignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot align: {self:?}")
    }
}

impl std::error::Error for AlignError {}

fn edge_value(b: &Bounds, edge: AlignEdge) -> f64 {
    match edge {
        AlignEdge::Left => b.min().x,
        AlignEdge::CenterX => b.center().x,
        AlignEdge::Right => b.max().x,
        AlignEdge::Bottom => b.min().y,
        AlignEdge::CenterY => b.center().y,
        AlignEdge::Top => b.max().y,
    }
}

/// Translation per input box so that `edge` lines up with the reference.
pub fn align_offsets(bounds: &[Bounds], edge: AlignEdge, to: AlignTo) -> Result<Vec<Vector>, AlignError> {
    let reference = match to {
        AlignTo::Selection => bounds.iter().copied().reduce(Bounds::union).ok_or(AlignError::NothingToAlign)?,
        AlignTo::Object(i) => *bounds.get(i).ok_or(AlignError::NoSuchReference(i))?,
    };
    let target = edge_value(&reference, edge);
    let horizontal = matches!(edge, AlignEdge::Left | AlignEdge::CenterX | AlignEdge::Right);
    Ok(bounds
        .iter()
        .map(|b| {
            let d = target - edge_value(b, edge);
            if horizontal { Vector::new(d, 0.0) } else { Vector::new(0.0, d) }
        })
        .collect())
}

/// Translation per input box; the two extreme boxes (by centre) stay where they are.
pub fn distribute_offsets(bounds: &[Bounds], dir: Direction, spacing: Spacing) -> Result<Vec<Vector>, AlignError> {
    let n = bounds.len();
    if n < 3 {
        return Err(AlignError::TooFew);
    }
    let horizontal = dir == Direction::Horizontal;
    let lo = |b: &Bounds| if horizontal { b.min().x } else { b.min().y };
    let hi = |b: &Bounds| if horizontal { b.max().x } else { b.max().y };
    let center = |b: &Bounds| if horizontal { b.center().x } else { b.center().y };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| center(&bounds[a]).total_cmp(&center(&bounds[b])).then(a.cmp(&b)));
    let mut shift = vec![0.0; n];
    match spacing {
        Spacing::Centers => {
            let (c0, cn) = (center(&bounds[order[0]]), center(&bounds[order[n - 1]]));
            for (k, &i) in order.iter().enumerate().take(n - 1).skip(1) {
                shift[i] = c0 + (cn - c0) * (k as f64) / ((n - 1) as f64) - center(&bounds[i]);
            }
        }
        Spacing::Gaps => {
            let span = hi(&bounds[order[n - 1]]) - lo(&bounds[order[0]]);
            let widths: f64 = bounds.iter().map(|b| hi(b) - lo(b)).sum();
            let gap = (span - widths) / ((n - 1) as f64);
            let mut cursor = lo(&bounds[order[0]]);
            for (k, &i) in order.iter().enumerate() {
                if k > 0 && k < n - 1 {
                    shift[i] = cursor - lo(&bounds[i]);
                }
                cursor += hi(&bounds[i]) - lo(&bounds[i]) + gap;
            }
        }
    }
    Ok(shift
        .into_iter()
        .map(|d| match dir {
            Direction::Horizontal => Vector::new(d, 0.0),
            Direction::Vertical => Vector::new(0.0, d),
        })
        .collect())
}
