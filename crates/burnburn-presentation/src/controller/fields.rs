//! Inspector fields: position, size, rotation, percent scale and shear of the selection (one
//! object or several, taken together), relative to the chosen reference point, and the exact tool
//! size.

use burnburn_document::{Anchor, Axis, CommandPayload, DocumentCommand};
use burnburn_geometry::{Affine, Bounds, Point, Vector};
use burnburn_types::{Angle, DocumentId, RequestId};

use crate::field::{parse_angle, parse_percent, parse_shear};
use crate::intent::FieldKey;
use crate::view_model::MessageKey;

use super::arrange::reference_in;
use super::canvas::handle_step;
use super::prefs::{LengthUnit, parse_length_in_unit, parse_position_in_unit};
use super::{AppController, AppUpdate, Session, after_change, apply_command, error};

impl AppController {
    pub(super) fn commit_field(&mut self, document: DocumentId, field: FieldKey, text: &str) -> AppUpdate {
        if field == FieldKey::Priority {
            // One command per selected shape: one request id each.
            let count = self.sessions.get(&document).map_or(0, |s| s.selection.len());
            let mut requests: Vec<Option<RequestId>> = (0..count).map(|_| self.next_id::<RequestId>()).collect();
            return self.with_session(document, |s| {
                if s.selection.is_empty() {
                    return AppUpdate::default();
                }
                super::process::commit_priority(s, document, text, &mut || requests.pop().flatten())
            });
        }
        let request = self.next_id::<RequestId>();
        // A bare number is in the display unit (X03).
        let units = self.app.prefs.units;
        self.with_session(document, |s| {
            if s.selection.is_empty() {
                return AppUpdate::default();
            }
            let snapshot = s.doc.snapshot();
            let Some(bounds) = s.selection.bounds(&snapshot) else { return AppUpdate::default() };
            let outcome = match field {
                FieldKey::Rotation => rotation(s, &bounds, text),
                FieldKey::X | FieldKey::Y => position(s, &bounds, field, text, units),
                FieldKey::Width | FieldKey::Height => size(s, &bounds, field, text, units),
                FieldKey::ScaleX | FieldKey::ScaleY => percent(s, &bounds, field, text),
                FieldKey::ShearX | FieldKey::ShearY => shear(s, &bounds, field, text),
                FieldKey::Priority => Outcome::Unchanged, // handled before the match
            };
            match outcome {
                Outcome::Invalid(e) => {
                    s.field_errors.insert(field, MessageKey::InvalidLength(e));
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                // An unchanged value sends no command (phase-B decision for field commits).
                Outcome::Unchanged => {
                    s.field_errors.remove(&field);
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Outcome::Rejected => error(Some(document), MessageKey::CommandRejected),
                Outcome::Send(payload) => {
                    s.field_errors.remove(&field);
                    match request {
                        Some(request) => send(s, document, *payload, request),
                        None => error(Some(document), MessageKey::CommandRejected),
                    }
                }
            }
        })
    }

    pub(super) fn commit_tool_size(&mut self, document: DocumentId, width: &str, height: &str) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        let units = self.app.prefs.units;
        self.with_session(document, |s| {
            if !s.editor.is_gesture_active() {
                return AppUpdate::default();
            }
            let w = parse_length_in_unit(width, units);
            let h = if height.trim().is_empty() { w } else { parse_length_in_unit(height, units) };
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

/// What a field commit turned into.
enum Outcome {
    Invalid(crate::field::FieldError),
    Unchanged,
    Rejected,
    Send(Box<CommandPayload>),
}

fn send(s: &mut Session, document: DocumentId, payload: CommandPayload, request: RequestId) -> AppUpdate {
    if super::arrange::touches_locked(s, &payload) {
        return error(Some(document), MessageKey::ObjectsLocked);
    }
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    let result = apply_command(&s.fonts, &mut s.doc, cmd);
    after_change(s, document, result)
}

fn transform(s: &Session, matrix: Result<Affine, burnburn_geometry::GeometryError>) -> Outcome {
    match matrix {
        Ok(transform) => {
            Outcome::Send(Box::new(CommandPayload::Transform { targets: s.selection.ids().to_vec(), transform }))
        }
        Err(_) => Outcome::Rejected,
    }
}

fn position(s: &Session, bounds: &Bounds, field: FieldKey, text: &str, units: LengthUnit) -> Outcome {
    let value = match parse_position_in_unit(text, units) {
        Ok(v) => v.mm(),
        Err(e) => return Outcome::Invalid(e),
    };
    let at = reference_in(bounds, s.arrange.reference);
    let d = if field == FieldKey::X { Vector::new(value - at.x, 0.0) } else { Vector::new(0.0, value - at.y) };
    if d.x == 0.0 && d.y == 0.0 {
        return Outcome::Unchanged;
    }
    transform(s, Affine::translate(d))
}

fn size(s: &Session, bounds: &Bounds, field: FieldKey, text: &str, units: LengthUnit) -> Outcome {
    let value = match parse_length_in_unit(text, units) {
        Ok(v) => v,
        Err(e) => return Outcome::Invalid(e),
    };
    let width = field == FieldKey::Width;
    let current = if width { bounds.width() } else { bounds.height() };
    if let Some(target) = s.selection.single()
        && !s.arrange.aspect_locked
    {
        // One object, one axis: the document's own resize (and its undo label).
        let reference = s.arrange.reference;
        let anchor = match if width { reference.column() } else { 2 - reference.row() } {
            0 => Anchor::Min,
            1 => Anchor::Center,
            _ => Anchor::Max,
        };
        let axis = if width { Axis::Width } else { Axis::Height };
        return Outcome::Send(Box::new(CommandPayload::SetDimension { target, axis, value, anchor }));
    }
    if !(current > 0.0) {
        return Outcome::Rejected;
    }
    let factor = value.mm() / current;
    if factor == 1.0 {
        return Outcome::Unchanged;
    }
    let (sx, sy) = match (s.arrange.aspect_locked, width) {
        (true, _) => (factor, factor),
        (false, true) => (factor, 1.0),
        (false, false) => (1.0, factor),
    };
    transform(s, Affine::scale_about(reference_in(bounds, s.arrange.reference), sx, sy))
}

fn percent(s: &Session, bounds: &Bounds, field: FieldKey, text: &str) -> Outcome {
    let factor = match parse_percent(text) {
        Ok(p) => p / 100.0,
        Err(e) => return Outcome::Invalid(e),
    };
    if factor == 1.0 {
        return Outcome::Unchanged;
    }
    let (sx, sy) = match (s.arrange.aspect_locked, field == FieldKey::ScaleX) {
        (true, _) => (factor, factor),
        (false, true) => (factor, 1.0),
        (false, false) => (1.0, factor),
    };
    transform(s, Affine::scale_about(reference_in(bounds, s.arrange.reference), sx, sy))
}

fn shear(s: &Session, bounds: &Bounds, field: FieldKey, text: &str) -> Outcome {
    let degrees = match parse_shear(text) {
        Ok(d) => d,
        Err(e) => return Outcome::Invalid(e),
    };
    if degrees == 0.0 {
        return Outcome::Unchanged;
    }
    let t = degrees.to_radians().tan();
    let at: Point = reference_in(bounds, s.arrange.reference);
    // x' = x + t·(y − y₀)   or   y' = y + t·(x − x₀): the reference point stays where it is.
    let matrix = if field == FieldKey::ShearX {
        Affine::new([1.0, 0.0, t, 1.0, -t * at.y, 0.0])
    } else {
        Affine::new([1.0, t, 0.0, 1.0, 0.0, -t * at.x])
    };
    transform(s, matrix)
}

/// Rotation field: turns the selection about the centre of its bounds by the typed angle.
fn rotation(s: &Session, bounds: &Bounds, text: &str) -> Outcome {
    let degrees = match parse_angle(text) {
        Ok(d) => d,
        Err(e) => return Outcome::Invalid(e),
    };
    if degrees.rem_euclid(360.0) == 0.0 {
        return Outcome::Unchanged;
    }
    let centre = bounds.center();
    let Ok(angle) = Angle::from_degrees(degrees) else { return Outcome::Rejected };
    let to_origin = Affine::translate(Vector::new(-centre.x, -centre.y));
    let back = Affine::translate(Vector::new(centre.x, centre.y));
    let (Ok(to_origin), Ok(back)) = (to_origin, back) else { return Outcome::Rejected };
    transform(s, Ok(to_origin.then(&Affine::rotate(angle)).then(&back)))
}
