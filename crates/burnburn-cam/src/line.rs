//! Line planning: contour order (inner before outer, then nearest neighbour), entry points and
//! the cut-path options of IM25 (tabs, leads, overcut, perforation) plus the shared-edge policy
//! of IM32.

use std::collections::{BTreeMap, BTreeSet};

use burnburn_document::process::{LeadStyle, LineOptions, SharedEdgePolicy, TabPlacement, TabSpec};
use burnburn_geometry::{Bounds, Point, SpatialIndex, Vector, math};
use burnburn_types::{CancellationToken, GeometryId, OperationId};

use crate::CompileError;
use crate::compile::Step;
use crate::nearest::{NearestIndex, SearchStats};

/// One flattened subpath of a referenced geometry, in bed-view mm.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Contour {
    pub geometry: GeometryId,
    /// Vertices; for a closed contour without the repeated start point.
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Contour {
    pub fn new(geometry: GeometryId, mut points: Vec<Point>, closed: bool) -> Self {
        if closed && points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        Contour { geometry, points, closed }
    }

    pub fn translate(&mut self, v: Vector) {
        for p in &mut self.points {
            *p = *p + v;
        }
    }
}

/// How contours are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// Inner before outer, then nearest neighbour (cutting).
    Containment,
    /// Exactly the given order; only the entry point is chosen (offset-fill rings).
    AsGiven,
}

/// The planner switches of the document's optimization settings. `Default` is the planner as
/// it was before the settings existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Heuristics {
    pub inner_first: bool,
    pub reduce_travel: bool,
    pub best_start: bool,
    pub best_direction: bool,
    pub hide_backlash: bool,
}

impl Default for Heuristics {
    fn default() -> Self {
        Heuristics {
            inner_first: true,
            reduce_travel: true,
            best_start: true,
            best_direction: true,
            hide_backlash: false,
        }
    }
}

impl From<&burnburn_document::process::OptimizationSettings> for Heuristics {
    fn from(o: &burnburn_document::process::OptimizationSettings) -> Self {
        Heuristics {
            inner_first: o.inner_first(),
            reduce_travel: o.reduce_travel(),
            best_start: o.best_start(),
            best_direction: o.best_direction(),
            hide_backlash: o.hide_backlash(),
        }
    }
}

/// Extra cost (mm of travel) of an approach that reverses the head at the start of a cut, when
/// the backlash heuristic is on. A fixed, documented constant: the heuristic only has to break
/// near-ties deterministically towards approaches that keep the head moving the same way.
pub(crate) const BACKLASH_REVERSAL_PENALTY_MM: f64 = 1.0;

/// Plans all contours of one operation pass, starting at `head`; updates `head`.
///
/// With [`Order::Containment`] a contour that lies inside a closed contour is cut before it
/// (inner before outer, so parts do not drop out early). Among the contours whose inner
/// contours are done, the one whose entry point is nearest to the head goes next; equal
/// distances keep input order (operation target order, then subpath order).
pub(crate) fn plan(
    contours: &[Contour],
    order: Order,
    options: &LineOptions,
    shared: SharedEdgePolicy,
    heuristics: &Heuristics,
    head: &mut Point,
    operation: OperationId,
    tolerance_mm: f64,
    cancel: &CancellationToken,
) -> Result<Vec<Step>, CompileError> {
    let mut stats = PlanStats::default();
    plan_counted(contours, order, options, shared, heuristics, head, operation, tolerance_mm, cancel, &mut stats)
}

/// What the contour selection of one [`plan_counted`] run cost: a deterministic measure for tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlanStats {
    /// How many times the entry of a contour was evaluated while choosing the next one.
    pub entry_evaluations: usize,
}

/// [`plan`], also reporting what the selection cost.
pub(crate) fn plan_counted(
    contours: &[Contour],
    order: Order,
    options: &LineOptions,
    shared: SharedEdgePolicy,
    heuristics: &Heuristics,
    head: &mut Point,
    operation: OperationId,
    tolerance_mm: f64,
    cancel: &CancellationToken,
    stats: &mut PlanStats,
) -> Result<Vec<Step>, CompileError> {
    let usable: Vec<&Contour> = contours.iter().filter(|c| c.points.len() >= 2).collect();
    let inner = match order {
        Order::Containment if heuristics.inner_first => containment(&usable, cancel)?,
        _ => vec![Vec::new(); usable.len()],
    };
    let depth = hole_depth(&usable);
    let mut steps = Vec::new();
    match order {
        Order::AsGiven => {
            for (k, c) in usable.iter().enumerate() {
                cancel.check()?;
                let e = entry(c, *head, heuristics);
                emit(c, &e, options, depth[k] % 2 == 1, tolerance_mm, &mut steps, head, operation)?;
            }
        }
        Order::Containment => {
            let mut chooser = Chooser::new(&usable, &inner, heuristics);
            while let Some((i, e)) = chooser.next(&usable, *head, heuristics, stats) {
                cancel.check()?;
                emit(usable[i], &e, options, depth[i] % 2 == 1, tolerance_mm, &mut steps, head, operation)?;
            }
        }
    }
    if shared == SharedEdgePolicy::CutOnce {
        let start = start_of(&steps);
        steps = cut_shared_edges_once(steps, start);
    }
    Ok(steps)
}

