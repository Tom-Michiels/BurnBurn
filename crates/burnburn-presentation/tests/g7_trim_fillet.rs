//! G7-editor E05 in the window: the fillet and chamfer sizes are options of the tool (a row per
//! setting, refused input named, kept across tool switches), extend continues a Bézier end along
//! its own curve, trim cuts at a curved boundary, and every action is exactly one undo entry.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::f64::consts::PI;
use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{DocumentSnapshot, EditLabel, GeometryObject, GeometrySource, ObjectKind, UndoLabel};
use burnburn_editor::Modifiers;
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, ParamKey, UiIntent};
use burnburn_types::{DocumentId, GeometryId, StableId};

type H = HeadlessEditorHarness;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn square(size: f64) -> Path {
    Path::polyline(&[p(0.0, 0.0), p(size, 0.0), p(size, size), p(0.0, size)], true).unwrap()
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
    let file = PathBuf::from("/work/g7.burnburn");
    save_project(h.fs(), &file, &Project::new(ProjectUuid::from_bytes([7; 16]), snapshot)).expect("fixture");
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

fn choose(h: &mut H, doc: DocumentId, id: &str) {
    h.send(UiIntent::SelectTool { document: Some(doc), id: id.to_owned() });
}

fn option(h: &mut H, doc: DocumentId, key: ParamKey, text: &str) {
    h.send(UiIntent::SetToolOption { document: doc, key, text: text.to_owned() });
}

fn row(h: &H, doc: DocumentId, key: ParamKey) -> Option<(String, bool)> {
    vm(h, doc).tool_options.iter().find(|r| r.key == key).map(|r| (r.text.clone(), r.error.is_some()))
}

fn area(h: &H, doc: DocumentId, index: usize) -> f64 {
    h.scene(doc).expect("scene").items[index].path.signed_area()
}

#[test]
fn the_fillet_tool_offers_its_radius_and_a_click_uses_it_in_one_undo_step() {
    let (mut h, doc) = opened(vec![square(20.0)]);
    choose(&mut h, doc, "sketch.fillet");
    assert_eq!(row(&h, doc, ParamKey::FilletRadius), Some(("2".to_owned(), false)), "default radius is offered");
    option(&mut h, doc, ParamKey::FilletRadius, "5");
    assert_eq!(row(&h, doc, ParamKey::FilletRadius), Some(("5".to_owned(), false)));
    let before = area(&h, doc, 0);
    h.click(doc, (0.3, 0.3), Modifiers::default());
    let cut = 25.0 * (1.0 - PI / 4.0); // a right-angle corner loses r² (1 - π/4)
    assert!((area(&h, doc, 0) - (before - cut)).abs() < 1e-9, "{} vs {}", area(&h, doc, 0), before - cut);
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::CutCorner)));
    h.invoke(doc, CommandKey::Undo);
    assert!((area(&h, doc, 0) - before).abs() < 1e-9, "one undo restores the corner");
    assert_ne!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::CutCorner)), "and it was the only step");
}

#[test]
fn a_refused_radius_is_named_and_the_old_one_stays_in_force() {
    let (mut h, doc) = opened(vec![square(20.0)]);
    choose(&mut h, doc, "sketch.fillet");
    option(&mut h, doc, ParamKey::FilletRadius, "4");
    option(&mut h, doc, ParamKey::FilletRadius, "-1");
    assert_eq!(row(&h, doc, ParamKey::FilletRadius).map(|r| r.1), Some(true), "the row carries the reason");
    option(&mut h, doc, ParamKey::FilletRadius, "abc");
    assert_eq!(row(&h, doc, ParamKey::FilletRadius).map(|r| r.1), Some(true));
    let before = area(&h, doc, 0);
    h.click(doc, (0.3, 0.3), Modifiers::default());
    assert!(
        (before - area(&h, doc, 0) - 16.0 * (1.0 - PI / 4.0)).abs() < 1e-9,
        "radius 4 was used, not the refused input"
    );
}

#[test]
fn the_radius_survives_a_tool_switch() {
    let (mut h, doc) = opened(vec![square(20.0)]);
    choose(&mut h, doc, "sketch.fillet");
    option(&mut h, doc, ParamKey::FilletRadius, "3");
    choose(&mut h, doc, "sketch.trim");
    assert!(row(&h, doc, ParamKey::FilletRadius).is_none(), "other tools show no fillet row");
    choose(&mut h, doc, "sketch.fillet");
    assert_eq!(row(&h, doc, ParamKey::FilletRadius), Some(("3".to_owned(), false)));
    let before = area(&h, doc, 0);
    h.click(doc, (0.3, 0.3), Modifiers::default());
    assert!((before - area(&h, doc, 0) - 9.0 * (1.0 - PI / 4.0)).abs() < 1e-9);
}

#[test]
fn the_chamfer_tools_offer_their_distances() {
    let (mut h, doc) = opened(vec![square(20.0)]);
    choose(&mut h, doc, "sketch.chamfer.equal_distance");
    assert!(row(&h, doc, ParamKey::ChamferDistance).is_some());
    assert!(row(&h, doc, ParamKey::ChamferDistance2).is_none(), "equal chamfer has one distance");
    option(&mut h, doc, ParamKey::ChamferDistance, "4");
    let before = area(&h, doc, 0);
    h.click(doc, (0.3, 0.3), Modifiers::default());
    assert!((before - area(&h, doc, 0) - 8.0).abs() < 1e-9, "a 4 x 4 triangle is cut off");

    choose(&mut h, doc, "sketch.chamfer.two_distance");
    assert!(row(&h, doc, ParamKey::ChamferDistance2).is_some());
    choose(&mut h, doc, "sketch.chamfer.distance_angle");
    assert!(row(&h, doc, ParamKey::ChamferAngle).is_some());
}

fn parabola() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(10.0 / 3.0, 0.0), p(20.0 / 3.0, 10.0 / 3.0), p(10.0, 10.0))
        .build()
        .unwrap()
}

#[test]
fn extending_a_bezier_end_is_one_undo_step() {
    let wall = Path::line(p(15.0, -5.0), p(15.0, 40.0)).unwrap();
    let (mut h, doc) = opened(vec![parabola(), wall]);
    choose(&mut h, doc, "sketch.extend");
    h.click(doc, (9.9, 9.9), Modifiers::default());
    let end = h.scene(doc).unwrap().items[0].path.subpaths()[0].end();
    assert!(end.distance(p(15.0, 22.5)) < 1e-6, "{end:?}");
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Edit(EditLabel::Extend)));
    h.invoke(doc, CommandKey::Undo);
    let end = h.scene(doc).unwrap().items[0].path.subpaths()[0].end();
    assert!(end.distance(p(10.0, 10.0)) < 1e-9);
}

#[test]
fn trimming_a_curve_at_a_circle_is_one_undo_step() {
    let ring = Path::circle(p(10.0, 0.0), 6.0).unwrap();
    let (mut h, doc) = opened(vec![parabola(), ring]);
    choose(&mut h, doc, "sketch.trim");
    h.click(doc, (2.0, 0.4), Modifiers::default());
    let start = h.scene(doc).unwrap().items[0].path.subpaths()[0].start();
    assert!(start.x > 4.0 && start.x < 4.5, "{start:?}");
    assert!((start.distance(p(10.0, 0.0)) - 6.0).abs() < 1e-6);
    h.invoke(doc, CommandKey::Undo);
    let start = h.scene(doc).unwrap().items[0].path.subpaths()[0].start();
    assert!(start.distance(p(0.0, 0.0)) < 1e-9);
}
