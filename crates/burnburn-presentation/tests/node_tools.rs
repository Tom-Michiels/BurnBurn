//! The node buttons of W1-L2a in the window: select nodes with the Node tool, then delete, break,
//! smooth, change segments, close, join ends or set the start point. Each is one document
//! command and one undo entry; a button that cannot act says why.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{DocumentSnapshot, EditLabel, GeometryObject, GeometrySource, ObjectKind, UndoLabel};
use burnburn_editor::{Modifiers, Overlay};
use burnburn_geometry::{Path, Point, Segment};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DisabledReason, DocumentViewModel, Enablement, HeadlessEditorHarness, HostEffect, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, StableId};

type H = HeadlessEditorHarness;

fn opened(path: Path) -> (H, DocumentId, GeometryId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let id = GeometryId::from_raw(1);
    let object = GeometryObject::new(id, "Pad".to_owned(), ObjectKind::Path, GeometrySource::Drawn, path);
    let snapshot = DocumentSnapshot::from_parts(vec![object], 2).expect("snapshot");
    let file = PathBuf::from("/work/nodes.burnburn");
    save_project(h.fs(), &file, &Project::new(ProjectUuid::from_bytes([3; 16]), snapshot)).expect("fixture");
    h.send(UiIntent::Open { path: file });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::SelectTool { document: Some(doc), id: "node".into() });
    (h, doc, id)
}

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn open_zigzag() -> Path {
    Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], false).unwrap()
}

fn closed_square() -> Path {
    Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap()
}

fn shift() -> Modifiers {
    Modifiers { shift: true, ..Modifiers::default() }
}

fn the_path(h: &H, doc: DocumentId) -> Path {
    h.scene(doc).expect("scene").items[0].path.as_ref().clone()
}

fn segments(h: &H, doc: DocumentId) -> Vec<Segment> {
    the_path(h, doc).subpaths()[0].segments().to_vec()
}

fn select_node(h: &mut H, doc: DocumentId, at: (f64, f64)) {
    h.click(doc, at, Modifiers::default());
}

fn add_node(h: &mut H, doc: DocumentId, at: (f64, f64)) {
    h.click(doc, at, shift());
}

fn enablement(h: &H, id: &str) -> Enablement {
    h.view().toolbar.button(id).unwrap_or_else(|| panic!("{id} is in the toolbar")).enablement
}

fn entry(h: &H, doc: DocumentId) -> Option<UndoLabel> {
    vm(h, doc).undo
}

#[test]
fn the_node_buttons_follow_the_node_selection() {
    let (mut h, doc, _) = opened(open_zigzag());
    for id in ["node.delete", "node.break", "node.smooth", "node.to_line"] {
        assert_eq!(enablement(&h, id), Enablement::Disabled(DisabledReason::NeedsNodes), "{id}: no node yet");
    }
    // Closing is the exception: without a selected node it closes every open path of the object.
    assert_eq!(enablement(&h, "node.close_path"), Enablement::Enabled);
    select_node(&mut h, doc, (10.0, 0.0));
    assert_eq!(vm(&h, doc).node_selection.len(), 1);
    for id in ["node.delete", "node.break", "node.smooth", "node.close_path"] {
        assert_eq!(enablement(&h, id), Enablement::Enabled, "{id}");
    }
    assert!(h.view().commands[&CommandKey::NodeDelete].enabled);
    // Leaving node mode drops the node selection and the buttons with it.
    h.send(UiIntent::SelectTool { document: Some(doc), id: "select".into() });
    assert!(vm(&h, doc).node_selection.is_empty());
    assert_eq!(enablement(&h, "node.delete"), Enablement::Disabled(DisabledReason::NeedsNodes));
}

#[test]
fn delete_and_break_work_on_the_selected_nodes_in_one_entry_each() {
    let (mut h, doc, _) = opened(open_zigzag());
    select_node(&mut h, doc, (10.0, 0.0));
    h.invoke(doc, CommandKey::NodeDelete);
    assert_eq!(segments(&h, doc).len(), 2, "four nodes became three");
    assert_eq!(entry(&h, doc), Some(UndoLabel::Edit(EditLabel::EditNodes)));
    assert!(vm(&h, doc).node_selection.is_empty(), "the numbering changed, so the selection went");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(segments(&h, doc).len(), 3);

    let (mut h, doc, _) = opened(closed_square());
    select_node(&mut h, doc, (10.0, 10.0));
    h.invoke(doc, CommandKey::NodeBreak);
    assert!(!the_path(&h, doc).subpaths()[0].is_closed(), "a closed path opens at the node");
}