/// Picks the next contour of a [`Order::Containment`] pass: among the ready contours (all
/// contours inside them are cut) the one with the cheapest entry; equal costs keep input order.
/// Without travel reduction the first ready contour in input order goes next. If containment
/// were ever cyclic (degenerate input) and nothing is ready, any remaining contour qualifies.
struct Chooser {
    done: Vec<bool>,
    /// Contours that contain contour `i` (it must be cut before them).
    outers: Vec<Vec<usize>>,
    /// How many contours inside `i` are still to be cut.
    pending: Vec<usize>,
    ready: BTreeSet<usize>,
    remaining: BTreeSet<usize>,
    /// Only with travel reduction: the ready contours in a spatial index.
    tree: Option<NearestIndex>,
}

impl Chooser {
    fn new(cs: &[&Contour], inner: &[Vec<usize>], h: &Heuristics) -> Self {
        let n = cs.len();
        let mut outers = vec![Vec::new(); n];
        for (j, list) in inner.iter().enumerate() {
            for &i in list {
                outers[i].push(j);
            }
        }
        let pending: Vec<usize> = inner.iter().map(Vec::len).collect();
        let ready: BTreeSet<usize> = (0..n).filter(|&i| pending[i] == 0).collect();
        let tree = h.reduce_travel.then(|| {
            let boxes: Vec<Bounds> = cs.iter().map(|c| entry_bounds(c, h)).collect();
            NearestIndex::build(&boxes, |i| pending[i] == 0)
        });
        Chooser { done: vec![false; n], outers, pending, ready, remaining: (0..n).collect(), tree }
    }

    fn next(&mut self, cs: &[&Contour], head: Point, h: &Heuristics, stats: &mut PlanStats) -> Option<(usize, Entry)> {
        let i = if !self.ready.is_empty() {
            match &self.tree {
                Some(tree) => {
                    let mut search = SearchStats::default();
                    let found = tree.cheapest(head, |i| entry(cs[i], head, h).cost, &mut search).map(|(i, _)| i);
                    stats.entry_evaluations += search.evaluated;
                    found
                }
                None => self.ready.first().copied(),
            }
        } else if h.reduce_travel {
            let mut best: Option<(usize, f64)> = None;
            for &i in &self.remaining {
                stats.entry_evaluations += 1;
                let cost = entry(cs[i], head, h).cost;
                if best.is_none_or(|(_, b)| cost < b) {
                    best = Some((i, cost));
                }
            }
            best.map(|(i, _)| i)
        } else {
            self.remaining.first().copied()
        }?;
        self.finish(i);
        Some((i, entry(cs[i], head, h)))
    }

    fn finish(&mut self, i: usize) {
        self.done[i] = true;
        self.remaining.remove(&i);
        if self.ready.remove(&i)
            && let Some(tree) = &mut self.tree
        {
            tree.set_live(i, false);
        }
        for k in 0..self.outers[i].len() {
            let j = self.outers[i][k];
            self.pending[j] -= 1;
            if self.pending[j] == 0 && !self.done[j] {
                self.ready.insert(j);
                if let Some(tree) = &mut self.tree {
                    tree.set_live(j, true);
                }
            }
        }
    }
}

/// A box around every point [`entry`] may choose for `c`.
fn entry_bounds(c: &Contour, h: &Heuristics) -> Bounds {
    let candidates: Vec<Point> = if c.closed {
        if h.best_start { c.points.clone() } else { c.points.first().copied().into_iter().collect() }
    } else {
        let mut v: Vec<Point> = c.points.first().copied().into_iter().collect();
        if h.best_direction {
            v.extend(c.points.last().copied());
        }
        v
    };
    bounds_of(&candidates).unwrap_or(Bounds::from_point(Point::ORIGIN))
}

