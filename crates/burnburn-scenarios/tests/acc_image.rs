//! Acceptance plan, image block: IM01 (composition and the physical pixel grid), IM02 (every
//! image mode on the same bitmap, against a Python reference) and IM03 (mask / crop / rotate /
//! replace / refresh with undo, through the app controller).
//!
//! Burn patterns are read back from the compiled plan's scan runs (`PlanEvent::ScanRun`), i.e.
//! from what the laser would be told to scan, never from the engraver's intermediate bitmap.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_cam;

use std::path::{Path as FsPath, PathBuf};

use acc_cam::*;
use burnburn_cam::{CompileError, compile_with};
use burnburn_document::DocumentSession;
use burnburn_document::process::{
    DitherMode, ImageRender, ImageSettings, OperationDraft, OperationMode, ScreenSettings, ToneSettings,
};
use burnburn_image::{DecodeLimits, SourceImage, decode, encode_png_gray8};
use burnburn_job::{JobPlan, PlanEvent};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::CancellationToken;

fn never() -> CancellationToken {
    CancellationToken::never()
}

fn draft(settings: ImageSettings) -> OperationDraft {
    OperationDraft {
        name: "Graveren".into(),
        mode: OperationMode::Image(settings),
        settings: acc_cam::settings(100.0, 60.0),
        output_enabled: true,
    }
}

/// A frame of `w x h` mm with its lower-left corner at (10, 10) and one image operation.
fn scene(settings: ImageSettings, w: f64, h: f64) -> DocumentSession {
    let mut s = session();
    let frame = rectangle(&mut s, 10.0, 10.0, w, h);
    add_op(&mut s, draft(settings), vec![frame]);
    s
}

fn plan(s: &DocumentSession, source: &SourceImage) -> Result<JobPlan, CompileError> {
    compile_with(&s.snapshot(), &profile(), &options(), &vec![source.clone()], &never())
}

/// The burn pattern of a `cols x rows` raster of 0.1 mm dots whose lower-left corner is (10, 10):
/// `#` where a scan run covers the dot. Rows are listed top row first.
fn pattern(plan: &JobPlan, cols: usize, rows: usize) -> Vec<String> {
    let mut grid = vec![vec!['.'; cols]; rows];
    for e in plan.events() {
        if let PlanEvent::ScanRun { line, .. } = *e {
            let (a, b) = (line.from.x_mm(), line.to.x_mm());
            let (x0, x1, y) = (a.min(b), a.max(b), line.from.y_mm());
            let row = rows - 1 - ((y - 10.0) / 0.1 - 0.5).round() as usize;
            let (c0, c1) = (((x0 - 10.0) / 0.1).round() as usize, ((x1 - 10.0) / 0.1).round() as usize);
            for cell in &mut grid[row][c0..c1] {
                *cell = '#';
            }
        }
    }
    grid.into_iter().map(|r| r.into_iter().collect()).collect()
}

fn base(source: &SourceImage, mode: DitherMode) -> ImageSettings {
    ImageSettings::new(source.content_hash(), mm(0.1), mm(0.1), ImageRender::Dither(mode)).unwrap()
}

fn burn(source: &SourceImage, mode: DitherMode, w_dots: usize, h_dots: usize) -> Vec<String> {
    let s = scene(base(source, mode), w_dots as f64 / 10.0, h_dots as f64 / 10.0);
    pattern(&plan(&s, source).unwrap(), w_dots, h_dots)
}

fn rows(strs: &[&str]) -> Vec<String> {
    strs.iter().map(|s| (*s).to_owned()).collect()
}

// ---------------------------------------------------------------------------------------------
// IM01 alpha gradient, black/white, step wedge, one-pixel border
// ---------------------------------------------------------------------------------------------

