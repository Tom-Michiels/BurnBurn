//! Deterministic work counters of the interaction route (G8-trace, X04 regression guard).
//!
//! Wall-clock time depends on the machine; these counts do not. They are per thread (so parallel
//! tests do not disturb each other) and cost one increment where they are bumped. A guard test
//! resets them, runs a route on a 100k-object document and bounds them per object.

/// What this thread's interaction code did since [`reset_editor_work`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EditorWork {
    /// Objects whose outline a hit test looked at (the candidates the spatial index handed over).
    pub pick_candidates: u64,
    /// Steps of the selection set: one per insert or lookup, and the length of every scan.
    pub selection_steps: u64,
    /// Items built into a scene.
    pub scene_items: u64,
}

use std::cell::Cell;

thread_local! {
    static PICK: Cell<u64> = const { Cell::new(0) };
    static SELECTION: Cell<u64> = const { Cell::new(0) };
    static SCENE: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn pick_candidate() {
    PICK.with(|c| c.set(c.get() + 1));
}

pub(crate) fn selection_steps(steps: usize) {
    SELECTION.with(|c| c.set(c.get() + steps as u64));
}

pub(crate) fn scene_item() {
    SCENE.with(|c| c.set(c.get() + 1));
}

/// The counters of this thread.
pub fn editor_work() -> EditorWork {
    EditorWork {
        pick_candidates: PICK.with(Cell::get),
        selection_steps: SELECTION.with(Cell::get),
        scene_items: SCENE.with(Cell::get),
    }
}

/// Sets this thread's counters to zero.
pub fn reset_editor_work() {
    PICK.with(|c| c.set(0));
    SELECTION.with(|c| c.set(0));
    SCENE.with(|c| c.set(0));
}
