//! The golf-7 trace sections of the preparation panel, headless (G7-trace): lens, pointing, protect
//! and keep, node goal, dropped pieces, own recipes, A/B comparison and batch. What the panel shows
//! comes from presentation's view models in the window language; a button sends the typed text of its
//! fields as one intent (DR-05).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod g5_fixtures;

use burnburn_macos::l10n::Locale;
use burnburn_macos::l10n_g7_trace::{G7Text, g7_text};
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, compare_bitmap, g5_intent, g5_panel};
use burnburn_presentation::{
    HeadlessEditorHarness, MaskMode, RecipeChoice, TraceFormInput, TracePick, TracePrepIntent, TracePreset,
    TraceRecipeIntent, UiIntent,
};
use burnburn_types::DocumentId;
use g5_fixtures::ring_png;

fn opened() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/in.png"), &ring_png()).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: std::path::PathBuf::from("/work/in.png") });
    h.run_tasks();
    (h, doc)
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::TracePrep, locale, &[]).expect("the preparation panel").lines
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    lines(h, doc, locale)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn has_text(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale, part: &str) -> bool {
    texts(h, doc, locale).iter().any(|t| t.contains(part))
}

fn has(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> bool {
    lines(h, doc, Locale::En).iter().any(|l| l.control() == Some(c))
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, values: &[(C, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (*c, (*v).to_owned())).collect();
    let intent = g5_intent(&h.view(), doc, G5Panel::TracePrep, c, &values)?;
    h.send(intent.clone());
    h.run_tasks();
    Some(intent)
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, change: impl FnOnce(&mut TraceFormInput)) {
    let mut input = h.document(doc).unwrap().tracer.unwrap().form.input;
    change(&mut input);
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
}

// --- Lens and pointing ---------------------------------------------------------------------------

#[test]
fn the_lens_buttons_send_the_typed_coefficients_and_the_panel_says_the_correction_is_in_use() {
    let (mut h, doc) = opened();
    assert!(has_text(&h, doc, Locale::En, "No lens correction."));
    assert!(press(&mut h, doc, C::LensClear, &[]).is_none(), "nothing to remove yet");
    let intent = press(&mut h, doc, C::LensApply, &[(C::LensK1, "-0,12"), (C::LensK2, "0.02")]).unwrap();
    assert_eq!(
        intent,
        UiIntent::TracePrep {
            document: doc,
            intent: TracePrepIntent::UseLens { k1: "-0,12".into(), k2: "0.02".into() }
        }
    );
    assert!(has_text(&h, doc, Locale::En, "Lens correction in use: k1 -0.12, k2 0.02."));
    assert!(has_text(&h, doc, Locale::Nl, "Lenscorrectie in gebruik: k1 -0.12, k2 0.02."));
    press(&mut h, doc, C::LensApply, &[(C::LensK1, "abc"), (C::LensK2, "0")]);
    assert!(has_text(&h, doc, Locale::En, "Lens k1: this is not a number."));
    press(&mut h, doc, C::LensClear, &[]).unwrap();
    assert!(has_text(&h, doc, Locale::En, "No lens correction."));
}

#[test]
fn the_pointing_buttons_start_corners_or_the_brush_with_the_typed_radius_and_stop() {
    let (mut h, doc) = opened();
    assert!(has_text(&h, doc, Locale::En, "Nothing is being pointed at."));
    assert!(press(&mut h, doc, C::PickEnd, &[]).is_none(), "nothing to stop");
    assert_eq!(
        press(&mut h, doc, C::PickCorners, &[]).unwrap(),
        UiIntent::TracePick { document: doc, pick: TracePick::BeginCorners }
    );
    assert!(has_text(&h, doc, Locale::En, "0 of 4 corners clicked."));
    let brush = press(&mut h, doc, C::PickBrushRemove, &[(C::BrushRadius, "8")]).unwrap();
    assert_eq!(
        brush,
        UiIntent::TracePick {
            document: doc,
            pick: TracePick::BeginBrushTyped { mode: MaskMode::Remove, radius: "8".into() }
        }
    );
    assert!(has_text(&h, doc, Locale::En, "Brush in hand: remove, radius 8 px."));
    assert!(has_text(&h, doc, Locale::Nl, "Kwast in de hand: verwijderen, straal 8 px."));
    press(&mut h, doc, C::PickBrushKeep, &[(C::BrushRadius, "x")]);
    assert!(has_text(&h, doc, Locale::En, "Radius of the brush: this is not a number."));
    press(&mut h, doc, C::PickEnd, &[]).unwrap();
    assert!(has_text(&h, doc, Locale::En, "Nothing is being pointed at."));
}

// --- Contours: protect, keep, node goal, dropped pieces --------------------------------------------

#[test]
fn every_contour_has_a_protect_and_a_keep_check_that_send_their_new_state() {
    let (mut h, doc) = opened();
    let rows = h.document(doc).unwrap().trace_prep.unwrap().contours;
    assert_eq!(rows.len(), 2, "the ring and its inside");
    let hole = rows.iter().find(|c| c.hole).unwrap().id;
    assert!(has(&h, doc, C::ContourProtect { id: hole }) && has(&h, doc, C::ContourKeep { id: hole }));
    assert!(has_text(&h, doc, Locale::En, "Contour 1: hole") || has_text(&h, doc, Locale::En, "hole,"));
    let intent = press(&mut h, doc, C::ContourProtect { id: hole }, &[(C::ContourProtect { id: hole }, "1")]).unwrap();
    assert_eq!(
        intent,
        UiIntent::TracePrep { document: doc, intent: TracePrepIntent::ProtectContour { id: hole, on: true } }
    );
    let protected = lines(&h, doc, Locale::En)
        .into_iter()
        .any(|l| matches!(l, G5Line::Check { control: C::ContourProtect { id }, on: true, .. } if id == hole));
    assert!(protected, "the check shows the new state");
    // A check without a reported value sends nothing.
    assert!(press(&mut h, doc, C::ContourKeep { id: hole }, &[]).is_none());
    press(&mut h, doc, C::ContourKeep { id: hole }, &[(C::ContourKeep { id: hole }, "1")]).unwrap();
    assert!(h.document(doc).unwrap().trace_prep.unwrap().contours.iter().any(|c| c.id == hole && c.kept));
}

#[test]
fn the_node_goal_is_typed_and_the_correction_says_whether_it_was_reached() {
    let (mut h, doc) = opened();
    assert!(has_text(&h, doc, Locale::En, "No node goal."));
    let intent = press(&mut h, doc, C::NodeApply, &[(C::NodeLimit, "1000"), (C::NodeTolerance, "2")]).unwrap();
    assert_eq!(
        intent,
        UiIntent::TracePrep {
            document: doc,
            intent: TracePrepIntent::SetNodeTarget { max_nodes: "1000".into(), max_tolerance_mm: "2".into() }
        }
    );
    assert!(has_text(&h, doc, Locale::En, "Goal: at most 1000 nodes, tolerance up to 2 mm."));
    // A correction of the whole ring with that goal: reached.
    press(
        &mut h,
        doc,
        C::PrepRetrace,
        &[
            (C::PrepAreaX, "0"),
            (C::PrepAreaY, "0"),
            (C::PrepAreaWidth, "64"),
            (C::PrepAreaHeight, "64"),
            (C::PrepRegionThreshold, "100"),
        ],
    )
    .unwrap();
    assert!(has_text(&h, doc, Locale::En, "node goal 1000 met with"));
    // One node in all cannot be: said as a problem.
    press(&mut h, doc, C::NodeApply, &[(C::NodeLimit, "1"), (C::NodeTolerance, "0.06")]).unwrap();
    press(
        &mut h,
        doc,
        C::PrepRetrace,
        &[
            (C::PrepAreaX, "0"),
            (C::PrepAreaY, "0"),
            (C::PrepAreaWidth, "64"),
            (C::PrepAreaHeight, "64"),
            (C::PrepRegionThreshold, "100"),
        ],
    )
    .unwrap();
    let missed = lines(&h, doc, Locale::En)
        .into_iter()
        .any(|l| matches!(l, G5Line::Problem(t) if t.contains("node goal 1 NOT met")));
    assert!(missed, "{:?}", texts(&h, doc, Locale::En));
    press(&mut h, doc, C::NodeApply, &[(C::NodeLimit, "abc"), (C::NodeTolerance, "2")]);
    assert!(has_text(&h, doc, Locale::En, "At most nodes: this is not a number."));
}

#[test]
fn a_dropped_piece_is_listed_with_a_button_that_brings_it_back() {
    let (mut h, doc) = opened();
    // The inside of the ring is π·1.4² = 6.2 mm²: below 10 mm² it is dropped.
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "10".to_owned());
    assert!(has(&h, doc, C::RestoreRemoved { index: 0 }));
    assert!(has_text(&h, doc, Locale::En, "Piece 1: hole"));
    let intent = press(&mut h, doc, C::RestoreRemoved { index: 0 }, &[]).unwrap();
    assert_eq!(intent, UiIntent::TracePrep { document: doc, intent: TracePrepIntent::RestoreRemoved { index: 0 } });
    assert!(!has(&h, doc, C::RestoreRemoved { index: 0 }), "it is back");
    assert!(h.document(doc).unwrap().trace_prep.unwrap().contours.iter().any(|c| c.hole && c.kept));
}

// --- Recipes, A/B, batch -----------------------------------------------------------------------------

#[test]
fn recipes_are_kept_applied_compared_and_deleted_from_the_panel() {
    let (mut h, doc) = opened();
    assert!(has_text(&h, doc, Locale::En, "No own recipes yet."));
    let save = press(&mut h, doc, C::RecipeSave, &[(C::RecipeName, "Ring scherp")]).unwrap();
    assert_eq!(
        save,
        UiIntent::TraceRecipe { document: doc, intent: TraceRecipeIntent::Save { name: "Ring scherp".into() } }
    );
    assert!(has_text(&h, doc, Locale::En, "Recipe: Ring scherp (contour)"));
    assert!(has(&h, doc, C::RecipeApply { index: 0 }) && has(&h, doc, C::RecipeDelete { index: 0 }));

    let compare = press(&mut h, doc, C::RecipeCompare { index: 0 }, &[]).unwrap();
    assert_eq!(
        compare,
        UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::Compare { with: RecipeChoice::Saved("Ring scherp".into()) }
        }
    );
    assert!(has_text(&h, doc, Locale::En, "B is: Ring scherp"));
    assert!(has_text(&h, doc, Locale::En, "A and B give exactly the same geometry."));
    assert!(has_text(&h, doc, Locale::Nl, "A en B geven precies dezelfde geometrie."));

    // A built-in recipe from the pop-up: other numbers, a picture of both outlines.
    let index = TracePreset::ALL.iter().position(|p| *p == TracePreset::Photo).unwrap().to_string();
    let intent = press(&mut h, doc, C::RecipeComparePreset, &[(C::RecipePreset, &index)]).unwrap();
    assert_eq!(
        intent,
        UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) }
        }
    );
    assert!(lines(&h, doc, Locale::En).iter().any(|l| matches!(l, G5Line::Image(_))), "both outlines in one picture");
    assert!(has_text(&h, doc, Locale::En, "A (current):") && has_text(&h, doc, Locale::En, "B: "));
    press(&mut h, doc, C::CompareAdopt, &[]).unwrap();
    assert!(!has(&h, doc, C::CompareAdopt), "the comparison is closed");

    press(&mut h, doc, C::RecipeDelete { index: 0 }, &[]).unwrap();
    assert!(has_text(&h, doc, Locale::En, "No own recipes yet."));
    // An index that is no recipe sends nothing.
    assert!(press(&mut h, doc, C::RecipeApply { index: 5 }, &[]).is_none());
}