#[test]
fn im01_an_alpha_gradient_composites_over_white_paper_before_the_threshold() {
    // Black with alpha 0, 36, 73, 109, 146, 182, 219, 255 over white paper: 255 - alpha
    // = 255, 219, 182, 146, 109, 73, 36, 0. Gray below 128 burns: the last four.
    let alphas = [0u8, 36, 73, 109, 146, 182, 219, 255];
    let rgba: Vec<u8> = alphas.iter().flat_map(|a| [0, 0, 0, *a]).collect();
    let source = SourceImage::from_rgba8(8, 1, rgba).unwrap();
    assert_eq!(burn(&source, DitherMode::Threshold { level: 128 }, 8, 1), rows(["....####"].as_slice()));
}

#[test]
fn im01_black_and_white_pixels_are_exact_and_the_negative_swaps_them() {
    let source = SourceImage::from_gray8(4, 2, vec![0, 255, 0, 255, 255, 0, 255, 0]).unwrap();
    let t = DitherMode::Threshold { level: 128 };
    assert_eq!(burn(&source, t, 4, 2), rows(&["#.#.", ".#.#"]));
    let s = scene(base(&source, t).with_negative(true), 0.4, 0.2);
    assert_eq!(pattern(&plan(&s, &source).unwrap(), 4, 2), rows(&[".#.#", "#.#."]));
}

#[test]
fn im01_a_step_wedge_burns_below_the_threshold_and_the_tone_curve_moves_that_edge_predictably() {
    // Eight steps 16, 48, ..., 240 (values below 128: 16, 48, 80, 112 burn).
    let wedge: Vec<u8> = (0..8).map(|i| 16 + 32 * i as u8).collect();
    let source = SourceImage::from_gray8(8, 1, wedge).unwrap();
    let t = DitherMode::Threshold { level: 128 };
    assert_eq!(burn(&source, t, 8, 1), rows(&["####...."]));
    // Brightness +0.2: out = v/255 + 0.2. 112 -> 0.639 (163), 80 -> 0.514 (131): both stop burning;
    // 48 -> 0.388 (99) still burns.
    let tone = ToneSettings::new(1.0, 0.2, 1.0).unwrap();
    let s = scene(base(&source, t).with_tone(tone), 0.8, 0.1);
    assert_eq!(pattern(&plan(&s, &source).unwrap(), 8, 1), rows(&["##......"]));
    // Brightness -0.2: 144 -> 0.565-0.2 = 0.365 (93) now burns, 176 -> 0.49 (125) burns, 208 -> 0.616 (157) does not.
    let tone = ToneSettings::new(1.0, -0.2, 1.0).unwrap();
    let s = scene(base(&source, t).with_tone(tone), 0.8, 0.1);
    assert_eq!(pattern(&plan(&s, &source).unwrap(), 8, 1), rows(&["######.."]));
}

fn bordered(w: usize, h: usize) -> SourceImage {
    let px: Vec<u8> = (0..h)
        .flat_map(|y| (0..w).map(move |x| if x == 0 || y == 0 || x == w - 1 || y == h - 1 { 0 } else { 255 }))
        .collect();
    SourceImage::from_gray8(w as u32, h as u32, px).unwrap()
}

#[test]
fn im01_a_one_pixel_border_stays_one_dot_on_the_grid_and_survives_a_non_integer_scale() {
    let source = bordered(10, 6);
    let t = DitherMode::Threshold { level: 128 };
    // 1:1 on the 0.1 mm grid.
    assert_eq!(
        burn(&source, t, 10, 6),
        rows(&["##########", "#........#", "#........#", "#........#", "#........#", "##########"])
    );
    // 2x: the border is two dots wide, exactly.
    let two = burn(&source, t, 20, 12);
    assert!(
        two[0].chars().all(|c| c == '#')
            && two[1].chars().all(|c| c == '#')
            && two[2].starts_with("##..")
            && two[2].ends_with("..##")
    );
    // 1.5x (15 x 9 dots): dot 0 is entirely border (burns), dot 1 is 1/3 border + 2/3 white (gray 170, no burn).
    let odd = burn(&source, t, 15, 9);
    assert!(odd[0].chars().all(|c| c == '#') && odd[8].chars().all(|c| c == '#'), "top and bottom rows: {odd:?}");
    // Dot rows 1 and 7 straddle the top/bottom border row (a third of it), so they are blends;
    // rows 2..=6 lie in the interior: one border dot at each side, then white (dot 1 is half
    // border = gray 127.5, rounded half up to 128, which does not burn).
    for row in &odd[2..=6] {
        assert!(row.starts_with("#.") && row.ends_with(".#"), "side border of one dot: {row}");
    }
    for row in &odd {
        let rev: String = row.chars().rev().collect();
        assert_eq!(row, &rev, "the pattern is mirror-symmetric like the source");
    }
}