fn start_of(steps: &[Step]) -> Option<Point> {
    match steps.first() {
        Some(Step::Travel(p)) => Some(*p),
        _ => None,
    }
}

/// Where and how to enter a contour.
pub(crate) struct Entry {
    /// Travel distance plus the backlash penalty (if enabled): what candidates are compared by.
    cost: f64,
    /// Closed: index of the entry vertex. Open: 0 = forward, otherwise reversed.
    index: usize,
}

/// The cost of moving the head to `at` and then cutting towards `next`.
fn approach_cost(head: Point, at: Point, next: Option<Point>, h: &Heuristics) -> f64 {
    let d = head.distance(at);
    let reverses =
        h.hide_backlash && next.is_some_and(|n| (at.x - head.x) * (n.x - at.x) + (at.y - head.y) * (n.y - at.y) < 0.0);
    if reverses { d + BACKLASH_REVERSAL_PENALTY_MM } else { d }
}

/// The entry of `c` for a head at `head`: the cheapest vertex of a closed contour (only its
/// first vertex, the shape's own start point, when the best start is off) or the cheaper end of
/// an open one (only its drawn start when the best direction is off). Ties keep the lowest index.
fn entry(c: &Contour, head: Point, h: &Heuristics) -> Entry {
    let n = c.points.len();
    if c.closed {
        let candidates = if h.best_start { n } else { 1.min(n) };
        let mut best = Entry { cost: f64::INFINITY, index: 0 };
        for i in 0..candidates {
            let cost = approach_cost(head, c.points[i], Some(c.points[(i + 1) % n]), h);
            if cost < best.cost {
                best = Entry { cost, index: i };
            }
        }
        best
    } else {
        let forward = match (c.points.first(), c.points.get(1)) {
            (Some(p), next) => approach_cost(head, *p, next.copied(), h),
            _ => f64::INFINITY,
        };
        let backward = match (c.points.last(), n.checked_sub(2).and_then(|k| c.points.get(k))) {
            (Some(p), next) if h.best_direction => approach_cost(head, *p, next.copied(), h),
            _ => f64::INFINITY,
        };
        if backward < forward { Entry { cost: backward, index: 1 } } else { Entry { cost: forward, index: 0 } }
    }
}

/// Twice the signed area (positive = counter-clockwise).
fn signed_area2(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n).map(|k| points[k].x * points[(k + 1) % n].y - points[(k + 1) % n].x * points[k].y).sum()
}

fn unit(v: Vector) -> Vector {
    let len = v.x.hypot(v.y);
    if len > 0.0 { Vector::new(v.x / len, v.y / len) } else { Vector::ZERO }
}

