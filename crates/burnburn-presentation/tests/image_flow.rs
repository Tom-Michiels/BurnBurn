//! W2-L1-beeld, headless: bitmap import with DPI proposal (P03), Adjust Image with a live
//! preview task, crop, Mask Image, Replace/Refresh and Save Processed Bitmap (T09). Everything
//! goes through the real `AppController` with the intents a native host sends.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::GeometryObject;
use burnburn_document::process::{GrayMethod, ImageCrop, ImageRender, ImageSettings, OperationMode};
use burnburn_geometry::Bounds;
use burnburn_image::{DecodeLimits, SourceImage, decode, encode_png_gray8};
use burnburn_io::fs::FileSystem;
use burnburn_io::{Project, load_project};
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, ImageAdjustInput, ImageField, ImageIssue, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};

use common::*;

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

/// A gray PNG; `pixels_per_metre` adds a `pHYs` chunk (unit metre) after the header.
fn png(width: u32, height: u32, gray: &[u8], pixels_per_metre: Option<u32>) -> Vec<u8> {
    let plain = encode_png_gray8(width, height, gray).unwrap();
    let Some(ppm) = pixels_per_metre else { return plain };
    let mut data = ppm.to_be_bytes().to_vec();
    data.extend_from_slice(&ppm.to_be_bytes());
    data.push(1);
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&data);
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let at = 8 + 25; // signature + IHDR chunk
    let mut out = plain[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&plain[at..]);
    out
}

/// Left half black, right half white.
fn two_tone(width: u32, height: u32) -> Vec<u8> {
    (0..height).flat_map(|_| (0..width).map(move |x| if x < width / 2 { 0 } else { 255 })).collect()
}

/// 1000 pixels per metre = 25.4 dpi = 1 mm per pixel.
const ONE_MM_PER_PX: Option<u32> = Some(1000);

