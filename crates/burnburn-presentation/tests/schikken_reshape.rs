//! E04 (W3-UI-schikken): Warp Selection (4 points) and Deform Selection (16 points) as tools with
//! handles and a live preview. Enter (or the Apply command) is one undo entry; Escape drops it.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::{EditLabel, ObjectKind, UndoLabel};
use burnburn_editor::{CanvasKey, Overlay, ToolKind, ToolPreview};
use burnburn_geometry::Point;
use burnburn_presentation::{CommandKey, MessageKey, ReshapeProblem, SchikkenIntent, ToolRejection, UiIntent};
use burnburn_types::DocumentId;
use edit_rig::{enabled, opened_with_kinds, points_of, select, square, view};

type Harness = burnburn_presentation::HeadlessEditorHarness;

fn start(h: &mut Harness, doc: DocumentId, key: CommandKey) {
    h.invoke(doc, key);
}

fn corner_set(h: &Harness, doc: DocumentId, id: burnburn_types::GeometryId) -> Vec<(i64, i64)> {
    points_of(h, doc, id).iter().map(|p| ((p.0 * 1000.0).round() as i64, (p.1 * 1000.0).round() as i64)).collect()
}

fn handle_overlays(h: &Harness, doc: DocumentId) -> Vec<(usize, Point, bool)> {
    h.scene(doc)
        .expect("scene")
        .overlays
        .iter()
        .filter_map(|o| match o {
            Overlay::ReshapeHandle { at, index, invalid, .. } => Some((*index, *at, *invalid)),
            _ => None,
        })
        .collect()
}

fn two_squares() -> (Harness, DocumentId, Vec<burnburn_types::GeometryId>) {
    let (mut h, doc, ids) = opened_with_kinds(vec![
        (square(0.0, 0.0, 20.0), ObjectKind::Path),
        (square(40.0, 0.0, 20.0), ObjectKind::Path),
    ]);
    select(&mut h, doc, &ids);
    (h, doc, ids)
}

#[test]
fn choosing_warp_shows_four_handles_on_the_selection_box_and_changes_nothing() {
    let (mut h, doc, _) = two_squares();
    let before = view(&h, doc).revision;
    start(&mut h, doc, CommandKey::ToolWarp);
    let vm = view(&h, doc);
    assert_eq!(vm.tool, ToolKind::Warp);
    assert_eq!(vm.revision, before);
    let handles = handle_overlays(&h, doc);
    assert_eq!(
        handles.iter().map(|(_, at, _)| *at).collect::<Vec<_>>(),
        vec![Point::new(0.0, 0.0), Point::new(60.0, 0.0), Point::new(60.0, 20.0), Point::new(0.0, 20.0)]
    );
    let reshape = vm.schikken.reshape.expect("reshape state");
    assert!(!reshape.modified && reshape.problem.is_none() && !reshape.bilinear);
    assert!(!enabled(&h, CommandKey::ReshapeApply) && !enabled(&h, CommandKey::ReshapeReset));
}

#[test]
fn dragging_a_handle_previews_and_enter_applies_one_warp_as_one_undo_entry() {
    let (mut h, doc, ids) = two_squares();
    start(&mut h, doc, CommandKey::ToolWarp);
    h.drag(doc, (60.0, 20.0), (50.0, 35.0));
    let vm = view(&h, doc);
    assert!(matches!(vm.preview, Some(ToolPreview::Reshape(_))), "{:?}", vm.preview);
    assert!(vm.schikken.reshape.expect("state").modified);
    assert_eq!(handle_overlays(&h, doc)[2].1, Point::new(50.0, 35.0));
    assert!(enabled(&h, CommandKey::ReshapeApply) && enabled(&h, CommandKey::ReshapeReset));
    assert_eq!(vm.undo, None, "nothing reaches the document before Enter");
    h.key(doc, CanvasKey::Return);
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Warp)));
    assert_eq!(vm.tool, ToolKind::Select, "the tool is done after applying");
    assert!(corner_set(&h, doc, ids[1]).contains(&(50_000, 35_000)), "the box corner went to the handle");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(corner_set(&h, doc, ids[1]), vec![(40_000, 0), (60_000, 0), (60_000, 20_000), (40_000, 20_000)]);
}

