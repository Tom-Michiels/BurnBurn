//! Headless IM09 interaction scenarios (docs/user-flows/gate-b.md B1–B5). They drive the real
//! AppController with the intents a native host sends (pointer positions in canvas points),
//! real editor tools, real domain functions and real document commands.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{DocumentSnapshot, EditLabel, GeometryObject, GeometrySource, ObjectKind, UndoLabel};
use burnburn_editor::{IssueKind, Modifiers, Overlay, PointerPhase, SnapKind, StyleRole, ToolKind, ToolPreview};
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CameraIntent, CommandKey, DocumentViewModel, FieldKey, HeadlessEditorHarness, HostEffect, SelectMode,
    SelectionCycle, UiIntent, ViewPointerEvent,
};
use burnburn_types::{DocumentId, GeometryId, MonotonicTime, StableId};

fn none() -> Modifiers {
    Modifiers::default()
}

fn alt() -> Modifiers {
    Modifiers { alt: true, ..none() }
}

fn command() -> Modifiers {
    Modifiers { command: true, ..none() }
}

fn view(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn bounds_of(h: &HeadlessEditorHarness, doc: DocumentId, id: GeometryId) -> (f64, f64, f64, f64) {
    let vm = view(h, doc);
    let field = |k| -> f64 {
        let i = vm.inspector.as_ref().filter(|i| i.object == id).expect("inspector of that object");
        i.field(k).expect("field").text.parse().expect("number")
    };
    (field(FieldKey::X), field(FieldKey::Y), field(FieldKey::Width), field(FieldKey::Height))
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

/// A window with rectangles drawn through the real rectangle tool.
fn with_rectangles(rects: &[((f64, f64), (f64, f64))]) -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::SnapToObjects); // drawing the fixture must not snap to earlier rectangles
    h.invoke(doc, CommandKey::ToolRectangle);
    for (from, to) in rects {
        h.drag(doc, *from, *to);
    }
    h.invoke(doc, CommandKey::SnapToObjects);
    h.invoke(doc, CommandKey::ToolSelect);
    let ids = view(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, ids)
}

/// A window opened from a project file with the given geometry (for shapes the UI cannot create
/// yet, and for imported-looking data).
fn opened_with(paths: Vec<Path>) -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
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
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([9; 16]), snapshot)).expect("fixture written");
    h.send(UiIntent::Open { path });
    let updates = h.run_tasks();
    let doc = updates
        .iter()
        .flat_map(|u| &u.effects)
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(*document),
            _ => None,
        })
        .expect("opened");
    let ids = view(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, ids)
}

/// DR-04: the host sends canvas points; presentation applies the window's camera. The same
/// points mean other millimetres after a zoom.
#[test]
fn pointer_points_are_mapped_through_the_window_camera() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    let t = MonotonicTime::from_offset(std::time::Duration::from_millis(1));
    let send = |h: &mut HeadlessEditorHarness, phase, pt| {
        h.send(UiIntent::Pointer { document: doc, event: ViewPointerEvent::mouse(phase, pt, t) })
    };
    // 800 × 600 pt, centre (0, 0) mm at 2 pt/mm: (400, 300) pt is the origin, y down.
    send(&mut h, PointerPhase::Down, [400.0, 300.0]);
    send(&mut h, PointerPhase::Up, [640.0, 140.0]);
    let first = view(&h, doc).objects[0].id;
    assert_eq!(bounds_of(&h, doc, first), (0.0, 0.0, 120.0, 80.0));

    h.send(UiIntent::Camera {
        document: doc,
        intent: CameraIntent::Magnify { magnification: 1.0, anchor_pt: [400.0, 300.0] },
    });
    let zoomed = view(&h, doc);
    assert_eq!(zoomed.camera.points_per_mm(), 4.0);
    assert_eq!(zoomed.revision, view(&h, doc).revision, "zoom is not a document change");
    send(&mut h, PointerPhase::Down, [400.0, 300.0]);
    send(&mut h, PointerPhase::Up, [640.0, 140.0]);
    let second = view(&h, doc).objects[1].id;
    assert_eq!(bounds_of(&h, doc, second), (0.0, 0.0, 60.0, 40.0));
}