fn put(h: &HeadlessEditorHarness, dir: &str, name: &str, bytes: &[u8]) -> PathBuf {
    h.fs().create_dir_all_for_test(FsPath::new(dir));
    let path = PathBuf::from(format!("{dir}/{name}"));
    let temp = h.fs().create_temp_in(FsPath::new(dir), ".fixture").unwrap();
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

/// Imports a PNG and commits it at the proposed resolution.
fn with_image(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId, PathBuf) {
    let (mut h, doc) = window();
    let path = put(&h, "/in", "photo.png", bytes);
    h.send(UiIntent::Import { document: doc, path: path.clone() });
    h.run_tasks();
    h.invoke(doc, CommandKey::CommitImport);
    (h, doc, path)
}

fn saved(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Project {
    let path = PathBuf::from("/out/p.burnburn");
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    load_project(h.fs(), &path).expect("saved project")
}

struct ImageOp {
    settings: ImageSettings,
    targets: Vec<GeometryId>,
}

fn image_ops(p: &Project) -> Vec<ImageOp> {
    p.document
        .process()
        .operations()
        .filter_map(|op| match op.mode() {
            OperationMode::Image(s) => Some(ImageOp { settings: s, targets: op.targets().to_vec() }),
            _ => None,
        })
        .collect()
}

fn object(p: &Project, id: GeometryId) -> &GeometryObject {
    p.document.object(id).expect("object")
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: Vec<GeometryId>) {
    h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
}

fn frame_of(h: &mut HeadlessEditorHarness, doc: DocumentId) -> GeometryId {
    h.document(doc).unwrap().objects[0].id
}

fn adjust_input(h: &HeadlessEditorHarness, doc: DocumentId) -> ImageAdjustInput {
    h.document(doc).unwrap().image.adjust.expect("adjust panel").input
}

fn set_adjust(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: impl FnOnce(&mut ImageAdjustInput)) {
    let mut input = adjust_input(h, doc);
    edit(&mut input);
    h.send(UiIntent::SetImageAdjust { document: doc, input: Box::new(input) });
}

/// 8 × 8 px at 1 mm per pixel: the whole sheet is 8 × 8 mm.
fn eight_by_eight() -> Vec<u8> {
    png(8, 8, &two_tone(8, 8), ONE_MM_PER_PX)
}

mod import {
    use super::*;

    /// P03: the file's own resolution is proposed, the physical size follows it and nothing is
    /// in the document before the user commits.
    #[test]
    fn png_phys_is_offered_as_the_dpi_proposal_with_the_physical_size() {
        let (mut h, doc) = window();
        let path = put(&h, "/in", "a.png", &png(100, 50, &vec![128; 5000], Some(10_000)));
        h.send(UiIntent::Import { document: doc, path });
        h.run_tasks();
        let vm = h.document(doc).unwrap();
        let sheet = vm.image.import.expect("bitmap import sheet");
        assert_eq!((sheet.width_px, sheet.height_px), (100, 50));
        assert!((sheet.dpi_suggestion.unwrap() - 254.0).abs() < 0.01, "{:?}", sheet.dpi_suggestion);
        let (w, hh) = sheet.size_mm.expect("size follows the proposal");
        assert!((w - 10.0).abs() < 0.01 && (hh - 5.0).abs() < 0.01, "{w} × {hh}");
        assert!(vm.objects.is_empty() && vm.import.is_none(), "preview only");
        assert!(h.view().commands[&CommandKey::CommitImport].enabled);
    }

    /// Like a unitless DXF: without a resolution in the file the user must decide, nothing is
    /// assumed (T01).
    #[test]
    fn a_bitmap_without_resolution_needs_an_explicit_dpi_before_commit() {
        let (mut h, doc) = window();
        let path = put(&h, "/in", "a.png", &png(8, 4, &[0; 32], None));
        h.send(UiIntent::Import { document: doc, path });
        h.run_tasks();
        let sheet = h.document(doc).unwrap().image.import.expect("sheet");
        assert_eq!((sheet.dpi_suggestion, sheet.size_mm), (None, None));
        assert!(!h.view().commands[&CommandKey::CommitImport].enabled);
        h.invoke(doc, CommandKey::CommitImport);
        assert!(h.document(doc).unwrap().objects.is_empty());

        for bad in ["abc", "0", "-5", ""] {
            h.send(UiIntent::SetImageDpi { document: doc, dpi: bad.into() });
            let sheet = h.document(doc).unwrap().image.import.unwrap();
            assert_eq!(sheet.size_mm, None, "{bad:?}");
            assert_eq!(sheet.issue, (!bad.is_empty()).then_some(ImageIssue::InvalidNumber(ImageField::Dpi)));
            assert!(!h.view().commands[&CommandKey::CommitImport].enabled, "{bad:?}");
        }
        h.send(UiIntent::SetImageDpi { document: doc, dpi: "127 dpi".into() });
        let sheet = h.document(doc).unwrap().image.import.unwrap();
        let (w, hh) = sheet.size_mm.unwrap();
        assert!((w - 1.6).abs() < 1e-9 && (hh - 0.8).abs() < 1e-9, "8 px at 127 dpi are 1.6 mm: {w} × {hh}");
        assert!(sheet.issue.is_none() && h.view().commands[&CommandKey::CommitImport].enabled);
    }

    /// The image becomes a frame of its physical size plus an Image operation that targets it;
    /// the source pixels travel as a project asset.
    #[test]
    fn committing_a_bitmap_adds_the_frame_the_operation_and_the_asset() {
        let bytes = eight_by_eight();
        let (mut h, doc, _) = with_image(&bytes);
        let vm = h.document(doc).unwrap();
        assert!(vm.image.import.is_none(), "sheet closed");
        assert_eq!(vm.objects.len(), 1);
        let project = saved(&mut h, doc);
        let ops = image_ops(&project);
        assert_eq!(ops.len(), 1, "one image operation");
        let b: Bounds = object(&project, ops[0].targets[0]).bounds().unwrap();
        assert!((b.width() - 8.0).abs() < 1e-6 && (b.height() - 8.0).abs() < 1e-6, "{b:?}");
        let source = decode(&bytes, DecodeLimits::default()).unwrap();
        assert_eq!(ops[0].settings.source(), source.content_hash(), "pixels are referenced by content");
        assert!(project.assets.iter().any(|(_, a)| a.bytes.as_ref() == bytes.as_slice()), "the file travels along");
        assert!(matches!(ops[0].settings.render(), ImageRender::Dither(_)));
    }

    #[test]
    fn cancelling_a_bitmap_import_changes_nothing() {
        let (mut h, doc) = window();
        let path = put(&h, "/in", "a.png", &eight_by_eight());
        h.send(UiIntent::Import { document: doc, path });
        h.run_tasks();
        let before = h.document(doc).unwrap();
        h.invoke(doc, CommandKey::CancelImport);
        let after = h.document(doc).unwrap();
        assert!(after.image.import.is_none() && after.objects.is_empty());
        assert_eq!((after.revision, after.undo), (before.revision, before.undo));
    }

    /// BMP and TIFF are not decodable yet: a clear error, nothing imported (honest limit).
    #[test]
    fn bitmap_formats_the_decoder_cannot_read_are_refused_without_importing() {
        let (mut h, doc) = window();
        for (name, bytes) in
            [("a.bmp", b"BM\x1e\0\0\0\0\0\0\0\x1a\0\0\0".to_vec()), ("a.tiff", b"II*\0\x08\0\0\0".to_vec())]
        {
            let path = put(&h, "/in", name, &bytes);
            h.send(UiIntent::Import { document: doc, path });
            let updates = h.run_tasks();
            let shown_error = updates.iter().flat_map(|u| u.effects.iter()).any(|e| {
                matches!(
                    e,
                    burnburn_presentation::HostEffect::ShowError {
                        message: burnburn_presentation::MessageKey::ImageFailed,
                        ..
                    }
                )
            });
            assert!(shown_error, "{name}");
            let vm = h.document(doc).unwrap();
            assert!(vm.image.import.is_none() && vm.import.is_none() && vm.objects.is_empty(), "{name}");
        }
    }

    /// The compiler gets the pixels from the project: an image job compiles, it does not fail with
    /// a missing image source.
    #[test]
    fn an_imported_image_compiles_from_the_stored_pixels() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        h.invoke(doc, CommandKey::Compile);
        h.run_tasks();
        let plan = h.document(doc).unwrap().plan.expect("the plan compiled");
        assert!(plan.errors == 0, "{:?}", plan.findings);
        // The black left half of 8 × 8 mm is 4 mm wide on 80 scan rows of 0.1 mm.
        assert!((plan.preview.scan_length_mm - 320.0).abs() < 8.0, "scan length {}", plan.preview.scan_length_mm);
    }
}

mod adjust {
    use super::*;

    fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) {
        let frame = frame_of(h, doc);
        select(h, doc, vec![frame]);
        h.invoke(doc, CommandKey::AdjustImage);
    }

    /// T09: opening the panel shows the operation's values and computes a preview of the pixels
    /// the laser would scan; apply is a single history entry.
    #[test]
    fn the_preview_shows_the_scanned_pixels_and_apply_is_one_undo_entry() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        assert!(!h.view().commands[&CommandKey::AdjustImage].enabled || h.document(doc).unwrap().selection.len() == 1);
        open(&mut h, doc);
        let panel = h.document(doc).unwrap().image.adjust.expect("adjust panel");
        assert_eq!(
            (panel.input.brightness.as_str(), panel.input.contrast.as_str(), panel.input.gamma.as_str()),
            ("0", "1", "1")
        );
        assert!(panel.input.crop_x.is_empty() && panel.preview.is_none() && panel.computing);
        h.run_tasks();
        let panel = h.document(doc).unwrap().image.adjust.unwrap();
        let preview = panel.preview.expect("preview");
        // 8 mm at the default 0.1 mm scan grid: 80 × 80 cells; burn = 0, white = 255.
        assert_eq!((preview.width, preview.height), (80, 80));
        let at = |x: usize, y: usize| preview.gray8[y * 80 + x];
        assert_eq!((at(0, 0), at(39, 79), at(40, 0), at(79, 40)), (0, 0, 255, 255), "left half burns");
        assert_eq!(panel.source_px, Some((8, 8)));
        assert!(!panel.can_apply, "nothing changed yet");
        assert!(!h.view().commands[&CommandKey::ApplyImageAdjust].enabled);

        let revision = h.document(doc).unwrap().revision;
        set_adjust(&mut h, doc, |i| i.brightness = "0,5".into());
        assert!(h.document(doc).unwrap().image.adjust.unwrap().computing);
        h.run_tasks();
        let panel = h.document(doc).unwrap().image.adjust.unwrap();
        assert!(panel.can_apply && h.view().commands[&CommandKey::ApplyImageAdjust].enabled);
        let brighter = panel.preview.unwrap();
        assert_eq!(brighter.gray8[0], 255, "+0.5 brightness lifts black above the threshold: nothing burns there");
        assert_eq!(h.document(doc).unwrap().revision, revision, "a preview is not a document change");

        let undo_before = h.document(doc).unwrap().undo;
        h.invoke(doc, CommandKey::ApplyImageAdjust);
        let vm = h.document(doc).unwrap();
        assert!(vm.image.adjust.is_none(), "panel closed");
        assert_ne!(vm.undo, undo_before, "one new history entry");
        let project = saved(&mut h, doc);
        assert!((image_ops(&project)[0].settings.tone().brightness() - 0.5).abs() < 1e-12);
        h.invoke(doc, CommandKey::Undo);
        let project = saved(&mut h, doc);
        assert_eq!(image_ops(&project)[0].settings.tone().brightness(), 0.0, "one undo restores the operation");
        assert_eq!(image_ops(&project).len(), 1);
    }

    /// AGENTS.md: a stale result is dropped by task identity; the older preview was cancelled.
    #[test]
    fn a_preview_for_older_values_is_cancelled_and_never_shown() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        open(&mut h, doc);
        h.run_tasks();
        set_adjust(&mut h, doc, |i| i.brightness = "0,5".into());
        let older = h.take_task_requests();
        set_adjust(&mut h, doc, |i| i.brightness = "-0,5".into());
        let newer = h.take_task_requests();
        assert!(older.iter().all(|r| r.cancel.is_cancelled()), "the superseded preview is cancelled");
        for r in older {
            h.complete_despite_cancel(r);
        }
        let panel = h.document(doc).unwrap().image.adjust.unwrap();
        assert!(panel.computing, "the newest is still running");
        assert_eq!(panel.preview.map(|p| p.gray8[0]), Some(0), "the earlier, valid preview stays");
        for r in newer {
            h.complete(r);
        }
        let panel = h.document(doc).unwrap().image.adjust.unwrap();
        assert!(!panel.computing);
        assert_eq!(panel.input.brightness, "-0,5");
    }

    #[test]
    fn invalid_input_names_the_field_and_starts_no_preview() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        open(&mut h, doc);
        h.run_tasks();
        let cases: [(&str, fn(&mut ImageAdjustInput, String), ImageField, ImageIssue); 5] = [
            ("0", |i, v| i.gamma = v, ImageField::Gamma, ImageIssue::OutOfRange(ImageField::Gamma)),
            ("11", |i, v| i.contrast = v, ImageField::Contrast, ImageIssue::OutOfRange(ImageField::Contrast)),
            ("x", |i, v| i.brightness = v, ImageField::Brightness, ImageIssue::InvalidNumber(ImageField::Brightness)),
            (
                "-1",
                |i, v| i.sharpen_amount = v,
                ImageField::SharpenAmount,
                ImageIssue::OutOfRange(ImageField::SharpenAmount),
            ),
            (
                "0",
                |i, v| {
                    i.sharpen_amount = "1".into();
                    i.sharpen_radius = v
                },
                ImageField::SharpenRadius,
                ImageIssue::OutOfRange(ImageField::SharpenRadius),
            ),
        ];
        for (text, edit, field, issue) in cases {
            set_adjust(&mut h, doc, |i| edit(i, text.to_string()));
            let panel = h.document(doc).unwrap().image.adjust.unwrap();
            assert_eq!(panel.issue, Some((field, issue)), "{text}");
            assert!(!panel.can_apply && !panel.computing && h.take_task_requests().is_empty(), "{text}");
            assert!(panel.preview.is_some(), "the last valid preview stays visible");
            set_adjust(&mut h, doc, |i| *i = h_default());
            h.run_tasks();
        }
    }

    fn h_default() -> ImageAdjustInput {
        ImageAdjustInput {
            brightness: "0".into(),
            contrast: "1".into(),
            gamma: "1".into(),
            sharpen_amount: "0".into(),
            sharpen_radius: "0,1".into(),
            gray: GrayMethod::Rec709,
            crop_x: String::new(),
            crop_y: String::new(),
            crop_width: String::new(),
            crop_height: String::new(),
        }
    }

    #[test]
    fn closing_the_panel_cancels_the_preview_and_changes_nothing() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        open(&mut h, doc);
        let pending = h.take_task_requests();
        let before = h.document(doc).unwrap();
        h.invoke(doc, CommandKey::CancelImageAdjust);
        assert!(pending.iter().all(|r| r.cancel.is_cancelled()));
        for r in pending {
            h.complete_despite_cancel(r);
        }
        let after = h.document(doc).unwrap();
        assert!(after.image.adjust.is_none());
        assert_eq!((after.revision, after.undo), (before.revision, before.undo));
    }

    /// Crop is a window in source pixels: it must lie inside the image and be complete.
    #[test]
    fn crop_is_checked_against_the_source_pixels_and_applies_as_a_window() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        open(&mut h, doc);
        h.run_tasks();
        let cases =
            [("0", "0", "9", "8", ImageIssue::CropOutsideSource), ("2", "", "4", "4", ImageIssue::CropIncomplete)];
        for (x, y, w, hh, issue) in cases {
            set_adjust(&mut h, doc, |i| {
                (i.crop_x, i.crop_y, i.crop_width, i.crop_height) = (x.into(), y.into(), w.into(), hh.into());
            });
            let panel = h.document(doc).unwrap().image.adjust.unwrap();
            assert_eq!(panel.issue.map(|(_, i)| i), Some(issue), "{x} {y} {w} {hh}");
            assert!(!panel.can_apply);
        }
        // The right half only: x 4…8 of the white side → nothing burns anywhere.
        set_adjust(&mut h, doc, |i| {
            (i.crop_x, i.crop_y, i.crop_width, i.crop_height) = ("4".into(), "0".into(), "4".into(), "8".into());
        });
        h.run_tasks();
        let panel = h.document(doc).unwrap().image.adjust.unwrap();
        assert!(panel.issue.is_none() && panel.can_apply);
        assert!(panel.preview.unwrap().gray8.iter().all(|&v| v == 255), "the white half stretched over the frame");
        h.invoke(doc, CommandKey::ApplyImageAdjust);
        let project = saved(&mut h, doc);
        assert_eq!(image_ops(&project)[0].settings.crop(), Some(ImageCrop { x: 4, y: 0, width: 4, height: 8 }));
    }

    #[test]
    fn the_gray_method_reaches_the_operation() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        open(&mut h, doc);
        h.run_tasks();
        set_adjust(&mut h, doc, |i| i.gray = GrayMethod::Red);
        h.run_tasks();
        h.invoke(doc, CommandKey::ApplyImageAdjust);
        let project = saved(&mut h, doc);
        assert_eq!(image_ops(&project)[0].settings.gray_method(), GrayMethod::Red);
    }

    #[test]
    fn without_a_selected_image_the_panel_does_not_open() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        select(&mut h, doc, vec![]);
        assert!(!h.view().commands[&CommandKey::AdjustImage].enabled);
        h.invoke(doc, CommandKey::AdjustImage);
        assert!(h.document(doc).unwrap().image.adjust.is_none());
    }
}

