//! Colour tracing (IM32, trace side): a deterministic k-means palette (integer arithmetic, seeded
//! k-means++ initialisation, fixed iteration cap) and a region graph in which every boundary
//! between two colour regions — or between a region and the outside — exists exactly once and is
//! fitted once. Regions refer to their boundaries with a direction, so two neighbouring regions
//! share the same geometry instead of carrying two nearly equal contours.

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_geometry::{Path, Point, Segment, Subpath};
use burnburn_image::{PixelScale, SourceImage};
use burnburn_types::{
    CancellationToken, CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, NonFiniteValue, TaskId,
};

use crate::error::TraceError;
use crate::fit::{self, FitParams};

pub const COLOUR_ENGINE_VERSION: &str = "burnburn-trace/colour-1";

/// Palette in straight RGB, sorted (lexicographically) for a canonical order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub colours: Vec<[u8; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundaryId(pub u32);

/// How CAM treats a boundary shared by two regions (CAM owns the scheduling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SharedEdgePolicy {
    #[default]
    CutOnce,
    CutPerRegion,
}

/// One use of a boundary by a region; `reversed` = the region traverses it backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoundaryUse {
    pub boundary: BoundaryId,
    pub reversed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub id: RegionId,
    /// Index into the palette.
    pub colour: usize,
    pub area_px: u64,
    pub area_mm2: f64,
    /// Every boundary use of the region, ring by ring.
    pub boundaries: Vec<BoundaryUse>,
    /// Closed rings (outer outline and holes) as ordered boundary uses.
    pub rings: Vec<Vec<BoundaryUse>>,
}

/// A maximal boundary between `left` and `right` (`None` = outside the image), from junction to
/// junction or a closed loop. `left` lies on the left of `path` in image (y-down) view.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryEdge {
    pub id: BoundaryId,
    pub left: RegionId,
    pub right: Option<RegionId>,
    /// Millimetres, y-up; fitted once within `tolerance_mm`, junction ends fixed.
    pub path: Subpath,
}

#[derive(Debug, Clone)]
pub struct ColourRequest {
    pub task: TaskId,
    pub source: Arc<SourceImage>,
    pub scale: PixelScale,
    pub k: u8,
    pub seed: u64,
    pub tolerance_mm: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionGraph {
    pub task: TaskId,
    pub inputs: ContentHash,
    pub engine: &'static str,
    pub palette: Palette,
    pub regions: Vec<Region>,
    pub boundaries: Vec<BoundaryEdge>,
    pub width_px: u32,
    pub height_px: u32,
    pub scale: PixelScale,
}

impl ColourRequest {
    pub fn key(&self) -> Result<ContentHash, NonFiniteValue> {
        let mut w = CanonicalWriter::new();
        w.str(COLOUR_ENGINE_VERSION);
        w.bytes(self.source.content_hash().as_bytes());
        self.scale.encode(&mut w);
        w.u64(u64::from(self.k));
        w.u64(self.seed);
        w.f64(self.tolerance_mm);
        Ok(ContentHash::of(HashDomain::TraceKey, &w.finish()?))
    }
}

impl RegionGraph {
    pub fn region(&self, id: RegionId) -> Option<&Region> {
        self.regions.iter().find(|r| r.id == id)
    }

    pub fn boundary(&self, id: BoundaryId) -> Option<&BoundaryEdge> {
        self.boundaries.iter().find(|b| b.id == id)
    }