/// Emits one contour: lead-in, path (with overcut), tabs/perforation gaps as laser-off
/// travel, lead-out.
fn emit(
    c: &Contour,
    e: &Entry,
    options: &LineOptions,
    waste_inside: bool,
    tolerance_mm: f64,
    steps: &mut Vec<Step>,
    head: &mut Point,
    operation: OperationId,
) -> Result<(), CompileError> {
    let n = c.points.len();
    let mut path: Vec<Point> = if c.closed {
        // Keep the drawn direction; start at the entry vertex and return to it.
        (0..=n).map(|k| c.points[(e.index + k) % n]).collect()
    } else if e.index == 0 {
        c.points.clone()
    } else {
        c.points.iter().rev().copied().collect()
    };
    let perimeter = length(&path);

    // Side of the waste for leads: right of the travel direction for a counter-clockwise outer
    // contour, flipped for clockwise contours and for holes.
    let ccw = c.closed && signed_area2(&c.points) > 0.0;
    let waste = |d: Vector| -> Vector {
        let right = Vector::new(d.y, -d.x);
        if c.closed == (ccw != waste_inside) { right } else { right * -1.0 }
    };

    if c.closed
        && let Some(o) = options.overcut
    {
        let mut extra = walk(&path, o.length_mm());
        path.append(&mut extra);
    }

    let mut gaps: Vec<(f64, f64)> = Vec::new();
    if let Some(t) = options.tabs
        && c.closed
    {
        let Some(n) = tab_count(&t, perimeter) else {
            return Err(CompileError::InvalidTabs { operation, geometry: c.geometry });
        };
        let count = f64::from(n);
        for i in 0..n {
            let centre = (f64::from(i) + 0.5) * perimeter / count;
            gaps.push((centre - t.width_mm() / 2.0, centre + t.width_mm() / 2.0));
        }
    }
    if let Some(p) = options.perforation {
        let total = length(&path);
        let period = p.on_mm() + p.off_mm();
        let mut k = 0.0;
        while k * period < total {
            gaps.push((k * period + p.on_mm(), (k + 1.0) * period));
            k += 1.0;
        }
    }

    let (Some(&first), Some(&last)) = (path.first(), path.last()) else { return Ok(()) };
    if let Some(dot) = options.dot {
        return emit_dots(&path, c, dot.spacing_mm(), steps, head, operation);
    }
    let lead_dir_in = path.get(1).map_or(Vector::ZERO, |p| unit(*p - first));
    match options.lead_in {
        Some(l) => {
            // Open paths: the lead continues the path backwards; closed: waste side.
            let lead = if c.closed {
                lead_points(first, lead_dir_in, waste(lead_dir_in), &l, true, tolerance_mm)
            } else if l.style() == LeadStyle::Arc {
                return Err(CompileError::ArcLeadOnOpenContour { operation, geometry: c.geometry });
            } else {
                vec![first + lead_dir_in * -1.0 * l.length_mm()]
            };
            let mut points = lead.into_iter();
            if let Some(start) = points.next() {
                steps.push(Step::Travel(start));
                for p in points {
                    steps.push(Step::Cut { to: p, source: c.geometry });
                }
                steps.push(Step::Cut { to: first, source: c.geometry });
            }
        }
        None => steps.push(Step::Travel(first)),
    }
    *head = first;
    emit_with_gaps(&path, &gaps, c.geometry, steps);
    *head = last;
    if let Some(l) = options.lead_out {
        let k = path.len();
        let d_last = if k >= 2 { unit(path[k - 1] - path[k - 2]) } else { Vector::ZERO };
        let lead = if c.closed {
            lead_points(last, d_last, waste(d_last), &l, false, tolerance_mm)
        } else if l.style() == LeadStyle::Arc {
            return Err(CompileError::ArcLeadOnOpenContour { operation, geometry: c.geometry });
        } else {
            vec![last + d_last * l.length_mm()]
        };
        for p in lead {
            steps.push(Step::Cut { to: p, source: c.geometry });
            *head = p;
        }
    }
    Ok(())
}

/// Number of tabs on a closed contour of `perimeter`, or `None` when they cannot be placed:
/// the tabs would cover the whole contour, or the spacing asks for more than
/// [`TabSpec::MAX_COUNT`] of them.
pub(crate) fn tab_count(t: &TabSpec, perimeter: f64) -> Option<u32> {
    let n = match t.placement() {
        TabPlacement::Count(n) => n,
        TabPlacement::Spacing(spacing) => {
            let n = (perimeter / spacing).round().max(1.0);
            if n > f64::from(TabSpec::MAX_COUNT) {
                return None;
            }
            n as u32
        }
    };
    (t.width_mm() * f64::from(n) < perimeter).then_some(n)
}

/// The points of a lead on the waste side `w` of a closed contour at `at`, where the contour
/// runs along `t` (the direction of travel). For a lead-in (`entering`) the points lead up to
/// `at`, which is not included; for a lead-out they lead away from it. A line lead is one point
/// at `angle` to the tangent (90° = perpendicular); an arc lead is a quarter circle of radius
/// `length`, tangent to the contour at `at`, flattened within `tolerance_mm`.
fn lead_points(
    at: Point,
    t: Vector,
    w: Vector,
    lead: &burnburn_document::process::Lead,
    entering: bool,
    tolerance_mm: f64,
) -> Vec<Point> {
    let len = lead.length_mm();
    match lead.style() {
        LeadStyle::Line => {
            let d = if lead.angle_deg() == 90.0 {
                w
            } else {
                let (sin, cos) = math::sin_cos(lead.angle_deg().to_radians());
                let along = if entering { t * -cos } else { t * cos };
                w * sin + along
            };
            vec![at + d * len]
        }
        LeadStyle::Arc => {
            // Centre c = at + r·w; points c + r(−w·cosθ + t·sinθ): θ = 0 is `at`, tangent +t.
            let centre = at + w * len;
            let step = if tolerance_mm >= len { 1.0 } else { (1.0 - tolerance_mm / len).acos() };
            let n = ((std::f64::consts::FRAC_PI_2 / step).ceil() as usize).clamp(4, 512);
            let on_circle = |theta: f64| {
                let (sin, cos) = math::sin_cos(theta);
                centre + (w * -cos + t * sin) * len
            };
            if entering {
                (0..n).map(|k| on_circle(-std::f64::consts::FRAC_PI_2 * (1.0 - k as f64 / n as f64))).collect()
            } else {
                (1..=n).map(|k| on_circle(std::f64::consts::FRAC_PI_2 * k as f64 / n as f64)).collect()
            }
        }
    }
}

