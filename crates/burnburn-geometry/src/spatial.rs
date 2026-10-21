//! A static bounding-volume hierarchy for hit-testing and snapping over many objects.

use crate::query::HitKind;
use crate::{Bounds, HitTestTolerance, Path, Point};

const LEAF_SIZE: usize = 8;

#[derive(Debug, Clone)]
enum Node {
    Leaf { bounds: Bounds, items: std::ops::Range<usize> },
    Inner { bounds: Bounds, left: usize, right: usize },
}

impl Node {
    fn bounds(&self) -> &Bounds {
        match self {
            Node::Leaf { bounds, .. } | Node::Inner { bounds, .. } => bounds,
        }
    }
}

/// Bulk-loaded index of `(key, bounds)`. Rebuild per document revision (`O(n log² n)`).
#[derive(Debug, Clone)]
pub struct SpatialIndex<K> {
    items: Vec<(K, Bounds)>,
    nodes: Vec<Node>,
}

impl<K: Copy> SpatialIndex<K> {
    pub fn build(items: impl IntoIterator<Item = (K, Bounds)>) -> Self {
        let mut items: Vec<(K, Bounds)> = items.into_iter().collect();
        let mut nodes = Vec::new();
        if !items.is_empty() {
            let len = items.len();
            build_node(&mut items, 0..len, &mut nodes);
        }
        SpatialIndex { items, nodes }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn visit(&self, overlaps: impl Fn(&Bounds) -> bool) -> Vec<K> {
        self.visit_counted(overlaps).0
    }

    /// Visits like `visit` and also returns how many node and item bounds were tested.
    fn visit_counted(&self, overlaps: impl Fn(&Bounds) -> bool) -> (Vec<K>, usize) {
        let mut out = Vec::new();
        let mut cost = 0usize;
        // The root is the last node built.
        let mut stack: Vec<usize> = self.nodes.len().checked_sub(1).into_iter().collect();
        while let Some(i) = stack.pop() {
            let Some(node) = self.nodes.get(i) else { continue };
            cost += 1;
            if !overlaps(node.bounds()) {
                continue;
            }
            match node {
                Node::Leaf { items, .. } => {
                    for (k, b) in &self.items[items.clone()] {
                        cost += 1;
                        if overlaps(b) {
                            out.push(*k);
                        }
                    }
                }
                Node::Inner { left, right, .. } => {
                    stack.push(*left);
                    stack.push(*right);
                }
            }
        }
        (out, cost)
    }

    /// Keys whose bounds intersect `b` (touching counts).
    pub fn query_rect(&self, b: &Bounds) -> Vec<K> {
        self.visit(|nb| nb.intersects(b))
    }

    /// Keys whose bounds come within `radius` of `p`.
    pub fn query_point(&self, p: Point, radius: f64) -> Vec<K> {
        self.visit(|nb| nb.distance_to(p) <= radius)
    }

    /// Like [`Self::query_point`], plus how many tree nodes and item bounds the query examined
    /// (a deterministic measure of its cost, for tests).
    pub fn query_point_cost(&self, p: Point, radius: f64) -> (Vec<K>, usize) {
        self.visit_counted(|nb| nb.distance_to(p) <= radius)
    }
}

fn union_of<K>(items: &[(K, Bounds)]) -> Option<Bounds> {
    items.iter().map(|(_, b)| *b).reduce(Bounds::union)
}

/// Builds the subtree over `items[range]`, returns its node index (children before parents).
fn build_node<K>(items: &mut [(K, Bounds)], range: std::ops::Range<usize>, nodes: &mut Vec<Node>) -> usize {
    let slice = &mut items[range.clone()];
    let bounds = union_of(slice).unwrap_or(Bounds::from_point(Point::ORIGIN));
    if slice.len() <= LEAF_SIZE {
        nodes.push(Node::Leaf { bounds, items: range });
        return nodes.len() - 1;
    }
    let by_x = bounds.width() >= bounds.height();
    slice.sort_by(|a, b| {
        let (ca, cb) = (a.1.center(), b.1.center());
        if by_x { ca.x.total_cmp(&cb.x) } else { ca.y.total_cmp(&cb.y) }
    });
    let mid = range.start + slice.len() / 2;
    let left = build_node(items, range.start..mid, nodes);
    let right = build_node(items, mid..range.end, nodes);
    nodes.push(Node::Inner { bounds, left, right });
    nodes.len() - 1
}

/// One hit for selection (cycling goes through the list in order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitCandidate<K> {
    pub key: K,
    pub kind: HitKind,
}

/// Every object hit at `q`: edge hits by distance first, then interiors from the smallest
/// bounding area up (an inner shape before the plate around it).
pub fn hit_test<'a, K: Copy + Ord>(
    index: &SpatialIndex<K>,
    lookup: impl Fn(K) -> Option<&'a Path>,
    q: Point,
    tol: &HitTestTolerance,
) -> Vec<HitCandidate<K>> {
    let mut edges = Vec::new();
    let mut interiors = Vec::new();
    for key in index.query_point(q, tol.mm()) {
        let Some(path) = lookup(key) else { continue };
        match path.hit(q, tol) {
            Some(HitKind::Edge(n)) => edges.push((n.distance, key, HitKind::Edge(n))),
            Some(HitKind::Interior) => {
                let area = path.bounds().map_or(0.0, |b| b.width() * b.height());
                interiors.push((area, key, HitKind::Interior));
            }
            None => {}
        }
    }
    edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    interiors.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    edges.into_iter().chain(interiors).map(|(_, key, kind)| HitCandidate { key, kind }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 × 100 unit boxes on a 2 mm pitch (10 000 items).
    fn grid(n: usize) -> SpatialIndex<(usize, usize)> {
        SpatialIndex::build((0..n * n).map(|k| {
            let (i, j) = (k % n, k / n);
            let min = Point::new(2.0 * i as f64, 2.0 * j as f64);
            ((i, j), Bounds::from_corners(min, Point::new(min.x + 1.0, min.y + 1.0)))
        }))
    }

    /// X04: a point query examines O(log N) nodes, not the whole index, and finds what a brute
    /// force scan over all bounds finds.
    #[test]
    fn point_query_examines_few_nodes_and_matches_brute_force() {
        let n = 100;
        let index = grid(n);
        let total = index.len();
        for (x, y) in [(0.5, 0.5), (101.0, 55.5), (198.9, 198.9), (-3.0, -3.0), (77.0, 500.0)] {
            let at = Point::new(x, y);
            let (mut got, cost) = index.query_point_cost(at, 1.5);
            got.sort_unstable();
            let mut want: Vec<(usize, usize)> = (0..n * n)
                .map(|k| (k % n, k / n))
                .filter(|&(i, j)| {
                    let min = Point::new(2.0 * i as f64, 2.0 * j as f64);
                    Bounds::from_corners(min, Point::new(min.x + 1.0, min.y + 1.0)).distance_to(at) <= 1.5
                })
                .collect();
            want.sort_unstable();
            assert_eq!(got, want, "query at ({x}, {y})");
            assert!(cost * 20 < total, "query at ({x}, {y}) examined {cost} of {total} items");
        }
    }
}
