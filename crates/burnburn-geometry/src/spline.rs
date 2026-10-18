//! Splines and conics as exact cubic Bézier paths.

use crate::{GeometryError, Path, PathBuilder, Point};

/// Pieces a conic is cut into (three halvings); each becomes one cubic.
const CONIC_HALVINGS: u32 = 3;

fn finite(points: &[Point]) -> Result<(), GeometryError> {
    if points.iter().all(|p| p.is_finite()) { Ok(()) } else { Err(GeometryError::NotFinite) }
}

fn combine(weights: &[(f64, Point)]) -> Point {
    let (x, y) = weights.iter().fold((0.0, 0.0), |(x, y), (w, p)| (x + w * p.x, y + w * p.y));
    Point::new(x, y)
}

impl Path {
    /// A cubic spline through `points` (at least two; repeated neighbours are merged), with
    /// Catmull-Rom tangents `(next − previous) / 2` that are continuous at every fit point.
    /// A closed spline needs three points and has one cubic per point.
    pub fn spline_fit(points: &[Point], closed: bool) -> Result<Path, GeometryError> {
        finite(points)?;
        let mut pts: Vec<Point> = Vec::with_capacity(points.len());
        for p in points {
            if pts.last() != Some(p) {
                pts.push(*p);
            }
        }
        if closed && pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        let n = pts.len();
        if n < if closed { 3 } else { 2 } {
            return Err(GeometryError::TooFewPoints);
        }
        // Neighbour lookup: clamped for an open spline, wrapping for a closed one.
        let at = |i: isize| -> Point {
            if closed { pts[i.rem_euclid(n as isize) as usize] } else { pts[i.clamp(0, n as isize - 1) as usize] }
        };
        let segments = if closed { n } else { n - 1 };
        let mut b = PathBuilder::new().move_to(pts[0]);
        for k in 0..segments as isize {
            let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
            let c1 = combine(&[(1.0, p1), (1.0 / 6.0, p2), (-1.0 / 6.0, p0)]);
            let c2 = combine(&[(1.0, p2), (-1.0 / 6.0, p3), (1.0 / 6.0, p1)]);
            b = b.cubic_to(c1, c2, p2);
        }
        if closed { b.close().build() } else { b.build() }
    }

    /// A cubic B-spline with the given control polygon (at least four points when open, three
    /// when closed): clamped when open, so it touches the first and last control point and four
    /// points give exactly one Bézier; periodic when closed.
    pub fn spline_control(points: &[Point], closed: bool) -> Result<Path, GeometryError> {
        finite(points)?;
        let n = points.len();
        if n < if closed { 3 } else { 4 } {
            return Err(GeometryError::TooFewPoints);
        }
        if closed {
            let at = |i: isize| points[i.rem_euclid(n as isize) as usize];
            let bezier = |k: isize| {
                let (q0, q1, q2, q3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
                [
                    combine(&[(1.0 / 6.0, q0), (4.0 / 6.0, q1), (1.0 / 6.0, q2)]),
                    combine(&[(2.0 / 3.0, q1), (1.0 / 3.0, q2)]),
                    combine(&[(1.0 / 3.0, q1), (2.0 / 3.0, q2)]),
                    combine(&[(1.0 / 6.0, q1), (4.0 / 6.0, q2), (1.0 / 6.0, q3)]),
                ]
            };
            let mut b = PathBuilder::new().move_to(bezier(0)[0]);
            for k in 0..n as isize {
                let [_, c1, c2, to] = bezier(k);
                b = b.cubic_to(c1, c2, to);
            }
            return b.close().build();
        }
        // Open uniform (clamped) knot vector for degree 3.
        let spans = n - 3;
        let mut knots = vec![0.0; 4];
        knots.extend((1..spans).map(|i| i as f64));
        knots.extend(std::iter::repeat_n(spans as f64, 4));
        let mut b = PathBuilder::new().move_to(points[0]);
        for i in 3..n {
            let (lo, hi) = (knots[i], knots[i + 1]);
            if hi > lo {
                b = b.cubic_to(
                    blossom(points, &knots, i, [lo, lo, hi]),
                    blossom(points, &knots, i, [lo, hi, hi]),
                    blossom(points, &knots, i, [hi, hi, hi]),
                );
            }
        }
        b.build()
    }

    /// A conic from `start` to `end` whose tangents meet at `apex`. `rho` in (0, 1) is the
    /// ratio of the curve's distance from the chord midpoint to the apex's (0.5 = parabola;
    /// larger bulges toward the apex). Built as eight cubics.
    pub fn conic(start: Point, end: Point, apex: Point, rho: f64) -> Result<Path, GeometryError> {
        finite(&[start, end, apex])?;
        if !(rho.is_finite() && rho > 0.0 && rho < 1.0) || start == end {
            return Err(GeometryError::InvalidParameter);
        }
        let w = rho / (1.0 - rho);
        // Rational quadratic Bézier in homogeneous coordinates (x·w, y·w, w).
        type H = [f64; 3];
        let mid = |a: H, b: H| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
        let mut pieces: Vec<[H; 3]> = vec![[[start.x, start.y, 1.0], [apex.x * w, apex.y * w, w], [end.x, end.y, 1.0]]];
        for _ in 0..CONIC_HALVINGS {
            pieces = pieces
                .into_iter()
                .flat_map(|[h0, h1, h2]| {
                    let (h01, h12) = (mid(h0, h1), mid(h1, h2));
                    let h = mid(h01, h12);
                    [[h0, h01, h], [h, h12, h2]]
                })
                .collect();
        }
        let euclid = |h: H| Point::new(h[0] / h[2], h[1] / h[2]);
        let mut b = PathBuilder::new().move_to(start);
        for [h0, h1, h2] in pieces {
            let (p0, p1, p2) = (euclid(h0), euclid(h1), euclid(h2));
            // Exact degree elevation of the control polygon.
            b = b.cubic_to(p0.lerp(p1, 2.0 / 3.0), p2.lerp(p1, 2.0 / 3.0), p2);
        }
        b.build()
    }
}

/// Blossom (polar form) of the cubic piece on knot span `i` of the B-spline with control
/// points `d`, evaluated with de Boor's recurrence using one argument per level.
fn blossom(d: &[Point], t: &[f64], i: usize, args: [f64; 3]) -> Point {
    let mut pts = [d[i - 3], d[i - 2], d[i - 1], d[i]];
    for r in 1..=3usize {
        for j in (r..=3usize).rev() {
            let (lo, hi) = (t[j + i - 3], t[j + 1 + i - r]);
            let alpha = (args[r - 1] - lo) / (hi - lo);
            pts[j] = pts[j - 1].lerp(pts[j], alpha);
        }
    }
    pts[3]
}
