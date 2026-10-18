//! Corner detection and snapping, polygon simplification (Douglas–Peucker) and least-squares cubic
//! Bézier fitting with Newton reparameterisation (after Schneider, Graphics Gems 1990).
//!
//! Every emitted segment satisfies the distance budget in both directions on its data: each source
//! point lies within `tol` of the segment (point-at-parameter bound) and sampled points of the
//! segment lie within `tol` of the source polyline. Otherwise the piece is split; the final fallback
//! is the simplified polyline itself, which is within budget by construction.

use burnburn_geometry::{Point, Segment, Vector};

/// Fitting input in millimetres.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FitParams {
    pub tol: f64,
    pub corner_cos: f64,
    pub window: f64,
    pub curves: bool,
}

/// A fitted closed contour; `spans[k]` = range of source indices `[a, b)` (b may exceed `n`, wrap).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fitted {
    pub start: Point,
    pub segments: Vec<Segment>,
    pub spans: Vec<(usize, usize)>,
}

const MAX_DEPTH: u32 = 24;
const NEWTON_ITERATIONS: usize = 4;
/// Curve samples per fitted cubic for the curve-to-data check.
const CURVE_SAMPLES: usize = 16;

/// A data point with its index in the source contour.
#[derive(Debug, Clone, Copy)]
struct Datum {
    p: Point,
    idx: usize,
}

fn unit(v: Vector) -> Option<Vector> {
    let l = v.length();
    if l > 1e-12 && l.is_finite() { Some(v * (1.0 / l)) } else { None }
}

pub(crate) fn dist_point_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 <= 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn bezier(b: &[Point; 4], t: f64) -> Point {
    let mt = 1.0 - t;
    let (w0, w1, w2, w3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
    Point::new(
        w0 * b[0].x + w1 * b[1].x + w2 * b[2].x + w3 * b[3].x,
        w0 * b[0].y + w1 * b[1].y + w2 * b[2].y + w3 * b[3].y,
    )
}

fn bezier_d1(b: &[Point; 4], t: f64) -> Vector {
    let mt = 1.0 - t;
    let (d0, d1, d2) = (b[1] - b[0], b[2] - b[1], b[3] - b[2]);
    (d0 * (mt * mt) + d1 * (2.0 * mt * t) + d2 * (t * t)) * 3.0
}

fn bezier_d2(b: &[Point; 4], t: f64) -> Vector {
    let e0 = (b[2] - b[1]) - (b[1] - b[0]);
    let e1 = (b[3] - b[2]) - (b[2] - b[1]);
    (e0 * (1.0 - t) + e1 * t) * 6.0
}

/// Distance from `p` to a cubic: dense sampling, then golden-section refinement of the best bracket.
pub(crate) fn dist_point_cubic(p: Point, b: &[Point; 4]) -> f64 {
    const N: usize = 64;
    let mut best = (f64::INFINITY, 0usize);
    for i in 0..=N {
        let d = p.distance(bezier(b, i as f64 / N as f64));
        if d < best.0 {
            best = (d, i);
        }
    }
    let (mut lo, mut hi) = (best.1.saturating_sub(1) as f64 / N as f64, ((best.1 + 1).min(N)) as f64 / N as f64);
    let g = 0.618_033_988_749_895;
    for _ in 0..40 {
        let (m1, m2) = (hi - g * (hi - lo), lo + g * (hi - lo));
        if p.distance(bezier(b, m1)) < p.distance(bezier(b, m2)) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    best.0.min(p.distance(bezier(b, 0.5 * (lo + hi))))
}

pub(crate) fn dist_point_to_segment(p: Point, from: Point, seg: &Segment) -> f64 {
    match *seg {
        Segment::Line { to } => dist_point_segment(p, from, to),
        Segment::Cubic { c1, c2, to } => dist_point_cubic(p, &[from, c1, c2, to]),
        // The tracer never emits arcs; measure against the chord as a conservative fallback.
        Segment::Arc(a) => dist_point_segment(p, from, a.end_point()),
    }
}

fn polyline_distance(p: Point, data: &[Datum]) -> f64 {
    data.windows(2).map(|w| dist_point_segment(p, w[0].p, w[1].p)).fold(f64::INFINITY, f64::min)
}

/// Turning angle cosine at each vertex, measured between chords of arc length `window`.
fn turn_cosines(pts: &[Point], window: f64) -> Vec<f64> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let reach = |step: isize| -> Point {
                let mut j = i;
                let mut acc = 0.0;
                for _ in 0..n {
                    let k = (j as isize + step).rem_euclid(n as isize) as usize;
                    acc += pts[j].distance(pts[k]);
                    j = k;
                    if acc >= window {
                        break;
                    }
                }
                pts[j]
            };
            match (unit(pts[i] - reach(-1)), unit(reach(1) - pts[i])) {
                (Some(a), Some(b)) => a.dot(b),
                _ => 1.0,
            }
        })
        .collect()
}

