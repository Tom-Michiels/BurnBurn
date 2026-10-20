//! W2-L1-tekenen in the window: the geometry operations of W1-L2a (round corners, start point,
//! join/close, cut, direction, optimize), the basic shapes of W1-L2b (parameters, convert to
//! path) and the batch tools, driven through the same intents a native host sends. Each gesture
//! is one document command and one undo entry; a refused one changes nothing and says why.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::f64::consts::PI;
use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{
    DocumentSnapshot, EditLabel, GeometryObject, GeometrySource, ObjectKind, ShapeLabel, UndoLabel,
};
use burnburn_editor::{CanvasKey, ToolKind};
use burnburn_geometry::{Path, Point};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DisabledReason, DocumentViewModel, Enablement, GeometryDialogKind, HeadlessEditorHarness, HostEffect,
    MessageKey, ParamKey, SelectMode, ToolRejection, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, StableId};

type H = HeadlessEditorHarness;

fn opened_with(paths: Vec<Path>) -> (H, DocumentId, Vec<GeometryId>) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let objects: Vec<_> = paths
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let id = GeometryId::from_raw(i as u64 + 1);
            GeometryObject::new(id, format!("Pad {}", i + 1), ObjectKind::Path, GeometrySource::Drawn, p)
        })
        .collect();
    let next = objects.len() as u64 + 1;
    let snapshot = DocumentSnapshot::from_parts(objects, next).expect("snapshot");
    let path = PathBuf::from("/work/fixture.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([7; 16]), snapshot)).expect("fixture");
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    let ids = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, ids)
}

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn polygon(points: &[(f64, f64)], closed: bool) -> Path {
    Path::polyline(&points.iter().map(|&(x, y)| p(x, y)).collect::<Vec<_>>(), closed).unwrap()
}

fn square(x: f64, y: f64, size: f64) -> Path {
    polygon(&[(x, y), (x + size, y), (x + size, y + size), (x, y + size)], true)
}

fn select(h: &mut H, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn choose(h: &mut H, doc: DocumentId, id: &str) {
    h.send(UiIntent::SelectTool { document: Some(doc), id: id.to_owned() });
}

/// The outline of the first drawn object, as the canvas would draw it.
fn first_path(h: &H, doc: DocumentId) -> Path {
    let scene = h.scene(doc).expect("scene");
    scene.items.first().expect("an object").path.as_ref().clone()
}

fn area_of(h: &H, doc: DocumentId, index: usize) -> f64 {
    h.scene(doc).expect("scene").items[index].path.signed_area()
}

fn enablement(h: &H, id: &str) -> Enablement {
    h.view().toolbar.button(id).unwrap_or_else(|| panic!("{id} is in the toolbar")).enablement
}

fn undo(h: &mut H, doc: DocumentId) {
    h.invoke(doc, CommandKey::Undo);
}

// ---------------------------------------------------------------------------------------------
// Tools that act on a clicked corner
// ---------------------------------------------------------------------------------------------

#[test]
fn the_round_corner_tool_rounds_the_clicked_corner_in_one_undo_entry() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "geometry.round_corners");
    assert_eq!(vm(&h, doc).tool, ToolKind::RoundCorner);
    h.click(doc, (0.0, 0.0), Default::default());
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::RoundCorners)));
    // The default radius (2 mm) takes r² (1 − π/4) off the square.
    assert!((area_of(&h, doc, 0) - (1600.0 - 4.0 * (1.0 - PI / 4.0))).abs() < 1e-6, "{}", area_of(&h, doc, 0));
    undo(&mut h, doc);
    assert!((area_of(&h, doc, 0) - 1600.0).abs() < 1e-9);
}

#[test]
fn the_start_point_tool_makes_the_clicked_node_where_cutting_starts() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "geometry.start_point");
    h.click(doc, (40.0, 40.0), Default::default());
    assert_eq!(first_path(&h, doc).subpaths()[0].start(), p(40.0, 40.0));
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::EditNodes)));
    undo(&mut h, doc);
    assert_eq!(first_path(&h, doc).subpaths()[0].start(), p(0.0, 0.0));
}

// ---------------------------------------------------------------------------------------------
// Selection-wide commands and their dialogs
// ---------------------------------------------------------------------------------------------