#[test]
fn smooth_symmetric_and_corner_change_the_handles_of_the_selected_node() {
    let (mut h, doc, _) = opened(closed_square());
    select_node(&mut h, doc, (10.0, 0.0));
    h.invoke(doc, CommandKey::NodeSmooth);
    assert!(segments(&h, doc).iter().any(|s| matches!(s, Segment::Cubic { .. })), "the corner became a curve");
    assert_eq!(entry(&h, doc), Some(UndoLabel::Edit(EditLabel::EditNodes)));
    assert_eq!(vm(&h, doc).node_selection.len(), 1, "the same node stays selected");
    let smooth = the_path(&h, doc);
    h.invoke(doc, CommandKey::NodeCorner);
    let corner = the_path(&h, doc);
    assert_ne!(corner, smooth, "a corner has no handles that pull the curve");
    h.invoke(doc, CommandKey::NodeSymmetric);
    assert_ne!(the_path(&h, doc), corner, "symmetric handles are equally long on both sides");
    let revision = vm(&h, doc).revision;
    h.invoke(doc, CommandKey::NodeSymmetric);
    assert_eq!(vm(&h, doc).revision, revision, "already symmetric: the button is off and nothing is recorded");
    h.invoke(doc, CommandKey::Undo);
    h.invoke(doc, CommandKey::Undo);
    h.invoke(doc, CommandKey::Undo);
    assert!(segments(&h, doc).iter().all(|s| matches!(s, Segment::Line { .. })), "three entries undone");
}

#[test]
fn line_and_curve_buttons_act_on_the_segment_between_two_selected_nodes() {
    let (mut h, doc, _) = opened(open_zigzag());
    select_node(&mut h, doc, (0.0, 0.0));
    add_node(&mut h, doc, (10.0, 0.0));
    assert_eq!(enablement(&h, "node.to_curve"), Enablement::Enabled);
    assert_eq!(enablement(&h, "node.to_line"), Enablement::Disabled(DisabledReason::NeedsNodes), "already a line");
    h.invoke(doc, CommandKey::NodeToCurve);
    let after = segments(&h, doc);
    assert!(matches!(after[0], Segment::Cubic { .. }) && matches!(after[1], Segment::Line { .. }), "{after:?}");
    h.invoke(doc, CommandKey::NodeToLine);
    assert!(segments(&h, doc).iter().all(|s| matches!(s, Segment::Line { .. })));
}

#[test]
fn close_and_join_ends_close_an_open_path() {
    let (mut h, doc, _) = opened(open_zigzag());
    select_node(&mut h, doc, (0.0, 0.0));
    h.invoke(doc, CommandKey::NodeClosePath);
    assert!(the_path(&h, doc).subpaths()[0].is_closed());
    h.invoke(doc, CommandKey::Undo);
    assert!(!the_path(&h, doc).subpaths()[0].is_closed());
    select_node(&mut h, doc, (0.0, 0.0));
    add_node(&mut h, doc, (0.0, 10.0));
    assert_eq!(enablement(&h, "node.join_ends"), Enablement::Enabled);
    h.invoke(doc, CommandKey::NodeJoinEnds);
    assert!(the_path(&h, doc).subpaths()[0].is_closed(), "the two ends of one path meet");
}

#[test]
fn the_start_point_button_makes_the_selected_node_the_start() {
    let (mut h, doc, _) = opened(closed_square());
    select_node(&mut h, doc, (10.0, 10.0));
    assert_eq!(enablement(&h, "node.start_point"), Enablement::Enabled);
    h.invoke(doc, CommandKey::NodeStartPoint);
    assert_eq!(the_path(&h, doc).subpaths()[0].start(), p(10.0, 10.0));
}

#[test]
fn the_scene_marks_the_selected_nodes() {
    let (mut h, doc, _) = opened(open_zigzag());
    select_node(&mut h, doc, (10.0, 10.0));
    let scene = h.scene(doc).expect("scene");
    let marked: Vec<_> = scene
        .overlays
        .iter()
        .filter_map(|o| match o {
            Overlay::Node { at, selected, .. } => Some((*at, *selected)),
            _ => None,
        })
        .collect();
    assert_eq!(marked.len(), 4, "every node is drawn");
    assert_eq!(marked.iter().filter(|(_, selected)| *selected).map(|(at, _)| *at).collect::<Vec<_>>(), [p(10.0, 10.0)]);
}
