//! Raster stages of the centerline: Zhang–Suen thinning, Euclidean distance transform and
//! 8-connected component labels. All on the binary mask; outside the image is background.

use burnburn_image::BinaryMask;
use burnburn_types::{CancellationToken, Cancelled};

/// Neighbour offsets P2..P9 of Zhang–Suen: N, NE, E, SE, S, SW, W, NW (clockwise from north).
pub(crate) const NEIGHBOURS: [(i64, i64); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

/// A raster of booleans with background outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Raster {
    pub w: usize,
    pub h: usize,
    pub on: Vec<bool>,
}

impl Raster {
    pub(crate) fn from_mask(mask: &BinaryMask) -> Raster {
        let (w, h) = (mask.width() as usize, mask.height() as usize);
        let mut on = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                on.push(mask.get(x as i64, y as i64));
            }
        }
        Raster { w, h, on }
    }

    pub(crate) fn get(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h && self.on[y as usize * self.w + x as usize]
    }

    /// Branches leaving a skeleton pixel: its 4-neighbours plus every diagonal neighbour that is not
    /// already reachable through one of the two 4-neighbours it shares with the pixel. Staircase
    /// pixels have 2, line ends 1, junctions ≥ 3 (also where the crossing number is 2 everywhere,
    /// e.g. at a thinned X).
    pub(crate) fn effective_degree(&self, x: i64, y: i64) -> u32 {
        let mut d = 0;
        for (dx, dy) in NEIGHBOURS {
            if !self.get(x + dx, y + dy) {
                continue;
            }
            let diagonal = dx != 0 && dy != 0;
            if !diagonal || !(self.get(x + dx, y) || self.get(x, y + dy)) {
                d += 1;
            }
        }
        d
    }

    /// (number of foreground neighbours, number of background→foreground transitions in P2..P9,P2).
    pub(crate) fn neighbourhood(&self, x: i64, y: i64) -> (u32, u32) {
        let p: [bool; 8] = NEIGHBOURS.map(|(dx, dy)| self.get(x + dx, y + dy));
        let count = p.iter().filter(|&&b| b).count() as u32;
        let transitions = (0..8).filter(|&i| !p[i] && p[(i + 1) % 8]).count() as u32;
        (count, transitions)
    }
}

/// Zhang–Suen thinning to an 8-connected, one-pixel skeleton that keeps the topology of the mask.
pub(crate) fn thin(mask: &BinaryMask, cancel: &CancellationToken) -> Result<Raster, Cancelled> {
    let mut r = Raster::from_mask(mask);
    let max_iterations = r.w + r.h + 2;
    let mut remove = Vec::new();
    for _ in 0..max_iterations {
        cancel.check()?;
        let mut changed = false;
        for step in 0..2 {
            remove.clear();
            for y in 0..r.h as i64 {
                for x in 0..r.w as i64 {
                    if !r.get(x, y) {
                        continue;
                    }
                    let (b, a) = r.neighbourhood(x, y);
                    if !(2..=6).contains(&b) || a != 1 {
                        continue;
                    }
                    let p = |i: usize| r.get(x + NEIGHBOURS[i].0, y + NEIGHBOURS[i].1);
                    // P2 = N (0), P4 = E (2), P6 = S (4), P8 = W (6).
                    let ok = if step == 0 {
                        !(p(0) && p(2) && p(4)) && !(p(2) && p(4) && p(6))
                    } else {
                        !(p(0) && p(2) && p(6)) && !(p(0) && p(4) && p(6))
                    };
                    if ok {
                        remove.push(y as usize * r.w + x as usize);
                    }
                }
            }
            for &i in &remove {
                r.on[i] = false;
            }
            changed |= !remove.is_empty();
        }
        if !changed {
            break;
        }
    }
    Ok(r)
}

/// Squared Euclidean distance (px²) from each pixel centre to the nearest background pixel centre
/// (outside the image counts as background), Felzenszwalb–Huttenlocher separable transform.
pub(crate) fn distance_sq(r: &Raster, cancel: &CancellationToken) -> Result<Vec<f64>, Cancelled> {
    // Pad by one ring of background so the outside is represented exactly.
    let (pw, ph) = (r.w + 2, r.h + 2);
    let big = ((pw + ph) * (pw + ph)) as f64;
    let mut d = vec![0.0f64; pw * ph];
    for y in 0..r.h {
        for x in 0..r.w {
            if r.on[y * r.w + x] {
                d[(y + 1) * pw + x + 1] = big;
            }
        }
    }
    let mut f = Vec::new();
    let mut out = Vec::new();
    for x in 0..pw {
        if x % 64 == 0 {
            cancel.check()?;
        }
        f.clear();
        f.extend((0..ph).map(|y| d[y * pw + x]));
        dt_1d(&f, &mut out);
        for y in 0..ph {
            d[y * pw + x] = out[y];
        }
    }
    for y in 0..ph {
        if y % 64 == 0 {
            cancel.check()?;
        }
        f.clear();
        f.extend_from_slice(&d[y * pw..(y + 1) * pw]);
        dt_1d(&f, &mut out);
        d[y * pw..(y + 1) * pw].copy_from_slice(&out);
    }
    let mut res = Vec::with_capacity(r.w * r.h);
    for y in 0..r.h {
        for x in 0..r.w {
            res.push(d[(y + 1) * pw + x + 1]);
        }
    }
    Ok(res)
}

/// Lower envelope of parabolas: `out[q] = min_p (q − p)² + f[p]`.
fn dt_1d(f: &[f64], out: &mut Vec<f64>) {
    let n = f.len();
    out.clear();
    out.resize(n, 0.0);
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let sect = |q: usize, p: usize| ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
    for q in 1..n {
        let mut s = sect(q, v[k]);
        // z[0] = −∞, so k never drops below 0.
        while k > 0 && s <= z[k] {
            k -= 1;
            s = sect(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        *o = dq * dq + f[p];
    }
}

/// 8-connected labels of `r` (`u32::MAX` for background) in raster order, and their count.
pub(crate) fn components(r: &Raster) -> (Vec<u32>, usize) {
    let mut label = vec![u32::MAX; r.on.len()];
    let mut count = 0u32;
    let mut stack = Vec::new();
    for start in 0..r.on.len() {
        if !r.on[start] || label[start] != u32::MAX {
            continue;
        }
        label[start] = count;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % r.w) as i64, (i / r.w) as i64);
            for (dx, dy) in NEIGHBOURS {
                let (nx, ny) = (x + dx, y + dy);
                if r.get(nx, ny) {
                    let j = ny as usize * r.w + nx as usize;
                    if label[j] == u32::MAX {
                        label[j] = count;
                        stack.push(j);
                    }
                }
            }
        }
        count += 1;
    }
    (label, count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_transform_matches_brute_force() {
        let mask =
            BinaryMask::from_fn(13, 9, |x, y| (2..11).contains(&x) && (1..8).contains(&y) && !(x == 6 && y == 4));
        let r = Raster::from_mask(&mask);
        let d = distance_sq(&r, &CancellationToken::never()).unwrap_or_default();
        for y in 0..9i64 {
            for x in 0..13i64 {
                let want = if r.get(x, y) {
                    let mut best = f64::INFINITY;
                    for by in -1..=9i64 {
                        for bx in -1..=13i64 {
                            if !r.get(bx, by) {
                                best = best.min(((bx - x).pow(2) + (by - y).pow(2)) as f64);
                            }
                        }
                    }
                    best
                } else {
                    0.0
                };
                assert_eq!(d[(y * 13 + x) as usize], want, "({x}, {y})");
            }
        }
    }
}