mod bitmap_file {
    use super::*;

    /// "Save Processed Bitmap" writes exactly the pixels the compiler scans: for a black/white
    /// source every dither leaves the pixels as they are, so the expected file is known without
    /// the implementation.
    #[test]
    fn the_processed_bitmap_holds_exactly_the_scanned_pixels() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        let frame = frame_of(&mut h, doc);
        select(&mut h, doc, vec![frame]);
        assert!(h.view().commands[&CommandKey::SaveProcessedBitmap].enabled);
        h.fs().create_dir_all_for_test(FsPath::new("/out"));
        let revision = h.document(doc).unwrap().revision;
        h.send(UiIntent::SaveProcessedBitmap { document: doc, path: PathBuf::from("/out/processed.png") });
        h.run_tasks();
        let bytes = h.fs().read(FsPath::new("/out/processed.png"), 1 << 20).expect("file written");
        let written: SourceImage = decode(&bytes, DecodeLimits::default()).expect("a PNG");
        assert_eq!((written.width(), written.height()), (80, 80));
        let expected: Vec<u8> = (0..80).flat_map(|_| (0..80).map(|x| if x < 40 { 0 } else { 255 })).collect();
        assert_eq!(written.samples(), expected.as_slice());
        assert_eq!(h.document(doc).unwrap().revision, revision, "exporting does not touch the document");
    }

    #[test]
    fn nothing_is_written_without_a_selected_image() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        select(&mut h, doc, vec![]);
        h.send(UiIntent::SaveProcessedBitmap { document: doc, path: PathBuf::from("/out/none.png") });
        h.run_tasks();
        assert!(!h.fs().exists(FsPath::new("/out/none.png")).unwrap());
    }
}