fn two_lines_with_a_gap() -> Vec<Path> {
    vec![polygon(&[(0.0, 0.0), (10.0, 0.0)], false), polygon(&[(10.03, 0.0), (20.0, 0.0)], false)]
}

#[test]
fn join_asks_for_a_tolerance_and_joins_the_selected_paths_in_one_entry() {
    let (mut h, doc, ids) = opened_with(two_lines_with_a_gap());
    select(&mut h, doc, &ids);
    assert_eq!(enablement(&h, "geometry.join"), Enablement::Enabled);
    h.invoke(doc, CommandKey::ShapesJoin);
    let dialog = vm(&h, doc).geometry_dialog.expect("the dialog opens");
    assert_eq!((dialog.kind, dialog.text.as_str(), dialog.error), (GeometryDialogKind::Join, "0.05", None));
    assert_eq!(vm(&h, doc).objects.len(), 2, "opening the dialog changes nothing");

    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "0.05 mm".into() });
    let after = vm(&h, doc);
    assert!(after.geometry_dialog.is_none());
    assert_eq!(after.objects.len(), 1, "two lines became one object");
    assert_eq!(after.undo, Some(UndoLabel::Edit(EditLabel::JoinShapes)));
    undo(&mut h, doc);
    assert_eq!(vm(&h, doc).objects.len(), 2);
}

#[test]
fn a_dialog_value_that_is_wrong_or_changes_nothing_keeps_the_dialog_and_the_document() {
    let (mut h, doc, ids) = opened_with(two_lines_with_a_gap());
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ShapesJoin);
    let revision = vm(&h, doc).revision;

    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "abc".into() });
    let dialog = vm(&h, doc).geometry_dialog.expect("still open");
    assert!(matches!(dialog.error, Some(MessageKey::InvalidLength(_))), "{:?}", dialog.error);
    assert_eq!(dialog.text, "abc", "what was typed stays");

    // A tolerance smaller than the gap joins nothing: said in words, nothing recorded.
    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "0.01".into() });
    let dialog = vm(&h, doc).geometry_dialog.expect("still open");
    assert_eq!(dialog.error, Some(MessageKey::NothingToChange));
    assert_eq!(vm(&h, doc).revision, revision);

    h.send(UiIntent::CancelGeometryDialog { document: doc });
    assert!(vm(&h, doc).geometry_dialog.is_none());
    assert_eq!(vm(&h, doc).revision, revision);
}

#[test]
fn closing_one_nearly_closed_path_keeps_its_identity() {
    let open = polygon(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 0.03)], false);
    let (mut h, doc, ids) = opened_with(vec![open]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ShapesJoin);
    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "0.05".into() });
    let after = vm(&h, doc);
    assert_eq!(after.objects.iter().map(|o| o.id).collect::<Vec<_>>(), ids);
    assert!(first_path(&h, doc).subpaths()[0].is_closed());
}

#[test]
fn round_corners_for_the_whole_selection_takes_the_typed_radius() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ShapesRoundCorners);
    let dialog = vm(&h, doc).geometry_dialog.expect("dialog");
    assert_eq!((dialog.kind, dialog.text.as_str()), (GeometryDialogKind::RoundCorners, "2"));
    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "5 mm".into() });
    // Four corners of 5 mm: the square loses 4 · r² (1 − π/4).
    let expected = 1600.0 - 4.0 * 25.0 * (1.0 - PI / 4.0);
    assert!((area_of(&h, doc, 0) - expected).abs() < 1e-6, "{}", area_of(&h, doc, 0));
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::RoundCorners)));
    // A radius of zero or less is a field error, not a command.
    h.invoke(doc, CommandKey::ShapesRoundCorners);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "0".into() });
    assert!(matches!(vm(&h, doc).geometry_dialog.expect("open").error, Some(MessageKey::InvalidLength(_))));
    assert_eq!(vm(&h, doc).revision, revision);
}

