//! G7-trace, T10: the user's own recipes, the A/B comparison and tracing a batch of files. Recipes
//! live in the user's folder and carry the settings of a kind of image, not those of one picture;
//! the comparison shows two recipes on one picture; a batch gives one report line per file and can be
//! cancelled.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_image::encode_png_gray8;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_presentation::{
    BatchProblemView, HeadlessEditorHarness, MaskMode, RecipeChoice, RecipeIssue, ThresholdChoice, TraceMode,
    TracePick, TracePreset, TraceRecipeIntent, TraceRecipesViewModel, UiIntent,
};
use burnburn_types::DocumentId;
use g7_support::*;

const USER_DIR: &str = "/Users/ik/Library/Application Support/BurnBurn";

fn started(fs: Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.controller().set_user_dir(PathBuf::from(USER_DIR));
    h
}

fn open(h: &mut HeadlessEditorHarness, name: &str, bytes: &[u8]) -> DocumentId {
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), &FsPath::new("/work").join(name), bytes).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work").join(name) });
    h.run_tasks();
    doc
}

fn recipes(h: &HeadlessEditorHarness, doc: DocumentId) -> TraceRecipesViewModel {
    vm(h, doc).trace_recipes.expect("the recipe panel is offered while a tracer is open")
}

fn recipe(h: &mut HeadlessEditorHarness, doc: DocumentId, intent: TraceRecipeIntent) {
    h.send(UiIntent::TraceRecipe { document: doc, intent });
    h.run_tasks();
}

fn names(r: &TraceRecipesViewModel) -> Vec<&str> {
    r.saved.iter().map(|s| s.name.as_str()).collect()
}

/// A plain PNG without a resolution in its header.
fn undated(w: usize, h: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let gray: Vec<u8> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| pixel(x, y)).collect();
    encode_png_gray8(w as u32, h as u32, &gray).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Saving, applying, deleting
// ---------------------------------------------------------------------------------------------

#[test]
fn a_recipe_is_saved_under_a_name_applied_to_another_picture_and_survives_a_restart() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    let doc = open(&mut h, "scene.png", &scene());
    // A recipe made for this kind of picture: 200 fixed, curves off, ignore specks below 3 mm².
    edit(&mut h, doc, |f| {
        f.threshold_choice = ThresholdChoice::Fixed;
        f.threshold = s("200");
        f.curves = false;
        f.ignore_below_mm2 = s("3");
        f.smoothness_mm = s("0.2");
    });
    // Things that belong to this picture only: a mask and a lens.
    pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Remove, radius_px: 4.0 });
    pick(&mut h, doc, TracePick::Stroke { points: vec![[20.0, 15.0]] });
    assert_eq!(prep(&h, doc).masks.len(), 1);

    recipe(&mut h, doc, TraceRecipeIntent::Save { name: s("  Mijn zwart op wit ") });
    let r = recipes(&h, doc);
    assert_eq!(r.issue, None);
    assert_eq!(names(&r), ["Mijn zwart op wit"], "stored trimmed");
    assert_eq!(r.saved[0].mode, TraceMode::Contour);
    assert!(fs.files().contains(&FsPath::new(USER_DIR).join("trace-recipes.json")), "{:?}", fs.files());

    // Another picture, another resolution (127 dpi): the recipe brings its settings, not the
    // resolution, the mask or anything else of the first picture.
    let second = open(&mut h, "other.png", &png_of_dpi(&scene_gray(), 200, 120, 5_000));
    recipe(&mut h, second, TraceRecipeIntent::Apply { name: s("Mijn zwart op wit") });
    let form = tracer(&h, second).form.input;
    assert_eq!(recipes(&h, second).issue, None);
    assert_eq!((form.threshold_choice, form.threshold.as_str(), form.curves), (ThresholdChoice::Fixed, "200", false));
    assert_eq!((form.ignore_below_mm2.as_str(), form.smoothness_mm.as_str()), ("3", "0.2"));
    assert_eq!(form.dpi, "127", "the resolution of the picture in the tracer stays");
    assert!(prep(&h, second).masks.is_empty(), "the mask of the first picture did not travel");

    // A second start of the app reads the recipe again.
    let mut again = started(Arc::clone(&fs));
    let doc = open(&mut again, "scene.png", &scene());
    assert_eq!(names(&recipes(&again, doc)), ["Mijn zwart op wit"]);
}

