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
        let mut out = Vec::new();
        // The root is the last node built.
        let mut stack: Vec<usize> = self.nodes.len().checked_sub(1).into_iter().collect();
        while let Some(i) = stack.pop() {
            let Some(node) = self.nodes.get(i) else { continue };
            if !overlaps(node.bounds()) {
                continue;
            }
            match node {
                Node::Leaf { items, .. } => {
                    for (k, b) in &self.items[items.clone()] {
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
        out
    }

    /// Keys whose bounds intersect `b` (touching counts).
    pub fn query_rect(&self, b: &Bounds) -> Vec<K> {
        self.visit(|nb| nb.intersects(b))
    }

    /// Keys whose bounds come within `radius` of `p`.
    pub fn query_point(&self, p: Point, radius: f64) -> Vec<K> {
        self.visit(|nb| nb.distance_to(p) <= radius)
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