/// Contract §5 / DR-02: the scene of a window is its document revision plus window overlays.
#[test]
fn scene_follows_document_selection_and_gesture_preview() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (40.0, 20.0)), ((60.0, 0.0), (80.0, 20.0))]);
    select(&mut h, doc, &[ids[1]]);
    let scene = h.scene(doc).expect("scene");
    assert_eq!(scene.revision, view(&h, doc).revision);
    let roles: Vec<_> = scene.items.iter().map(|i| (i.source, i.role)).collect();
    assert_eq!(roles, vec![(ids[0], StyleRole::Design), (ids[1], StyleRole::Selected)]);

    h.invoke(doc, CommandKey::ToolEllipse);
    h.begin_drag(doc, (0.0, 50.0), (30.0, 70.0));
    let during = h.scene(doc).expect("scene");
    assert!(during.overlays.iter().any(|o| matches!(o, Overlay::DragPreview { .. })), "preview drawn");
    assert_eq!(during.items.len(), 2, "a preview is not a document item");
    h.escape(doc);
    assert!(!h.scene(doc).expect("scene").overlays.iter().any(|o| matches!(o, Overlay::DragPreview { .. })));
}

/// E03 / B2: Alt-click walks through the overlapping candidates; the status shows "2 van 3".
#[test]
fn alt_click_cycles_overlapping_objects_and_reports_the_position() {
    let (mut h, doc, ids) =
        with_rectangles(&[((0.0, 0.0), (50.0, 50.0)), ((10.0, 10.0), (60.0, 60.0)), ((20.0, 20.0), (70.0, 70.0))]);
    h.click(doc, (100.0, 100.0), none()); // clear
    let mut seen = Vec::new();
    for _ in 0..3 {
        h.click(doc, (30.0, 30.0), alt());
        let vm = view(&h, doc);
        seen.push((vm.selection.clone(), vm.selection_cycle));
    }
    assert_eq!(
        seen,
        vec![
            (vec![ids[2]], Some(SelectionCycle { position: 1, count: 3 })),
            (vec![ids[1]], Some(SelectionCycle { position: 2, count: 3 })),
            (vec![ids[0]], Some(SelectionCycle { position: 3, count: 3 })),
        ]
    );
    h.click(doc, (5.0, 5.0), none());
    assert_eq!(view(&h, doc).selection_cycle, None, "a plain click ends the cycle display");
}

/// E03: object tree and canvas show the same selection.
#[test]
fn tree_and_canvas_share_selection() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (10.0, 10.0)), ((20.0, 0.0), (30.0, 10.0))]);
    select(&mut h, doc, &[ids[0]]);
    let selected_items = |h: &HeadlessEditorHarness| -> Vec<GeometryId> {
        h.scene(doc).expect("scene").items.iter().filter(|i| i.role == StyleRole::Selected).map(|i| i.source).collect()
    };
    assert_eq!(selected_items(&h), vec![ids[0]], "tree → canvas");
    h.click(doc, (25.0, 5.0), none());
    let vm = view(&h, doc);
    assert_eq!(
        vm.objects.iter().filter(|o| o.selected).map(|o| o.id).collect::<Vec<_>>(),
        vec![ids[1]],
        "canvas → tree"
    );
    assert_eq!(selected_items(&h), vec![ids[1]]);
    h.drag(doc, (-5.0, -5.0), (35.0, 15.0));
    assert_eq!(view(&h, doc).selection, ids, "rubber band selects both");
}

/// Move via drag is one history entry; Esc mid-drag restores without an entry (P02).
#[test]
fn move_drag_is_one_history_entry_and_escape_restores() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (40.0, 20.0))]);
    select(&mut h, doc, &[ids[0]]);
    let before = view(&h, doc).revision;
    h.drag(doc, (20.0, 10.0), (30.0, 15.0));
    let moved = view(&h, doc);
    assert_eq!(bounds_of(&h, doc, ids[0]), (10.0, 5.0, 40.0, 20.0));
    assert_eq!(moved.undo, Some(UndoLabel::Transform));
    assert_eq!(moved.revision.raw(), before.raw() + 1, "one command for the whole drag");

    h.begin_drag(doc, (30.0, 15.0), (90.0, 90.0));
    assert!(matches!(view(&h, doc).preview, Some(ToolPreview::Transform { .. })));
    h.escape(doc);
    let after = view(&h, doc);
    assert_eq!(after.preview, None, "Esc ends the gesture");
    assert_eq!(after.revision, moved.revision, "cancelled drag sends nothing");
    assert_eq!(bounds_of(&h, doc, ids[0]), (10.0, 5.0, 40.0, 20.0));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(bounds_of(&h, doc, ids[0]), (0.0, 0.0, 40.0, 20.0), "one undo restores the move");
}

