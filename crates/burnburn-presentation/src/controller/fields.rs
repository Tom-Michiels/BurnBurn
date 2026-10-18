//! Inspector fields: position, size, rotation and the exact tool size.

use burnburn_document::{Anchor, Axis, CommandPayload, DocumentCommand};
use burnburn_geometry::{Affine, Vector};
use burnburn_types::{Angle, DocumentId, GeometryId, RequestId};

use crate::field::{parse_angle, parse_length, parse_position};
use crate::intent::FieldKey;
use crate::view_model::MessageKey;

use super::canvas::handle_step;
use super::{AppController, AppUpdate, Session, after_change, apply_command, apply_edit, error};

impl AppController {
    pub(super) fn commit_field(&mut self, document: DocumentId, field: FieldKey, text: &str) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let Some(target) = s.selection.single() else { return AppUpdate::default() };
            if field == FieldKey::Rotation {
                return rotate_selection(s, document, target, text, request);
            }
            let parsed = match field {
                FieldKey::X | FieldKey::Y => parse_position(text),
                _ => parse_length(text),
            };
            let value = match parsed {
                Ok(v) => v,
                Err(e) => {
                    s.field_errors.insert(field, MessageKey::InvalidLength(e));
                    return AppUpdate { view_changed: true, ..AppUpdate::default() };
                }
            };
            s.field_errors.remove(&field);
            let snapshot = s.doc.snapshot();
            let Some(bounds) = snapshot.object(target).and_then(|o| o.bounds()) else { return AppUpdate::default() };
            let payload = match field {
                FieldKey::X | FieldKey::Y => {
                    let d = if field == FieldKey::X {
                        Vector::new(value.mm() - bounds.min().x, 0.0)
                    } else {
                        Vector::new(0.0, value.mm() - bounds.min().y)
                    };
                    // An unchanged value sends no command (phase-B decision for field commits).
                    if d.x == 0.0 && d.y == 0.0 {
                        return AppUpdate { view_changed: true, ..AppUpdate::default() };
                    }
                    let Ok(transform) = Affine::translate(d) else {
                        return error(Some(document), MessageKey::CommandRejected);
                    };
                    CommandPayload::Transform { targets: vec![target], transform }
                }
                _ => {
                    let axis = if field == FieldKey::Height { Axis::Height } else { Axis::Width };
                    CommandPayload::SetDimension { target, axis, value, anchor: Anchor::Min }
                }
            };
            let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
            let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
            let result = apply_command(&s.fonts, &mut s.doc, cmd);
            after_change(s, document, result)
        })
    }

    pub(super) fn commit_tool_size(&mut self, document: DocumentId, width: &str, height: &str) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            if !s.editor.is_gesture_active() {
                return AppUpdate::default();
            }
            let w = parse_length(width);
            let h = if height.trim().is_empty() { w } else { parse_length(height) };
            let (w, h) = match (w, h) {
                (Ok(w), Ok(h)) => (w, h),
                (Err(e), _) | (_, Err(e)) => {
                    // The gesture stays in preview; the drawing HUD shows the error.
                    s.tool_size_error = Some(MessageKey::InvalidLength(e));
                    return AppUpdate { view_changed: true, ..AppUpdate::default() };
                }
            };
            s.tool_size_error = None;
            let step = s.editor.exact_size(w, h);
            match request {
                Some(request) => handle_step(s, document, step, request),
                None => error(Some(document), MessageKey::CommandRejected),
            }
        })
    }
}

/// Rotation field: turns `target` about its bounds centre by the typed angle (one Transform).
pub(super) fn rotate_selection(
    s: &mut Session,
    document: DocumentId,
    target: GeometryId,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let degrees = match parse_angle(text) {
        Ok(d) => d,
        Err(e) => {
            s.field_errors.insert(FieldKey::Rotation, MessageKey::InvalidLength(e));
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
    };
    s.field_errors.remove(&FieldKey::Rotation);
    if degrees.rem_euclid(360.0) == 0.0 {
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    }
    let snapshot = s.doc.snapshot();
    let Some(centre) = snapshot.object(target).and_then(|o| o.bounds()).map(|b| b.center()) else {
        return AppUpdate::default();
    };
    let Ok(angle) = Angle::from_degrees(degrees) else { return error(Some(document), MessageKey::CommandRejected) };
    let to_origin = Affine::translate(Vector::new(-centre.x, -centre.y));
    let back = Affine::translate(Vector::new(centre.x, centre.y));
    let (Ok(to_origin), Ok(back)) = (to_origin, back) else {
        return error(Some(document), MessageKey::CommandRejected);
    };
    let transform = to_origin.then(&Affine::rotate(angle)).then(&back);
    apply_edit(s, document, CommandPayload::Transform { targets: vec![target], transform }, request)
}