// ---------------------------------------------------------------------------------------------
// IM02 every mode, same bitmap, golden from the Python reference
// ---------------------------------------------------------------------------------------------

fn golden() -> (SourceImage, Vec<(String, usize, Vec<String>)>) {
    let text = String::from_utf8(read_fixture("acceptance", "image-modes-golden.txt").unwrap()).unwrap();
    let mut px = Vec::new();
    let mut cases = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                px.extend(rest.split(' ').map(|v| v.parse::<u8>().unwrap()));
            }
            continue;
        }
        let (head, body) = line.split_once(": ").unwrap();
        let (mode, scale) = head.split_once(' ').unwrap();
        cases.push((mode.to_owned(), scale.parse::<usize>().unwrap(), body.split('/').map(str::to_owned).collect()));
    }
    (SourceImage::from_gray8(12, 6, px).unwrap(), cases)
}

fn settings_of(source: &SourceImage, mode: &str) -> ImageSettings {
    let screen = |angle| ScreenSettings::new(mm(0.4), angle).unwrap();
    match mode {
        "threshold128" => base(source, DitherMode::Threshold { level: 128 }),
        "floyd" => base(source, DitherMode::FloydSteinberg),
        "jarvis" => base(source, DitherMode::Jarvis),
        "stucki" => base(source, DitherMode::Stucki),
        "atkinson" => base(source, DitherMode::Atkinson),
        "ordered4" => base(source, DitherMode::Ordered { size: 4 }),
        "halftone0" => base(source, DitherMode::Halftone).with_screen(screen(0.0)),
        "halftone30" => base(source, DitherMode::Halftone).with_screen(screen(30.0)),
        "newsprint" => base(source, DitherMode::Newsprint).with_screen(screen(0.0)),
        "sketch100" => base(source, DitherMode::Sketch { level: 100 }),
        "passthrough" => base(source, DitherMode::PassThrough),
        other => panic!("unknown mode {other}"),
    }
}