/// Scale and rotate through handles: one entry each; the scale keeps the opposite corner.
#[test]
fn scale_and_rotate_handles_commit_one_entry_each() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (40.0, 20.0))]);
    select(&mut h, doc, &[ids[0]]);
    h.drag(doc, (40.0, 20.0), (80.0, 30.0)); // top-right handle
    assert_eq!(bounds_of(&h, doc, ids[0]), (0.0, 0.0, 80.0, 30.0));
    // Rotate handle 24 pt (12 mm at 2 pt/mm) above the top edge centre; quarter turn left.
    let center = (40.0, 15.0);
    h.drag(doc, (40.0, 42.0), (center.0 - 27.0, center.1));
    let (_, _, w, hgt) = bounds_of(&h, doc, ids[0]);
    assert!((w - 30.0).abs() < 1e-6 && (hgt - 80.0).abs() < 1e-6, "rotated 90°: {w} × {hgt}");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(bounds_of(&h, doc, ids[0]), (0.0, 0.0, 80.0, 30.0), "one undo per handle drag");
}

/// E04 (story-coverage test name): the cursor shows the named target; Cmd/Ctrl suppresses.
#[test]
fn snap_names_endpoint_and_primary_modifier_suppresses() {
    let (mut h, doc, _) = with_rectangles(&[((0.0, 0.0), (40.0, 20.0))]);
    h.invoke(doc, CommandKey::ToolRectangle);
    h.hover(doc, (40.8, 20.6));
    let vm = view(&h, doc);
    assert_eq!(vm.snap.map(|s| (s.kind, s.at)), Some((SnapKind::Endpoint, Point::new(40.0, 20.0))));
    assert!(
        h.scene(doc)
            .expect("scene")
            .overlays
            .contains(&Overlay::SnapCandidate { at: Point::new(40.0, 20.0), kind: SnapKind::Endpoint })
    );
    h.drag(doc, (40.8, 20.6), (70.0, 50.0));
    let snapped = *view(&h, doc).objects.last().map(|o| &o.id).expect("created");
    assert_eq!(bounds_of(&h, doc, snapped), (40.0, 20.0, 30.0, 30.0), "corner exactly on the endpoint");

    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag_with(doc, (40.8, 20.6), (70.0, 50.0), command());
    let free = *view(&h, doc).objects.last().map(|o| &o.id).expect("created");
    assert_eq!(bounds_of(&h, doc, free), (40.8, 20.6, 29.2, 29.4), "Cmd/Ctrl: no snap");
    assert_eq!(view(&h, doc).snap, None);
}

#[test]
fn grid_snapping_is_a_window_toggle() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    assert!(!h.view().commands[&CommandKey::SnapToGrid].checked);
    h.invoke(doc, CommandKey::SnapToGrid);
    assert!(h.view().commands[&CommandKey::SnapToGrid].checked);
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.4, 0.3), (12.6, 7.7));
    let id = view(&h, doc).objects[0].id;
    assert_eq!(bounds_of(&h, doc, id), (0.0, 0.0, 13.0, 8.0));
}

/// Inspector X/Y: one Transform each; an unchanged value sends nothing.
#[test]
fn numeric_position_in_the_inspector_is_one_command() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (40.0, 20.0))]);
    select(&mut h, doc, &[ids[0]]);
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "10".into() });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Y, text: "-5,5".into() });
    assert_eq!(bounds_of(&h, doc, ids[0]), (10.0, -5.5, 40.0, 20.0));
    let rev = view(&h, doc).revision;
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "10 mm".into() });
    assert_eq!(view(&h, doc).revision, rev, "unchanged value: no command");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(bounds_of(&h, doc, ids[0]), (10.0, 0.0, 40.0, 20.0), "Y and X are separate entries");
}

/// E01 / B1: typed size while dragging gives exact geometry; invalid text keeps the preview.
#[test]
fn typed_size_while_drawing_creates_an_exact_rectangle() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.begin_drag(doc, (10.0, 10.0), (37.3, 21.9));
    h.send(UiIntent::CommitToolSize { document: doc, width: "abc".into(), height: "80".into() });
    let vm = view(&h, doc);
    assert!(vm.tool_size_error.is_some() && vm.preview.is_some() && vm.objects.is_empty(), "error, still drawing");
    h.send(UiIntent::CommitToolSize { document: doc, width: "12cm".into(), height: "80".into() });
    let vm = view(&h, doc);
    assert_eq!((vm.tool_size_error, vm.preview.is_some()), (None, false));
    assert_eq!(bounds_of(&h, doc, vm.objects[0].id), (10.0, 10.0, 120.0, 80.0));
    assert_eq!(vm.undo, Some(UndoLabel::CreateRectangle));
}

