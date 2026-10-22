//! Arrange ▸ Array (AR02, G5-cam): grid, circular and along-a-path arrays with a live preview.
//! The dialog is a draft in the window; the document changes only on Apply, as one modifier
//! chain (one undo step). The preview is drawn on the canvas as ghosts of the source. The dialog
//! works on the one selected object and follows the selection while it is open.

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_document::structure::{EvalMode, Modifier, ModifierChain, ModifierStep, StructureCommand};
use burnburn_document::{CommandPayload, DocumentSnapshot};
use burnburn_editor::Overlay;
use burnburn_geometry::array::{
    CircularArray, GridArray, MAX_COPIES, PathArray, circular_array, grid_array, path_array,
};
use burnburn_geometry::{Affine, Bounds, Point};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId};

use crate::cam_dialogs::{
    CamDialog, DialogAction, DialogActionKind, DialogActionState, DialogChoice, DialogError, DialogField, DialogLabel,
    DialogNote, DialogRow, DialogValue, DialogViewModel,
};
use crate::cut_options::FieldState;
use crate::field::{format_mm, parse_number, parse_position};

use super::{AppUpdate, HostEffect, Session, apply_edit};

/// More ghosts than this are not drawn (the array itself has no such limit).
const MAX_GHOSTS: usize = 2000;
/// Up to this many copies the extent is computed from the real outlines, beyond it from the
/// corners of the source's box.
const EXACT_EXTENT_COPIES: usize = 2000;
/// Gap between copies the starting pitch leaves, mm.
const DEFAULT_GAP_MM: f64 = 5.0;
/// How far right of the object the circular array's centre starts, mm.
const DEFAULT_CENTRE_OFFSET_MM: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Grid,
    Circular,
    Path,
}

impl Kind {
    fn choice(self) -> DialogChoice {
        match self {
            Kind::Grid => DialogChoice::ArrayGrid,
            Kind::Circular => DialogChoice::ArrayCircular,
            Kind::Path => DialogChoice::ArrayPath,
        }
    }
}

/// The draft of the dialog while it is open: what was typed, never a parsed value.
#[derive(Debug, Clone)]
pub(super) struct ArrayDraft {
    kind: Kind,
    texts: BTreeMap<DialogField, String>,
    circular_rotate: bool,
    path_orient: bool,
    along: Option<GeometryId>,
}

impl ArrayDraft {
    fn new(source: Option<Bounds>) -> Self {
        let (w, h) = source.map_or((10.0, 10.0), |b| (b.width(), b.height()));
        let centre =
            source.map_or(Point::new(0.0, 0.0), |b| Point::new(b.max().x + DEFAULT_CENTRE_OFFSET_MM, b.center().y));
        let texts = [
            (DialogField::GridColumns, "3".to_owned()),
            (DialogField::GridRows, "3".to_owned()),
            (DialogField::GridPitchX, format_mm(w + DEFAULT_GAP_MM)),
            (DialogField::GridPitchY, format_mm(h + DEFAULT_GAP_MM)),
            (DialogField::CircularCount, "6".to_owned()),
            (DialogField::CircularCentreX, format_mm(centre.x)),
            (DialogField::CircularCentreY, format_mm(centre.y)),
            (DialogField::CircularSweep, "360".to_owned()),
            (DialogField::PathCount, "5".to_owned()),
        ]
        .into_iter()
        .collect();
        ArrayDraft { kind: Kind::Grid, texts, circular_rotate: true, path_orient: true, along: None }
    }

    fn text(&self, field: DialogField) -> &str {
        self.texts.get(&field).map_or("", String::as_str)
    }
}

/// What the draft makes, when it is valid.
struct Made {
    source: GeometryId,
    modifier: Modifier,
    transforms: Vec<Affine>,
}

/// The one selected object, when exactly one is selected and it has an outline.
fn source_of(s: &Session, snapshot: &DocumentSnapshot) -> Option<(GeometryId, Bounds)> {
    match s.selection.ids() {
        [one] => snapshot.object(*one).and_then(|o| o.bounds()).map(|b| (*one, b)),
        _ => None,
    }
}

/// The objects an array could follow, with their names: every other object with an outline.
fn candidates(snapshot: &DocumentSnapshot, source: Option<GeometryId>) -> Vec<(GeometryId, String)> {
    snapshot
        .objects()
        .filter(|(id, o)| Some(*id) != source && !o.path().is_empty())
        .map(|(id, o)| (id, o.name().to_owned()))
        .collect()
}