#[test]
fn im02_every_image_mode_matches_the_python_reference_at_one_and_two_times_scale() {
    let (source, cases) = golden();
    assert_eq!(cases.len(), 22, "11 modes x 2 scales");
    let mut failures = Vec::new();
    for (mode, scale, expected) in &cases {
        let (cols, rows) = (12 * scale, 6 * scale);
        let s = scene(settings_of(&source, mode), cols as f64 / 10.0, rows as f64 / 10.0);
        let got = pattern(&plan(&s, &source).unwrap(), cols, rows);
        if &got != expected {
            failures.push(format!("{mode} x{scale}:\n  got      {}\n  expected {}", got.join("/"), expected.join("/")));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 22 differ from the independent reference:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn im02_grayscale_power_modulation_is_refused_not_replaced_by_dithering() {
    let source = SourceImage::from_gray8(2, 1, vec![0, 255]).unwrap();
    let settings = ImageSettings::new(source.content_hash(), mm(0.1), mm(0.1), ImageRender::Grayscale).unwrap();
    let s = scene(settings, 0.2, 0.1);
    assert!(plan(&s, &source).is_err(), "no profile has proven grayscale modulation, so the compile refuses");
}

// ---------------------------------------------------------------------------------------------
// IM03 mask / crop / rotate / replace / refresh with undo, through the app controller
// ---------------------------------------------------------------------------------------------

fn png(width: u32, height: u32, gray: &[u8]) -> Vec<u8> {
    encode_png_gray8(width, height, gray).unwrap()
}

fn put(h: &HeadlessEditorHarness, name: &str, bytes: &[u8]) -> PathBuf {
    use burnburn_io::fs::FileSystem;
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let path = PathBuf::from(format!("/in/{name}"));
    let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, bytes).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    path
}

/// 12 x 6 px: left half black, right half white; imported at 25.4 dpi = 1 mm per pixel.
fn two_tone() -> Vec<u8> {
    let gray: Vec<u8> = (0..6).flat_map(|_| (0..12).map(|x| if x < 6 { 0 } else { 255 })).collect();
    png(12, 6, &gray)
}

fn imported() -> (HeadlessEditorHarness, burnburn_types::DocumentId, PathBuf) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile()),
        ruida: burnburn_ruida::RuidaProfile {
            magic: burnburn_ruida::Magic::FIXTURE_0X88,
            overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
        },
    });
    let path = put(&h, "plaat.png", &two_tone());
    h.send(UiIntent::Import { document: doc, path: path.clone() });
    h.run_tasks();
    h.send(UiIntent::SetImageDpi { document: doc, dpi: "25.4".into() });
    h.invoke(doc, CommandKey::CommitImport);
    (h, doc, path)
}

fn frame_bounds(h: &HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> (f64, f64) {
    let id = h.document(doc).unwrap().objects[0].id;
    let b = h.scene(doc).unwrap().items.iter().find(|i| i.source == id).unwrap().path.bounds().unwrap();
    ((b.width() * 1e6).round() / 1e6, (b.height() * 1e6).round() / 1e6)
}

fn processed(h: &mut HeadlessEditorHarness, doc: burnburn_types::DocumentId, name: &str) -> SourceImage {
    use burnburn_io::fs::FileSystem;
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    let path = PathBuf::from(format!("/out/{name}"));
    h.send(UiIntent::SaveProcessedBitmap { document: doc, path: path.clone() });
    h.run_tasks();
    decode(&h.fs().read(&path, 1 << 22).expect("processed bitmap"), DecodeLimits::default()).unwrap()
}

#[test]
fn im03_rotate_replace_refresh_and_undo_keep_the_frame_size_contract() {
    let (mut h, doc, path) = imported();
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    assert_eq!(frame_bounds(&h, doc), (12.0, 6.0), "1 mm per pixel");
    let first = processed(&mut h, doc, "a.png");
    assert_eq!((first.width(), first.height()), (120, 60), "12 x 6 mm on a 0.1 mm grid");
    assert!(first.samples()[..60].iter().all(|v| *v == 0) && first.samples()[60..120].iter().all(|v| *v == 255));

    // Rotate 90 degrees: the frame is 6 x 12 and the burn follows the frame (black stays the left-turned half).
    h.invoke(doc, CommandKey::Rotate90Cw);
    assert_eq!(frame_bounds(&h, doc), (6.0, 12.0));
    let rotated = processed(&mut h, doc, "b.png");
    assert_eq!((rotated.width(), rotated.height()), (60, 120));
    let black = rotated.samples().iter().filter(|v| **v == 0).count();
    assert_eq!(black, 60 * 120 / 2, "half of the picture still burns after a quarter turn");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(frame_bounds(&h, doc), (12.0, 6.0), "one undo turns it back");

    // Replace with a 8 x 4 px picture: the frame keeps its placement and size (contract), pixels change.
    let other = png(8, 4, &[255; 32]);
    let new_file = put(&h, "andere.png", &other);
    h.send(UiIntent::ReplaceImage { document: doc, path: new_file });
    h.run_tasks();
    assert_eq!(frame_bounds(&h, doc), (12.0, 6.0));
    let replaced = processed(&mut h, doc, "c.png");
    assert_eq!((replaced.width(), replaced.height()), (120, 60));
    assert!(replaced.samples().iter().all(|v| *v == 255), "the new, all-white pixels");
    h.invoke(doc, CommandKey::Undo);
    let back = processed(&mut h, doc, "d.png");
    assert_eq!(back.samples(), first.samples(), "undo restores the previous pixels exactly");

    // Refresh from the changed original file.
    let inverse: Vec<u8> = (0..6).flat_map(|_| (0..12).map(|x| if x < 6 { 255 } else { 0 })).collect();
    put_over(&h, &path, &png(12, 6, &inverse));
    h.invoke(doc, CommandKey::RefreshImage);
    h.run_tasks();
    let refreshed = processed(&mut h, doc, "e.png");
    assert!(
        refreshed.samples()[..60].iter().all(|v| *v == 255) && refreshed.samples()[60..120].iter().all(|v| *v == 0)
    );
    assert_eq!(frame_bounds(&h, doc), (12.0, 6.0));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(processed(&mut h, doc, "f.png").samples(), first.samples());
}

fn put_over(h: &HeadlessEditorHarness, path: &FsPath, bytes: &[u8]) {
    use burnburn_io::fs::FileSystem;
    let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, bytes).unwrap();
    h.fs().replace(&temp, path).unwrap();
}