/// E04: align left to the reference (first selected) in one entry; sizes unchanged.
#[test]
fn align_left_moves_to_the_reference_in_one_entry_and_keeps_sizes() {
    let (mut h, doc, ids) = with_rectangles(&[((30.0, 0.0), (50.0, 10.0)), ((0.0, 20.0), (5.0, 35.0))]);
    select(&mut h, doc, &[ids[0], ids[1]]);
    assert_eq!(view(&h, doc).align_reference, Some(ids[0]));
    let rev = view(&h, doc).revision;
    h.invoke(doc, CommandKey::AlignLeft);
    let vm = view(&h, doc);
    assert_eq!((vm.revision.raw(), vm.undo), (rev.raw() + 1, Some(UndoLabel::Edit(EditLabel::Align))));
    assert_eq!(vm.selection, vec![ids[0], ids[1]], "selection and reference stay");
    select(&mut h, doc, &[ids[1]]);
    assert_eq!(bounds_of(&h, doc, ids[1]), (30.0, 20.0, 5.0, 15.0));
}

#[test]
fn align_and_distribute_of_many_objects_is_one_entry() {
    let (mut h, doc, ids) =
        with_rectangles(&[((0.0, 0.0), (10.0, 10.0)), ((12.0, 3.0), (22.0, 13.0)), ((100.0, 7.0), (110.0, 17.0))]);
    select(&mut h, doc, &ids);
    let rev = view(&h, doc).revision;
    h.invoke(doc, CommandKey::AlignBottom);
    h.invoke(doc, CommandKey::DistributeHorizontal);
    assert_eq!(view(&h, doc).revision.raw(), rev.raw() + 2);
    select(&mut h, doc, &[ids[1]]);
    assert_eq!(bounds_of(&h, doc, ids[1]), (50.0, 0.0, 10.0, 10.0));
}

/// Command enablement follows the selection (menu validation, X02).
#[test]
fn arrange_and_boolean_commands_follow_the_selection() {
    let (mut h, doc, ids) =
        with_rectangles(&[((0.0, 0.0), (10.0, 10.0)), ((5.0, 5.0), (15.0, 15.0)), ((40.0, 0.0), (50.0, 10.0))]);
    let enabled = |h: &HeadlessEditorHarness, k| h.view().commands[&k].enabled;
    select(&mut h, doc, &[ids[0]]);
    assert!(!enabled(&h, CommandKey::AlignLeft) && !enabled(&h, CommandKey::BooleanUnion));
    select(&mut h, doc, &ids[..2]);
    assert!(enabled(&h, CommandKey::AlignLeft) && enabled(&h, CommandKey::BooleanUnion));
    assert!(!enabled(&h, CommandKey::DistributeHorizontal), "distribute needs three");
    select(&mut h, doc, &ids);
    assert!(enabled(&h, CommandKey::DistributeHorizontal));
    assert!(!enabled(&h, CommandKey::ApplyPreview), "nothing computed yet");

    let line = Path::line(Point::new(0.0, 0.0), Point::new(9.0, 9.0)).expect("line");
    let (mut h, doc, ids) = opened_with(vec![Path::rectangle(rect(0.0, 0.0, 10.0, 10.0)), line]);
    select(&mut h, doc, &ids);
    assert!(!enabled(&h, CommandKey::BooleanUnion), "an open line cannot take part in a boolean");
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> burnburn_geometry::Rect {
    use burnburn_types::Length;
    burnburn_geometry::Rect::new(Point::new(x, y), Length::from_mm(w).expect("w"), Length::from_mm(h).expect("h"))
        .expect("rect")
}

/// E05 / B4: the boolean result is a revision-checked preview; the document is untouched and a
/// result computed for an older revision is never shown.
#[test]
fn boolean_preview_is_revision_checked_and_never_changes_the_document() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (120.0, 80.0)), ((100.0, 50.0), (150.0, 100.0))]);
    select(&mut h, doc, &ids);
    let before = view(&h, doc);
    h.invoke(doc, CommandKey::BooleanUnion);
    assert_eq!(view(&h, doc).tasks.len(), 1, "computed off the UI thread");
    h.run_tasks();
    let vm = view(&h, doc);
    let preview = vm.boolean_preview.clone().expect("preview");
    assert!((preview.result.signed_area().abs() - 11500.0).abs() < 1e-6);
    assert_eq!((vm.revision, vm.undo, vm.objects.len()), (before.revision, before.undo, 2), "document untouched");
    assert!(h.scene(doc).expect("scene").overlays.iter().any(|o| matches!(o, Overlay::ResultPreview { .. })));
    assert!(h.view().commands[&CommandKey::ApplyPreview].enabled);

    // A preview requested before an edit is never shown for the newer revision.
    h.invoke(doc, CommandKey::BooleanIntersection);
    let stale = h.take_task_requests();
    h.invoke(doc, CommandKey::AlignLeft); // an edit while the worker runs
    assert_ne!(view(&h, doc).revision, before.revision);
    for r in stale {
        h.complete_despite_cancel(r);
    }
    let vm = view(&h, doc);
    assert_eq!(vm.boolean_preview, None, "old result dropped");
    assert!(!h.view().commands[&CommandKey::ApplyPreview].enabled);
}