    /// The region's outline (outer ring and holes, even-odd) assembled from its shared boundaries:
    /// the same segments as the neighbour's outline, traversed in the other direction.
    pub fn region_path(&self, id: RegionId) -> Option<Path> {
        let region = self.region(id)?;
        let mut subpaths = Vec::with_capacity(region.rings.len());
        for ring in &region.rings {
            let mut start = None;
            let mut segments: Vec<Segment> = Vec::new();
            for u in ring {
                let b = self.boundary(u.boundary)?;
                let (s, segs) =
                    if u.reversed { reversed(&b.path) } else { (b.path.start(), b.path.segments().to_vec()) };
                start.get_or_insert(s);
                segments.extend(segs);
            }
            subpaths.push(Subpath::new(start?, segments, true).ok()?);
        }
        Some(Path::from_subpaths(subpaths))
    }
}

fn reversed(path: &Subpath) -> (Point, Vec<Segment>) {
    let segs = path.segments();
    let mut froms = Vec::with_capacity(segs.len());
    let mut from = path.start();
    for s in segs {
        froms.push(from);
        from = s.end();
    }
    (path.end(), segs.iter().zip(&froms).rev().map(|(s, &f)| s.reversed(f)).collect())
}

/// Straight RGB of a pixel composited over white.
fn rgb_at(source: &SourceImage, x: u32, y: u32) -> [u8; 3] {
    let [r, g, b, a] = source.rgba_at(x, y);
    let alpha = a / 255.0;
    [r, g, b].map(|c| (c * alpha + 255.0 * (1.0 - alpha)).round().clamp(0.0, 255.0) as u8)
}

fn dist2(a: [i64; 3], b: [i64; 3]) -> i64 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

/// SplitMix64 (Steele, Lea & Flood 2014): small, seedable, platform-independent.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, n)` for `n > 0` (rejection sampling, no modulo bias).
    fn below(&mut self, n: u128) -> u128 {
        let n = n.max(1);
        let wide = |r: &mut Rng| (u128::from(r.next()) << 64) | u128::from(r.next());
        let zone = u128::MAX - u128::MAX % n;
        loop {
            let v = wide(self);
            if v < zone {
                return v % n;
            }
        }
    }
}

const MAX_KMEANS_ITERATIONS: usize = 100;

/// Deterministic k-means palette: histogram of composited RGB, k-means++ seeding from `seed`,
/// Lloyd iterations with integer sums and rounded integer means; ties go to the lower index.
pub fn quantize(source: &SourceImage, k: u8, seed: u64, cancel: &CancellationToken) -> Result<Palette, TraceError> {
    if k == 0 {
        return Err(TraceError::InvalidRecipe("colour.k"));
    }
    cancel.check()?;
    let mut hist: BTreeMap<[u8; 3], u64> = BTreeMap::new();
    for y in 0..source.height() {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..source.width() {
            *hist.entry(rgb_at(source, x, y)).or_default() += 1;
        }
    }
    let colours: Vec<([i64; 3], u64)> = hist.iter().map(|(c, &n)| (c.map(i64::from), n)).collect();
    if colours.len() <= usize::from(k) {
        return Ok(Palette { colours: hist.keys().copied().collect() });
    }
    let mut rng = Rng(seed);
    let total: u128 = colours.iter().map(|c| u128::from(c.1)).sum();
    let pick = |rng: &mut Rng, weights: &[u128], total: u128| -> usize {
        let mut r = rng.below(total);
        for (i, &w) in weights.iter().enumerate() {
            if r < w {
                return i;
            }
            r -= w;
        }
        weights.len() - 1
    };
    let counts: Vec<u128> = colours.iter().map(|c| u128::from(c.1)).collect();
    let mut centres: Vec<[i64; 3]> = vec![colours[pick(&mut rng, &counts, total)].0];
    while centres.len() < usize::from(k) {
        let weights: Vec<u128> = colours
            .iter()
            .map(|(c, n)| {
                let d = centres.iter().map(|&m| dist2(*c, m)).min().unwrap_or(0);
                d as u128 * u128::from(*n)
            })
            .collect();
        let sum: u128 = weights.iter().sum();
        if sum == 0 {
            break;
        }
        centres.push(colours[pick(&mut rng, &weights, sum)].0);
    }
    for iteration in 0..MAX_KMEANS_ITERATIONS {
        if iteration % 8 == 0 {
            cancel.check()?;
        }
        let mut sums = vec![[0u64; 3]; centres.len()];
        let mut ns = vec![0u64; centres.len()];
        for (c, n) in &colours {
            let j = nearest(&centres, *c);
            for i in 0..3 {
                sums[j][i] += c[i] as u64 * n;
            }
            ns[j] += n;
        }
        let next: Vec<[i64; 3]> = centres
            .iter()
            .enumerate()
            .map(|(j, &m)| if ns[j] == 0 { m } else { sums[j].map(|s| ((s + ns[j] / 2) / ns[j]) as i64) })
            .collect();
        if next == centres {
            break;
        }
        centres = next;
    }
    let mut out: Vec<[u8; 3]> = centres.iter().map(|c| c.map(|v| v.clamp(0, 255) as u8)).collect();
    out.sort_unstable();
    out.dedup();
    Ok(Palette { colours: out })
}