/// The object to follow: the chosen one if it is still a candidate, else the first.
fn effective_along(draft: &ArrayDraft, list: &[(GeometryId, String)]) -> Option<GeometryId> {
    draft.along.filter(|g| list.iter().any(|(id, _)| id == g)).or_else(|| list.first().map(|(id, _)| *id))
}

fn count(text: &str) -> Result<u32, DialogError> {
    let v = parse_number(text, &[]).map_err(|_| DialogError::Invalid)?;
    if !v.is_finite() {
        return Err(DialogError::Invalid);
    }
    if v.fract() != 0.0 {
        return Err(DialogError::NotWhole);
    }
    if !(1.0..=MAX_COPIES as f64).contains(&v) {
        return Err(DialogError::TooManyCopies);
    }
    Ok(v as u32)
}

fn length(text: &str) -> Result<Length, DialogError> {
    parse_position(text).map_err(|_| DialogError::Invalid)
}

/// `Some(v)` for a value; an error is collected against its field.
fn take<T>(errors: &mut Vec<(DialogField, DialogError)>, field: DialogField, r: Result<T, DialogError>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push((field, e));
            None
        }
    }
}

/// Parses the draft against the document. All problems of the visible fields, or what it makes.
fn make(
    draft: &ArrayDraft,
    source: GeometryId,
    bounds: Bounds,
    along: Option<GeometryId>,
    snapshot: &DocumentSnapshot,
) -> Result<Made, Vec<(DialogField, DialogError)>> {
    let mut errors: Vec<(DialogField, DialogError)> = Vec::new();
    let tolerance = crate::commit::boolean_options().map(|o| o.tolerance);
    let (modifier, transforms) = match draft.kind {
        Kind::Grid => {
            let columns = take(&mut errors, DialogField::GridColumns, count(draft.text(DialogField::GridColumns)));
            let rows = take(&mut errors, DialogField::GridRows, count(draft.text(DialogField::GridRows)));
            let pitch_x = take(&mut errors, DialogField::GridPitchX, length(draft.text(DialogField::GridPitchX)));
            let pitch_y = take(&mut errors, DialogField::GridPitchY, length(draft.text(DialogField::GridPitchY)));
            match (columns, rows, pitch_x, pitch_y) {
                (Some(columns), Some(rows), Some(pitch_x), Some(pitch_y)) => {
                    let grid = GridArray { columns, rows, pitch_x, pitch_y };
                    match grid_array(&grid) {
                        Ok(t) => (Some(Modifier::GridArray(grid)), t),
                        Err(_) => {
                            errors.push((DialogField::GridColumns, DialogError::TooManyCopies));
                            errors.push((DialogField::GridRows, DialogError::TooManyCopies));
                            (None, Vec::new())
                        }
                    }
                }
                _ => (None, Vec::new()),
            }
        }
        Kind::Circular => {
            let n = take(&mut errors, DialogField::CircularCount, count(draft.text(DialogField::CircularCount)));
            let x = take(&mut errors, DialogField::CircularCentreX, length(draft.text(DialogField::CircularCentreX)));
            let y = take(&mut errors, DialogField::CircularCentreY, length(draft.text(DialogField::CircularCentreY)));
            let sweep = take(
                &mut errors,
                DialogField::CircularSweep,
                parse_number(draft.text(DialogField::CircularSweep), &["°", "deg"])
                    .map_err(|_| DialogError::Invalid)
                    .and_then(|v| {
                        if v.is_finite() && v != 0.0 && v.abs() <= 360.0 { Ok(v) } else { Err(DialogError::OutOfRange) }
                    }),
            );
            match (n, x, y, sweep) {
                (Some(count), Some(x), Some(y), Some(sweep_deg)) => {
                    let circular = CircularArray {
                        count,
                        center: Point::new(x.mm(), y.mm()),
                        sweep_deg,
                        rotate_items: draft.circular_rotate,
                        reference: bounds.center(),
                    };
                    match circular_array(&circular) {
                        Ok(t) => (Some(Modifier::CircularArray(circular)), t),
                        Err(_) => (None, Vec::new()),
                    }
                }
                _ => (None, Vec::new()),
            }
        }
        Kind::Path => {
            let n = take(&mut errors, DialogField::PathCount, count(draft.text(DialogField::PathCount)));
            let carrier = take(&mut errors, DialogField::PathAlong, along.ok_or(DialogError::NoPath));
            match (n, carrier, tolerance) {
                (Some(count), Some(along), Some(tolerance)) => {
                    let spec = PathArray { count, orient: draft.path_orient };
                    let path = snapshot.object(along).map(|o| Arc::clone(o.path()));
                    match path.map(|p| path_array(&p, &spec, &tolerance)) {
                        Some(Ok(t)) => (Some(Modifier::PathArray { count, along, orient: draft.path_orient }), t),
                        _ => {
                            errors.push((DialogField::PathAlong, DialogError::NoPath));
                            (None, Vec::new())
                        }
                    }
                }
                _ => (None, Vec::new()),
            }
        }
    };
    match modifier {
        Some(modifier) if errors.is_empty() => Ok(Made { source, modifier, transforms }),
        _ => Err(errors),
    }
}

