//! W4-UI-gaten, bewerken (E03–E05, E09): typefilter, isoleren, snapkandidaten, offsetvoorbeeld.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::{ObjectKind, UndoLabel};
use burnburn_presentation::{CommandKey, DocumentViewModel, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId};

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

/// Two rectangles and a circle, drawn with the tools: ids in drawing order.
fn three_shapes() -> (HeadlessEditorHarness, DocumentId, [GeometryId; 3]) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.drag(doc, (40.0, 0.0), (60.0, 10.0));
    h.invoke(doc, CommandKey::ToolCircle);
    h.drag(doc, (100.0, 50.0), (110.0, 50.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<GeometryId> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    assert_eq!(ids.len(), 3, "{:?}", vm(&h, doc).objects);
    (h, doc, [ids[0], ids[1], ids[2]])
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn selected(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    vm(h, doc).objects.iter().filter(|o| o.selected).map(|o| o.id).collect()
}

// ---------------------------------------------------------------------------------------------
// E03: typefilter en isoleren
// ---------------------------------------------------------------------------------------------

/// The type filter selects every visible, unlocked object of a kind.
#[test]
fn select_by_kind_picks_every_object_of_that_kind() {
    let (mut h, doc, [r1, r2, c]) = three_shapes();
    h.send(UiIntent::SelectByKind { document: doc, kind: ObjectKind::Rectangle, mode: SelectMode::Replace });
    assert_eq!(selected(&h, doc), [r1, r2]);
    h.send(UiIntent::SelectByKind { document: doc, kind: ObjectKind::Circle, mode: SelectMode::Toggle });
    assert_eq!(selected(&h, doc), [r1, r2, c], "toggle adds the circle");
    h.send(UiIntent::SelectByKind { document: doc, kind: ObjectKind::Circle, mode: SelectMode::Replace });
    assert_eq!(selected(&h, doc), [c]);
    h.send(UiIntent::SelectByKind { document: doc, kind: ObjectKind::Star, mode: SelectMode::Replace });
    assert!(selected(&h, doc).is_empty(), "no object of that kind: nothing selected");
}

/// Hidden and locked objects are never picked by the filter.
#[test]
fn the_type_filter_skips_hidden_and_locked_objects() {
    let (mut h, doc, [r1, r2, _]) = three_shapes();
    select(&mut h, doc, &[r1]);
    h.invoke(doc, CommandKey::HideSelected);
    select(&mut h, doc, &[r2]);
    h.invoke(doc, CommandKey::LockSelected);
    h.send(UiIntent::SelectByKind { document: doc, kind: ObjectKind::Rectangle, mode: SelectMode::Replace });
    assert!(selected(&h, doc).is_empty());
}

/// "Selecteer zelfde type": from the selected objects to all of their kinds.
#[test]
fn select_same_type_extends_the_selection_to_the_kinds_selected() {
    let (mut h, doc, [r1, r2, c]) = three_shapes();
    select(&mut h, doc, &[r2]);
    assert!(h.view().commands[&CommandKey::SelectSameType].enabled);
    h.invoke(doc, CommandKey::SelectSameType);
    assert_eq!(selected(&h, doc), [r1, r2]);
    select(&mut h, doc, &[r1, c]);
    h.invoke(doc, CommandKey::SelectSameType);
    assert_eq!(selected(&h, doc), [r1, r2, c]);
    select(&mut h, doc, &[]);
    assert!(!h.view().commands[&CommandKey::SelectSameType].enabled, "nothing selected");
}

/// Isolate hides everything but the selection in one undoable step; Show All brings it back.
#[test]
fn isolate_hides_the_others_in_one_undo_step_and_show_all_restores_them() {
    let (mut h, doc, [r1, r2, c]) = three_shapes();
    select(&mut h, doc, &[r2]);
    assert!(h.view().commands[&CommandKey::IsolateSelection].enabled);
    h.invoke(doc, CommandKey::IsolateSelection);
    let after = vm(&h, doc);
    let visible: Vec<GeometryId> = after.objects.iter().filter(|o| o.visible).map(|o| o.id).collect();
    assert_eq!(visible, [r2]);
    assert_eq!(selected(&h, doc), [r2], "the isolated object stays selected");
    assert!(matches!(after.undo, Some(UndoLabel::Arrange(_))));
    assert!(!h.view().commands[&CommandKey::IsolateSelection].enabled, "nothing else to hide");
    h.invoke(doc, CommandKey::ShowAll);
    assert_eq!(vm(&h, doc).objects.iter().filter(|o| o.visible).count(), 3);
    h.invoke(doc, CommandKey::Undo);
    h.invoke(doc, CommandKey::Undo);
    let _ = (r1, c);
    assert_eq!(vm(&h, doc).objects.iter().filter(|o| o.visible).count(), 3, "undo of isolate shows all again");
}

// ---------------------------------------------------------------------------------------------
// E04: Tab door de snapkandidaten
// ---------------------------------------------------------------------------------------------

type SnapSeen = Option<(burnburn_geometry::Point, burnburn_editor::SnapKind)>;

fn snap_overlay(h: &HeadlessEditorHarness, doc: DocumentId) -> SnapSeen {
    h.scene(doc).unwrap().overlays.iter().find_map(|o| match o {
        burnburn_editor::Overlay::SnapCandidate { at, kind } => Some((*at, *kind)),
        _ => None,
    })
}

/// Near a rectangle's top edge the pointer is within reach of the edge midpoint and of the closest
/// outline point. Tab steps through them (best first, then wrapping); a gesture started now uses
/// the one shown; moving the pointer starts the list over.
#[test]
fn tab_steps_through_the_snap_candidates_and_the_gesture_uses_the_one_shown() {
    use burnburn_editor::{CanvasKey, SnapKind};
    use burnburn_geometry::Point;
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolLine);
    h.hover(doc, (21.0, 19.0));
    assert_eq!(snap_overlay(&h, doc), Some((Point::new(20.0, 20.0), SnapKind::Midpoint)), "best first");
    h.key(doc, CanvasKey::Tab);
    assert_eq!(snap_overlay(&h, doc), Some((Point::new(21.0, 20.0), SnapKind::Nearest)), "the next candidate");
    h.key(doc, CanvasKey::Tab);
    assert_eq!(snap_overlay(&h, doc).map(|c| c.1), Some(SnapKind::Midpoint), "wraps around");
    h.key(doc, CanvasKey::Tab);
    assert_eq!(snap_overlay(&h, doc).map(|c| c.1), Some(SnapKind::Nearest));

    // Moving the pointer starts the list over.
    h.hover(doc, (22.0, 19.0));
    h.hover(doc, (21.0, 19.0));
    assert_eq!(snap_overlay(&h, doc).map(|c| c.1), Some(SnapKind::Midpoint), "a pointer move resets Tab");
    h.key(doc, CanvasKey::Tab);
    assert_eq!(snap_overlay(&h, doc).map(|c| c.1), Some(SnapKind::Nearest));

    // The line starts at the candidate shown.
    h.drag(doc, (21.0, 19.0), (60.0, 60.0));
    let objects = vm(&h, doc).objects;
    assert_eq!(objects.len(), 2);
    let scene = h.scene(doc).unwrap();
    let b = scene.items.last().unwrap().path.bounds().unwrap();
    assert!((b.min().x - 21.0).abs() < 1e-9 && (b.min().y - 20.0).abs() < 1e-9, "{b:?}");
}