#[test]
fn saving_under_a_known_name_replaces_it_and_a_recipe_can_be_deleted() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    let doc = open(&mut h, "scene.png", &scene());
    recipe(&mut h, doc, TraceRecipeIntent::Save { name: s("A") });
    edit(&mut h, doc, |f| f.smoothness_mm = s("0.3"));
    recipe(&mut h, doc, TraceRecipeIntent::Save { name: s("B") });
    edit(&mut h, doc, |f| f.smoothness_mm = s("0.4"));
    recipe(&mut h, doc, TraceRecipeIntent::Save { name: s("A") });
    assert_eq!(names(&recipes(&h, doc)), ["A", "B"]);
    edit(&mut h, doc, |f| f.smoothness_mm = s("0.05"));
    recipe(&mut h, doc, TraceRecipeIntent::Apply { name: s("A") });
    assert_eq!(tracer(&h, doc).form.input.smoothness_mm, "0.4", "A holds the later settings");

    recipe(&mut h, doc, TraceRecipeIntent::Delete { name: s("A") });
    assert_eq!(names(&recipes(&h, doc)), ["B"]);
    let mut second = started(Arc::clone(&fs));
    let doc2 = open(&mut second, "scene.png", &scene());
    assert_eq!(names(&recipes(&second, doc2)), ["B"], "the deletion is on disk too");
}

#[test]
fn a_recipe_that_cannot_be_named_or_found_is_refused_and_nothing_changes() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    let doc = open(&mut h, "scene.png", &scene());
    recipe(&mut h, doc, TraceRecipeIntent::Save { name: s("   ") });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::EmptyName));
    recipe(&mut h, doc, TraceRecipeIntent::Save { name: "x".repeat(81) });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::NameTooLong));
    recipe(&mut h, doc, TraceRecipeIntent::Apply { name: s("bestaat niet") });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::Unknown));
    recipe(&mut h, doc, TraceRecipeIntent::Delete { name: s("bestaat niet") });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::Unknown));
    assert!(recipes(&h, doc).saved.is_empty());
}

#[test]
fn a_recipe_file_that_cannot_be_read_is_reported_and_left_alone() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.create_dir_all_for_test(FsPath::new(USER_DIR));
    let file = FsPath::new(USER_DIR).join("trace-recipes.json");
    burnburn_io::fs::FileSystem::write_all(fs.as_ref(), &file, b"{\"format\": \"oops\"").unwrap();
    let mut h = started(Arc::clone(&fs));
    let doc = open(&mut h, "scene.png", &scene());
    assert!(recipes(&h, doc).load_problem);
    assert!(recipes(&h, doc).saved.is_empty());
    assert_eq!(burnburn_io::fs::FileSystem::read(fs.as_ref(), &file, 1 << 20).unwrap(), b"{\"format\": \"oops\"");
}

// ---------------------------------------------------------------------------------------------
// A/B comparison
// ---------------------------------------------------------------------------------------------

#[test]
fn two_recipes_are_traced_on_one_picture_and_shown_with_their_numbers_and_outlines() {
    let (mut h, doc) = opened_scene();
    // B is the photo preset (local threshold, median 1, ignore below 0.5 mm², fit 0.1 mm).
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) });
    let c = recipes(&h, doc).compare.expect("a comparison");
    assert_eq!(recipes(&h, doc).issue, None);
    assert!(!c.computing);
    let (a, b) = (c.a.expect("numbers of A"), c.b.expect("numbers of B"));
    // A is the plain trace of the scene: ring with counter, rectangle, far disc.
    assert_eq!((a.closed_shapes, a.holes), (3, 1));
    assert_ne!((a.nodes, a.max_deviation_mm), (b.nodes, b.max_deviation_mm), "the recipes differ in what they give");
    assert!(b.max_deviation_mm >= a.max_deviation_mm - 1e-9 || b.nodes != a.nodes);
    assert!(!c.same_geometry);
    let (ao, bo) = (c.a_outline.expect("outline of A"), c.b_outline.expect("outline of B"));
    // Both are in the frame of the same picture (20 × 12 mm), so they can be laid over each other.
    for outline in [&ao, &bo] {
        let b = outline.bounds().unwrap();
        assert!(b.min().x >= -1e-9 && b.max().x <= 20.0 + 1e-9 && b.max().y <= 12.0 + 1e-9);
    }
    assert_eq!(c.b_choice, RecipeChoice::Preset(TracePreset::Photo));

    // Taking B over makes it the recipe in force: the form follows, the trace is made again.
    recipe(&mut h, doc, TraceRecipeIntent::AdoptCompared);
    assert_eq!(tracer(&h, doc).form.preset, Some(TracePreset::Photo));
    assert_eq!(tracer(&h, doc).form.input.threshold_choice, ThresholdChoice::Adaptive);
    assert!(recipes(&h, doc).compare.is_none(), "the comparison is done");
}

