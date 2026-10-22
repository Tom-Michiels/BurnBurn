//! G9-editor S01 in the window: a Bézier end extended past three parameter lengths follows its own
//! cubic where that is stable, and where it has to go straight the status line says why. One undo
//! step either way.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{DocumentSnapshot, EditLabel, GeometryObject, GeometrySource, ObjectKind, UndoLabel};
use burnburn_editor::Modifiers;
use burnburn_geometry::{ExtendNote, Path, PathBuilder, Point};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, MessageKey, UiIntent};
use burnburn_types::{DocumentId, GeometryId, StableId};

type H = HeadlessEditorHarness;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn opened(paths: Vec<Path>) -> (H, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let objects: Vec<_> = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let id = GeometryId::from_raw(i as u64 + 1);
            GeometryObject::new(id, format!("Pad {}", i + 1), ObjectKind::Path, GeometrySource::Drawn, path)
        })
        .collect();
    let next = objects.len() as u64 + 1;
    let snapshot = DocumentSnapshot::from_parts(objects, next).expect("snapshot");
    let file = PathBuf::from("/work/g9.burnburn");
    save_project(h.fs(), &file, &Project::new(ProjectUuid::from_bytes([9; 16]), snapshot)).expect("fixture");
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
    (h, doc)
}

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn extend_at(h: &mut H, doc: DocumentId, at: (f64, f64)) {
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.extend".to_owned() });
    h.click(doc, at, Modifiers::default());
}

fn said(h: &H, doc: DocumentId) -> Vec<MessageKey> {
    vm(h, doc).messages.iter().map(|m| m.key).collect()
}

/// The parabola `y = x^2 / 10` from the origin to (10, 10).
fn parabola() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(10.0 / 3.0, 0.0), p(20.0 / 3.0, 10.0 / 3.0), p(10.0, 10.0))
        .build()
        .unwrap()
}

/// A cubic that curls round to the left and back beyond its end.
fn swing() -> Path {
    PathBuilder::new().move_to(p(0.0, 0.0)).cubic_to(p(5.0, 0.0), p(10.0, 5.0), p(10.0, 10.0)).build().unwrap()
}

#[test]
fn a_stable_bezier_goes_on_along_its_curve_to_a_far_wall_and_says_nothing() {
    let wall = Path::line(p(100.0, -5000.0), p(100.0, 5000.0)).unwrap();
    let (mut h, doc) = opened(vec![parabola(), wall]);
    extend_at(&mut h, doc, (9.9, 9.9));
    let end = h.scene(doc).unwrap().items[0].path.subpaths()[0].end();
    assert!(end.distance(p(100.0, 1000.0)) < 1e-6, "9 parameter lengths along y = x^2 / 10: {end:?}");
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::Extend)));
    assert!(!said(&h, doc).iter().any(|m| matches!(m, MessageKey::ExtendedStraight(_))), "{:?}", said(&h, doc));
}

#[test]
fn a_bezier_that_has_to_go_straight_says_why_and_it_is_one_undo_step() {
    let wall = Path::line(p(-400.0, -1000.0), p(-400.0, 1000.0)).unwrap();
    let ceiling = Path::line(p(-600.0, 50.0), p(600.0, 50.0)).unwrap();
    let (mut h, doc) = opened(vec![swing(), wall, ceiling]);
    extend_at(&mut h, doc, (9.9, 9.9));
    let end = h.scene(doc).unwrap().items[0].path.subpaths()[0].end();
    assert!(end.distance(p(10.0, 50.0)) < 1e-6, "straight up to the ceiling: {end:?}");
    assert!(said(&h, doc).contains(&MessageKey::ExtendedStraight(ExtendNote::StraightTurns)), "{:?}", said(&h, doc));
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::Extend)));
    h.invoke(doc, CommandKey::Undo);
    let end = h.scene(doc).unwrap().items[0].path.subpaths()[0].end();
    assert!(end.distance(p(10.0, 10.0)) < 1e-9);
}
