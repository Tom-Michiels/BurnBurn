//! Skeleton → graph in pixel coordinates (pixel centres `x + 0.5`, `y + 0.5`, y-down): node pixels
//! (ends: one neighbour run; junctions: three or more), chains walked between them, spur pruning,
//! junction refinement by the least-squares intersection of the branch lines and junction merging.

use std::collections::BTreeSet;

use burnburn_types::{CancellationToken, Cancelled};

use super::skeleton::{NEIGHBOURS, Raster};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct P {
    pub x: f64,
    pub y: f64,
}

impl P {
    fn dist(self, o: P) -> f64 {
        (self.x - o.x).hypot(self.y - o.y)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GNode {
    pub at: P,
    /// Local stroke width (px).
    pub width: f64,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GChain {
    pub from: Option<usize>,
    pub to: Option<usize>,
    /// Pixel-centre points; first/last are the node positions for node-bound ends.
    pub pts: Vec<P>,
    /// Local stroke width (px) per point.
    pub widths: Vec<f64>,
    pub alive: bool,
}

impl GChain {
    pub(crate) fn length(&self) -> f64 {
        self.pts.windows(2).map(|w| w[0].dist(w[1])).sum()
    }

    pub(crate) fn median_width(&self) -> f64 {
        let mut w: Vec<f64> = self.widths.iter().copied().filter(|v| v.is_finite()).collect();
        w.sort_by(f64::total_cmp);
        w.get(w.len() / 2).copied().unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spur {
    pub tip: P,
    pub base: P,
    pub length: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Graph {
    pub nodes: Vec<GNode>,
    pub chains: Vec<GChain>,
    pub spurs: Vec<Spur>,
}

/// Parameters of the graph stage in pixels.
#[derive(Debug, Clone)]
pub(crate) struct GraphParams {
    pub min_spur_px: f64,
    pub spur_width_factor: f64,
    pub keep_spurs_near: Vec<P>,
    pub junction_merge_factor: f64,
}

fn centre(i: usize, w: usize) -> P {
    P { x: (i % w) as f64 + 0.5, y: (i / w) as f64 + 0.5 }
}

/// Width (px) of the stroke at a pixel from its squared distance to the background.
fn width_at(d2: &[f64], i: usize) -> f64 {
    (2.0 * d2.get(i).copied().unwrap_or(0.0).sqrt() - 1.0).max(1.0)
}

pub(crate) fn build(sk: &Raster, d2: &[f64], cancel: &CancellationToken) -> Result<Graph, Cancelled> {
    let (w, h) = (sk.w, sk.h);
    // Node pixels and their clusters (8-connected groups of junction pixels form one node).
    const NO: usize = usize::MAX;
    let mut node_of = vec![NO; w * h];
    let mut nodes: Vec<GNode> = Vec::new();
    let mut node_pixels: Vec<Vec<usize>> = Vec::new();
    let class = |i: usize| -> u8 {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        match sk.effective_degree(x, y) {
            0 | 1 => 1, // end (or isolated dot)
            2 => 2,
            _ => 3,
        }
    };
    for i in 0..w * h {
        if !sk.on[i] || node_of[i] != NO {
            continue;
        }
        match class(i) {
            1 => {
                node_of[i] = nodes.len();
                nodes.push(GNode { at: centre(i, w), width: width_at(d2, i), alive: true });
                node_pixels.push(vec![i]);
            }
            3 => {
                let id = nodes.len();
                let mut cluster = vec![i];
                node_of[i] = id;
                let mut k = 0;
                while k < cluster.len() {
                    let j = cluster[k];
                    let (x, y) = ((j % w) as i64, (j / w) as i64);
                    for (dx, dy) in NEIGHBOURS {
                        let (nx, ny) = (x + dx, y + dy);
                        if sk.get(nx, ny) {
                            let q = ny as usize * w + nx as usize;
                            if node_of[q] == NO && class(q) == 3 {
                                node_of[q] = id;
                                cluster.push(q);
                            }
                        }
                    }
                    k += 1;
                }
                let n = cluster.len() as f64;
                let at = P {
                    x: cluster.iter().map(|&j| centre(j, w).x).sum::<f64>() / n,
                    y: cluster.iter().map(|&j| centre(j, w).y).sum::<f64>() / n,
                };
                let width = cluster.iter().map(|&j| width_at(d2, j)).fold(0.0, f64::max);
                nodes.push(GNode { at, width, alive: true });
                node_pixels.push(cluster);
            }
            _ => {}
        }
    }
    cancel.check()?;

    // Walk chains from every node pixel; 4-neighbours first so staircases are followed, not cut.
    let order: [usize; 8] = [0, 2, 4, 6, 1, 3, 5, 7];
    let neighbours = |i: usize| -> Vec<usize> {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        order
            .iter()
            .filter_map(|&k| {
                let (nx, ny) = (x + NEIGHBOURS[k].0, y + NEIGHBOURS[k].1);
                sk.get(nx, ny).then(|| ny as usize * w + nx as usize)
            })
            .collect()
    };
    let mut visited = vec![false; w * h];
    let mut direct: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut chains: Vec<GChain> = Vec::new();
    for start in 0..nodes.len() {
        if start % 256 == 0 {
            cancel.check()?;
        }
        let pixels = node_pixels[start].clone();
        for &p in &pixels {
            for q in neighbours(p) {
                let qn = node_of[q];
                if qn == start {
                    continue;
                }
                if qn != NO {
                    let key = (start.min(qn), start.max(qn));
                    if direct.insert(key) {
                        chains.push(GChain {
                            from: Some(start),
                            to: Some(qn),
                            pts: vec![nodes[start].at, nodes[qn].at],
                            widths: vec![nodes[start].width, nodes[qn].width],
                            alive: true,
                        });
                    }
                    continue;
                }
                if visited[q] {
                    continue;
                }
                // Walk.
                let mut pts = vec![nodes[start].at, centre(q, w)];
                let mut widths = vec![nodes[start].width, width_at(d2, q)];
                visited[q] = true;
                let (mut prev, mut cur) = (p, q);
                let end = loop {
                    let nb = neighbours(cur);
                    let steps = pts.len() - 1;
                    let node_next = nb
                        .iter()
                        .copied()
                        .find(|&j| j != prev && node_of[j] != NO && (node_of[j] != start || steps >= 3));
                    if let Some(j) = node_next {
                        break Some(node_of[j]);
                    }
                    let next = nb.iter().copied().find(|&j| j != prev && node_of[j] == NO && !visited[j]);
                    match next {
                        Some(j) => {
                            visited[j] = true;
                            pts.push(centre(j, w));
                            widths.push(width_at(d2, j));
                            prev = cur;
                            cur = j;
                        }
                        None => break None,
                    }
                };
                let to = match end {
                    Some(n) => {
                        pts.push(nodes[n].at);
                        widths.push(nodes[n].width);
                        n
                    }
                    None => {
                        // Dead end without an end pixel (thinning artefact): make it an end.
                        let n = nodes.len();
                        let at = pts[pts.len() - 1];
                        nodes.push(GNode { at, width: widths[widths.len() - 1], alive: true });
                        node_pixels.push(vec![cur]);
                        n
                    }
                };
                chains.push(GChain { from: Some(start), to: Some(to), pts, widths, alive: true });
            }
        }
    }
    cancel.check()?;

    // Closed loops without nodes.
    for s in 0..w * h {
        if !sk.on[s] || visited[s] || node_of[s] != NO {
            continue;
        }
        let mut pts = vec![centre(s, w)];
        let mut widths = vec![width_at(d2, s)];
        visited[s] = true;
        let (mut prev, mut cur) = (s, s);
        let mut closed = false;
        loop {
            let nb = neighbours(cur);
            if pts.len() >= 3 && cur != s && nb.contains(&s) && prev != s {
                closed = true;
                break;
            }
            match nb.iter().copied().find(|&j| j != prev && node_of[j] == NO && !visited[j]) {
                Some(j) => {
                    visited[j] = true;
                    pts.push(centre(j, w));
                    widths.push(width_at(d2, j));
                    prev = cur;
                    cur = j;
                }
                None => break,
            }
        }
        if closed && pts.len() >= 4 {
            pts.push(pts[0]);
            widths.push(widths[0]);
            chains.push(GChain { from: None, to: None, pts, widths, alive: true });
        }
    }
    Ok(Graph { nodes, chains, spurs: Vec::new() })
}

impl Graph {
    pub(crate) fn degree(&self, n: usize) -> usize {
        self.chains
            .iter()
            .filter(|c| c.alive)
            .map(|c| usize::from(c.from == Some(n)) + usize::from(c.to == Some(n)))
            .sum()
    }

    fn incident(&self, n: usize) -> Vec<usize> {
        (0..self.chains.len())
            .filter(|&c| self.chains[c].alive && (self.chains[c].from == Some(n) || self.chains[c].to == Some(n)))
            .collect()
    }

    /// Removes short end↔junction branches (shortest first), then dissolves degree-2 nodes.
    pub(crate) fn prune_spurs(&mut self, p: &GraphParams) {
        loop {
            let mut best: Option<(f64, usize, usize)> = None;
            for (ci, c) in self.chains.iter().enumerate() {
                if !c.alive {
                    continue;
                }
                let (Some(a), Some(b)) = (c.from, c.to) else { continue };
                let (da, db) = (self.degree(a), self.degree(b));
                let (tip, base) = match (da, db) {
                    (1, d) if d >= 3 => (a, b),
                    (d, 1) if d >= 3 => (b, a),
                    _ => continue,
                };
                let len = c.length();
                let limit = p.min_spur_px.max(p.spur_width_factor * self.nodes[base].width);
                let protected = p.keep_spurs_near.iter().any(|k| c.pts.iter().any(|q| q.dist(*k) <= 1.0));
                if len < limit && !protected && best.is_none_or(|(l, _, _)| len < l) {
                    best = Some((len, ci, tip));
                }
            }
            let Some((len, ci, tip)) = best else { break };
            let c = &mut self.chains[ci];
            c.alive = false;
            let base = if c.from == Some(tip) { c.to } else { c.from };
            self.nodes[tip].alive = false;
            if let Some(b) = base {
                self.spurs.push(Spur { tip: self.nodes[tip].at, base: self.nodes[b].at, length: len });
                self.dissolve_if_degree_two(b);
            }
        }
    }

    /// A node of degree 2 between two different chains disappears; the chains are concatenated.
    fn dissolve_if_degree_two(&mut self, n: usize) {
        let inc = self.incident(n);
        if inc.len() != 2 || self.degree(n) != 2 {
            return;
        }
        let (a, b) = (inc[0], inc[1]);
        let mut first = self.chains[a].clone();
        if first.to != Some(n) {
            first.pts.reverse();
            first.widths.reverse();
            std::mem::swap(&mut first.from, &mut first.to);
        }
        let mut second = self.chains[b].clone();
        if second.from != Some(n) {
            second.pts.reverse();
            second.widths.reverse();
            std::mem::swap(&mut second.from, &mut second.to);
        }
        first.pts.pop();
        first.widths.pop();
        first.pts.extend(second.pts);
        first.widths.extend(second.widths);
        first.to = second.to;
        if first.from.is_none() || first.from == first.to && first.from == Some(n) {
            first.from = None;
            first.to = None;
        }
        self.chains[a] = first;
        self.chains[b].alive = false;
        self.nodes[n].alive = false;
    }

    /// Junctions joined by a chain shorter than `factor · width`, or whose refined positions lie
    /// within half that distance, become one node; tiny self-loops
    /// at a junction (thinning artefacts) are removed.
    pub(crate) fn merge_junctions(&mut self, p: &GraphParams) {
        loop {
            let mut target: Option<usize> = None;
            for (ci, c) in self.chains.iter().enumerate() {
                let (Some(a), Some(b)) = (c.from, c.to) else { continue };
                if !c.alive || self.degree(a) < 3 || self.degree(b) < 3 {
                    continue;
                }
                let width = self.nodes[a].width.max(self.nodes[b].width);
                let limit = p.junction_merge_factor * width;
                let limit = if a == b { 2.0 * limit } else { limit };
                // Short connecting chain, or both (refined) positions describe the same crossing.
                let same_place = a != b && self.nodes[a].at.dist(self.nodes[b].at) < 0.5 * limit;
                if c.length() < limit || same_place {
                    target = Some(ci);
                    break;
                }
            }
            let Some(ci) = target else { break };
            let (Some(a), Some(b)) = (self.chains[ci].from, self.chains[ci].to) else { break };
            self.chains[ci].alive = false;
            if a != b {
                let (da, db) = (self.degree(a) as f64, self.degree(b) as f64);
                let at = P {
                    x: (self.nodes[a].at.x * da + self.nodes[b].at.x * db) / (da + db),
                    y: (self.nodes[a].at.y * da + self.nodes[b].at.y * db) / (da + db),
                };
                self.nodes[a].at = at;
                self.nodes[a].width = self.nodes[a].width.max(self.nodes[b].width);
                self.nodes[b].alive = false;
                for c in self.chains.iter_mut().filter(|c| c.alive) {
                    if c.from == Some(b) {
                        c.from = Some(a);
                    }
                    if c.to == Some(b) {
                        c.to = Some(a);
                    }
                }
                self.sync_ends(a);
            }
            self.dissolve_if_degree_two(a);
        }
    }

    /// Moves chain end points onto their node positions.
    fn sync_ends(&mut self, n: usize) {
        let at = self.nodes[n].at;
        for c in self.chains.iter_mut().filter(|c| c.alive) {
            if c.from == Some(n)
                && let Some(p) = c.pts.first_mut()
            {
                *p = at;
            }
            if c.to == Some(n)
                && let Some(p) = c.pts.last_mut()
            {
                *p = at;
            }
        }
    }

    /// Unit direction (px) leaving node `n` along chain `ci` from the given end, measured over
    /// points between one and three widths away (the junction zone itself is distorted).
    pub(crate) fn branch_direction(&self, ci: usize, at_start: bool) -> Option<(P, (f64, f64))> {
        let c = &self.chains[ci];
        let n = if at_start { c.from } else { c.to }?;
        let w = self.nodes[n].width.max(1.0);
        let mut pts = c.pts.clone();
        if !at_start {
            pts.reverse();
        }
        let origin = pts[0];
        let mut sel: Vec<P> = pts
            .iter()
            .copied()
            .filter(|q| {
                let d = q.dist(origin);
                d >= w && d <= 3.0 * w
            })
            .collect();
        if sel.len() < 2 {
            sel = pts.iter().copied().skip(1).take(8).collect();
        }
        let far = *sel.last()?;
        let (cx, cy) = (
            sel.iter().map(|q| q.x).sum::<f64>() / sel.len() as f64,
            sel.iter().map(|q| q.y).sum::<f64>() / sel.len() as f64,
        );
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        for q in &sel {
            let (dx, dy) = (q.x - cx, q.y - cy);
            sxx += dx * dx;
            sxy += dx * dy;
            syy += dy * dy;
        }
        let ang = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let (mut ux, mut uy) = (ang.cos(), ang.sin());
        if (far.x - origin.x) * ux + (far.y - origin.y) * uy < 0.0 {
            ux = -ux;
            uy = -uy;
        }
        if sel.len() < 3 {
            let l = (far.x - origin.x).hypot(far.y - origin.y);
            if l <= 1e-9 {
                return None;
            }
            (ux, uy) = ((far.x - origin.x) / l, (far.y - origin.y) / l);
        }
        Some((P { x: cx, y: cy }, (ux, uy)))
    }

    /// Junction position = least-squares intersection of its branch lines, accepted only within
    /// one stroke width of the skeleton junction.
    pub(crate) fn refine_junctions(&mut self) {
        for n in 0..self.nodes.len() {
            if !self.nodes[n].alive || self.degree(n) < 3 {
                continue;
            }
            let mut lines = Vec::new();
            for ci in self.incident(n) {
                for at_start in [true, false] {
                    let c = &self.chains[ci];
                    if (if at_start { c.from } else { c.to }) == Some(n)
                        && let Some(l) = self.branch_direction(ci, at_start)
                    {
                        lines.push(l);
                    }
                }
            }
            // Minimise Σ |(I − u uᵀ)(x − c)|².
            let (mut a11, mut a12, mut a22, mut b1, mut b2) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for (c, (ux, uy)) in &lines {
                let (m11, m12, m22) = (1.0 - ux * ux, -ux * uy, 1.0 - uy * uy);
                a11 += m11;
                a12 += m12;
                a22 += m22;
                b1 += m11 * c.x + m12 * c.y;
                b2 += m12 * c.x + m22 * c.y;
            }
            let det = a11 * a22 - a12 * a12;
            if det.abs() < 1e-6 {
                continue;
            }
            let at = P { x: (b1 * a22 - a12 * b2) / det, y: (a11 * b2 - a12 * b1) / det };
            if at.x.is_finite() && at.y.is_finite() && at.dist(self.nodes[n].at) <= self.nodes[n].width {
                self.nodes[n].at = at;
                self.sync_ends(n);
            }
        }
    }
}
