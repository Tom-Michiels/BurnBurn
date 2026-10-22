//! A static bounding-box tree with switchable items, for "the cheapest of the live items".
//!
//! The line planner picks, again and again, the ready contour whose entry is cheapest for the
//! head. Doing that by scanning every contour is quadratic. Here the contours sit in a tree of
//! boxes; an item can be switched on and off (a contour becomes ready, then is cut) in
//! `O(log n)`, and the cheapest live item is found by visiting only boxes that could still beat
//! the best so far. The result is *exactly* what a scan in index order would give: equal costs
//! go to the lowest index.

use burnburn_geometry::{Bounds, Point};

const LEAF_SIZE: usize = 4;

#[derive(Debug, Clone)]
struct Node {
    bounds: Bounds,
    /// Leaf: `items[start..end]`; inner: both children are set and `start..end` is unused.
    start: usize,
    end: usize,
    left: Option<usize>,
    right: Option<usize>,
    parent: Option<usize>,
    /// How many live items lie below.
    live: usize,
}

/// What a [`NearestIndex::cheapest`] search examined.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SearchStats {
    pub evaluated: usize,
}

#[derive(Debug)]
pub(crate) struct NearestIndex {
    /// Item ids in tree order.
    items: Vec<usize>,
    nodes: Vec<Node>,
    /// The leaf of each item id (indexed by id).
    leaf_of: Vec<usize>,
    live: Vec<bool>,
}

impl NearestIndex {
    /// Builds over `boxes[id]`, with `alive(id)` deciding which items start live. Every box must
    /// contain every point whose cost the caller will report for that item.
    pub fn build(boxes: &[Bounds], alive: impl Fn(usize) -> bool) -> Self {
        let mut items: Vec<usize> = (0..boxes.len()).collect();
        let mut nodes = Vec::new();
        let mut leaf_of = vec![0; boxes.len()];
        if !items.is_empty() {
            let len = items.len();
            build_node(boxes, &mut items, 0..len, None, &mut nodes, &mut leaf_of);
        }
        let live: Vec<bool> = (0..boxes.len()).map(alive).collect();
        let mut index = NearestIndex { items, nodes, leaf_of, live };
        index.recount();
        index
    }

    fn recount(&mut self) {
        for k in 0..self.nodes.len() {
            // Children are built before parents, so a single forward pass has the children's counts.
            let count = match (self.nodes[k].left, self.nodes[k].right) {
                (Some(l), Some(r)) => self.nodes[l].live + self.nodes[r].live,
                _ => self.items[self.nodes[k].start..self.nodes[k].end].iter().filter(|&&id| self.live[id]).count(),
            };
            self.nodes[k].live = count;
        }
    }

    /// Switches an item on or off.
    pub fn set_live(&mut self, id: usize, on: bool) {
        let Some(slot) = self.live.get_mut(id) else { return };
        if *slot == on {
            return;
        }
        *slot = on;
        let mut at = self.leaf_of.get(id).copied();
        while let Some(k) = at {
            let node = &mut self.nodes[k];
            if on {
                node.live += 1;
            } else {
                node.live -= 1;
            }
            at = node.parent;
        }
    }

    /// The live item with the lowest `cost(id)` (lowest id among equal costs), with its cost.
    /// `cost` must never be lower than the Euclidean distance from `from` to the item's box.
    pub fn cheapest(
        &self,
        from: Point,
        mut cost: impl FnMut(usize) -> f64,
        stats: &mut SearchStats,
    ) -> Option<(usize, f64)> {
        let root = self.nodes.len().checked_sub(1)?;
        let mut best: Option<(usize, f64)> = None;
        self.visit(root, from, &mut cost, &mut best, stats);
        best
    }

    fn visit(
        &self,
        k: usize,
        from: Point,
        cost: &mut impl FnMut(usize) -> f64,
        best: &mut Option<(usize, f64)>,
        stats: &mut SearchStats,
    ) {
        let node = &self.nodes[k];
        if node.live == 0 {
            return;
        }
        match (node.left, node.right) {
            (Some(l), Some(r)) => {
                let (dl, dr) = (self.nodes[l].bounds.distance_to(from), self.nodes[r].bounds.distance_to(from));
                let (first, second) = if dr < dl { ((r, dr), (l, dl)) } else { ((l, dl), (r, dr)) };
                for (child, lower) in [first, second] {
                    if !Self::cannot_win(lower, best) {
                        self.visit(child, from, cost, best, stats);
                    }
                }
            }
            _ => {
                for &id in &self.items[node.start..node.end] {
                    if !self.live[id] {
                        continue;
                    }
                    stats.evaluated += 1;
                    let c = cost(id);
                    let wins = match *best {
                        None => true,
                        Some((bi, bc)) => c < bc || (c == bc && id < bi),
                    };
                    if wins {
                        *best = Some((id, c));
                    }
                }
            }
        }
    }