/// Dot mode: a pulse every `spacing` of arc length from the entry point. A closed contour does
/// not repeat its start; an open one gets a dot on its end only when it falls on the spacing.
fn emit_dots(
    path: &[Point],
    c: &Contour,
    spacing: f64,
    steps: &mut Vec<Step>,
    head: &mut Point,
    operation: OperationId,
) -> Result<(), CompileError> {
    let ratio = length(path) / spacing;
    let whole = (ratio + 1e-9).floor();
    // On a closed contour the last position coincides with the first when the spacing divides it.
    let closes_exactly = c.closed && (ratio - ratio.round()).abs() < 1e-9;
    let count = if closes_exactly { whole } else { whole + 1.0 };
    if !(count <= MAX_DOTS as f64) {
        return Err(CompileError::TooManyDots { operation, dots: count as u64 });
    }
    for k in 0..count as u64 {
        let at = point_at(path, k as f64 * spacing);
        steps.push(Step::Travel(at));
        steps.push(Step::Dot { source: c.geometry });
        *head = at;
    }
    Ok(())
}

/// Upper bound on dots per contour pass; keeps one setting from making a compile unbounded.
pub(crate) const MAX_DOTS: u64 = 1_000_000;

/// The point `distance` along `path` (clamped to its end).
fn point_at(path: &[Point], distance: f64) -> Point {
    let mut left = distance;
    for w in path.windows(2) {
        let d = w[0].distance(w[1]);
        if d >= left {
            return w[0].lerp(w[1], if d > 0.0 { left / d } else { 0.0 });
        }
        left -= d;
    }
    path.last().copied().unwrap_or(Point::new(0.0, 0.0))
}

fn length(path: &[Point]) -> f64 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The points after `path[0]` along `path` for `distance` (the last one interpolated).
fn walk(path: &[Point], distance: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let mut left = distance;
    for w in path.windows(2) {
        let d = w[0].distance(w[1]);
        if d >= left {
            out.push(w[0].lerp(w[1], if d > 0.0 { left / d } else { 0.0 }));
            return out;
        }
        out.push(w[1]);
        left -= d;
    }
    out
}

/// Cuts along `path` except inside `gaps` (arc-length intervals), which become travel.
fn emit_with_gaps(path: &[Point], gaps: &[(f64, f64)], source: GeometryId, steps: &mut Vec<Step>) {
    let in_gap = |s: f64| gaps.iter().any(|&(a, b)| s >= a && s < b);
    let mut s0 = 0.0;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = a.distance(b);
        // Split points of this segment at gap boundaries.
        let mut cuts: Vec<f64> = gaps.iter().flat_map(|&(g0, g1)| [g0, g1]).filter(|&t| t > s0 && t < s0 + d).collect();
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        cuts.push(s0 + d);
        let mut from = s0;
        for t in cuts {
            let to = if t >= s0 + d { b } else { a.lerp(b, (t - s0) / d) };
            let mid = 0.5 * (from + t);
            if in_gap(mid) {
                steps.push(Step::Travel(to));
            } else {
                steps.push(Step::Cut { to, source });
            }
            from = t;
        }
        s0 += d;
    }
}

/// Exact identity of an undirected segment.
type SegmentKey = ((u64, u64), (u64, u64));

fn key(a: Point, b: Point) -> SegmentKey {
    let (p, q) = ((a.x.to_bits(), a.y.to_bits()), (b.x.to_bits(), b.y.to_bits()));
    if p <= q { (p, q) } else { (q, p) }
}

/// IM32 `CutOnce`: a segment that another geometry already cut in this pass (exactly the same
/// endpoints, either direction) becomes laser-off travel. No tolerance: shared boundaries from
/// one region graph are bit-identical; near-coincident edges are not merged.
fn cut_shared_edges_once(steps: Vec<Step>, start: Option<Point>) -> Vec<Step> {
    let mut cut: BTreeMap<SegmentKey, GeometryId> = BTreeMap::new();
    let mut out = Vec::with_capacity(steps.len());
    let mut at = start;
    for step in steps {
        match step {
            Step::Cut { to, source } => {
                let Some(from) = at else {
                    out.push(step);
                    at = Some(to);
                    continue;
                };
                let k = key(from, to);
                match cut.get(&k) {
                    Some(g) if *g != source => out.push(Step::Travel(to)),
                    _ => {
                        cut.insert(k, source);
                        out.push(step);
                    }
                }
                at = Some(to);
            }
            Step::Travel(p) => {
                out.push(step);
                at = Some(p);
            }
            Step::Scan { to, .. } | Step::PowerScan { to, .. } | Step::RampScan { to, .. } => {
                out.push(step);
                at = Some(to);
            }
            Step::Dot { .. } => out.push(step),
        }
    }
    // Merge consecutive travels: only the last target matters.
    let mut merged: Vec<Step> = Vec::with_capacity(out.len());
    for s in out {
        if let (Step::Travel(_), Some(Step::Travel(_))) = (&s, merged.last()) {
            merged.pop();
        }
        merged.push(s);
    }
    merged
}

