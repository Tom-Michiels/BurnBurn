//! Arrange (W2-L1-bewerken): draw order, groups, flips, lock/hide, selection commands and the
//! inspector's transform state.

use std::collections::BTreeSet;

use burnburn_document::{
    ArrangeCommand, ArrangeError, CommandError, CommandPayload, DocumentCommand, DocumentSnapshot, GeometryCommand,
    GeometrySource, ObjectKind, OrderChange,
};
use burnburn_editor::SelectionChange;
use burnburn_geometry::{Affine, Bounds, Point, Vector};
use burnburn_types::{Angle, DocumentId, GeometryId, RequestId};

use crate::intent::{CommandKey, RefPoint};
use crate::view_model::{MessageKey, StatusMessage};

use super::{AppUpdate, Session, after_change, apply_command, apply_edit, apply_selection, error};

/// Two shapes count as the same drawing when every segment end agrees within this (mm).
const DUPLICATE_TOLERANCE_MM: f64 = 1e-3;

/// Window state of the transform inspector and the view.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ArrangeState {
    pub reference: RefPoint,
    pub aspect_locked: bool,
    pub wireframe: bool,
}

pub(super) fn set_reference_point(s: &mut Session, point: RefPoint) -> AppUpdate {
    s.arrange.reference = point;
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn set_aspect_lock(s: &mut Session, locked: bool) -> AppUpdate {
    s.arrange.aspect_locked = locked;
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

/// The point of `bounds` a reference point stands for (document coordinates, y up).
#[allow(dead_code)]
pub(super) fn reference_in(bounds: &Bounds, point: RefPoint) -> Point {
    let x = bounds.min().x + bounds.width() * f64::from(point.column()) / 2.0;
    let y = bounds.max().y - bounds.height() * f64::from(point.row()) / 2.0;
    Point::new(x, y)
}

/// An arrangement command as one document command. A command that would change nothing (an
/// object already at the front, say) is silent: no history entry and no error.
fn arrange_edit(
    s: &mut Session,
    document: DocumentId,
    command: ArrangeCommand,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let cmd =
        DocumentCommand { document, expected: s.doc.revision(), request, payload: CommandPayload::Arrange(command) };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Err(CommandError::Arrange(ArrangeError::NothingChanged)) => AppUpdate::default(),
        result => after_change(s, document, result),
    }
}

fn selectable_ids(snapshot: &DocumentSnapshot) -> Vec<GeometryId> {
    snapshot.draw_order().into_iter().filter(|id| snapshot.is_selectable(*id)).collect()
}

fn area_of(snapshot: &DocumentSnapshot, id: GeometryId) -> Option<f64> {
    snapshot.object(id)?.bounds().map(|b| b.width() * b.height())
}

/// Replaces the selection by `ids`, or says that nothing matched (the selection stays then).
fn select_found(s: &mut Session, document: DocumentId, ids: Vec<GeometryId>) -> AppUpdate {
    if ids.is_empty() {
        s.messages.push(StatusMessage { key: MessageKey::NothingFound });
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    }
    s.messages.retain(|m| m.key != MessageKey::NothingFound);
    apply_selection(s, &SelectionChange::Replace(ids));
    s.cycle = None;
    AppUpdate::redraw(document)
}

fn transform_about_centre(
    s: &mut Session,
    document: DocumentId,
    request: Option<RequestId>,
    make: impl FnOnce(Point) -> Option<Affine>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(bounds) = s.selection.bounds(&snapshot) else { return AppUpdate::default() };
    let Some(transform) = make(bounds.center()) else { return error(Some(document), MessageKey::CommandRejected) };
    let targets = s.selection.ids().to_vec();
    apply_edit(s, document, CommandPayload::Transform { targets, transform }, request)
}

/// Runs the Edit/Arrange commands of this module; `None` when `command` is not one of them.
pub(super) fn command(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    request: Option<RequestId>,
) -> Option<AppUpdate> {
    let targets = s.selection.ids().to_vec();
    let order = |s: &mut Session, change| {
        if targets.is_empty() {
            return AppUpdate::default();
        }
        arrange_edit(s, document, ArrangeCommand::Order { targets: targets.clone(), change }, request)
    };
    Some(match command {
        CommandKey::PushForward => order(s, OrderChange::Forward),
        CommandKey::PushBackward => order(s, OrderChange::Backward),
        CommandKey::PushToFront => order(s, OrderChange::ToFront),
        CommandKey::PushToBack => order(s, OrderChange::ToBack),
        CommandKey::Group if targets.len() >= 2 => {
            apply_edit(s, document, CommandPayload::Edit(GeometryCommand::Group { targets }), request)
        }
        CommandKey::Ungroup if !targets.is_empty() => {
            arrange_edit(s, document, ArrangeCommand::Ungroup { targets }, request)
        }
        CommandKey::AutoGroup => {
            let targets = if targets.len() >= 2 { targets } else { selectable_ids(&s.doc.snapshot()) };
            arrange_edit(s, document, ArrangeCommand::AutoGroup { targets }, request)
        }
        CommandKey::BreakApart if !targets.is_empty() => {
            arrange_edit(s, document, ArrangeCommand::BreakApart { targets }, request)
        }
        CommandKey::Group | CommandKey::Ungroup | CommandKey::BreakApart => AppUpdate::default(),
        CommandKey::FlipHorizontal => {
            transform_about_centre(s, document, request, |c| Affine::scale_about(c, -1.0, 1.0).ok())
        }
        CommandKey::FlipVertical => {
            transform_about_centre(s, document, request, |c| Affine::scale_about(c, 1.0, -1.0).ok())
        }
        CommandKey::Rotate90Cw => transform_about_centre(s, document, request, |c| quarter_turn(c, -90.0)),
        CommandKey::Rotate90Ccw => transform_about_centre(s, document, request, |c| quarter_turn(c, 90.0)),
        CommandKey::LockSelected if !targets.is_empty() => {
            arrange_edit(s, document, ArrangeCommand::SetLocked { targets, locked: true }, request)
        }
        CommandKey::UnlockSelected => {
            let snapshot = s.doc.snapshot();
            let targets: Vec<_> = targets.into_iter().filter(|id| snapshot.is_locked(*id)).collect();
            if targets.is_empty() {
                return Some(AppUpdate::default());
            }
            arrange_edit(s, document, ArrangeCommand::SetLocked { targets, locked: false }, request)
        }
        CommandKey::UnlockAll => {
            let targets: Vec<_> = s.doc.snapshot().arrange().locked().collect();
            if targets.is_empty() {
                return Some(AppUpdate::default());
            }
            arrange_edit(s, document, ArrangeCommand::SetLocked { targets, locked: false }, request)
        }
        CommandKey::HideSelected if !targets.is_empty() => {
            let update = arrange_edit(s, document, ArrangeCommand::SetHidden { targets, hidden: true }, request);
            // Hidden objects cannot stay selected: nothing would show what a command acts on.
            let snapshot = s.doc.snapshot();
            let keep: Vec<_> = s.selection.ids().iter().copied().filter(|id| !snapshot.is_hidden(*id)).collect();
            apply_selection(s, &SelectionChange::Replace(keep));
            update
        }
        CommandKey::ShowAll => {
            let targets: Vec<_> = s.doc.snapshot().arrange().hidden().collect();
            if targets.is_empty() {
                return Some(AppUpdate::default());
            }
            arrange_edit(s, document, ArrangeCommand::SetHidden { targets, hidden: false }, request)
        }
        CommandKey::LockSelected | CommandKey::HideSelected => AppUpdate::default(),
        CommandKey::DeleteDuplicates => delete_duplicates(s, document, request),
        CommandKey::ViewWireframe => {
            s.arrange.wireframe = !s.arrange.wireframe;
            AppUpdate::redraw(document)
        }
        CommandKey::ZoomIn => zoom_by(s, document, ZOOM_STEP),
        CommandKey::ZoomOut => zoom_by(s, document, 1.0 / ZOOM_STEP),
        CommandKey::ZoomSelection => match s.selection.bounds(&s.doc.snapshot()) {
            Some(bounds) => fit_to(s, document, bounds),
            None => AppUpdate::default(),
        },
        CommandKey::InvertSelection => {
            let ids = selectable_ids(&s.doc.snapshot()).into_iter().filter(|id| !s.selection.contains(*id)).collect();
            select_found(s, document, ids)
        }
        CommandKey::SelectOpenShapes => {
            let snapshot = s.doc.snapshot();
            let ids = selectable_ids(&snapshot)
                .into_iter()
                .filter(|id| snapshot.object(*id).is_some_and(|o| o.path().subpaths().iter().any(|sp| !sp.is_closed())))
                .collect();
            select_found(s, document, ids)
        }
        CommandKey::SelectCircles => {
            let snapshot = s.doc.snapshot();
            let ids = selectable_ids(&snapshot)
                .into_iter()
                .filter(|id| snapshot.object(*id).is_some_and(|o| o.kind() == ObjectKind::Circle))
                .collect();
            select_found(s, document, ids)
        }
        CommandKey::SelectContained => {
            let snapshot = s.doc.snapshot();
            let ids = selectable_ids(&snapshot)
                .into_iter()
                .filter(|id| !s.selection.contains(*id))
                .filter(|id| {
                    let Some(inner) = snapshot.object(*id) else { return false };
                    targets
                        .iter()
                        .filter_map(|t| snapshot.object(*t))
                        .any(|outer| burnburn_geometry::encloses(outer.path(), inner.path(), ENCLOSE_TOLERANCE_MM))
                })
                .collect();
            select_found(s, document, ids)
        }
        CommandKey::SelectSmaller => {
            let snapshot = s.doc.snapshot();
            let Some(limit) = s.selection.reference().and_then(|r| area_of(&snapshot, r)) else {
                return Some(AppUpdate::default());
            };
            let ids = selectable_ids(&snapshot)
                .into_iter()
                .filter(|id| !s.selection.contains(*id) && area_of(&snapshot, *id).is_some_and(|a| a < limit))
                .collect();
            select_found(s, document, ids)
        }
        CommandKey::SelectInLayer => {
            let snapshot = s.doc.snapshot();
            let layers: BTreeSet<_> = targets.iter().flat_map(|t| snapshot.process().operations_using(*t)).collect();
            let ids = selectable_ids(&snapshot)
                .into_iter()
                .filter(|id| snapshot.process().operations_using(*id).iter().any(|o| layers.contains(o)))
                .collect();
            select_found(s, document, ids)
        }
        _ => return None,
    })
}

/// One Zoom In / Zoom Out step.
const ZOOM_STEP: f64 = 1.25;
/// Free space around what Zoom to Page / Selection frames, as a fraction of its size.
const FIT_MARGIN: f64 = 0.1;

fn zoom_by(s: &mut Session, document: DocumentId, factor: f64) -> AppUpdate {
    let anchor = [s.viewport.width_pt() / 2.0, s.viewport.height_pt() / 2.0];
    match s.camera.zoom_at(&s.viewport, anchor, factor) {
        Ok(()) => AppUpdate::redraw(document),
        Err(_) => AppUpdate::default(),
    }
}

/// Centres the view on `bounds` at the largest zoom that shows all of it with a margin.
pub(super) fn fit_to(s: &mut Session, document: DocumentId, bounds: Bounds) -> AppUpdate {
    // A point or a line still gets a usable view.
    let (w, h) = (bounds.width().max(1.0), bounds.height().max(1.0));
    let scale = ((s.viewport.width_pt() / (w * (1.0 + FIT_MARGIN)))
        .min(s.viewport.height_pt() / (h * (1.0 + FIT_MARGIN))))
    .max(f64::MIN_POSITIVE);
    match burnburn_editor::Camera::new(bounds.center(), scale) {
        Ok(camera) => {
            s.camera = camera;
            AppUpdate::redraw(document)
        }
        Err(_) => AppUpdate::default(),
    }
}

/// Zoom to Page: the machine bed when a profile is known, else everything that is drawn.
pub(super) fn zoom_page(s: &mut Session, document: DocumentId, bed: Option<Bounds>) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let content = snapshot.objects().filter_map(|(_, o)| o.bounds()).reduce(Bounds::union);
    match bed.or(content) {
        Some(bounds) => fit_to(s, document, bounds),
        None => AppUpdate::default(),
    }
}

