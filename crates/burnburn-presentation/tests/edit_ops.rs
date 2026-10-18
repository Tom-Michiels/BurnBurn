//! B4/B5 (E05, E09) in the UI: offset panel, trim mode and gap repair per chosen problem. Each is
//! one document command and one undo entry; a failure names its place and changes nothing.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{DocumentSnapshot, EditLabel, GeometryObject, GeometrySource, ObjectKind, UndoLabel};
use burnburn_editor::{IssueKind, Modifiers};
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, FieldError, HeadlessEditorHarness, HostEffect, MessageKey, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, StableId};

fn view(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

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
    let ids = view(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, ids)
}

fn square(x: f64, y: f64, size: f64) -> Path {
    Path::polyline(
        &[Point::new(x, y), Point::new(x + size, y), Point::new(x + size, y + size), Point::new(x, y + size)],
        true,
    )
    .unwrap()
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

/// Offset panel: "2 mm" outward around the selection is one new object and one undo entry.
#[test]
fn offset_panel_adds_one_offset_object_in_one_entry() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 40.0)]);
    select(&mut h, doc, &ids);
    h.send(UiIntent::Offset { document: doc, distance: "2 mm".into() });
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 2, "source kept, one offset object");
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Offset)));
    let inspector = vm.inspector.expect("new object selected");
    assert_eq!(inspector.field(burnburn_presentation::FieldKey::Width).unwrap().text, "44");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).objects.len(), 1);
}

/// B4 error path: an inward offset that removes a whole object names that object and its place;
/// the document does not change.
#[test]
fn impossible_offset_reports_location_and_keeps_document() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 100.0), square(200.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    let before = view(&h, doc).revision;
    let update = h.send(UiIntent::Offset { document: doc, distance: "-6 mm".into() });
    assert!(
        update.effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::OffsetImpossible, .. }))
    );
    let vm = view(&h, doc);
    assert_eq!(vm.revision, before, "document unchanged");
    let problem = vm.problem.expect("located problem");
    assert_eq!((problem.message, problem.object), (MessageKey::OffsetImpossible, Some(ids[1])));
    assert_eq!(problem.at, Point::new(205.0, 5.0), "centre of the small square");

    // Invalid text is a field error, no command.
    h.send(UiIntent::Offset { document: doc, distance: "abc".into() });
    assert_eq!(view(&h, doc).revision, before);
    assert_eq!(view(&h, doc).problem.map(|p| p.message), Some(MessageKey::InvalidLength(FieldError::NotANumber)));
}

/// B5: repair one chosen problem with its own tolerance. The seam (0,05 mm) closes; the intended
/// slot (0,15 mm) stays open, also when it is chosen with the same tolerance. Undo restores.
#[test]
fn repair_closes_chosen_seam_but_keeps_intended_slot() {
    let open = |x: f64, gap: f64| {
        PathBuilder::new()
            .move_to(Point::new(x, 0.0))
            .line_to(Point::new(x + 10.0, 0.0))
            .line_to(Point::new(x + 10.0, 10.0))
            .line_to(Point::new(x, gap))
            .build()
            .unwrap()
    };
    let (mut h, doc, ids) = opened_with(vec![open(0.0, 0.05), open(20.0, 0.15)]);
    let issues = view(&h, doc).issues;
    assert_eq!(issues.iter().filter(|i| i.kind == IssueKind::OpenContour).count(), 2);
    let seam = issues.iter().position(|i| i.objects == vec![ids[0]]).unwrap();

    h.send(UiIntent::RepairIssue { document: doc, issue: seam, tolerance: "0,1 mm".into() });
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::CloseGaps)));
    assert_eq!(
        vm.issues.iter().map(|i| i.objects.clone()).collect::<Vec<_>>(),
        vec![vec![ids[1]]],
        "only the slot remains"
    );
    let revision = vm.revision;

    let slot_now = vm.issues.iter().position(|i| i.objects == vec![ids[1]]).unwrap();
    h.send(UiIntent::RepairIssue { document: doc, issue: slot_now, tolerance: "0,1 mm".into() });
    let vm = view(&h, doc);
    assert_eq!(vm.revision, revision, "a gap above the tolerance is left alone; no history entry");
    assert_eq!(vm.problem.map(|p| p.message), Some(MessageKey::NothingToRepair));

    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).issues.len(), 2, "undo restores the seam");
}

/// E05 trim: in trim mode a click on a line removes the piece up to the nearest crossing (one entry).
#[test]
fn trim_mode_click_removes_the_picked_piece_up_to_the_crossing() {
    let line =
        |a: (f64, f64), b: (f64, f64)| Path::polyline(&[Point::new(a.0, a.1), Point::new(b.0, b.1)], false).unwrap();
    let (mut h, doc, ids) = opened_with(vec![line((0.0, 0.0), (40.0, 0.0)), line((20.0, -10.0), (20.0, 10.0))]);
    h.invoke(doc, CommandKey::ToolTrim);
    assert!(h.view().commands[&CommandKey::ToolTrim].checked);
    h.click(doc, (35.0, 0.0), Modifiers::default());
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Trim)));
    select(&mut h, doc, &ids[..1]);
    let w = view(&h, doc).inspector.unwrap().field(burnburn_presentation::FieldKey::Width).unwrap().text.clone();
    assert_eq!(w, "20", "the piece right of the crossing is gone");
    // Esc leaves trim mode without changes.
    let revision = view(&h, doc).revision;
    h.escape(doc);
    assert!(!h.view().commands[&CommandKey::ToolTrim].checked);
    assert_eq!(view(&h, doc).revision, revision);
}