#[test]
fn comparing_a_recipe_with_itself_gives_the_same_geometry_and_a_saved_recipe_can_be_b() {
    let (mut h, doc) = opened_scene();
    recipe(&mut h, doc, TraceRecipeIntent::Save { name: s("zelfde") });
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Saved(s("zelfde")) });
    let c = recipes(&h, doc).compare.expect("a comparison");
    assert!(c.same_geometry, "identical inputs give an identical geometry");
    assert_eq!(c.a, c.b);
    assert_eq!(c.b_choice, RecipeChoice::Saved(s("zelfde")));
    recipe(&mut h, doc, TraceRecipeIntent::ClearCompare);
    assert!(recipes(&h, doc).compare.is_none());
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Saved(s("bestaat niet")) });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::Unknown));
    assert!(recipes(&h, doc).compare.is_none());
}

#[test]
fn only_the_newest_comparison_is_shown() {
    let (mut h, doc) = opened_scene();
    h.send(UiIntent::TraceRecipe {
        document: doc,
        intent: TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) },
    });
    assert!(recipes(&h, doc).compare.expect("started").computing);
    h.send(UiIntent::TraceRecipe {
        document: doc,
        intent: TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Technical) },
    });
    h.run_tasks();
    let c = recipes(&h, doc).compare.expect("the newer comparison");
    assert!(!c.computing);
    assert_eq!(c.b_choice, RecipeChoice::Preset(TracePreset::Technical));
}

#[test]
fn comparing_needs_a_contour_trace_with_a_resolution() {
    let (mut h, doc) = opened_scene();
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Logo) });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::NeedsContourMode));
    let bare = open(&mut h, "bare.png", &undated(40, 30, |x, _| if x < 20 { 0 } else { 255 }));
    recipe(&mut h, bare, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Logo) });
    assert_eq!(recipes(&h, bare).issue, Some(RecipeIssue::NeedsScale));
}

// ---------------------------------------------------------------------------------------------
// Batch
// ---------------------------------------------------------------------------------------------

fn files(h: &HeadlessEditorHarness) -> Vec<PathBuf> {
    let put = |name: &str, bytes: &[u8]| {
        burnburn_io::fs::FileSystem::write_all(h.fs(), &FsPath::new("/batch").join(name), bytes).unwrap();
        PathBuf::from("/batch").join(name)
    };
    h.fs().create_dir_all_for_test(FsPath::new("/batch"));
    vec![
        put("a-logo.png", &image(100, 60, |x, y| if inside_disc(x, y, 50.0, 30.0, 20.0) { 0 } else { 255 })),
        put("b-leeg.png", &image(100, 60, |_, _| 255)),
        put("c-broken.png", b"this is not a picture"),
        // 3 mm² speck (r 5 px of 0.1 mm = 0.79 mm²) beside a big disc.
        put(
            "d-speck.png",
            &image(100, 60, |x, y| {
                if inside_disc(x, y, 30.0, 30.0, 20.0) || inside_disc(x, y, 80.0, 30.0, 5.0) { 0 } else { 255 }
            }),
        ),
    ]
}