/// Distance an inner shape must keep from the outline of the shape around it (mm).
const ENCLOSE_TOLERANCE_MM: f64 = 1e-6;

fn quarter_turn(centre: Point, degrees: f64) -> Option<Affine> {
    let angle = Angle::from_degrees(degrees).ok()?;
    let to_origin = Affine::translate(Vector::new(-centre.x, -centre.y)).ok()?;
    let back = Affine::translate(Vector::new(centre.x, centre.y)).ok()?;
    Some(to_origin.then(&Affine::rotate(angle)).then(&back))
}

/// Whether `command` can do something for the selection (menu and toolbar enablement).
pub(super) fn command_enabled(command: CommandKey, s: &Session) -> Option<bool> {
    let snapshot = s.doc.snapshot();
    let selected = s.selection.ids();
    let any = |f: &dyn Fn(GeometryId) -> bool| selected.iter().any(|id| f(*id));
    Some(match command {
        CommandKey::Cut
        | CommandKey::Copy
        | CommandKey::Duplicate
        | CommandKey::PushForward
        | CommandKey::PushBackward
        | CommandKey::PushToFront
        | CommandKey::PushToBack
        | CommandKey::FlipHorizontal
        | CommandKey::FlipVertical
        | CommandKey::Rotate90Cw
        | CommandKey::Rotate90Ccw
        | CommandKey::LockSelected
        | CommandKey::HideSelected
        | CommandKey::ZoomSelection
        | CommandKey::SelectContained
        | CommandKey::SelectSmaller
        | CommandKey::SelectInLayer => !selected.is_empty(),
        CommandKey::Paste
        | CommandKey::PasteInPlace
        | CommandKey::ZoomPage
        | CommandKey::ZoomIn
        | CommandKey::ZoomOut => true,
        CommandKey::ViewWireframe => true,
        CommandKey::Group => selected.len() >= 2,
        CommandKey::Ungroup => any(&|id| snapshot.object(id).is_some_and(|o| !o.groups().is_empty())),
        CommandKey::AutoGroup => snapshot.len() >= 2,
        CommandKey::BreakApart => any(&|id| snapshot.object(id).is_some_and(|o| o.path().subpaths().len() > 1)),
        CommandKey::UnlockSelected => any(&|id| snapshot.is_locked(id)),
        CommandKey::UnlockAll => snapshot.arrange().locked().len() > 0,
        CommandKey::ShowAll => snapshot.arrange().hidden().len() > 0,
        CommandKey::DeleteDuplicates
        | CommandKey::InvertSelection
        | CommandKey::SelectOpenShapes
        | CommandKey::SelectCircles => !snapshot.is_empty(),
        _ => return None,
    })
}
/// Delete Duplicates: of identical stacked shapes only the first (lowest id) stays. Works on the
/// selection when it holds several objects, else on the whole drawing.
pub(super) fn delete_duplicates(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let scope: Vec<GeometryId> = if s.selection.len() >= 2 {
        let mut ids = s.selection.ids().to_vec();
        ids.sort();
        ids
    } else {
        snapshot.objects().map(|(id, _)| id).collect()
    };
    let mut kept: Vec<GeometryId> = Vec::new();
    let mut doomed: Vec<GeometryId> = Vec::new();
    for id in scope {
        let Some(obj) = snapshot.object(id) else { continue };
        if matches!(obj.source(), GeometrySource::Derived(_)) {
            continue;
        }
        let same = kept.iter().any(|k| {
            snapshot
                .object(*k)
                .is_some_and(|other| burnburn_geometry::coincident(other.path(), obj.path(), DUPLICATE_TOLERANCE_MM))
        });
        if same { doomed.push(id) } else { kept.push(id) }
    }
    if doomed.is_empty() {
        s.messages.push(StatusMessage { key: MessageKey::NoDuplicatesFound });
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    }
    apply_edit(s, document, CommandPayload::Delete { targets: doomed }, request)
}