mod replace {
    use super::*;

    fn other_png() -> Vec<u8> {
        // Right half black this time, 12 × 6 px.
        let gray: Vec<u8> = (0..6).flat_map(|_| (0..12).map(|x| if x < 6 { 255 } else { 0 })).collect();
        png(12, 6, &gray, ONE_MM_PER_PX)
    }

    /// Replace keeps the frame (placement and size) and the operation's other settings; only the
    /// pixels change. One history entry.
    #[test]
    fn replacing_keeps_the_frame_and_the_settings_and_changes_the_pixels() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        let frame = frame_of(&mut h, doc);
        select(&mut h, doc, vec![frame]);
        let before = saved(&mut h, doc);
        let new_file = put(&h, "/in", "other.png", &other_png());
        let undo_before = h.document(doc).unwrap().undo;
        h.send(UiIntent::ReplaceImage { document: doc, path: new_file });
        h.run_tasks();
        assert_ne!(h.document(doc).unwrap().undo, undo_before, "one new entry");
        let after = saved(&mut h, doc);
        let (a, b) = (&image_ops(&before)[0], &image_ops(&after)[0]);
        let new_source = decode(&other_png(), DecodeLimits::default()).unwrap();
        assert_eq!(b.settings.source(), new_source.content_hash());
        assert_ne!(a.settings.source(), b.settings.source());
        assert_eq!(a.targets, b.targets, "same frame");
        assert_eq!(object(&before, a.targets[0]).bounds(), object(&after, b.targets[0]).bounds(), "same placement");
        assert_eq!(
            (a.settings.dot_pitch(), a.settings.line_interval()),
            (b.settings.dot_pitch(), b.settings.line_interval())
        );
        h.invoke(doc, CommandKey::Undo);
        assert_eq!(image_ops(&saved(&mut h, doc))[0].settings.source(), a.settings.source());
    }

    #[test]
    fn a_crop_that_does_not_fit_the_new_pixels_is_dropped_by_replace() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        let frame = frame_of(&mut h, doc);
        select(&mut h, doc, vec![frame]);
        h.invoke(doc, CommandKey::AdjustImage);
        h.run_tasks();
        set_adjust(&mut h, doc, |i| {
            (i.crop_x, i.crop_y, i.crop_width, i.crop_height) = ("0".into(), "0".into(), "8".into(), "8".into());
        });
        h.run_tasks();
        h.invoke(doc, CommandKey::ApplyImageAdjust);
        // 4 × 4 px: the 8 × 8 window is outside.
        let small = put(&h, "/in", "small.png", &png(4, 4, &[7; 16], ONE_MM_PER_PX));
        h.send(UiIntent::ReplaceImage { document: doc, path: small });
        h.run_tasks();
        assert_eq!(image_ops(&saved(&mut h, doc))[0].settings.crop(), None);
    }

    /// Refresh re-reads the file the image came from; an unchanged file is no new entry.
    #[test]
    fn refresh_rereads_the_file_and_an_unchanged_file_changes_nothing() {
        let (mut h, doc, path) = with_image(&eight_by_eight());
        let frame = frame_of(&mut h, doc);
        select(&mut h, doc, vec![frame]);
        assert!(h.view().commands[&CommandKey::RefreshImage].enabled);
        let revision = h.document(doc).unwrap().revision;
        h.invoke(doc, CommandKey::RefreshImage);
        h.run_tasks();
        assert_eq!(h.document(doc).unwrap().revision, revision, "same pixels: nothing to do");

        let changed = other_png();
        let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
        h.fs().write_all(&temp, &changed).unwrap();
        h.fs().replace(&temp, &path).unwrap();
        h.invoke(doc, CommandKey::RefreshImage);
        h.run_tasks();
        let project = saved(&mut h, doc);
        let new_source = decode(&changed, DecodeLimits::default()).unwrap();
        assert_eq!(image_ops(&project)[0].settings.source(), new_source.content_hash());
    }
}