/// Nesting depth of each closed contour among the closed contours of the same geometry
/// (odd = hole).
fn hole_depth(cs: &[&Contour]) -> Vec<usize> {
    let index = SpatialIndex::build(
        cs.iter()
            .enumerate()
            .filter(|(_, c)| c.closed && c.points.len() >= 3)
            .filter_map(|(j, c)| Some((j, bounds_of(&c.points)?))),
    );
    cs.iter()
        .enumerate()
        .map(|(i, c)| {
            let Some(p) = c.points.first() else { return 0 };
            // A point outside a polygon's box is never inside it; the margin absorbs rounding of
            // the crossing computation for points right on the box edge.
            index
                .query_rect(&Bounds::from_point(*p).expanded(BOX_MARGIN_MM))
                .into_iter()
                .filter(|&j| {
                    let o = cs[j];
                    j != i && o.geometry == c.geometry && point_in_polygon(*p, &o.points)
                })
                .count()
        })
        .collect()
}

/// Slack on box tests, far above the rounding of coordinates in mm and far below any feature.
const BOX_MARGIN_MM: f64 = 1e-9;

/// For each contour, the contours that lie inside it (and must be cut first).
fn containment(cs: &[&Contour], cancel: &CancellationToken) -> Result<Vec<Vec<usize>>, CompileError> {
    containment_counted(cs, cancel, &mut 0)
}

/// [`containment`], also counting how many (inner, outer) pairs were examined.
fn containment_counted(
    cs: &[&Contour],
    cancel: &CancellationToken,
    pairs: &mut usize,
) -> Result<Vec<Vec<usize>>, CompileError> {
    let bounds: Vec<Option<Bounds>> = cs.iter().map(|c| bounds_of(&c.points)).collect();
    let index = SpatialIndex::build(bounds.iter().enumerate().filter_map(|(i, b)| Some((i, (*b)?))));
    // inside_of[j]: the contours that lie inside j, ascending.
    let mut inside_of: Vec<Vec<usize>> = vec![Vec::new(); cs.len()];
    for (j, outer) in cs.iter().enumerate() {
        cancel.check()?;
        let Some(bj) = bounds[j] else { continue };
        if !outer.closed || outer.points.len() < 3 {
            continue;
        }
        for i in index.query_rect(&bj) {
            *pairs += 1;
            let Some(bi) = bounds[i] else { continue };
            let within = bj.contains(bi.min()) && bj.contains(bi.max()) && bi != bj;
            if i != j && within && cs[i].points.iter().all(|p| point_in_polygon(*p, &outer.points)) {
                inside_of[j].push(i);
            }
        }
        inside_of[j].sort_unstable();
    }
    Ok((0..cs.len())
        .map(|j| inside_of[j].iter().copied().filter(|&i| inside_of[i].binary_search(&j).is_err()).collect())
        .collect())
}

pub(crate) fn bounds_of(points: &[Point]) -> Option<Bounds> {
    let (first, rest) = points.split_first()?;
    let mut b = Bounds::from_point(*first);
    for p in rest {
        b.include(*p);
    }
    Some(b)
}