#[test]
fn applying_a_boolean_is_one_undo_entry() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (20.0, 20.0)), ((10.0, 10.0), (30.0, 30.0))]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::BooleanUnion);
    h.run_tasks();
    h.invoke(doc, CommandKey::ApplyPreview);
    assert_eq!(view(&h, doc).objects.len(), 1);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).objects.iter().map(|o| o.id).collect::<Vec<_>>(), ids);
}

/// E09 / B5: open contours and self-intersections are located overlays with physical size.
#[test]
fn issues_overlay_marks_open_contour_and_self_intersection() {
    let gap = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 0.0))
        .line_to(Point::new(10.0, 10.0))
        .line_to(Point::new(0.0, 0.08))
        .build()
        .expect("open triangle");
    let bow = Path::polyline(
        &[Point::new(20.0, 0.0), Point::new(30.0, 10.0), Point::new(30.0, 0.0), Point::new(20.0, 10.0)],
        true,
    )
    .expect("bow tie");
    let (h, doc, ids) = opened_with(vec![gap, bow]);
    let vm = view(&h, doc);
    let kinds: Vec<_> = vm.issues.iter().map(|i| (i.kind, i.objects.clone())).collect();
    assert_eq!(kinds, vec![(IssueKind::OpenContour, vec![ids[0]]), (IssueKind::SelfIntersection, vec![ids[1]])]);
    assert!((vm.issues[0].size_mm.expect("gap") - 0.08).abs() < 1e-12);
    let overlays = h.scene(doc).expect("scene").overlays.clone();
    assert!(overlays.contains(&Overlay::Issue { at: Point::new(25.0, 5.0), kind: IssueKind::SelfIntersection }));
}

#[test]
fn ellipse_circle_line_polyline_and_pen_each_create_one_object_with_one_undo() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolEllipse);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolCircle);
    h.drag(doc, (60.0, 10.0), (62.0, 10.0));
    h.invoke(doc, CommandKey::ToolLine);
    h.drag(doc, (0.0, 30.0), (40.0, 30.0));
    h.invoke(doc, CommandKey::ToolPolyline);
    for p in [(0.0, 40.0), (10.0, 40.0), (10.0, 50.0)] {
        h.click(doc, p, none());
    }
    h.key(doc, burnburn_editor::CanvasKey::Return);
    h.invoke(doc, CommandKey::ToolPen);
    h.click(doc, (20.0, 40.0), none());
    h.drag(doc, (30.0, 40.0), (35.0, 45.0));
    h.key(doc, burnburn_editor::CanvasKey::Return);
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 5);
    let names: Vec<_> = vm.objects.iter().map(|o| o.label.clone()).collect();
    assert_eq!(names, vec!["Ellips 1", "Cirkel 1", "Lijn 1", "Polylijn 1", "Curve 1"]);
    for _ in 0..5 {
        h.invoke(doc, CommandKey::Undo);
    }
    assert!(view(&h, doc).objects.is_empty());
}