/// The union of the copies' boxes in whole micrometres (copies include the original).
fn extent(
    snapshot: &DocumentSnapshot,
    source: GeometryId,
    bounds: Bounds,
    transforms: &[Affine],
) -> Option<(u64, u64)> {
    let mut union: Option<Bounds> = None;
    let mut add = |b: Bounds| union = Some(union.map_or(b, |u| u.union(b)));
    if transforms.len() <= EXACT_EXTENT_COPIES {
        let path = snapshot.object(source)?.path();
        for t in transforms {
            add(path.transform(t).bounds()?);
        }
    } else {
        let corners = [
            bounds.min(),
            Point::new(bounds.max().x, bounds.min().y),
            bounds.max(),
            Point::new(bounds.min().x, bounds.max().y),
        ];
        for t in transforms {
            for c in corners {
                add(Bounds::from_point(t.apply(c)));
            }
        }
    }
    let u = union?;
    let um = |mm: f64| (mm * 1000.0).round().max(0.0) as u64;
    Some((um(u.width()), um(u.height())))
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn view(s: &Session) -> Option<DialogViewModel> {
    let draft = s.cam.array.as_ref()?;
    let snapshot = s.doc.snapshot();
    let source = source_of(s, &snapshot);
    let list = candidates(&snapshot, source.map(|(id, _)| id));
    let along = effective_along(draft, &list);
    let made = source.map(|(id, b)| make(draft, id, b, along, &snapshot));
    let errors: BTreeMap<DialogField, DialogError> = match &made {
        Some(Err(e)) => e.iter().copied().collect(),
        _ => BTreeMap::new(),
    };
    let visible = |field: DialogField| match field {
        DialogField::ArrayKind => true,
        DialogField::GridColumns | DialogField::GridRows | DialogField::GridPitchX | DialogField::GridPitchY => {
            draft.kind == Kind::Grid
        }
        DialogField::CircularCount
        | DialogField::CircularCentreX
        | DialogField::CircularCentreY
        | DialogField::CircularSweep
        | DialogField::CircularRotate => draft.kind == Kind::Circular,
        _ => draft.kind == Kind::Path,
    };
    let row = |field: DialogField, value: DialogValue| DialogRow {
        field,
        value,
        state: if visible(field) { FieldState::Editable } else { FieldState::Hidden },
        error: if visible(field) { errors.get(&field).copied() } else { None },
        blocked_choices: Vec::new(),
        reorder: None,
        labels: Vec::new(),
    };
    let text = |field: DialogField| row(field, DialogValue::Text(draft.text(field).to_owned()));
    let kind_index = DialogField::ArrayKind.choices().iter().position(|c| *c == draft.kind.choice()).unwrap_or(0);
    let along_index = along.and_then(|g| list.iter().position(|(id, _)| *id == g)).unwrap_or(0);
    let mut rows = vec![
        row(DialogField::ArrayKind, DialogValue::Choice(kind_index)),
        text(DialogField::GridColumns),
        text(DialogField::GridRows),
        text(DialogField::GridPitchX),
        text(DialogField::GridPitchY),
        text(DialogField::CircularCount),
        text(DialogField::CircularCentreX),
        text(DialogField::CircularCentreY),
        text(DialogField::CircularSweep),
        row(DialogField::CircularRotate, DialogValue::Flag(draft.circular_rotate)),
        text(DialogField::PathCount),
    ];
    let mut path_row = row(DialogField::PathAlong, DialogValue::Choice(along_index));
    path_row.labels = list.iter().map(|(_, name)| DialogLabel::plain(name.clone())).collect();
    if visible(DialogField::PathAlong) && list.is_empty() {
        path_row.error = Some(DialogError::NoPath);
        path_row.state = FieldState::Disabled(None);
    }
    rows.push(path_row);
    rows.push(row(DialogField::PathOrient, DialogValue::Flag(draft.path_orient)));

    let mut notes = Vec::new();
    let mut can_apply = false;
    match (&source, &made) {
        (None, _) => notes.push(DialogNote::ArrayNeedsOneObject),
        (Some((id, bounds)), Some(Ok(m))) => {
            can_apply = true;
            if let Some((width_um, height_um)) = extent(&snapshot, *id, *bounds, &m.transforms) {
                notes.push(DialogNote::ArrayExtent { copies: m.transforms.len() as u32, width_um, height_um });
            }
        }
        _ => {}
    }
    Some(DialogViewModel {
        dialog: CamDialog::Array,
        rows,
        notes,
        actions: vec![
            DialogActionState { action: DialogActionKind::Apply, enabled: can_apply, reason: None },
            DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
        ],
        capabilities: Vec::new(),
        result: None,
        busy: false,
        lines: Vec::new(),
    })
}

pub(super) fn open(s: &mut Session) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let bounds = source_of(s, &snapshot).map(|(_, b)| b);
    s.cam.array = Some(ArrayDraft::new(bounds));
    changed()
}

