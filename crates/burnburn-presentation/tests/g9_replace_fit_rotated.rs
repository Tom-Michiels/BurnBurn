//! G9-trace: Replace to Fit for a frame that was turned. The frame stays a rectangle with its angle; the
//! picture's own x axis runs along the edge nearest the sheet's x axis (the burn covers the frame's
//! bounds, so that is the edge the picture's width must follow), and the frame takes the shape of the new
//! picture inside the old frame, about its middle. A frame that is not a rectangle is still refused.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;
mod g7_support;

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::process::OperationMode;
use burnburn_geometry::{Point, Segment};
use burnburn_io::fs::FileSystem;
use burnburn_io::{Project, load_project};
use burnburn_presentation::{CommandKey, FieldKey, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_types::DocumentId;
use common::*;
use g7_support::png_of_dpi;

fn put(h: &HeadlessEditorHarness, name: &str, bytes: &[u8]) -> PathBuf {
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let path = PathBuf::from(format!("/in/{name}"));
    let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, bytes).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    path
}

/// A gray picture of `w × h` pixels at 25.4 dpi (1 mm per pixel).
fn picture(w: u32, h: u32) -> Vec<u8> {
    let gray: Vec<u8> = (0..h).flat_map(|_| (0..w).map(move |x| if x < w / 2 { 0 } else { 255 })).collect();
    png_of_dpi(&gray, w, h, 1000)
}

fn window_with(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    let path = put(&h, "photo.png", bytes);
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    h.invoke(doc, CommandKey::CommitImport);
    let frame = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![frame], mode: SelectMode::Replace });
    (h, doc)
}

fn rotate(h: &mut HeadlessEditorHarness, doc: DocumentId, degrees: &str) {
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Rotation, text: degrees.into() });
}

fn saved(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Project {
    let path = PathBuf::from("/out/p.burnburn");
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    load_project(h.fs(), &path).expect("saved project")
}

/// The four corners of the frame of the one image operation, in path order.
fn corners(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<Point> {
    let p = saved(h, doc);
    let op = p.document.process().operations().find(|op| matches!(op.mode(), OperationMode::Image(_))).unwrap();
    let path = p.document.object(op.targets()[0]).unwrap().path().clone();
    let sub = &path.subpaths()[0];
    let mut pts = vec![sub.start()];
    for s in sub.segments() {
        if let Segment::Line { to } = s {
            pts.push(*to);
        }
    }
    if pts.len() == 5 && pts[4].distance(pts[0]) < 1e-9 {
        pts.pop();
    }
    assert_eq!(pts.len(), 4, "a rectangle: {pts:?}");
    pts
}

fn replace_to_fit(h: &mut HeadlessEditorHarness, doc: DocumentId, bytes: &[u8]) {
    let path = put(h, "wide.png", bytes);
    h.send(UiIntent::ReplaceImageToFit { document: doc, path });
    h.run_tasks();
}

fn centre(c: &[Point]) -> Point {
    Point::new((c[0].x + c[2].x) / 2.0, (c[0].y + c[2].y) / 2.0)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_frame_turned_by_30_degrees_keeps_its_angle_and_takes_the_shape_of_the_picture() {
    let (mut h, doc) = window_with(&picture(8, 8)); // an 8 x 8 mm frame
    rotate(&mut h, doc, "30");
    let before = corners(&mut h, doc);
    let undo_before = h.document(doc).unwrap().undo;

    replace_to_fit(&mut h, doc, &picture(12, 6)); // 2 : 1
    let after = corners(&mut h, doc);
    // The edge nearest the sheet's x axis is the picture's width: 8 mm, and the other edge 4 mm.
    let edges: Vec<(f64, f64)> =
        (0..4).map(|i| (after[(i + 1) % 4].x - after[i].x, after[(i + 1) % 4].y - after[i].y)).collect();
    let (wide, high) = if edges[0].0.abs() >= edges[1].0.abs() { (edges[0], edges[1]) } else { (edges[1], edges[0]) };
    assert!(close(wide.0.hypot(wide.1), 8.0) && close(high.0.hypot(high.1), 4.0), "{wide:?} {high:?}");
    assert!(close(wide.0 * high.0 + wide.1 * high.1, 0.0), "still a rectangle");
    // The angle is the one it had (30 degrees, modulo the half turn), and the middle did not move.
    let angle = wide.1.atan2(wide.0).to_degrees();
    assert!(close((angle + 180.0) % 180.0, 30.0), "angle {angle}");
    let (c0, c1) = (centre(&before), centre(&after));
    assert!(close(c0.x, c1.x) && close(c0.y, c1.y), "same middle: {c1:?} against {c0:?}");
    assert_eq!(
        h.document(doc).unwrap().undo,
        Some(burnburn_document::UndoLabel::Compound(burnburn_document::CompoundLabel::ReplaceToFit))
    );
    assert_ne!(h.document(doc).unwrap().undo, undo_before);

    // One undo brings the old frame back, corner for corner.
    h.invoke(doc, CommandKey::Undo);
    let back = corners(&mut h, doc);
    assert!(back.iter().zip(&before).all(|(a, b)| a.distance(*b) < 1e-9));
}

#[test]
fn a_frame_turned_a_quarter_follows_the_edge_that_lies_along_the_sheet_x_axis() {
    let (mut h, doc) = window_with(&picture(8, 8));
    rotate(&mut h, doc, "90");
    replace_to_fit(&mut h, doc, &picture(12, 6));
    let after = corners(&mut h, doc);
    let xs: Vec<f64> = after.iter().map(|p| p.x).collect();
    let ys: Vec<f64> = after.iter().map(|p| p.y).collect();
    let span = |v: &[f64]| {
        v.iter().copied().fold(f64::NEG_INFINITY, f64::max) - v.iter().copied().fold(f64::INFINITY, f64::min)
    };
    // Exactly as an unturned frame: 8 mm along x, 4 mm along y.
    assert!(close(span(&xs), 8.0) && close(span(&ys), 4.0), "{} x {}", span(&xs), span(&ys));
}

#[test]
fn a_frame_that_is_not_a_rectangle_is_still_refused_and_nothing_changes() {
    let (mut h, doc) = window_with(&picture(8, 8));
    // Skew the frame (not a rotation or a scale): a parallelogram has no "width" and "height" to fit.
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::ShearX, text: "20".into() });
    let before = h.document(doc).unwrap().revision;
    let shape = corners(&mut h, doc);
    replace_to_fit(&mut h, doc, &picture(12, 6));
    assert_eq!(h.document(doc).unwrap().revision, before, "the fit of a skewed frame is not guessed");
    assert_eq!(corners(&mut h, doc), shape);
    assert!(
        h.document(doc).unwrap().messages.iter().any(|m| m.key == burnburn_presentation::MessageKey::CommandRejected)
    );
}
