//! Marching squares on pixel centres and the contour tree.

/// Policy for two foreground pixels that touch only diagonally (a marching-squares saddle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagonalPolicy {
    /// Diagonal neighbours are one part (8-connected foreground, 4-connected background).
    Connect,
    /// Diagonal neighbours are separate parts (4-connected foreground, 8-connected background).
    Separate,
}

/// Role of a contour in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContourKind {
    /// Boundary of a foreground part; counter-clockwise in millimetres (y-up).
    Outer,
    /// Boundary of a hole in a part; clockwise.
    Hole,
}

use burnburn_image::PreparedImage;
use burnburn_types::{CancellationToken, Cancelled};

/// A closed sub-pixel contour in pixel coordinates (y-down) with the labels of the foreground
/// part and the background region on either side of it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawContour {
    pub points: Vec<[f64; 2]>,
    pub fg_label: u32,
    pub bg_label: u32,
}

/// All contours plus the label of the unbounded background region.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContourSet {
    pub contours: Vec<RawContour>,
    pub outside_label: u32,
}

/// The prepared field padded with one ring of background, so every contour closes.
struct Grid {
    w: usize,
    h: usize,
    v: Vec<f32>,
}

const PAD_VALUE: f32 = -0.5;
const NONE: u32 = u32::MAX;

impl Grid {
    fn new(p: &PreparedImage) -> Grid {
        let (iw, ih) = (p.width() as usize, p.height() as usize);
        let (w, h) = (iw + 2, ih + 2);
        let mut v = vec![PAD_VALUE; w * h];
        for y in 0..ih {
            for x in 0..iw {
                v[(y + 1) * w + x + 1] = p.field_at(x as u32, y as u32);
            }
        }
        Grid { w, h, v }
    }

    fn inside(&self, i: usize) -> bool {
        self.v.get(i).is_some_and(|&v| v > 0.0)
    }
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        let gp = parent[parent[i as usize] as usize];
        parent[i as usize] = gp;
        i = gp;
    }
    i
}

fn union(parent: &mut [u32], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a as u32), find(parent, b as u32));
    if ra != rb {
        // Smaller root wins: labels are deterministic (raster order of the first pixel).
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[hi as usize] = lo;
    }
}

/// Connected-component labels for foreground and background with complementary connectivity.
fn label(g: &Grid, policy: DiagonalPolicy, cancel: &CancellationToken) -> Result<Vec<u32>, Cancelled> {
    let mut parent: Vec<u32> = (0..(g.w * g.h) as u32).collect();
    for y in 0..g.h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..g.w {
            let i = y * g.w + x;
            let fg = g.inside(i);
            let diagonal = fg == (policy == DiagonalPolicy::Connect);
            let mut join = |j: usize| {
                if g.inside(j) == fg {
                    union(&mut parent, i, j);
                }
            };
            if x > 0 {
                join(i - 1);
            }
            if y > 0 {
                join(i - g.w);
                if diagonal && x > 0 {
                    join(i - g.w - 1);
                }
                if diagonal && x + 1 < g.w {
                    join(i - g.w + 1);
                }
            }
        }
    }
    for i in 0..parent.len() {
        parent[i] = find(&mut parent, i as u32);
    }
    Ok(parent)
}

/// Marching squares on pixel centres; outer contours come out counter-clockwise in y-up millimetres.
pub(crate) fn extract(
    p: &PreparedImage,
    policy: DiagonalPolicy,
    cancel: &CancellationToken,
) -> Result<ContourSet, Cancelled> {
    let g = Grid::new(p);
    let labels = label(&g, policy, cancel)?;
    let (w, h) = (g.w, g.h);
    let horizontal = w * h;
    let mut next = vec![NONE; 2 * w * h];
    for y in 0..h - 1 {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..w - 1 {
            let a = y * w + x;
            let corners = [a, a + 1, a + w + 1, a + w];
            let ins = corners.map(|c| g.inside(c));
            if ins.iter().all(|&b| b == ins[0]) {
                continue;
            }
            // Cell edges in clockwise order: top, right, bottom, left (edge k joins corner k, k+1).
            let edges = [a, horizontal + a + 1, a + w, horizontal + a];
            let leave = |k: usize| ins[k] && !ins[(k + 1) % 4];
            let enter = |k: usize| !ins[k] && ins[(k + 1) % 4];
            for k in (0..4).filter(|&k| leave(k)) {
                let target = (1..4)
                    .map(|j| match policy {
                        // Connect: the next entering edge clockwise (cuts off background corners).
                        DiagonalPolicy::Connect => (k + j) % 4,
                        // Separate: the previous entering edge (cuts off foreground corners).
                        DiagonalPolicy::Separate => (k + 4 - j) % 4,
                    })
                    .find(|&t| enter(t));
                if let Some(t) = target {
                    next[edges[k]] = edges[t] as u32;
                }
            }
        }
    }

    let endpoints =
        |e: usize| -> (usize, usize) { if e < horizontal { (e, e + 1) } else { (e - horizontal, e - horizontal + w) } };
    let position = |e: usize| -> [f64; 2] {
        let (i, j) = endpoints(e);
        let (vi, vj) = (f64::from(g.v[i]), f64::from(g.v[j]));
        let t = vi / (vi - vj);
        let (xi, yi) = ((i % w) as f64, (i / w) as f64);
        let (xj, yj) = ((j % w) as f64, (j / w) as f64);
        // Padded centre (X, Y) is pixel-space (X − 0.5, Y − 0.5).
        [xi + t * (xj - xi) - 0.5, yi + t * (yj - yi) - 0.5]
    };

    let mut contours = Vec::new();
    for start in 0..next.len() {
        if start % 65_536 == 0 {
            cancel.check()?;
        }
        if next[start] == NONE {
            continue;
        }
        let (i, j) = endpoints(start);
        let (fg, bg) = if g.inside(i) { (i, j) } else { (j, i) };
        let mut points = Vec::new();
        let mut cur = start;
        loop {
            points.push(position(cur));
            let n = next[cur];
            next[cur] = NONE;
            if n == NONE || n as usize == start {
                break;
            }
            cur = n as usize;
        }
        // The linked direction keeps foreground on the left in y-down pixels; reverse it so outer
        // contours are counter-clockwise once y points up.
        points.reverse();
        contours.push(RawContour { points, fg_label: labels[fg], bg_label: labels[bg] });
    }
    Ok(ContourSet { contours, outside_label: labels[0] })
}