#[test]
fn im03_mask_clips_to_the_shape_and_the_crop_window_is_the_covered_source_part() {
    let (mut h, doc, _) = imported();
    let frame = h.document(doc).unwrap().objects[0].id;
    if h.view().commands[&CommandKey::SnapToObjects].checked {
        h.invoke(doc, CommandKey::SnapToObjects);
    }
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (3.0, 1.0), (9.0, 5.0)); // spans the black/white border at x = 6
    h.invoke(doc, CommandKey::ToolSelect);
    let shape = h.document(doc).unwrap().objects.last().unwrap().id;
    h.send(UiIntent::Select { document: doc, ids: vec![frame, shape], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::MaskImage);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 2, "mask adds nothing");
    h.send(UiIntent::Select { document: doc, ids: vec![frame], mode: SelectMode::Replace });
    // The processed bitmap of the masked picture is the shape's bounding box: 6 x 4 mm.
    h.send(UiIntent::Select { document: doc, ids: vec![shape], mode: SelectMode::Replace });
    let out = processed(&mut h, doc, "mask.png");
    assert_eq!((out.width(), out.height()), (60, 40));
    // x 3..6 black, 6..9 white: the left 30 columns burn, the right 30 do not.
    for row in out.samples().chunks(60) {
        assert!(row[..30].iter().all(|v| *v == 0) && row[30..].iter().all(|v| *v == 255));
    }
    h.invoke(doc, CommandKey::Undo);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 2, "undo of the mask keeps the drawn rectangle (a separate entry)");
}

fn inverse_png() -> Vec<u8> {
    let inverse: Vec<u8> = (0..6).flat_map(|_| (0..12).map(|x| if x < 6 { 255 } else { 0 })).collect();
    png(12, 6, &inverse)
}

#[test]
fn im03_refresh_after_replace_and_undo_reads_the_original_file_again() {
    let (mut h, doc, path) = imported();
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    let first = processed(&mut h, doc, "a.png");
    let new_file = put(&h, "andere.png", &png(8, 4, &[255; 32]));
    h.send(UiIntent::ReplaceImage { document: doc, path: new_file });
    h.run_tasks();
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(processed(&mut h, doc, "b.png").samples(), first.samples(), "undo restored the pixels");
    put_over(&h, &path, &inverse_png());
    h.invoke(doc, CommandKey::RefreshImage);
    h.run_tasks();
    let refreshed = processed(&mut h, doc, "c.png");
    assert!(
        refreshed.samples()[..60].iter().all(|v| *v == 255) && refreshed.samples()[60..120].iter().all(|v| *v == 0),
        "refresh must re-read the file this picture came from (plaat.png), not the replaced one"
    );
}
