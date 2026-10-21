//! The spatial index behind canvas picking (hit test and snapping), X04.
//!
//! Building it costs the bounds of every object, so with 100k objects it must not be rebuilt for
//! every click or pointer move. The index is cached per thread and reused as long as the
//! document's [`PickingKey`] (objects + arrange layer, compared by identity) is unchanged; any
//! edit, hide, lock or reorder produces a different key and therefore a fresh index. The cache
//! holds the key, which keeps the indexed content alive and immutable.

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::sync::Arc;

use burnburn_document::{DocumentSnapshot, PickingKey};
use burnburn_geometry::SpatialIndex;
use burnburn_types::GeometryId;

/// Index key: draw rank first (so ties resolve topmost-first), then the id.
pub(crate) type PickKey = (Reverse<usize>, GeometryId);

/// Spatial index over **all** objects of one document state; callers filter by selectable/locked.
pub(crate) struct PickIndex {
    pub(crate) index: SpatialIndex<PickKey>,
    key: PickingKey,
}

thread_local! {
    static CACHE: RefCell<Option<Arc<PickIndex>>> = const { RefCell::new(None) };
    static BUILDS: Cell<usize> = const { Cell::new(0) };
}

/// How many times this thread built a pick index (diagnostics and tests: a cache hit does not
/// count).
pub fn pick_index_builds() -> usize {
    BUILDS.with(Cell::get)
}

fn build(snapshot: &DocumentSnapshot) -> PickIndex {
    BUILDS.with(|b| b.set(b.get() + 1));
    let order = snapshot.draw_order();
    let index = SpatialIndex::build(
        order
            .iter()
            .enumerate()
            .filter_map(|(rank, id)| snapshot.object(*id)?.bounds().map(|b| ((Reverse(rank), *id), b))),
    );
    PickIndex { index, key: snapshot.picking_key() }
}

/// The pick index for `snapshot`, from the cache when the content is the one last indexed.
pub(crate) fn pick_index(snapshot: &DocumentSnapshot) -> Arc<PickIndex> {
    let key = snapshot.picking_key();
    CACHE.with(|cache| {
        // A re-entrant call cannot happen (the borrow is released before returning), but a
        // failed borrow must degrade to an uncached build, never panic.
        let Ok(mut slot) = cache.try_borrow_mut() else { return Arc::new(build(snapshot)) };
        if let Some(hit) = slot.as_ref().filter(|i| i.key == key) {
            return Arc::clone(hit);
        }
        let fresh = Arc::new(build(snapshot));
        *slot = Some(Arc::clone(&fresh));
        fresh
    })
}