#[test]
fn optimize_removes_a_node_that_is_on_the_line_and_says_so_when_nothing_is_left_to_do() {
    let wasteful = polygon(&[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false);
    let (mut h, doc, ids) = opened_with(vec![wasteful]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ShapesOptimize);
    assert_eq!(vm(&h, doc).geometry_dialog.expect("dialog").text, "0.01");
    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "0.01 mm".into() });
    assert_eq!(first_path(&h, doc).subpaths()[0].segments().len(), 2, "three nodes are enough");
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::OptimizeShapes)));
    // Again: already optimal.
    h.invoke(doc, CommandKey::ShapesOptimize);
    h.send(UiIntent::ApplyGeometryDialog { document: doc, text: "0.01".into() });
    assert_eq!(vm(&h, doc).geometry_dialog.expect("open").error, Some(MessageKey::NothingToChange));
}

#[test]
fn cut_shapes_cuts_the_first_selected_by_the_last_and_undo_restores_both() {
    let (mut h, doc, ids) = opened_with(vec![
        square(0.0, 0.0, 10.0),
        polygon(&[(5.0, -5.0), (15.0, -5.0), (15.0, 15.0), (5.0, 15.0)], true),
    ]);
    select(&mut h, doc, &ids);
    assert_eq!(enablement(&h, "geometry.cut"), Enablement::Enabled);
    h.invoke(doc, CommandKey::ShapesCut);
    let after = vm(&h, doc);
    assert_eq!(after.objects.len(), 2, "the subject became two pieces, the cutter is gone");
    assert_eq!(after.undo, Some(UndoLabel::Edit(EditLabel::CutShapes)));
    let total: f64 = (0..2).map(|i| area_of(&h, doc, i).abs()).sum();
    assert!((total - 100.0).abs() < 1e-6, "{total}");
    undo(&mut h, doc);
    assert_eq!(vm(&h, doc).objects.len(), 2);
    assert!((area_of(&h, doc, 0) - 100.0).abs() < 1e-9);
}

#[test]
fn direction_commands_reverse_or_set_the_winding_in_one_entry() {
    let ccw = square(0.0, 0.0, 10.0);
    assert!(ccw.signed_area() > 0.0, "the fixture is counter-clockwise");
    let (mut h, doc, ids) = opened_with(vec![ccw]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::ShapesClockwise);
    assert!(area_of(&h, doc, 0) < 0.0);
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::Direction)));
    let revision = vm(&h, doc).revision;
    h.invoke(doc, CommandKey::ShapesClockwise);
    assert_eq!(vm(&h, doc).revision, revision, "already clockwise: nothing recorded");
    h.invoke(doc, CommandKey::ShapesCounterClockwise);
    assert!(area_of(&h, doc, 0) > 0.0);
    h.invoke(doc, CommandKey::ShapesReverse);
    assert!(area_of(&h, doc, 0) < 0.0);
    assert_eq!(first_path(&h, doc).subpaths()[0].start(), p(0.0, 0.0), "the start node is kept");
}

#[test]
fn shape_commands_are_disabled_with_the_reason_the_selection_does_not_fit() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(20.0, 0.0, 10.0)]);
    select(&mut h, doc, &[]);
    for id in ["geometry.join", "geometry.cut", "geometry.round_corners_all", "geometry.optimize", "geometry.reverse"] {
        assert_eq!(enablement(&h, id), Enablement::Disabled(DisabledReason::NeedsSelection), "{id}");
    }
    for key in [CommandKey::ShapesJoin, CommandKey::ShapesCut, CommandKey::ShapesOptimize] {
        assert!(!h.view().commands[&key].enabled, "{key:?}");
    }
    // Closed shapes have nothing to join; a single shape cannot cut itself; plain shapes are not basic shapes.
    select(&mut h, doc, &ids[..1]);
    assert_eq!(enablement(&h, "geometry.join"), Enablement::Disabled(DisabledReason::NeedsSelection));
    assert_eq!(enablement(&h, "geometry.cut"), Enablement::Disabled(DisabledReason::NeedsSelection));
    assert_eq!(enablement(&h, "geometry.convert_to_path"), Enablement::Disabled(DisabledReason::NeedsSelection));
    assert_eq!(enablement(&h, "geometry.optimize"), Enablement::Enabled);
    select(&mut h, doc, &ids);
    assert_eq!(enablement(&h, "geometry.cut"), Enablement::Enabled);
    // A disabled command invoked anyway does nothing.
    select(&mut h, doc, &[]);
    let revision = vm(&h, doc).revision;
    h.invoke(doc, CommandKey::ShapesOptimize);
    assert!(vm(&h, doc).geometry_dialog.is_none());
    assert_eq!(vm(&h, doc).revision, revision);
}

