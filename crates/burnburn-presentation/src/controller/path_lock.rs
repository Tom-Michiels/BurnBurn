//! Shapes ▸ Lock Start Point / Lock Direction (C06): toggles on the selection.
//!
//! A press locks the switch on every selected shape, unless they all have it already: then it
//! frees them. Each shape keeps the other switch it had. Shapes that end up with the same lock are
//! one command, so a press is one undo step unless the selection holds shapes with different locks.

use std::collections::BTreeMap;

use burnburn_document::process::{PathLock, ProcessCommand};
use burnburn_document::{CommandPayload, DocumentSnapshot};
use burnburn_types::{GeometryId, RequestId};

use super::{AppController, AppUpdate, DocumentId, Session, apply_edit};
use crate::intent::CommandKey;

fn switch(lock: PathLock, key: CommandKey) -> bool {
    match key {
        CommandKey::LockPathStart => lock.start,
        _ => lock.direction,
    }
}

fn with_switch(lock: PathLock, key: CommandKey, on: bool) -> PathLock {
    match key {
        CommandKey::LockPathStart => PathLock { start: on, ..lock },
        _ => PathLock { direction: on, ..lock },
    }
}

/// Whether every selected shape has the lock `key` stands for (the menu item's check mark).
pub(super) fn checked(s: &Session, key: CommandKey) -> bool {
    let snapshot = s.doc.snapshot();
    let ids = s.selection.ids();
    !ids.is_empty() && ids.iter().all(|id| switch(snapshot.process().path_lock(*id), key))
}

/// Locks or frees the selection, one command per distinct resulting lock.
pub(super) fn toggle(
    s: &mut Session,
    document: DocumentId,
    key: CommandKey,
    mut requests: impl FnMut() -> Option<RequestId>,
) -> AppUpdate {
    let snapshot: std::sync::Arc<DocumentSnapshot> = s.doc.snapshot();
    let ids: Vec<GeometryId> = s.selection.ids().to_vec();
    if ids.is_empty() {
        return AppUpdate::default();
    }
    let on = !checked(s, key);
    let mut groups: BTreeMap<(bool, bool), Vec<GeometryId>> = BTreeMap::new();
    for id in ids {
        let now = snapshot.process().path_lock(id);
        let next = with_switch(now, key, on);
        if next != now {
            groups.entry((next.start, next.direction)).or_default().push(id);
        }
    }
    let mut update = AppUpdate { view_changed: true, ..AppUpdate::default() };
    for ((start, direction), geometries) in groups {
        let edit = ProcessCommand::SetPathLock { geometries, lock: PathLock { start, direction } };
        let next = apply_edit(s, document, CommandPayload::Process(edit), requests());
        update.effects.extend(next.effects);
        update.scenes_changed.extend(next.scenes_changed);
    }
    update
}

impl AppController {
    /// What the planner may not change about `geometry` in `document`.
    pub fn path_lock(&self, document: DocumentId, geometry: GeometryId) -> Option<PathLock> {
        let session = self.sessions.get(&document)?;
        Some(session.doc.snapshot().process().path_lock(geometry))
    }

    pub(super) fn toggle_path_lock(&mut self, document: DocumentId, key: CommandKey) -> AppUpdate {
        let mut ids = Vec::new();
        for _ in 0..3 {
            ids.push(self.next_id::<RequestId>());
        }
        self.with_session(document, |s| toggle(s, document, key, || ids.pop().flatten()))
    }
}