#[test]
fn a_batch_takes_one_path_per_line_and_reports_each_file() {
    let (mut h, doc) = opened();
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/two.png"), &ring_png()).unwrap();
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/bad.png"), b"nope").unwrap();
    let intent = press(&mut h, doc, C::BatchRun, &[(C::BatchFiles, "/work/two.png\n\n  /work/bad.png  \n")]).unwrap();
    assert_eq!(
        intent,
        UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::RunBatch { paths: vec!["/work/two.png".into(), "/work/bad.png".into()] }
        }
    );
    assert!(has_text(&h, doc, Locale::En, "2 file(s), 1 with something to look at."));
    assert!(has_text(&h, doc, Locale::En, "two.png: 1 parts, 1 holes,"));
    assert!(has_text(&h, doc, Locale::En, "bad.png: not traced"));
    assert!(has_text(&h, doc, Locale::En, "could not be read or is not a picture"));
    assert!(has_text(&h, doc, Locale::Nl, "kon niet gelezen worden of is geen afbeelding"));
    // Nothing to trace: the refusal is said.
    press(&mut h, doc, C::BatchClear, &[]).unwrap();
    press(&mut h, doc, C::BatchRun, &[(C::BatchFiles, "  \n")]).unwrap();
    assert!(has_text(&h, doc, Locale::En, "No files given."));
}