// ---------------------------------------------------------------------------------------------
// Basic shapes
// ---------------------------------------------------------------------------------------------

fn new_doc() -> (H, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    (h, doc)
}

fn rows(vm: &DocumentViewModel) -> Vec<(ParamKey, String)> {
    vm.shape_panel.as_ref().map(|p| p.rows.iter().map(|r| (r.key, r.text.clone())).collect()).unwrap_or_default()
}

#[test]
fn a_dragged_star_is_a_named_shape_with_parameters_in_the_inspector() {
    let (mut h, doc) = new_doc();
    choose(&mut h, doc, "shape.star");
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    let created = vm(&h, doc);
    assert_eq!(created.objects.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["Ster 1"]);
    assert_eq!(created.undo, Some(UndoLabel::Shape(ShapeLabel::Create)));
    assert_eq!(rows(&created), [(ParamKey::Points, "5".to_owned()), (ParamKey::Ratio, "0.382".to_owned())]);

    h.send(UiIntent::CommitShapeParam { document: doc, key: ParamKey::Points, text: "7".into() });
    let edited = vm(&h, doc);
    assert_eq!(edited.undo, Some(UndoLabel::Shape(ShapeLabel::SetParams)));
    assert_eq!(rows(&edited)[0], (ParamKey::Points, "7".to_owned()));
    assert_eq!(edited.objects.len(), 1, "the same object, other tips");
    // 14 vertices.
    assert_eq!(first_path(&h, doc).subpaths()[0].segments().len() + 1, 14);
    undo(&mut h, doc);
    assert_eq!(rows(&vm(&h, doc))[0], (ParamKey::Points, "5".to_owned()));
}

#[test]
fn a_wrong_shape_parameter_is_shown_in_its_row_and_changes_nothing() {
    let (mut h, doc) = new_doc();
    choose(&mut h, doc, "shape.star");
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    let revision = vm(&h, doc).revision;
    for (key, text) in [(ParamKey::Points, "2"), (ParamKey::Ratio, "1.5"), (ParamKey::Points, "x")] {
        h.send(UiIntent::CommitShapeParam { document: doc, key, text: text.into() });
        let after = vm(&h, doc);
        assert_eq!(after.revision, revision, "{text}");
        let row = after.shape_panel.expect("panel").rows.into_iter().find(|r| r.key == key).expect("row");
        assert!(row.error.is_some(), "{text}: the row says why");
    }
    // The same value again is not a command either.
    h.send(UiIntent::CommitShapeParam { document: doc, key: ParamKey::Points, text: "5".into() });
    assert_eq!(vm(&h, doc).revision, revision);
}

#[test]
fn convert_to_path_removes_the_shape_section_and_is_one_undo_entry() {
    let (mut h, doc) = new_doc();
    choose(&mut h, doc, "shape.pentagon");
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    assert_eq!(enablement(&h, "geometry.convert_to_path"), Enablement::Enabled);
    assert!(vm(&h, doc).shape_panel.is_some());
    let outline = first_path(&h, doc);
    h.invoke(doc, CommandKey::ShapesConvertToPath);
    let after = vm(&h, doc);
    assert!(after.shape_panel.is_none(), "a plain path has no definition");
    assert_eq!(after.undo, Some(UndoLabel::Shape(ShapeLabel::ConvertToPath)));
    assert_eq!(first_path(&h, doc), outline, "the outline stays");
    assert_eq!(enablement(&h, "geometry.convert_to_path"), Enablement::Disabled(DisabledReason::NeedsSelection));
    undo(&mut h, doc);
    assert!(vm(&h, doc).shape_panel.is_some());
}