/// Indices of points `[from, to)` walking `step` (±1) from `start`, with arc distance from `start`
/// in `[lo, hi]`.
fn side_points(pts: &[Point], start: usize, step: isize, lo: f64, hi: f64) -> Vec<Point> {
    let n = pts.len() as isize;
    let mut out = Vec::new();
    let mut acc = 0.0;
    let mut j = start as isize;
    for _ in 0..pts.len() {
        let k = (j + step).rem_euclid(n);
        acc += pts[j as usize].distance(pts[k as usize]);
        j = k;
        if acc > hi {
            break;
        }
        if acc >= lo {
            out.push(pts[j as usize]);
        }
    }
    out
}

/// Least-squares line (centroid, unit direction) through `pts`.
fn fit_line(pts: &[Point]) -> Option<(Point, Vector)> {
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len() as f64;
    let c = Point::new(pts.iter().map(|p| p.x).sum::<f64>() / n, pts.iter().map(|p| p.y).sum::<f64>() / n);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in pts {
        let d = *p - c;
        sxx += d.x * d.x;
        sxy += d.x * d.y;
        syy += d.y * d.y;
    }
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    Some((c, Vector::new(angle.cos(), angle.sin())))
}

fn intersect(p: Point, u: Vector, q: Point, v: Vector) -> Option<Point> {
    let den = u.cross(v);
    if den.abs() < 0.34 {
        // Lines closer than ~20° to parallel give an unstable intersection.
        return None;
    }
    let t = (q - p).cross(v) / den;
    Some(p + u * t)
}

/// An anchor of the contour: a corner (tangent break) or a smooth split point.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    idx: usize,
    at: Point,
    /// Unit tangents leaving forward and arriving (pointing forward) at this anchor.
    out_tangent: Option<Vector>,
    in_tangent: Option<Vector>,
    corner: bool,
}

