//! Draw order and locks (W2-L1-bewerken): the arrangement layer of a document.
//!
//! Draw order is the render stacking of objects, separate from the cut order of the process
//! layer. Without an explicit order, objects are drawn in creation order (id order). Once an
//! order was set it lists the arranged objects back to front; objects created after that are
//! drawn on top, in creation order.

use std::collections::BTreeSet;

use burnburn_types::GeometryId;

/// Why stored arrangement parts do not form a valid layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArrangeLayerError {
    /// An object occurs twice in the stored draw order.
    DuplicateOrder(GeometryId),
}

/// Draw order and locks of one document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArrangeLayer {
    order: Vec<GeometryId>,
    locked: BTreeSet<GeometryId>,
}

impl ArrangeLayer {
    /// A layer from stored parts (open/recovery). The snapshot checks that the objects exist.
    pub fn from_parts(order: Vec<GeometryId>, locked: Vec<GeometryId>) -> Result<Self, ArrangeLayerError> {
        let mut seen = BTreeSet::new();
        for id in &order {
            if !seen.insert(*id) {
                return Err(ArrangeLayerError::DuplicateOrder(*id));
            }
        }
        Ok(ArrangeLayer { order, locked: locked.into_iter().collect() })
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty() && self.locked.is_empty()
    }

    /// The explicitly ordered objects, back to front (empty: creation order).
    pub fn order(&self) -> &[GeometryId] {
        &self.order
    }

    pub fn locked(&self) -> impl ExactSizeIterator<Item = GeometryId> + '_ {
        self.locked.iter().copied()
    }

    pub fn is_locked(&self, id: GeometryId) -> bool {
        self.locked.contains(&id)
    }

    /// Back-to-front drawing sequence of `existing` (ascending ids): the explicitly ordered
    /// objects that still exist, then the remaining ones on top.
    pub fn draw_order(&self, existing: impl IntoIterator<Item = GeometryId>) -> Vec<GeometryId> {
        let existing: Vec<GeometryId> = existing.into_iter().collect();
        if self.order.is_empty() {
            return existing;
        }
        let present: BTreeSet<GeometryId> = existing.iter().copied().collect();
        let mut out: Vec<GeometryId> = self.order.iter().copied().filter(|id| present.contains(id)).collect();
        let listed: BTreeSet<GeometryId> = out.iter().copied().collect();
        out.extend(existing.into_iter().filter(|id| !listed.contains(id)));
        out
    }

    pub(crate) fn set_order(&mut self, order: Vec<GeometryId>) {
        self.order = order;
    }

    /// Locks or unlocks `id`; true when that changed something.
    pub(crate) fn set_locked(&mut self, id: GeometryId, locked: bool) -> bool {
        if locked { self.locked.insert(id) } else { self.locked.remove(&id) }
    }

    /// Forgets objects that no longer exist.
    pub(crate) fn retain_existing(&mut self, exists: impl Fn(GeometryId) -> bool) {
        self.order.retain(|id| exists(*id));
        self.locked.retain(|id| exists(*id));
    }
}

/// Moves `selected` within `order` (back to front); true when the sequence changed.
pub(crate) fn reorder(
    order: &mut Vec<GeometryId>,
    selected: &BTreeSet<GeometryId>,
    change: crate::OrderChange,
) -> bool {
    use crate::OrderChange;
    let before = order.clone();
    match change {
        OrderChange::Forward => {
            for i in (0..order.len().saturating_sub(1)).rev() {
                if selected.contains(&order[i]) && !selected.contains(&order[i + 1]) {
                    order.swap(i, i + 1);
                }
            }
        }
        OrderChange::Backward => {
            for i in 1..order.len() {
                if selected.contains(&order[i]) && !selected.contains(&order[i - 1]) {
                    order.swap(i, i - 1);
                }
            }
        }
        OrderChange::ToFront => order.sort_by_key(|id| selected.contains(id)),
        OrderChange::ToBack => order.sort_by_key(|id| !selected.contains(id)),
    }
    *order != before
}