/// Even-odd point-in-polygon (crossing number). Points exactly on an edge may go either way;
/// callers only use this for contours that do not touch.
pub(crate) fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    //! Characterisation of the contour selection against the original quadratic planner
    //! (`reference`), plus a cost counter: the indexed planner must give exactly the same steps.

    use super::*;
    use burnburn_types::StableId;
    use proptest::prelude::*;

    /// The planner as it was before the spatial index (W4-prestaties): the oracle.
    mod reference {
        use super::*;

        pub fn plan(
            contours: &[Contour],
            order: Order,
            options: &LineOptions,
            shared: SharedEdgePolicy,
            heuristics: &Heuristics,
            head: &mut Point,
            operation: OperationId,
            tolerance_mm: f64,
            cancel: &CancellationToken,
        ) -> Result<Vec<Step>, CompileError> {
            let usable: Vec<&Contour> = contours.iter().filter(|c| c.points.len() >= 2).collect();
            let inner = match order {
                Order::Containment if heuristics.inner_first => containment(&usable, cancel)?,
                _ => vec![Vec::new(); usable.len()],
            };
            let depth = hole_depth(&usable);
            let mut done = vec![false; usable.len()];
            let mut steps = Vec::new();
            for k in 0..usable.len() {
                cancel.check()?;
                let chosen = match order {
                    Order::AsGiven => Some((k, entry(usable[k], *head, heuristics))),
                    Order::Containment => {
                        let ready = |i: usize| !done[i] && inner[i].iter().all(|&k| done[k]);
                        let any_ready = (0..usable.len()).any(ready);
                        let mut best: Option<(usize, Entry)> = None;
                        for (i, c) in usable.iter().enumerate() {
                            if done[i] || (any_ready && !ready(i)) {
                                continue;
                            }
                            let e = entry(c, *head, heuristics);
                            if best.as_ref().is_none_or(|(_, b)| heuristics.reduce_travel && e.cost < b.cost) {
                                best = Some((i, e));
                            }
                        }
                        best
                    }
                };
                let Some((i, e)) = chosen else { break };
                done[i] = true;
                let waste_inside = depth[i] % 2 == 1;
                emit(usable[i], &e, options, waste_inside, tolerance_mm, &mut steps, head, operation)?;
            }
            if shared == SharedEdgePolicy::CutOnce {
                let start = start_of(&steps);
                steps = cut_shared_edges_once(steps, start);
            }
            Ok(steps)
        }

        pub fn hole_depth(cs: &[&Contour]) -> Vec<usize> {
            cs.iter()
                .enumerate()
                .map(|(i, c)| {
                    let Some(p) = c.points.first() else { return 0 };
                    cs.iter()
                        .enumerate()
                        .filter(|(j, o)| {
                            *j != i
                                && o.geometry == c.geometry
                                && o.closed
                                && o.points.len() >= 3
                                && point_in_polygon(*p, &o.points)
                        })
                        .count()
                })
                .collect()
        }

        pub fn containment(cs: &[&Contour], cancel: &CancellationToken) -> Result<Vec<Vec<usize>>, CompileError> {
            let bounds: Vec<Option<Bounds>> = cs.iter().map(|c| bounds_of(&c.points)).collect();
            let mut inside = vec![vec![false; cs.len()]; cs.len()];
            for (j, outer) in cs.iter().enumerate() {
                cancel.check()?;
                if !outer.closed || outer.points.len() < 3 {
                    continue;
                }
                for (i, c) in cs.iter().enumerate() {
                    let within = match (bounds[i], bounds[j]) {
                        (Some(bi), Some(bj)) => bj.contains(bi.min()) && bj.contains(bi.max()) && (bi != bj),
                        _ => false,
                    };
                    if i != j && within && c.points.iter().all(|p| point_in_polygon(*p, &outer.points)) {
                        inside[i][j] = true;
                    }
                }
            }
            Ok((0..cs.len()).map(|j| (0..cs.len()).filter(|&i| inside[i][j] && !inside[j][i]).collect()).collect())
        }
    }

    fn pt(x: i32, y: i32) -> Point {
        Point::new(f64::from(x), f64::from(y))
    }

    /// Integer coordinates on a small grid: plenty of exact ties and of nested or touching shapes.
    fn arb_contour() -> impl Strategy<Value = Contour> {
        let coords = prop::collection::vec((0..24, 0..24), 2..7);
        (0u64..3, coords, any::<bool>(), prop::bool::weighted(0.3)).prop_map(|(g, pts, closed, boxy)| {
            let points: Vec<Point> = if boxy {
                // An axis-aligned box: the typical nesting case.
                let (x0, y0) = pts[0];
                let (x1, y1) = pts[1];
                vec![pt(x0, y0), pt(x1.max(x0 + 1), y0), pt(x1.max(x0 + 1), y1.max(y0 + 1)), pt(x0, y1.max(y0 + 1))]
            } else {
                pts.into_iter().map(|(x, y)| pt(x, y)).collect()
            };
            Contour::new(GeometryId::from_raw(g), points, closed || boxy)
        })
    }

    fn arb_heuristics() -> impl Strategy<Value = Heuristics> {
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
            |(inner_first, reduce_travel, best_start, best_direction, hide_backlash)| Heuristics {
                inner_first,
                reduce_travel,
                best_start,
                best_direction,
                hide_backlash,
            },
        )
    }

    fn run_both(
        contours: &[Contour],
        order: Order,
        h: &Heuristics,
        start: Point,
    ) -> (Vec<Step>, Point, Vec<Step>, Point) {
        let op = OperationId::from_raw(1);
        let options = LineOptions::default();
        let cancel = CancellationToken::never();
        let (mut ha, mut hb) = (start, start);
        let a =
            reference::plan(contours, order, &options, SharedEdgePolicy::CutPerRegion, h, &mut ha, op, 0.01, &cancel);
        let b = plan(contours, order, &options, SharedEdgePolicy::CutPerRegion, h, &mut hb, op, 0.01, &cancel);
        (a.unwrap_or_default(), ha, b.unwrap_or_default(), hb)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        #[test]
        fn the_indexed_planner_gives_exactly_the_steps_of_the_original(
            contours in prop::collection::vec(arb_contour(), 0..40),
            h in arb_heuristics(),
            (sx, sy) in (-5..30, -5..30),
            as_given in any::<bool>(),
        ) {
            let order = if as_given { Order::AsGiven } else { Order::Containment };
            let (a, ha, b, hb) = run_both(&contours, order, &h, pt(sx, sy));
            prop_assert_eq!(a, b);
            prop_assert_eq!(ha, hb);
        }

        #[test]
        fn containment_and_hole_depth_equal_the_original(contours in prop::collection::vec(arb_contour(), 0..40)) {
            let usable: Vec<&Contour> = contours.iter().filter(|c| c.points.len() >= 2).collect();
            let cancel = CancellationToken::never();
            prop_assert_eq!(
                containment(&usable, &cancel).unwrap_or_default(),
                reference::containment(&usable, &cancel).unwrap_or_default()
            );
            prop_assert_eq!(hole_depth(&usable), reference::hole_depth(&usable));
        }
    }

    /// A rows x cols lattice of unit squares, 3 mm apart, in a scrambled input order.
    fn lattice(rows: usize, cols: usize) -> Vec<Contour> {
        let out: Vec<Contour> = (0..rows * cols)
            .map(|k| {
                let (r, c) = ((k / cols) as i32, (k % cols) as i32);
                let (x, y) = (c * 3, r * 3);
                Contour::new(
                    GeometryId::from_raw(k as u64),
                    vec![pt(x, y), pt(x + 1, y), pt(x + 1, y + 1), pt(x, y + 1)],
                    true,
                )
            })
            .collect();
        // Deterministic scramble (a fixed stride coprime to the length).
        let n = out.len();
        (0..n).map(|k| out[(k * 7919) % n].clone()).collect()
    }

    #[test]
    fn choosing_the_next_contour_examines_a_few_contours_not_all_of_them() {
        let contours = lattice(40, 50); // 2 000 contours
        let n = contours.len();
        let mut stats = PlanStats::default();
        let mut head = pt(0, 0);
        let steps = plan_counted(
            &contours,
            Order::Containment,
            &LineOptions::default(),
            SharedEdgePolicy::CutPerRegion,
            &Heuristics::default(),
            &mut head,
            OperationId::from_raw(1),
            0.01,
            &CancellationToken::never(),
            &mut stats,
        );
        assert!(steps.is_ok());
        // The original examines n*(n+1)/2 = 2 001 000 entries; a spatial index needs a small multiple of n.
        assert!(stats.entry_evaluations < 40 * n, "examined {} entries for {n} contours", stats.entry_evaluations);
    }

    #[test]
    fn containment_and_hole_depth_examine_neighbours_not_every_pair() {
        let contours = lattice(40, 50);
        let usable: Vec<&Contour> = contours.iter().collect();
        let n = usable.len();
        let mut pairs = 0;
        let inner = containment_counted(&usable, &CancellationToken::never(), &mut pairs);
        assert!(inner.is_ok_and(|v| v.iter().all(Vec::is_empty)), "a lattice of separate squares nests nothing");
        assert!(pairs < 40 * n, "examined {pairs} pairs for {n} contours");
    }

    #[test]
    fn a_cancelled_plan_returns_cancelled() {
        let contours = lattice(10, 10);
        let source = burnburn_types::CancellationSource::new();
        source.cancel();
        let mut head = pt(0, 0);
        let r = plan(
            &contours,
            Order::Containment,
            &LineOptions::default(),
            SharedEdgePolicy::CutPerRegion,
            &Heuristics::default(),
            &mut head,
            OperationId::from_raw(1),
            0.01,
            &source.token(),
        );
        assert!(matches!(r, Err(CompileError::Cancelled)));
    }
}