#[test]
fn a_cancelled_batch_is_said_and_the_cancel_button_only_works_while_it_runs() {
    let (mut h, doc) = opened();
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/two.png"), &ring_png()).unwrap();
    let run =
        g5_intent(&h.view(), doc, G5Panel::TracePrep, C::BatchRun, &[(C::BatchFiles, "/work/two.png".to_owned())])
            .unwrap();
    h.send(run);
    assert!(has_text(&h, doc, Locale::En, "Working on 1 file(s)…"));
    press(&mut h, doc, C::BatchCancel, &[]).unwrap();
    assert!(has_text(&h, doc, Locale::En, "The batch was cancelled; there is no report."));
    assert!(!has_text(&h, doc, Locale::En, "Working on"));
}

// --- The picture of the comparison, the words --------------------------------------------------------

#[test]
fn the_comparison_picture_draws_a_black_and_a_red_outline_at_one_scale() {
    use burnburn_geometry::{Path, Rect};
    use burnburn_types::Length;
    let rect = |w: f64, h: f64| {
        Path::rectangle(
            Rect::new(
                burnburn_geometry::Point::new(0.0, 0.0),
                Length::from_mm(w).unwrap(),
                Length::from_mm(h).unwrap(),
            )
            .unwrap(),
        )
    };
    let bitmap = compare_bitmap(&rect(10.0, 5.0), &rect(8.0, 5.0), 100).expect("a picture");
    assert!(bitmap.width <= 100 && bitmap.height <= 100);
    let pixels: Vec<&[u8]> = bitmap.rgba.chunks(4).collect();
    let black = pixels.iter().filter(|p| p[0] == 0 && p[1] == 0 && p[2] == 0).count();
    let red = pixels.iter().filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
    assert!(black > 20 && red > 20, "black {black}, red {red}");
    // The smaller rectangle ends earlier (8 of 10 mm): red lies inside the black box, never beyond it.
    let right_edge_red =
        pixels.chunks(bitmap.width as usize).any(|row| row.last().is_some_and(|p| p[0] > 200 && p[1] < 60));
    assert!(!right_edge_red);
    assert_eq!(compare_bitmap(&Path::from_subpaths(Vec::new()), &Path::from_subpaths(Vec::new()), 100), None);
}

#[test]
fn every_text_has_both_languages_and_the_same_placeholders() {
    for text in G7Text::ALL {
        let (nl, en) = (g7_text(*text, Locale::Nl), g7_text(*text, Locale::En));
        assert!(!nl.trim().is_empty() && !en.trim().is_empty(), "{text:?}");
        for n in 0..8 {
            let mark = format!("{{{n}}}");
            assert_eq!(nl.contains(&mark), en.contains(&mark), "{text:?} placeholder {mark}");
        }
    }
}