fn nearest(centres: &[[i64; 3]], c: [i64; 3]) -> usize {
    let mut best = (i64::MAX, 0);
    for (j, &m) in centres.iter().enumerate() {
        let d = dist2(c, m);
        if d < best.0 {
            best = (d, j);
        }
    }
    best.1
}

/// One crack edge of the pixel lattice from vertex `a` to `b` with the regions on either side.
#[derive(Debug, Clone, Copy)]
struct Crack {
    a: usize,
    b: usize,
    left: Option<u32>,
    right: Option<u32>,
}

pub fn colour_regions(req: &ColourRequest, cancel: &CancellationToken) -> Result<RegionGraph, TraceError> {
    if !(req.tolerance_mm.is_finite() && req.tolerance_mm > 0.0) {
        return Err(TraceError::InvalidRecipe("colour.tolerance_mm"));
    }
    let inputs = req.key().map_err(|_| TraceError::NonFinite)?;
    let palette = quantize(&req.source, req.k, req.seed, cancel)?;
    let src = &req.source;
    let (w, h) = (src.width() as usize, src.height() as usize);
    let centres: Vec<[i64; 3]> = palette.colours.iter().map(|c| c.map(i64::from)).collect();

    // Palette label per pixel, then 4-connected regions in raster order.
    let mut label = vec![0usize; w * h];
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..w {
            label[y * w + x] = nearest(&centres, rgb_at(src, x as u32, y as u32).map(i64::from));
        }
    }
    let mut region = vec![u32::MAX; w * h];
    let mut regions: Vec<Region> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if region[start] != u32::MAX {
            continue;
        }
        if regions.len().is_multiple_of(256) {
            cancel.check()?;
        }
        let id = regions.len() as u32;
        region[start] = id;
        stack.push(start);
        let mut area = 0u64;
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if region[j] == u32::MAX && label[j] == label[start] {
                    region[j] = id;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        regions.push(Region {
            id: RegionId(id),
            colour: label[start],
            area_px: area,
            area_mm2: area as f64 * req.scale.mm_per_pixel_x() * req.scale.mm_per_pixel_y(),
            boundaries: Vec::new(),
            rings: Vec::new(),
        });
    }
    cancel.check()?;

    // Crack edges between different regions (or a region and the outside).
    let vw = w + 1;
    let vid = |x: usize, y: usize| y * vw + x;
    let at = |x: isize, y: isize| -> Option<u32> {
        (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h).then(|| region[y as usize * w + x as usize])
    };
    let mut cracks: Vec<Crack> = Vec::new();
    for y in 0..=h {
        for x in 0..w {
            let (above, below) = (at(x as isize, y as isize - 1), at(x as isize, y as isize));
            if above != below {
                cracks.push(Crack { a: vid(x, y), b: vid(x + 1, y), left: above, right: below });
            }
        }
    }
    for y in 0..h {
        for x in 0..=w {
            let (west, east) = (at(x as isize - 1, y as isize), at(x as isize, y as isize));
            if west != east {
                cracks.push(Crack { a: vid(x, y), b: vid(x, y + 1), left: east, right: west });
            }
        }
    }
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); vw * (h + 1)];
    for (i, c) in cracks.iter().enumerate() {
        incident[c.a].push(i);
        incident[c.b].push(i);
    }
    let junction = |v: usize| incident[v].len() >= 3;

    // Maximal chains between junction vertices; then closed loops.
    let mut used = vec![false; cracks.len()];
    let mut chains: Vec<(Vec<usize>, Option<u32>, Option<u32>, bool)> = Vec::new();
    let walk = |start_v: usize, first: usize, used: &mut Vec<bool>| -> (Vec<usize>, Option<u32>, Option<u32>, bool) {
        let c0 = cracks[first];
        let (left, right) = if c0.a == start_v { (c0.left, c0.right) } else { (c0.right, c0.left) };
        let mut verts = vec![start_v];
        let (mut v, mut e) = (start_v, first);
        loop {
            used[e] = true;
            let c = cracks[e];
            v = if c.a == v { c.b } else { c.a };
            verts.push(v);
            if v == start_v || junction(v) {
                break;
            }
            match incident[v].iter().copied().find(|&n| !used[n]) {
                Some(n) => e = n,
                None => break,
            }
        }
        let closed = v == start_v;
        (verts, left, right, closed)
    };
    for (v, at_v) in incident.iter().enumerate() {
        if v.is_multiple_of(4096) {
            cancel.check()?;
        }
        if at_v.len() < 3 {
            continue;
        }
        let mut edges = at_v.clone();
        edges.sort_unstable();
        for e in edges {
            if !used[e] {
                chains.push(walk(v, e, &mut used));
            }
        }
    }
    for e in 0..cracks.len() {
        if !used[e] {
            let start = cracks[e].a;
            chains.push(walk(start, e, &mut used));
        }
    }
    cancel.check()?;

    // Fit every boundary once.
    let (sx, sy) = (req.scale.mm_per_pixel_x(), req.scale.mm_per_pixel_y());
    let to_mm = |v: usize| Point::new((v % vw) as f64 * sx, (h as f64 - (v / vw) as f64) * sy);
    let params = FitParams { tol: req.tolerance_mm, corner_cos: 0.5, window: 2.5 * 0.5 * (sx + sy), curves: true };
    let mut boundaries = Vec::with_capacity(chains.len());
    let mut ends: Vec<(usize, usize)> = Vec::with_capacity(chains.len());
    for (k, (mut verts, mut left, mut right, closed)) in chains.into_iter().enumerate() {
        cancel.check()?;
        if left.is_none() {
            verts.reverse();
            std::mem::swap(&mut left, &mut right);
        }
        let Some(left) = left else { continue };
        let pts: Vec<Point> = verts.iter().map(|&v| to_mm(v)).collect();
        let mids: Vec<Point> = pts.windows(2).map(|p| p[0].lerp(p[1], 0.5)).collect();
        let path = if closed {
            let fitted = fit::fit_closed(&mids, &params);
            Subpath::new(fitted.start, fitted.segments, true)
        } else {
            let mut data: Vec<(Point, usize)> = vec![(pts[0], 0)];
            data.extend(mids.iter().copied().zip(1..));
            data.push((pts[pts.len() - 1], mids.len() + 1));
            let mut out = fit::Fitted { start: pts[0], segments: Vec::new(), spans: Vec::new() };
            fit::fit_open_indexed(&data, &[], &params, &mut out);
            Subpath::new(out.start, out.segments, false)
        }
        .map_err(|_| TraceError::NonFinite)?;
        boundaries.push(BoundaryEdge {
            id: BoundaryId(k as u32),
            left: RegionId(left),
            right: right.map(RegionId),
            path,
        });
        ends.push((verts[0], verts[verts.len() - 1]));
    }

    // Rings per region from its boundary uses (closed loops are rings on their own).
    let mut uses_of: Vec<Vec<(BoundaryUse, usize, usize)>> = vec![Vec::new(); regions.len()];
    for (b, &(s, e)) in boundaries.iter().zip(&ends) {
        uses_of[b.left.0 as usize].push((BoundaryUse { boundary: b.id, reversed: false }, s, e));
        if let Some(r) = b.right {
            uses_of[r.0 as usize].push((BoundaryUse { boundary: b.id, reversed: true }, e, s));
        }
    }
    for (r, uses) in regions.iter_mut().zip(uses_of) {
        let mut taken = vec![false; uses.len()];
        for first in 0..uses.len() {
            if taken[first] {
                continue;
            }
            taken[first] = true;
            let mut ring = vec![uses[first].0];
            let ring_start = uses[first].1;
            let mut at = uses[first].2;
            while at != ring_start {
                match (0..uses.len()).find(|&j| !taken[j] && uses[j].1 == at) {
                    Some(j) => {
                        taken[j] = true;
                        ring.push(uses[j].0);
                        at = uses[j].2;
                    }
                    None => break,
                }
            }
            r.boundaries.extend(ring.iter().copied());
            r.rings.push(ring);
        }
    }
    Ok(RegionGraph {
        task: req.task,
        inputs,
        engine: COLOUR_ENGINE_VERSION,
        palette,
        regions,
        boundaries,
        width_px: src.width(),
        height_px: src.height(),
        scale: req.scale,
    })
}