#[test]
fn a_batch_gives_one_line_per_file_with_what_to_look_at() {
    let (mut h, doc) = opened_scene();
    edit(&mut h, doc, |f| f.ignore_below_mm2 = s("1"));
    let paths = files(&h);
    recipe(&mut h, doc, TraceRecipeIntent::RunBatch { paths: paths.clone() });
    let b = recipes(&h, doc).batch.expect("a batch report");
    assert_eq!(recipes(&h, doc).issue, None);
    assert!(!b.running && !b.cancelled);
    assert_eq!(b.total, 4);
    assert_eq!(
        b.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["a-logo.png", "b-leeg.png", "c-broken.png", "d-speck.png"]
    );
    // a: one disc, nothing to look at.
    assert_eq!((b.rows[0].summary.unwrap().closed_shapes, b.rows[0].problems.len()), (1, 0));
    // b: an empty sheet gives no geometry.
    assert_eq!(b.rows[1].problems, [BatchProblemView::NoGeometry]);
    // c: a file that is no picture is said to be unreadable; the batch went on.
    assert!(b.rows[2].summary.is_none());
    assert_eq!(b.rows[2].problems, [BatchProblemView::Unreadable]);
    // d: the speck is dropped by "ignore less than 1 mm²" and reported.
    assert_eq!(b.rows[3].summary.unwrap().closed_shapes, 1);
    assert_eq!(b.rows[3].problems, [BatchProblemView::RemovedFeatures { count: 1 }]);
    assert_eq!(b.problem_count(), 3);

    recipe(&mut h, doc, TraceRecipeIntent::ClearBatch);
    assert!(recipes(&h, doc).batch.is_none());
}

#[test]
fn a_file_without_a_resolution_is_traced_at_the_scale_of_the_recipe_or_reported() {
    // The tracer holds a picture with a resolution: every file is traced at that scale.
    let (mut h, doc) = opened_scene();
    let plain = undated(60, 40, |x, y| if inside_disc(x, y, 30.0, 20.0, 12.0) { 0 } else { 255 });
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/undated.png"), &plain).unwrap();
    recipe(&mut h, doc, TraceRecipeIntent::RunBatch { paths: vec![PathBuf::from("/work/undated.png")] });
    let row = recipes(&h, doc).batch.unwrap().rows.remove(0);
    assert_eq!(row.problems, []);
    assert_eq!(row.summary.unwrap().closed_shapes, 1);

    // The tracer holds a picture without one: the files bring their own, or are reported.
    let bare = open(&mut h, "bare.png", &undated(40, 30, |x, _| if x < 20 { 0 } else { 255 }));
    let dated = png_of_dpi(&scene_gray(), 200, 120, 10_000);
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/dated.png"), &dated).unwrap();
    recipe(
        &mut h,
        bare,
        TraceRecipeIntent::RunBatch {
            paths: vec![PathBuf::from("/work/dated.png"), PathBuf::from("/work/undated.png")],
        },
    );
    let rows = recipes(&h, bare).batch.unwrap().rows;
    assert_eq!(rows[0].problems, []);
    assert_eq!(rows[0].summary.unwrap().closed_shapes, 3);
    assert_eq!(rows[1].problems, [BatchProblemView::NeedsScale]);
}

#[test]
fn a_batch_can_be_cancelled_and_leaves_no_report() {
    let (mut h, doc) = opened_scene();
    let paths = files(&h);
    h.send(UiIntent::TraceRecipe { document: doc, intent: TraceRecipeIntent::RunBatch { paths } });
    let running = recipes(&h, doc).batch.expect("started");
    assert!(running.running && running.rows.is_empty());
    h.send(UiIntent::TraceRecipe { document: doc, intent: TraceRecipeIntent::CancelBatch });
    h.run_tasks();
    let b = recipes(&h, doc).batch.expect("the batch is still shown, as cancelled");
    assert!(b.cancelled && !b.running && b.rows.is_empty());

    // A new batch afterwards works.
    let first = files(&h)[..1].to_vec();
    recipe(&mut h, doc, TraceRecipeIntent::RunBatch { paths: first });
    let b = recipes(&h, doc).batch.unwrap();
    assert!(!b.cancelled && b.rows.len() == 1);
}

#[test]
fn a_batch_without_files_or_in_centre_line_mode_is_refused() {
    let (mut h, doc) = opened_scene();
    recipe(&mut h, doc, TraceRecipeIntent::RunBatch { paths: vec![] });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::NoFiles));
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    let all = files(&h);
    recipe(&mut h, doc, TraceRecipeIntent::RunBatch { paths: all });
    assert_eq!(recipes(&h, doc).issue, Some(RecipeIssue::NeedsContourMode));
    assert!(recipes(&h, doc).batch.is_none());
}