    /// A box at `lower` or more can only beat the best through a tie at a lower id, so it is
    /// skipped only when it is strictly farther (a hair of slack for rounding).
    fn cannot_win(lower: f64, best: &Option<(usize, f64)>) -> bool {
        best.is_some_and(|(_, bc)| lower > bc + 1e-9 * (1.0 + bc.abs()))
    }
}

fn build_node(
    boxes: &[Bounds],
    items: &mut [usize],
    range: std::ops::Range<usize>,
    parent: Option<usize>,
    nodes: &mut Vec<Node>,
    leaf_of: &mut [usize],
) -> usize {
    let slice = &mut items[range.clone()];
    let bounds = slice.iter().map(|&id| boxes[id]).reduce(Bounds::union).unwrap_or(Bounds::from_point(Point::ORIGIN));
    if slice.len() <= LEAF_SIZE {
        nodes.push(Node { bounds, start: range.start, end: range.end, left: None, right: None, parent, live: 0 });
        let k = nodes.len() - 1;
        for &id in &items[range] {
            leaf_of[id] = k;
        }
        return k;
    }
    let by_x = bounds.width() >= bounds.height();
    slice.sort_by(|&a, &b| {
        let (ca, cb) = (boxes[a].center(), boxes[b].center());
        let o = if by_x { ca.x.total_cmp(&cb.x) } else { ca.y.total_cmp(&cb.y) };
        o.then(a.cmp(&b))
    });
    let mid = range.start + slice.len() / 2;
    // Parents are pushed after their children, so their index is not known yet: patch afterwards.
    let left = build_node(boxes, items, range.start..mid, None, nodes, leaf_of);
    let right = build_node(boxes, items, mid..range.end, None, nodes, leaf_of);
    nodes.push(Node { bounds, start: 0, end: 0, left: Some(left), right: Some(right), parent, live: 0 });
    let me = nodes.len() - 1;
    nodes[left].parent = Some(me);
    nodes[right].parent = Some(me);
    me
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn square(x: f64, y: f64) -> Bounds {
        Bounds::from_corners(Point::new(x, y), Point::new(x + 1.0, y + 1.0))
    }

    #[test]
    fn an_empty_or_fully_switched_off_index_finds_nothing() {
        let empty = NearestIndex::build(&[], |_| true);
        assert_eq!(empty.cheapest(Point::ORIGIN, |_| 0.0, &mut SearchStats::default()), None);
        let mut one = NearestIndex::build(&[square(0.0, 0.0)], |_| true);
        one.set_live(0, false);
        assert_eq!(one.cheapest(Point::ORIGIN, |_| 0.0, &mut SearchStats::default()), None);
    }

    proptest! {
        #[test]
        fn the_cheapest_live_item_is_the_first_minimum_of_a_scan(
            cells in prop::collection::vec((0..12i32, 0..12i32), 1..80),
            flips in prop::collection::vec(0usize..80, 0..40),
            (hx, hy) in (-3..15i32, -3..15i32),
        ) {
            let boxes: Vec<Bounds> = cells.iter().map(|&(x, y)| square(f64::from(x), f64::from(y))).collect();
            let mut index = NearestIndex::build(&boxes, |id| id % 3 != 0);
            let mut live: Vec<bool> = (0..boxes.len()).map(|id| id % 3 != 0).collect();
            for f in flips {
                let id = f % boxes.len();
                live[id] = !live[id];
                index.set_live(id, live[id]);
            }
            let head = Point::new(f64::from(hx), f64::from(hy));
            // A cost that is the distance to the box centre, never below the distance to the box.
            let cost = |id: usize| head.distance(boxes[id].center());
            let mut expected: Option<(usize, f64)> = None;
            for (id, _) in live.iter().enumerate().filter(|(_, on)| **on) {
                if expected.is_none_or(|(_, c)| cost(id) < c) {
                    expected = Some((id, cost(id)));
                }
            }
            prop_assert_eq!(index.cheapest(head, cost, &mut SearchStats::default()), expected);
        }
    }
}