#[test]
fn escape_leaves_the_tool_and_the_document_untouched() {
    let (mut h, doc, _) = two_squares();
    start(&mut h, doc, CommandKey::ToolWarp);
    h.drag(doc, (60.0, 20.0), (50.0, 35.0));
    let revision = view(&h, doc).revision;
    h.escape(doc);
    let vm = view(&h, doc);
    assert_eq!(vm.tool, ToolKind::Select);
    assert_eq!(vm.revision, revision);
    assert!(handle_overlays(&h, doc).is_empty());
    assert_eq!(vm.undo, None);
}

#[test]
fn a_warp_without_a_usable_selection_says_so_and_stays_in_select() {
    let (mut h, doc, ids) = two_squares();
    select(&mut h, doc, &[]);
    start(&mut h, doc, CommandKey::ToolWarp);
    let vm = view(&h, doc);
    assert_eq!(vm.tool, ToolKind::Select);
    assert!(vm.messages.iter().any(|m| m.key == MessageKey::ToolRejected(ToolRejection::NeedsSelection)));
    // A flat selection (a vertical line) has no box to map.
    let line = burnburn_geometry::Path::line(Point::new(0.0, 0.0), Point::new(0.0, 9.0)).unwrap();
    let (mut h, doc, ids2) = opened_with_kinds(vec![(line, ObjectKind::Path)]);
    select(&mut h, doc, &ids2);
    start(&mut h, doc, CommandKey::ToolDeform);
    assert_eq!(view(&h, doc).tool, ToolKind::Select);
    assert!(view(&h, doc).messages.iter().any(|m| m.key == MessageKey::ToolRejected(ToolRejection::InvalidSelection)));
    let _ = ids;
}

#[test]
fn a_folded_quadrilateral_is_marked_invalid_and_cannot_be_applied() {
    let (mut h, doc, _) = two_squares();
    start(&mut h, doc, CommandKey::ToolWarp);
    h.drag(doc, (60.0, 20.0), (30.0, -40.0));
    let vm = view(&h, doc);
    assert_eq!(vm.schikken.reshape.expect("state").problem, Some(ReshapeProblem::NotConvex));
    assert!(handle_overlays(&h, doc).iter().all(|(_, _, invalid)| *invalid));
    assert!(!enabled(&h, CommandKey::ReshapeApply));
    let revision = vm.revision;
    h.key(doc, CanvasKey::Return);
    assert_eq!(view(&h, doc).revision, revision);
    assert_eq!(view(&h, doc).tool, ToolKind::Warp, "the handles stay so they can be fixed");
}

#[test]
fn deform_has_sixteen_handles_reset_and_apply() {
    let (mut h, doc, ids) = two_squares();
    start(&mut h, doc, CommandKey::ToolDeform);
    assert_eq!(handle_overlays(&h, doc).len(), 16);
    let inner = (60.0 / 3.0, 20.0 / 3.0);
    h.drag(doc, inner, (25.0, 12.0));
    assert_eq!(handle_overlays(&h, doc)[5].1, Point::new(25.0, 12.0));
    h.invoke(doc, CommandKey::ReshapeReset);
    assert_eq!(handle_overlays(&h, doc)[5].1, Point::new(inner.0, inner.1));
    assert!(!view(&h, doc).schikken.reshape.expect("state").modified);
    h.drag(doc, inner, (25.0, 12.0));
    let outline =
        |h: &Harness| h.scene(doc).expect("scene").items.iter().find(|i| i.source == ids[0]).map(|i| (*i.path).clone());
    let before = outline(&h);
    h.invoke(doc, CommandKey::ReshapeApply);
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Deform)));
    // The corners of the squares sit on the border of the net, which the inner handle cannot move;
    // the edges between them bend.
    assert_ne!(outline(&h), before, "the interior moved");
    assert_eq!(corner_set(&h, doc, ids[0]), vec![(0, 0), (20_000, 0), (20_000, 20_000), (0, 20_000)]);
    assert_eq!(vm.tool, ToolKind::Select);
}

#[test]
fn the_warp_kind_is_a_setting_of_the_tool() {
    let (mut h, doc, _) = two_squares();
    start(&mut h, doc, CommandKey::ToolWarp);
    h.send(UiIntent::Schikken(SchikkenIntent::SetWarpKind { document: doc, bilinear: true }));
    assert!(view(&h, doc).schikken.reshape.expect("state").bilinear);
    h.drag(doc, (60.0, 20.0), (50.0, 35.0));
    assert!(view(&h, doc).schikken.reshape.expect("state").bilinear, "it survives dragging");
}