mod mask {
    use super::*;

    fn rectangle(h: &mut HeadlessEditorHarness, doc: DocumentId, from: (f64, f64), to: (f64, f64)) -> GeometryId {
        // Object snapping is on by default and would pull the corners onto the frame's points.
        if h.view().commands[&CommandKey::SnapToObjects].checked {
            h.invoke(doc, CommandKey::SnapToObjects);
        }
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, from, to);
        h.invoke(doc, CommandKey::ToolSelect);
        h.document(doc).unwrap().objects.last().unwrap().id
    }

    /// Mask Image clips the picture to a closed shape and keeps it where it was: the shape
    /// becomes the target, the crop window is the part of the source the shape covers.
    #[test]
    fn masking_targets_the_shape_and_keeps_the_picture_in_place() {
        let (mut h, doc, _) = with_image(&png(10, 10, &[100; 100], ONE_MM_PER_PX));
        let frame = frame_of(&mut h, doc);
        // The frame sits at the origin and is 10 × 10 mm; the shape lies inside it.
        let shape = rectangle(&mut h, doc, (2.0, 2.0), (6.0, 8.0));
        select(&mut h, doc, vec![frame, shape]);
        assert!(h.view().commands[&CommandKey::MaskImage].enabled);
        h.invoke(doc, CommandKey::MaskImage);
        let project = saved(&mut h, doc);
        let op = &image_ops(&project)[0];
        assert_eq!(op.targets, vec![shape], "the shape is what the operation scans now");
        assert!(op.settings.mask());
        // y is up in the document, rows count from the top: y 2…8 mm of a 10 mm frame = rows 2…8.
        assert_eq!(op.settings.crop(), Some(ImageCrop { x: 2, y: 2, width: 4, height: 6 }));
    }

    #[test]
    fn a_shape_that_leaves_the_image_cannot_mask_it() {
        let (mut h, doc, _) = with_image(&png(10, 10, &[100; 100], ONE_MM_PER_PX));
        let frame = frame_of(&mut h, doc);
        let shape = rectangle(&mut h, doc, (5.0, 5.0), (15.0, 15.0));
        select(&mut h, doc, vec![frame, shape]);
        assert!(!h.view().commands[&CommandKey::MaskImage].enabled);
        let revision = h.document(doc).unwrap().revision;
        h.invoke(doc, CommandKey::MaskImage);
        assert_eq!(h.document(doc).unwrap().revision, revision);
    }

    #[test]
    fn masking_needs_an_image_and_a_shape() {
        let (mut h, doc, _) = with_image(&eight_by_eight());
        let frame = frame_of(&mut h, doc);
        select(&mut h, doc, vec![frame]);
        assert!(!h.view().commands[&CommandKey::MaskImage].enabled, "only the image");
    }
}
