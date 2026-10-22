//! G6-machine, C05: an already dithered image in pass-through mode that lands on an uneven grid
//! shows up in the preflight panel as a warning with the image as its subject. The preflight
//! of the compile has the source pixels of the project, so the panel can judge it.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    CamDialog, CommandKey, CutChoice, CutField, DialogField, DialogNote, FindingKind, HeadlessEditorHarness, UiIntent,
};
use burnburn_types::DocumentId;
use common::{RUIDA, profile};

/// A black and white 8 x 8 px PNG at 1 mm per pixel (pHYs 1000 px/m), via a hand-made chunk list.
fn png() -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    let gray: Vec<u8> = (0..64).map(|i| if (i / 8 + i % 8) % 2 == 0 { 0 } else { 255 }).collect();
    let plain = burnburn_image::encode_png_gray8(8, 8, &gray).unwrap();
    let mut data = 1000u32.to_be_bytes().to_vec();
    data.extend_from_slice(&1000u32.to_be_bytes());
    data.push(1);
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&data);
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let at = 8 + 25;
    let mut out = plain[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&plain[at..]);
    out
}

fn window() -> (HeadlessEditorHarness, DocumentId, burnburn_types::OperationId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let path = PathBuf::from("/in/foto.png");
    let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, &png()).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    h.invoke(doc, CommandKey::CommitImport);
    let op = h.document(doc).unwrap().operations[0].id;
    (h, doc, op)
}

fn pass_through(h: &mut HeadlessEditorHarness, doc: DocumentId, op: burnburn_types::OperationId, pitch: &str) {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let sheet = h.document(doc).unwrap().cut_settings.expect("sheet");
    let mut input = sheet.input();
    input.extras.set_choice(CutField::ImageMode, CutChoice::PassThrough);
    input.extras.set_text(CutField::ImageDotPitch, pitch);
    input.extras.set_text(CutField::ImageInterval, pitch);
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
    assert!(h.document(doc).unwrap().cut_settings.is_none(), "the sheet took the values");
}

fn findings(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<Option<DialogNote>> {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: CamDialog::Preflight });
    let vm = h.controller().cam_dialog(doc, CamDialog::Preflight).expect("dialog");
    vm.row(DialogField::PreflightPick).unwrap().labels.iter().map(|l| l.note).collect()
}

#[test]
fn an_uneven_grid_for_a_pass_through_image_is_listed_as_a_warning() {
    let (mut h, doc, op) = window();
    pass_through(&mut h, doc, op, "0.3"); // 8 mm / 0.3 mm = 26 dots for 8 pixels: 3.25 per pixel
    let notes = findings(&mut h, doc);
    assert!(notes.contains(&Some(DialogNote::Finding(FindingKind::PassThroughResampled))), "{notes:?}");
}

#[test]
fn a_whole_number_of_dots_per_pixel_is_not_a_finding() {
    let (mut h, doc, op) = window();
    pass_through(&mut h, doc, op, "0.25"); // 32 dots for 8 pixels: 4 per pixel
    let notes = findings(&mut h, doc);
    assert!(!notes.contains(&Some(DialogNote::Finding(FindingKind::PassThroughResampled))), "{notes:?}");
}
