//! Clipboard (W2-L1-bewerken): cut, copy, paste, paste in place and duplicate.
//!
//! The controller keeps the last copy for all windows and hands the same bytes to the host for the
//! OS pasteboard (`HostEffect::SetClipboard`). A paste is one `ArrangeCommand::Paste`, so it is one
//! undo entry; copying never touches the document. Pasting objects that came from another project
//! drops their layer references (an operation id only means something in its own project).

use std::collections::{BTreeMap, BTreeSet};

use burnburn_document::{ArrangeCommand, CommandPayload, PasteItem};
use burnburn_geometry::{Bounds, Vector};
use burnburn_io::clipboard::{Clipboard, decode_clipboard, encode_clipboard};
use burnburn_types::{DocumentId, GeometryId, GroupId, RequestId};

use crate::intent::CommandKey;
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, HostEffect, Session, apply_edit, error};

/// The objects of `ids` as paste items, back to front (the draw order is part of a copy).
pub(super) fn items_of(s: &Session, ids: &[GeometryId]) -> Vec<PasteItem> {
    let snapshot = s.doc.snapshot();
    let wanted: BTreeSet<GeometryId> = ids.iter().copied().collect();
    let mut keys: BTreeMap<GroupId, u32> = BTreeMap::new();
    let mut items = Vec::new();
    for id in snapshot.draw_order().into_iter().filter(|id| wanted.contains(id)) {
        let Some(obj) = snapshot.object(id) else { continue };
        let group = obj
            .groups()
            .iter()
            .map(|g| {
                let next = u32::try_from(keys.len()).unwrap_or(u32::MAX);
                *keys.entry(*g).or_insert(next)
            })
            .collect();
        items.push(PasteItem {
            name: obj.name().to_owned(),
            kind: obj.kind(),
            path: obj.path().as_ref().clone(),
            parametric: obj.parametric().cloned(),
            group,
            operations: snapshot.process().operations_using(id),
        });
    }
    items
}

fn bounds_of(items: &[PasteItem]) -> Option<Bounds> {
    items.iter().filter_map(|i| i.path.bounds()).reduce(Bounds::union)
}

/// Pastes `clip` into the session: centred on the view, or where it was copied from.
fn paste(
    s: &mut Session,
    document: DocumentId,
    clip: &Clipboard,
    in_place: bool,
    request: Option<RequestId>,
) -> AppUpdate {
    let mut items = clip.items.clone();
    if clip.project != s.uuid {
        for item in &mut items {
            item.operations.clear();
        }
    }
    let offset = match (in_place, bounds_of(&items)) {
        (false, Some(bounds)) => {
            let target =
                s.camera.points_to_world(&s.viewport, [s.viewport.width_pt() / 2.0, s.viewport.height_pt() / 2.0]);
            let centre = bounds.center();
            Vector::new(target.x - centre.x, target.y - centre.y)
        }
        _ => Vector::ZERO,
    };
    apply_edit(s, document, CommandPayload::Arrange(ArrangeCommand::Paste { items, offset }), request)
}

impl AppController {
    pub(super) fn clipboard_command(&mut self, document: DocumentId, command: CommandKey) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        match command {
            CommandKey::Copy | CommandKey::Cut => {
                let Some(s) = self.sessions.get_mut(&document) else { return AppUpdate::default() };
                let items = items_of(s, s.selection.ids());
                if items.is_empty() {
                    return AppUpdate::default();
                }
                let bytes = match encode_clipboard(s.uuid, &items) {
                    Ok(bytes) => bytes,
                    Err(_) => return error(Some(document), MessageKey::CommandRejected),
                };
                let clip = Clipboard { project: s.uuid, items };
                let mut update = if command == CommandKey::Cut {
                    let targets = s.selection.ids().to_vec();
                    // A cut that cannot delete copies nothing either.
                    if super::arrange::touches_locked(s, &CommandPayload::Delete { targets: targets.clone() }) {
                        return error(Some(document), MessageKey::ObjectsLocked);
                    }
                    apply_edit(s, document, CommandPayload::Delete { targets }, request)
                } else {
                    AppUpdate::default()
                };
                self.clipboard = Some(clip);
                update.effects.push(HostEffect::SetClipboard { bytes });
                update
            }
            CommandKey::Paste | CommandKey::PasteInPlace => {
                let Some(clip) = self.clipboard.clone() else { return AppUpdate::default() };
                let in_place = command == CommandKey::PasteInPlace;
                self.with_session(document, |s| paste(s, document, &clip, in_place, request))
            }
            CommandKey::Duplicate => self.with_session(document, |s| {
                let items = items_of(s, s.selection.ids());
                if items.is_empty() {
                    return AppUpdate::default();
                }
                let payload = CommandPayload::Arrange(ArrangeCommand::Paste { items, offset: Vector::ZERO });
                apply_edit(s, document, payload, request)
            }),
            _ => AppUpdate::default(),
        }
    }

    /// Paste with a payload the host read from the OS pasteboard.
    pub(super) fn paste_data(&mut self, document: Option<DocumentId>, bytes: Vec<u8>, in_place: bool) -> AppUpdate {
        let Some(document) = document.or(self.active) else { return AppUpdate::default() };
        let clip = match decode_clipboard(&bytes) {
            Ok(clip) => clip,
            Err(_) => return error(Some(document), MessageKey::ClipboardUnreadable),
        };
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| paste(s, document, &clip, in_place, request))
    }
}