pub(crate) fn fit_closed(pts: &[Point], params: &FitParams) -> Fitted {
    let n = pts.len();
    if n < 3 {
        return polygon_fallback(pts);
    }
    let perimeter: f64 = (0..n).map(|i| pts[i].distance(pts[(i + 1) % n])).sum();
    let w = params.window;
    if perimeter < 4.0 * w {
        return simplify_closed(pts, params.tol);
    }

    // Corners: turning angle above the threshold and a local maximum within the window.
    let cos = turn_cosines(pts, w);
    let mut corners = Vec::new();
    for i in 0..n {
        if cos[i] > params.corner_cos {
            continue;
        }
        let mut is_max = true;
        for step in [-1isize, 1] {
            let mut acc = 0.0;
            let mut j = i as isize;
            for _ in 0..n {
                let k = (j + step).rem_euclid(n as isize);
                acc += pts[j as usize].distance(pts[k as usize]);
                j = k;
                if acc > w || j as usize == i {
                    break;
                }
                let cj = cos[j as usize];
                if cj < cos[i] || (cj == cos[i] && (j as usize) < i) {
                    is_max = false;
                }
            }
        }
        if is_max {
            corners.push(i);
        }
    }

    let arc_between = |a: usize, b: usize| -> f64 {
        let mut acc = 0.0;
        let mut j = a;
        while j != b {
            let k = (j + 1) % n;
            acc += pts[j].distance(pts[k]);
            j = k;
        }
        acc
    };

    let mut anchors: Vec<Anchor> = Vec::new();
    let excl = 0.5 * w;
    if corners.is_empty() {
        // Smooth closed curve: two split points with central-difference tangents.
        let mut half = 0;
        let mut acc = 0.0;
        while acc < perimeter / 2.0 && half + 1 < n {
            acc += pts[half].distance(pts[half + 1]);
            half += 1;
        }
        for idx in [0, half] {
            let back = side_points(pts, idx, -1, 0.0, w);
            let ahead = side_points(pts, idx, 1, 0.0, w);
            let t = match (back.last(), ahead.last()) {
                (Some(&b), Some(&a)) => unit(a - b),
                _ => None,
            };
            anchors.push(Anchor { idx, at: pts[idx], out_tangent: t, in_tangent: t, corner: false });
        }
    } else {
        let m = corners.len();
        for (ci, &c) in corners.iter().enumerate() {
            let prev = corners[(ci + m - 1) % m];
            let next = corners[(ci + 1) % m];
            let before_len = if m == 1 { perimeter } else { arc_between(prev, c) };
            let after_len = if m == 1 { perimeter } else { arc_between(c, next) };
            let before = side_points(pts, c, -1, excl, (4.0 * w).min(0.45 * before_len).max(excl));
            let after = side_points(pts, c, 1, excl, (4.0 * w).min(0.45 * after_len).max(excl));
            // `before` runs backwards from the corner, `after` forwards; orient both along traversal.
            let orient = |l: Option<(Point, Vector)>, from: Option<&Point>, to: Option<&Point>| {
                let (p, d) = l?;
                let (f, t) = (from?, to?);
                Some((p, if (*t - *f).dot(d) < 0.0 { -d } else { d }))
            };
            let lb = orient(fit_line(&before), before.last(), before.first());
            let la = orient(fit_line(&after), after.first(), after.last());
            let mut at = pts[c];
            if let (Some((pb, db)), Some((pa, da))) = (lb, la)
                && let Some(x) = intersect(pb, db, pa, da)
                && x.distance(pts[c]) <= 2.0 * w
            {
                at = x;
            }
            anchors.push(Anchor {
                idx: c,
                at,
                out_tangent: la.map(|l| l.1),
                in_tangent: lb.map(|l| l.1),
                corner: true,
            });
        }
    }

    let mut out = Fitted { start: anchors[0].at, segments: Vec::new(), spans: Vec::new() };
    let k = anchors.len();
    for ai in 0..k {
        let a = anchors[ai];
        let b = anchors[(ai + 1) % k];
        let end_idx = if b.idx > a.idx { b.idx } else { b.idx + n };
        let mut data = vec![Datum { p: a.at, idx: a.idx }];
        let mut acc = 0.0;
        let total = arc_between(a.idx, b.idx % n).max(if k == 1 { perimeter } else { 0.0 });
        let mut j = a.idx;
        for idx in a.idx + 1..end_idx {
            acc += pts[j % n].distance(pts[idx % n]);
            j = idx;
            let near_a = a.corner && acc < excl;
            let near_b = b.corner && total - acc < excl;
            if !(near_a || near_b) {
                data.push(Datum { p: pts[idx % n], idx });
            }
        }
        data.push(Datum { p: b.at, idx: end_idx });
        let t1 = a.out_tangent;
        let t2 = b.in_tangent.map(|t| -t);
        fit_piece(&data, t1, t2, params, 0, &mut out);
    }
    out
}

fn line_fits(data: &[Datum], tol: f64) -> bool {
    let (a, b) = match (data.first(), data.last()) {
        (Some(a), Some(b)) => (a.p, b.p),
        _ => return true,
    };
    data.iter().all(|d| dist_point_segment(d.p, a, b) <= tol)
}

fn push_line(data: &[Datum], out: &mut Fitted) {
    if let (Some(a), Some(b)) = (data.first(), data.last()) {
        out.segments.push(Segment::Line { to: b.p });
        out.spans.push((a.idx, b.idx));
    }
}

/// Douglas–Peucker on an open chain, emitting lines.
fn push_simplified(data: &[Datum], tol: f64, out: &mut Fitted) {
    let mut keep = vec![false; data.len()];
    if let Some(l) = keep.last_mut() {
        *l = true;
    }
    let mut stack = vec![(0usize, data.len().saturating_sub(1))];
    while let Some((i, j)) = stack.pop() {
        if j <= i + 1 {
            continue;
        }
        let (mut worst, mut at) = (0.0, i);
        for m in i + 1..j {
            let d = dist_point_segment(data[m].p, data[i].p, data[j].p);
            if d > worst {
                worst = d;
                at = m;
            }
        }
        if worst > tol {
            keep[at] = true;
            stack.push((i, at));
            stack.push((at, j));
        }
    }
    let mut from = 0;
    for (m, &k) in keep.iter().enumerate().skip(1) {
        if k {
            push_line(&data[from..=m], out);
            from = m;
        }
    }
}