/// A canvas selection that picks one member of a group picks the whole outermost group (as far
/// as its members can be selected); a Shift-click toggles the whole group.
pub(super) fn whole_groups(s: &Session, change: SelectionChange) -> SelectionChange {
    let snapshot = s.doc.snapshot();
    let expand = |ids: &[GeometryId]| -> Vec<GeometryId> {
        let mut out: Vec<GeometryId> = Vec::new();
        for id in ids {
            let outer = snapshot.object(*id).and_then(|o| o.groups().first().copied());
            let members = match outer {
                Some(group) => snapshot
                    .draw_order()
                    .into_iter()
                    .filter(|m| {
                        snapshot.is_selectable(*m)
                            && snapshot.object(*m).is_some_and(|o| o.groups().first() == Some(&group))
                    })
                    .collect(),
                None => vec![*id],
            };
            for m in members {
                if !out.contains(&m) {
                    out.push(m);
                }
            }
        }
        out
    };
    match change {
        SelectionChange::Replace(ids) => SelectionChange::Replace(expand(&ids)),
        SelectionChange::Extend(ids) => SelectionChange::Extend(expand(&ids)),
        SelectionChange::Toggle(id) => {
            let members = expand(&[id]);
            if members.len() < 2 {
                SelectionChange::Toggle(id)
            } else if members.iter().all(|m| s.selection.contains(*m)) {
                SelectionChange::Replace(s.selection.ids().iter().copied().filter(|x| !members.contains(x)).collect())
            } else {
                SelectionChange::Extend(members)
            }
        }
        other => other,
    }
}

/// True when `payload` would move, resize, delete or regroup locked objects. A locked object can
/// still be selected from the object list (to unlock it), but nothing edits it meanwhile.
pub(super) fn touches_locked(s: &Session, payload: &CommandPayload) -> bool {
    let snapshot = s.doc.snapshot();
    let targets: &[GeometryId] = match payload {
        CommandPayload::Transform { targets, .. } | CommandPayload::Delete { targets } => targets,
        CommandPayload::SetDimension { target, .. } | CommandPayload::Rename { target, .. } => {
            std::slice::from_ref(target)
        }
        CommandPayload::Edit(
            GeometryCommand::Group { targets }
            | GeometryCommand::Align { targets, .. }
            | GeometryCommand::Distribute { targets, .. },
        ) => targets,
        _ => return false,
    };
    targets.iter().any(|id| snapshot.is_locked(*id))
}