/// E02 / B3: dragging one Bézier handle previews a change of only that span; Esc restores.
#[test]
fn node_drag_previews_only_the_dragged_span_and_escape_restores() {
    let wave = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .cubic_to(Point::new(10.0, 10.0), Point::new(20.0, 10.0), Point::new(30.0, 0.0))
        .cubic_to(Point::new(40.0, -10.0), Point::new(50.0, -10.0), Point::new(60.0, 0.0))
        .cubic_to(Point::new(70.0, 10.0), Point::new(80.0, 10.0), Point::new(90.0, 0.0))
        .build()
        .expect("wave");
    let (mut h, doc, ids) = opened_with(vec![wave.clone()]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::ToolNode);
    assert_eq!(view(&h, doc).tool, ToolKind::Node);
    let nodes = h.scene(doc).expect("scene").overlays.iter().filter(|o| matches!(o, Overlay::Node { .. })).count();
    assert_eq!(nodes, 10, "4 anchors + 6 handles");
    h.begin_drag(doc, (40.0, -10.0), (44.0, -20.0));
    let Some(ToolPreview::Node { path, .. }) = view(&h, doc).preview else { panic!("node preview") };
    let (old, new) = (&wave.subpaths()[0], &path.subpaths()[0]);
    assert_eq!((new.segments()[0], new.segments()[2]), (old.segments()[0], old.segments()[2]), "other spans identical");
    assert_ne!(new.segments()[1], old.segments()[1]);
    h.escape(doc);
    assert_eq!(view(&h, doc).preview, None);
    assert_eq!(view(&h, doc).undo, None);
}

#[test]
fn node_edit_is_one_undo_entry_and_other_spans_stay_identical() {
    let wave = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .cubic_to(Point::new(10.0, 10.0), Point::new(20.0, 10.0), Point::new(30.0, 0.0))
        .cubic_to(Point::new(40.0, -10.0), Point::new(50.0, -10.0), Point::new(60.0, 0.0))
        .build()
        .expect("wave");
    let (mut h, doc, ids) = opened_with(vec![wave.clone()]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::ToolNode);
    h.drag(doc, (40.0, -10.0), (44.0, -20.0));
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::EditNodes)));
    let stored = |h: &HeadlessEditorHarness| h.scene(doc).expect("scene").items[0].path.subpaths()[0].clone();
    let (old, new) = (&wave.subpaths()[0], stored(&h));
    assert_eq!(new.segments()[0], old.segments()[0], "span 0 bit-identical in the document");
    assert_ne!(new.segments()[1], old.segments()[1]);
    assert_eq!(vm.objects[0].id, ids[0], "same object identity");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).undo, None);
    assert_eq!(&stored(&h), old, "one undo restores the outline");
}

/// GUI request (gui.md fase C #5): the inspector's rotation field takes raw text in degrees and
/// turns the selection about its bounds centre in one command; 0 or invalid text sends nothing.
#[test]
fn rotation_field_turns_the_selection_about_its_centre_in_one_command() {
    let (mut h, doc, ids) = with_rectangles(&[((0.0, 0.0), (40.0, 20.0))]);
    select(&mut h, doc, &[ids[0]]);
    let field = view(&h, doc).inspector.and_then(|i| i.field(FieldKey::Rotation).cloned()).expect("rotation field");
    assert_eq!((field.text.as_str(), field.editable), ("0", true));
    let rev = view(&h, doc).revision;
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Rotation, text: "90".into() });
    let (x, y, w, hgt) = bounds_of(&h, doc, ids[0]);
    for (got, want) in [(x, 10.0), (y, -10.0), (w, 20.0), (hgt, 40.0)] {
        assert!((got - want).abs() < 1e-9, "{:?}", (x, y, w, hgt));
    }
    let vm = view(&h, doc);
    assert_eq!((vm.revision.raw(), vm.undo), (rev.raw() + 1, Some(UndoLabel::Transform)));
    for text in ["0", "0°", "abc", ""] {
        h.send(UiIntent::CommitField { document: doc, field: FieldKey::Rotation, text: text.into() });
        assert_eq!(view(&h, doc).revision, vm.revision, "{text:?} sends no command");
    }
    let error = view(&h, doc).inspector.and_then(|i| i.field(FieldKey::Rotation).cloned()).and_then(|f| f.error);
    assert!(error.is_some(), "invalid text shows a field error");
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Rotation, text: "-90,0°".into() });
    let (x, y, w, hgt) = bounds_of(&h, doc, ids[0]);
    for (got, want) in [(x, 0.0), (y, 0.0), (w, 40.0), (hgt, 20.0)] {
        assert!((got - want).abs() < 1e-9, "{:?}", (x, y, w, hgt));
    }
}
