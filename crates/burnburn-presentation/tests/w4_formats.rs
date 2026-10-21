//! W4-UI-gaten (P03): BMP and TIFF through the bitmap import sheet and the tracer, with their own
//! resolution proposed; the original bytes travel with the project under their own media type.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_io::load_project;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_types::DocumentId;

use common::*;

/// A bottom-up 24-bit BMP of `size` × `size` pixels, left half black, right half white, with a
/// resolution of 1000 pixels per metre (25.4 dpi = 1 mm per pixel).
fn bmp(size: u32) -> Vec<u8> {
    let row_bytes = (size * 3).div_ceil(4) * 4;
    let mut data = Vec::new();
    for _ in 0..size {
        let mut row = Vec::new();
        for x in 0..size {
            let v = if x < size / 2 { 0 } else { 255 };
            row.extend([v, v, v]);
        }
        row.resize(row_bytes as usize, 0);
        data.extend(row);
    }
    let mut out = b"BM".to_vec();
    out.extend((54 + data.len() as u32).to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(54u32.to_le_bytes());
    out.extend(40u32.to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(24u16.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(1000u32.to_le_bytes());
    out.extend(1000u32.to_le_bytes());
    out.extend([0u8; 8]);
    out.extend(data);
    out
}

/// An uncompressed 8-bit gray TIFF without a resolution tag.
fn tiff(width: u16, height: u16) -> Vec<u8> {
    let samples: Vec<u8> = (0..u32::from(width) * u32::from(height)).map(|i| (i * 7 % 256) as u8).collect();
    let ifd_at = 8 + samples.len().div_ceil(2) * 2;
    let mut out = b"II*\0".to_vec();
    out.extend((ifd_at as u32).to_le_bytes());
    out.extend(&samples);
    out.resize(ifd_at, 0);
    out.extend(9u16.to_le_bytes());
    let entry = |tag: u16, kind: u16, value: u32| -> Vec<u8> {
        let mut e = tag.to_le_bytes().to_vec();
        e.extend(kind.to_le_bytes());
        e.extend(1u32.to_le_bytes());
        if kind == 3 {
            e.extend((value as u16).to_le_bytes());
            e.extend([0, 0]);
        } else {
            e.extend(value.to_le_bytes());
        }
        e
    };
    for (tag, kind, value) in [
        (256, 3, u32::from(width)),
        (257, 3, u32::from(height)),
        (258, 3, 8),
        (259, 3, 1),
        (262, 3, 1),
        (273, 4, 8),
        (277, 3, 1),
        (278, 3, u32::from(height)),
        (279, 4, samples.len() as u32),
    ] {
        out.extend(entry(tag, kind, value));
    }
    out.extend(0u32.to_le_bytes());
    out
}

fn put(h: &HeadlessEditorHarness, name: &str, bytes: &[u8]) -> PathBuf {
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let path = PathBuf::from(format!("/in/{name}"));
    let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, bytes).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    path
}

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc)
}

/// A BMP is offered with the resolution in its header; committing stores the original bytes as
/// an asset of its own media type, and the frame has the physical size.
#[test]
fn a_bmp_is_imported_with_its_own_resolution_and_travels_as_image_bmp() {
    let (mut h, doc) = window();
    let bytes = bmp(8);
    let path = put(&h, "a.bmp", &bytes);
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let sheet = h.document(doc).unwrap().image.import.expect("the bitmap sheet opens for a BMP");
    assert_eq!((sheet.width_px, sheet.height_px), (8, 8));
    assert!((sheet.dpi_suggestion.unwrap() - 25.4).abs() < 0.01, "{:?}", sheet.dpi_suggestion);
    let (w, hh) = sheet.size_mm.expect("size follows the proposal");
    assert!((w - 8.0).abs() < 0.01 && (hh - 8.0).abs() < 0.01, "{w} × {hh}");
    h.invoke(doc, CommandKey::CommitImport);
    assert_eq!(h.document(doc).unwrap().objects.len(), 1);
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    let saved = PathBuf::from("/out/p.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(saved.clone()) });
    h.run_tasks();
    let project = load_project(h.fs(), &saved).expect("saved project");
    let asset = project.assets.iter().find(|(_, a)| a.bytes.as_ref() == bytes.as_slice()).expect("original bytes");
    assert_eq!(asset.1.media, "image/bmp");
}

/// A TIFF without a resolution needs an explicit dpi before it can be committed (nothing is
/// assumed, T01), then imports like any bitmap.
#[test]
fn a_tiff_without_resolution_waits_for_a_dpi_and_travels_as_image_tiff() {
    let (mut h, doc) = window();
    let bytes = tiff(6, 4);
    let path = put(&h, "scan.tif", &bytes);
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let sheet = h.document(doc).unwrap().image.import.expect("sheet");
    assert_eq!((sheet.width_px, sheet.height_px, sheet.dpi_suggestion), (6, 4, None));
    assert!(!h.view().commands[&CommandKey::CommitImport].enabled);
    h.send(UiIntent::SetImageDpi { document: doc, dpi: "254".into() });
    h.invoke(doc, CommandKey::CommitImport);
    assert_eq!(h.document(doc).unwrap().objects.len(), 1);
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    let saved = PathBuf::from("/out/p.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(saved.clone()) });
    h.run_tasks();
    let project = load_project(h.fs(), &saved).expect("saved project");
    let asset = project.assets.iter().find(|(_, a)| a.bytes.as_ref() == bytes.as_slice()).expect("original bytes");
    assert_eq!(asset.1.media, "image/tiff");
}

/// The tracer takes a BMP too, at the scale of its header.
#[test]
fn the_tracer_opens_a_bmp_and_proposes_its_resolution() {
    let (mut h, doc) = window();
    let path = put(&h, "logo.bmp", &bmp(16));
    h.send(UiIntent::OpenTraceSource { document: doc, path });
    h.run_tasks();
    let t = h.document(doc).unwrap().tracer.expect("tracer panel");
    assert_eq!((t.width_px, t.height_px), (16, 16));
    assert!((t.form.dpi_suggestion.unwrap() - 25.4).abs() < 0.01);
    assert!(t.result.is_some(), "traced at the proposed scale");
    // The source travels with the project under its own media type.
    h.invoke(doc, CommandKey::ApplyTrace);
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    let saved = PathBuf::from("/out/p.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(saved.clone()) });
    h.run_tasks();
    let project = load_project(h.fs(), &saved).expect("saved project");
    let media: Vec<&str> = project.assets.iter().map(|(_, a)| a.media.as_str()).collect();
    assert_eq!(media, ["image/bmp"]);
}