pub(super) fn set(s: &mut Session, field: DialogField, value: DialogValue) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let source = source_of(s, &snapshot).map(|(id, _)| id);
    let list = candidates(&snapshot, source);
    let Some(draft) = s.cam.array.as_mut() else { return AppUpdate::default() };
    match (field, value) {
        (DialogField::ArrayKind, DialogValue::Choice(i)) => {
            draft.kind = match DialogField::ArrayKind.choices().get(i) {
                Some(DialogChoice::ArrayCircular) => Kind::Circular,
                Some(DialogChoice::ArrayPath) => Kind::Path,
                _ => Kind::Grid,
            };
        }
        (DialogField::CircularRotate, DialogValue::Flag(on)) => draft.circular_rotate = on,
        (DialogField::PathOrient, DialogValue::Flag(on)) => draft.path_orient = on,
        (DialogField::PathAlong, DialogValue::Choice(i)) => draft.along = list.get(i).map(|(id, _)| *id),
        (
            DialogField::GridColumns
            | DialogField::GridRows
            | DialogField::GridPitchX
            | DialogField::GridPitchY
            | DialogField::CircularCount
            | DialogField::CircularCentreX
            | DialogField::CircularCentreY
            | DialogField::CircularSweep
            | DialogField::PathCount,
            DialogValue::Text(t),
        ) => {
            draft.texts.insert(field, t);
        }
        _ => return AppUpdate::default(),
    }
    AppUpdate::redraw(s.doc.id())
}

pub(super) fn action(
    s: &mut Session,
    document: DocumentId,
    action: DialogAction,
    request: Option<RequestId>,
) -> AppUpdate {
    match action {
        DialogAction::Close => {
            s.cam.array = None;
            AppUpdate::redraw(document)
        }
        DialogAction::Apply => apply(s, document, request),
        _ => AppUpdate::default(),
    }
}

fn apply(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(draft) = s.cam.array.as_ref() else { return AppUpdate::default() };
    let Some((source, bounds)) = source_of(s, &snapshot) else { return changed() };
    let list = candidates(&snapshot, Some(source));
    let along = effective_along(draft, &list);
    let Ok(made) = make(draft, source, bounds, along, &snapshot) else { return changed() };
    let Some(tolerance) = crate::commit::boolean_options().map(|o| o.tolerance) else { return changed() };
    let chain = ModifierChain {
        name: "Reeks".to_owned(),
        source: made.source,
        steps: vec![ModifierStep { modifier: made.modifier, enabled: true }],
        tolerance,
        mode: EvalMode::Inline,
    };
    let update =
        apply_edit(s, document, CommandPayload::Structure(StructureCommand::CreateModifier { chain }), request);
    if update.effects.iter().all(|e| !matches!(e, HostEffect::ShowError { .. })) {
        s.cam.array = None;
    }
    update
}

/// The ghosts of the copies while the dialog is open and valid: every copy but the original.
pub(super) fn scene_overlays(s: &Session) -> Vec<Overlay> {
    let Some(draft) = s.cam.array.as_ref() else { return Vec::new() };
    let snapshot = s.doc.snapshot();
    let Some((source, bounds)) = source_of(s, &snapshot) else { return Vec::new() };
    let list = candidates(&snapshot, Some(source));
    let along = effective_along(draft, &list);
    let Ok(made) = make(draft, source, bounds, along, &snapshot) else { return Vec::new() };
    let Some(object) = snapshot.object(source) else { return Vec::new() };
    let path = Arc::clone(object.path());
    made.transforms
        .iter()
        .skip(1)
        .take(MAX_GHOSTS)
        .map(|t| Overlay::DragPreview { path: Arc::clone(&path), transform: *t })
        .collect()
}