#[test]
fn the_polygon_constructions_and_the_rounded_rectangle_make_editable_shapes() {
    let (mut h, doc) = new_doc();
    choose(&mut h, doc, "sketch.polygon.inscribed");
    assert_eq!(vm(&h, doc).tool, ToolKind::Polygon);
    assert!(h.view().toolbar.button("sketch.polygon.inscribed").unwrap().selected);
    assert!(!h.view().toolbar.button("shape.polygon").unwrap().selected, "one tool, the mode decides the button");
    h.drag(doc, (0.0, 0.0), (10.0, 0.0));
    let polygon = vm(&h, doc);
    assert_eq!(polygon.objects[0].label, "Veelhoek 1");
    assert_eq!(rows(&polygon), [(ParamKey::Sides, "6".to_owned())]);
    // The inscribed hexagon has its first vertex on the second point.
    assert_eq!(first_path(&h, doc).subpaths()[0].start(), p(10.0, 0.0));

    choose(&mut h, doc, "sketch.rectangle.rounded");
    h.drag(doc, (100.0, 0.0), (120.0, 10.0));
    let rounded = vm(&h, doc);
    assert_eq!(rounded.objects.last().map(|o| o.label.as_str()), Some("Afgeronde rechthoek 1"));
    assert_eq!(rows(&rounded), [(ParamKey::CornerRadius, "3".to_owned())]);
    h.send(UiIntent::CommitShapeParam { document: doc, key: ParamKey::CornerRadius, text: "2".into() });
    assert_eq!(rows(&vm(&h, doc)), [(ParamKey::CornerRadius, "2".to_owned())]);
}

#[test]
fn tool_options_decide_the_next_shape() {
    let (mut h, doc) = new_doc();
    choose(&mut h, doc, "shape.polygon");
    assert_eq!(
        vm(&h, doc).tool_options.iter().map(|r| (r.key, r.text.as_str())).collect::<Vec<_>>(),
        [(ParamKey::Sides, "6")]
    );
    h.send(UiIntent::SetToolOption { document: doc, key: ParamKey::Sides, text: "9".into() });
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    assert_eq!(rows(&vm(&h, doc)), [(ParamKey::Sides, "9".to_owned())]);
    // An invalid option keeps the old one and says why.
    choose(&mut h, doc, "shape.polygon");
    h.send(UiIntent::SetToolOption { document: doc, key: ParamKey::Sides, text: "2".into() });
    let options = vm(&h, doc).tool_options;
    assert_eq!((options[0].text.as_str(), options[0].error.is_some()), ("9", true));
    // Tools without options show none.
    choose(&mut h, doc, "select");
    assert!(vm(&h, doc).tool_options.is_empty());
}

#[test]
fn a_sketch_refuses_a_basic_shape_in_words_instead_of_ignoring_it() {
    let (mut h, doc) = new_doc();
    h.invoke(doc, CommandKey::SketchNew);
    choose(&mut h, doc, "shape.star");
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    let after = vm(&h, doc);
    assert_eq!(after.objects.len(), 0, "nothing was drawn");
    assert!(after.sketch_mode.is_some_and(|m| m.refused.is_some()), "the sketch panel says why");
}

#[test]
fn the_delete_key_in_node_mode_deletes_nodes_and_otherwise_the_object() {
    let (mut h, doc, ids) = opened_with(vec![polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], false)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "node");
    h.click(doc, (10.0, 0.0), Default::default());
    assert_eq!(vm(&h, doc).node_selection.len(), 1, "the node is selected");
    h.key(doc, CanvasKey::Delete);
    let after = vm(&h, doc);
    assert_eq!(after.objects.len(), 1, "the object stays");
    assert_eq!(first_path(&h, doc).subpaths()[0].segments().len(), 2, "one node fewer");
    assert!(after.node_selection.is_empty());
    // Without a node selected the key deletes the object as before.
    h.key(doc, CanvasKey::Delete);
    assert_eq!(vm(&h, doc).objects.len(), 0);
}

#[test]
fn a_refused_single_object_gesture_says_so() {
    // Mirror with copies of two objects would be two history entries.
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(20.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    choose(&mut h, doc, "sketch.mirror");
    h.send(UiIntent::SetToolOption { document: doc, key: ParamKey::KeepOriginal, text: "1".into() });
    let revision = vm(&h, doc).revision;
    h.click(doc, (50.0, -10.0), Default::default());
    h.click(doc, (50.0, 10.0), Default::default());
    let after = vm(&h, doc);
    assert_eq!(after.revision, revision);
    assert!(after.messages.iter().any(|m| m.key == MessageKey::ToolRejected(ToolRejection::SingleObjectOnly)));
}