fn chord_params(data: &[Datum]) -> Vec<f64> {
    let mut u = vec![0.0; data.len()];
    for i in 1..data.len() {
        u[i] = u[i - 1] + data[i].p.distance(data[i - 1].p);
    }
    let total = u.last().copied().unwrap_or(0.0);
    if total > 0.0 {
        for v in &mut u {
            *v /= total;
        }
    }
    u
}

fn generate(data: &[Datum], u: &[f64], t1: Vector, t2: Vector) -> [Point; 4] {
    let (p0, p3) = (data[0].p, data[data.len() - 1].p);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (d, &t) in data.iter().zip(u) {
        let mt = 1.0 - t;
        let (b0, b1, b2, b3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
        let a1 = t1 * b1;
        let a2 = t2 * b2;
        c00 += a1.dot(a1);
        c01 += a1.dot(a2);
        c11 += a2.dot(a2);
        let base = p0.to_vector() * (b0 + b1) + p3.to_vector() * (b2 + b3);
        let r = d.p.to_vector() - base;
        x0 += r.dot(a1);
        x1 += r.dot(a2);
    }
    let det = c00 * c11 - c01 * c01;
    let seg_len = p0.distance(p3);
    let eps = 1e-6 * seg_len;
    let (mut a1, mut a2) =
        if det.abs() > 1e-12 { ((x0 * c11 - c01 * x1) / det, (c00 * x1 - c01 * x0) / det) } else { (0.0, 0.0) };
    if !(a1 > eps && a2 > eps && a1.is_finite() && a2.is_finite()) {
        a1 = seg_len / 3.0;
        a2 = seg_len / 3.0;
    }
    [p0, p0 + t1 * a1, p3 + t2 * a2, p3]
}

fn reparameterize(data: &[Datum], u: &mut [f64], b: &[Point; 4]) {
    for (d, t) in data.iter().zip(u.iter_mut()) {
        let q = bezier(b, *t) - d.p;
        let (q1, q2) = (bezier_d1(b, *t), bezier_d2(b, *t));
        let den = q1.dot(q1) + q.dot(q2);
        if den.abs() > 1e-18 {
            *t = (*t - q.dot(q1) / den).clamp(0.0, 1.0);
        }
    }
}

/// (max point-at-parameter error, index of the worst interior point).
fn max_error(data: &[Datum], u: &[f64], b: &[Point; 4]) -> (f64, usize) {
    let mut worst = (0.0, data.len() / 2);
    for i in 1..data.len().saturating_sub(1) {
        let e = bezier(b, u[i]).distance(data[i].p);
        if e > worst.0 {
            worst = (e, i);
        }
    }
    worst
}

fn curve_stays_near_data(b: &[Point; 4], data: &[Datum], tol: f64) -> bool {
    (1..CURVE_SAMPLES).all(|k| polyline_distance(bezier(b, k as f64 / CURVE_SAMPLES as f64), data) <= tol)
}

fn fit_piece(data: &[Datum], t1: Option<Vector>, t2: Option<Vector>, params: &FitParams, depth: u32, out: &mut Fitted) {
    if data.len() <= 2 || line_fits(data, params.tol) {
        push_line(data, out);
        return;
    }
    let (Some(t1), Some(t2), true) = (t1, t2, params.curves) else {
        push_simplified(data, params.tol, out);
        return;
    };
    if depth >= MAX_DEPTH || data.len() < 4 {
        push_simplified(data, params.tol, out);
        return;
    }
    let mut u = chord_params(data);
    let mut b = generate(data, &u, t1, t2);
    let (mut err, mut split) = max_error(data, &u, &b);
    if err <= 4.0 * params.tol {
        for _ in 0..NEWTON_ITERATIONS {
            if err <= params.tol && curve_stays_near_data(&b, data, params.tol) {
                break;
            }
            reparameterize(data, &mut u, &b);
            b = generate(data, &u, t1, t2);
            (err, split) = max_error(data, &u, &b);
        }
    }
    if err <= params.tol && curve_stays_near_data(&b, data, params.tol) {
        out.segments.push(Segment::Cubic { c1: b[1], c2: b[2], to: b[3] });
        out.spans.push((data[0].idx, data[data.len() - 1].idx));
        return;
    }
    let split = split.clamp(1, data.len() - 2);
    let tc = unit(data[split + 1].p - data[split - 1].p);
    fit_piece(&data[..=split], Some(t1), tc.map(|t| -t), params, depth + 1, out);
    fit_piece(&data[split..], tc, Some(t2), params, depth + 1, out);
}

fn polygon_fallback(pts: &[Point]) -> Fitted {
    let n = pts.len();
    let start = pts.first().copied().unwrap_or(Point::ORIGIN);
    let mut out = Fitted { start, segments: Vec::new(), spans: Vec::new() };
    for i in 1..=n {
        out.segments.push(Segment::Line { to: pts[i % n] });
        out.spans.push((i - 1, i));
    }
    out
}

/// Closed-polygon simplification within `tol`, anchored at the vertex farthest from vertex 0.
fn simplify_closed(pts: &[Point], tol: f64) -> Fitted {
    let n = pts.len();
    let far = (0..n).max_by(|&a, &b| pts[a].distance(pts[0]).total_cmp(&pts[b].distance(pts[0]))).unwrap_or(0);
    let mut out = Fitted { start: pts[0], segments: Vec::new(), spans: Vec::new() };
    let first: Vec<Datum> = (0..=far).map(|i| Datum { p: pts[i], idx: i }).collect();
    let second: Vec<Datum> = (far..=n).map(|i| Datum { p: pts[i % n], idx: i }).collect();
    push_simplified(&first, tol, &mut out);
    push_simplified(&second, tol, &mut out);
    out
}

/// End tangent estimate of an open chain: chord from `data[0]` to the point about `window` along it.
fn end_tangent(data: &[Datum], window: f64, forward: bool) -> Option<Vector> {
    let n = data.len();
    if n < 2 {
        return None;
    }
    let at = |k: usize| if forward { data[k].p } else { data[n - 1 - k].p };
    let mut acc = 0.0;
    let mut k = 1;
    while k + 1 < n {
        acc += at(k).distance(at(k - 1));
        if acc >= window {
            break;
        }
        k += 1;
    }
    unit(at(k) - at(0))
}

/// Fits an open chain whose first and last data points are fixed nodes (anchors). Interior
/// `splits` (indices into `data`, strictly inside) become nodes too. Every segment honours the
/// distance budget like [`fit_closed`]; `idx` values are passed through into the spans.
pub(crate) fn fit_open_indexed(data: &[(Point, usize)], splits: &[usize], params: &FitParams, out: &mut Fitted) {
    let data: Vec<Datum> = data.iter().map(|&(p, idx)| Datum { p, idx }).collect();
    let mut cuts: Vec<usize> = splits.iter().copied().filter(|&s| s > 0 && s + 1 < data.len()).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut from = 0;
    for &c in cuts.iter().chain(std::iter::once(&(data.len().saturating_sub(1)))) {
        if c <= from {
            continue;
        }
        let piece = &data[from..=c];
        let t1 = end_tangent(piece, params.window, true);
        let t2 = end_tangent(piece, params.window, false);
        fit_piece(piece, t1, t2, params, 0, out);
        from = c;
    }
}

/// Closed contour with user anchors (indices into `pts`, node placed exactly at the given point).
/// Without anchors this is [`fit_closed`].
pub(crate) fn fit_closed_anchored(pts: &[Point], anchors: &[(usize, Point)], params: &FitParams) -> Fitted {
    let n = pts.len();
    let mut anchors: Vec<(usize, Point)> = anchors.iter().copied().filter(|a| a.0 < n).collect();
    anchors.sort_by_key(|a| a.0);
    anchors.dedup_by_key(|a| a.0);
    let Some(&(_, first_at)) = anchors.first() else { return fit_closed(pts, params) };
    let mut out = Fitted { start: first_at, segments: Vec::new(), spans: Vec::new() };
    let k = anchors.len();
    for ai in 0..k {
        let (a, a_at) = anchors[ai];
        let (b, b_at) = anchors[(ai + 1) % k];
        let end = if b > a { b } else { b + n };
        let mut data = vec![(a_at, a)];
        data.extend((a + 1..end).map(|i| (pts[i % n], i)));
        data.push((b_at, end));
        fit_open_indexed(&data, &[], params, &mut out);
    }
    out
}
