//! G7-trace, T09: Replace to Fit. The new picture goes into the frame of the old one the way it
//! fits: same aspect ratio as its own pixels, as large as the old frame allows, in the middle of it.
//! Settings stay, the frame is resized with the picture in one undo step. Replace (as before) keeps
//! the frame as it is.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;
mod g7_support;

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::process::{ImageSettings, OperationMode};
use burnburn_geometry::Bounds;
use burnburn_io::fs::FileSystem;
use burnburn_io::{Project, load_project};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};
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

/// A gray picture of `w × h` pixels at 25.4 dpi (1 mm per pixel), left half black.
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

fn saved(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Project {
    let path = PathBuf::from("/out/p.burnburn");
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    load_project(h.fs(), &path).expect("saved project")
}

/// (settings, frame bounds) of the one image operation.
fn image(h: &mut HeadlessEditorHarness, doc: DocumentId) -> (ImageSettings, Bounds) {
    let p = saved(h, doc);
    let op = p.document.process().operations().find(|op| matches!(op.mode(), OperationMode::Image(_))).unwrap();
    let OperationMode::Image(settings) = op.mode() else { panic!("image") };
    (settings, p.document.object(op.targets()[0]).unwrap().bounds().unwrap())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn replace_to_fit(h: &mut HeadlessEditorHarness, doc: DocumentId, name: &str, bytes: &[u8]) {
    let path = put(h, name, bytes);
    h.send(UiIntent::ReplaceImageToFit { document: doc, path });
    h.run_tasks();
}

#[test]
fn a_wider_picture_fills_the_width_of_the_old_frame_and_sits_in_the_middle() {
    let (mut h, doc) = window_with(&picture(8, 8)); // an 8 × 8 mm frame
    let (old_settings, old) = image(&mut h, doc);
    assert!(close(old.width(), 8.0) && close(old.height(), 8.0));
    let undo_before = h.document(doc).unwrap().undo;

    replace_to_fit(&mut h, doc, "wide.png", &picture(12, 6)); // 2 : 1
    let (settings, now) = image(&mut h, doc);
    assert_ne!(settings.source(), old_settings.source(), "new pixels");
    assert_eq!(
        (settings.dot_pitch(), settings.line_interval()),
        (old_settings.dot_pitch(), old_settings.line_interval())
    );
    assert!(close(now.width(), 8.0) && close(now.height(), 4.0), "8 × 4 mm: {} × {}", now.width(), now.height());
    let (cx, cy) = ((now.min().x + now.max().x) / 2.0, (now.min().y + now.max().y) / 2.0);
    let (ox, oy) = ((old.min().x + old.max().x) / 2.0, (old.min().y + old.max().y) / 2.0);
    assert!(close(cx, ox) && close(cy, oy), "same middle: ({cx}, {cy}) against ({ox}, {oy})");
    assert_ne!(h.document(doc).unwrap().undo, undo_before, "one new entry");
    assert_eq!(
        h.document(doc).unwrap().undo,
        Some(burnburn_document::UndoLabel::Compound(burnburn_document::CompoundLabel::ReplaceToFit))
    );

    // One undo brings back both the pixels and the frame.
    h.invoke(doc, CommandKey::Undo);
    let (settings, back) = image(&mut h, doc);
    assert_eq!(settings.source(), old_settings.source());
    assert!(close(back.width(), 8.0) && close(back.height(), 8.0) && close(back.min().x, old.min().x));
    h.invoke(doc, CommandKey::Redo);
    let (_, again) = image(&mut h, doc);
    assert!(close(again.width(), 8.0) && close(again.height(), 4.0));
}

#[test]
fn a_taller_picture_fills_the_height_and_the_physical_size_of_the_file_does_not_matter() {
    let (mut h, doc) = window_with(&picture(8, 8));
    let (_, old) = image(&mut h, doc);
    // 6 × 12 px at 254 dpi would be 0.6 × 1.2 mm; it still fills the 8 mm tall frame.
    let gray = vec![0u8; 72];
    replace_to_fit(&mut h, doc, "tall.png", &png_of_dpi(&gray, 6, 12, 10_000));
    let (_, now) = image(&mut h, doc);
    assert!(close(now.width(), 4.0) && close(now.height(), 8.0), "{} × {}", now.width(), now.height());
    assert!(close(now.min().x, old.min().x + 2.0) && close(now.min().y, old.min().y));
}

#[test]
fn a_picture_with_the_same_aspect_ratio_leaves_the_frame_exactly_as_it_is() {
    let (mut h, doc) = window_with(&picture(8, 8));
    let (_, old) = image(&mut h, doc);
    replace_to_fit(&mut h, doc, "square.png", &picture(20, 20));
    let (_, now) = image(&mut h, doc);
    assert_eq!(now, old);
}

#[test]
fn plain_replace_keeps_the_frame_as_it_is_for_a_picture_of_another_shape() {
    let (mut h, doc) = window_with(&picture(8, 8));
    let (_, old) = image(&mut h, doc);
    let path = put(&h, "wide.png", &picture(12, 6));
    h.send(UiIntent::ReplaceImage { document: doc, path });
    h.run_tasks();
    let (_, now) = image(&mut h, doc);
    assert_eq!(now, old, "Replace is not Replace to Fit");
}

#[test]
fn the_command_needs_a_selected_image_and_without_one_nothing_happens() {
    let (mut h, doc) = window_with(&picture(8, 8));
    assert!(h.view().commands[&CommandKey::ReplaceImageToFit].enabled);
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(!h.view().commands[&CommandKey::ReplaceImageToFit].enabled, "nothing selected");
    let before = h.document(doc).unwrap().revision;
    replace_to_fit(&mut h, doc, "wide.png", &picture(12, 6));
    assert_eq!(h.document(doc).unwrap().revision, before, "no image selected: nothing happens");
}

#[test]
fn a_file_that_cannot_be_read_changes_nothing() {
    let (mut h, doc) = window_with(&picture(8, 8));
    let (settings, old) = image(&mut h, doc);
    let before = h.document(doc).unwrap().revision;
    replace_to_fit(&mut h, doc, "broken.png", b"not a picture at all");
    assert_eq!(h.document(doc).unwrap().revision, before);
    let (after, bounds) = image(&mut h, doc);
    assert_eq!((after.source(), bounds), (settings.source(), old));
}
