//! G8-trace, T10: the A/B comparison with the local corrections of the tracer, and its two outlines
//! on the canvas. A is the trace as the tracer shows it (corrections included); B is the other recipe
//! with the same corrections where they still bind. The outlines lie on the sheet where applying
//! the trace would put them, A and B in their own look, as a window aid that goes away with the
//! comparison.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_editor::{CompareSide, Overlay};
use burnburn_presentation::{
    CompareCorrectionsView, HeadlessEditorHarness, RecipeChoice, TracePreset, TraceRecipeIntent, TraceRecipesViewModel,
    UiIntent,
};
use burnburn_types::DocumentId;
use g7_support::*;

const REGION: [&str; 4] = ["30", "40", "90", "70"];

fn recipe(h: &mut HeadlessEditorHarness, doc: DocumentId, intent: TraceRecipeIntent) {
    h.send(UiIntent::TraceRecipe { document: doc, intent });
    h.run_tasks();
}

fn recipes(h: &HeadlessEditorHarness, doc: DocumentId) -> TraceRecipesViewModel {
    vm(h, doc).trace_recipes.expect("the recipe panel")
}

fn outlines(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(CompareSide, std::sync::Arc<burnburn_geometry::Path>)> {
    h.scene(doc)
        .expect("a scene")
        .overlays
        .iter()
        .filter_map(|o| match o {
            Overlay::CompareOutline { path, side } => Some((*side, std::sync::Arc::clone(path))),
            _ => None,
        })
        .collect()
}

#[test]
fn a_comparison_carries_the_local_corrections_and_variant_a_is_the_trace_on_screen() {
    let (mut h, doc) = opened_scene();
    // The correction: the area at 200 fills the counter (150) and takes the speck (170).
    retrace(&mut h, doc, REGION, "200", "");
    assert_eq!(counts(&h, doc), (4, 0));
    recipe(&mut h, doc, TraceRecipeIntent::Save { name: s("zelfde") });
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Saved(s("zelfde")) });
    let c = recipes(&h, doc).compare.expect("a comparison");
    assert_eq!(c.corrections, Some(CompareCorrectionsView { stored: 1, a_applied: 1, b_applied: 1 }));
    let (a, b) = (c.a.expect("A"), c.b.expect("B"));
    assert_eq!((a.closed_shapes, a.holes), (4, 0), "A is what the tracer shows, not the plain trace (3, 1)");
    assert_eq!((b.closed_shapes, b.holes), (4, 0));
    assert!(c.same_geometry);
}

#[test]
fn a_correction_that_no_longer_binds_on_b_is_said_and_b_has_none() {
    let (mut h, doc) = opened_scene();
    let id = counter_id(&h, doc);
    send(&mut h, doc, burnburn_presentation::TracePrepIntent::ProtectContour { id, on: true });
    retrace(&mut h, doc, REGION, "200", "");
    // The photo preset traces with a local threshold: the contours the correction was bound to change.
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) });
    let c = recipes(&h, doc).compare.expect("a comparison");
    let view = c.corrections.expect("corrections are stored");
    assert_eq!((view.stored, view.a_applied), (1, 1));
    assert_eq!(view.b_applied, 0, "the photo recipe sees other contours: nothing is applied to them");
    assert!(!c.same_geometry);
}

#[test]
fn without_local_corrections_the_comparison_says_nothing_about_them() {
    let (mut h, doc) = opened_scene();
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) });
    assert_eq!(recipes(&h, doc).compare.expect("a comparison").corrections, None);
}

fn counter_id(h: &HeadlessEditorHarness, doc: DocumentId) -> u64 {
    prep(h, doc).contours.iter().find(|c| c.hole).map(|c| c.id).expect("the counter")
}

#[test]
fn both_outlines_are_on_the_canvas_where_applying_would_put_them_and_go_with_the_comparison() {
    let (mut h, doc) = opened_scene();
    assert!(outlines(&h, doc).is_empty(), "no comparison, no outlines");
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) });
    let c = recipes(&h, doc).compare.expect("a comparison");
    let on_canvas = outlines(&h, doc);
    assert_eq!(on_canvas.len(), 2);
    let a = on_canvas.iter().find(|(s, _)| *s == CompareSide::A).map(|(_, p)| p).expect("A");
    let b = on_canvas.iter().find(|(s, _)| *s == CompareSide::B).map(|(_, p)| p).expect("B");
    // The very paths of the panel's picture: what is judged on the sheet is what the panel counts.
    assert_eq!(**a, *c.a_outline.expect("outline A"));
    assert_eq!(**b, *c.b_outline.expect("outline B"));
    // In the frame of the picture (20 × 12 mm), as applying puts the trace on the sheet.
    for p in [a, b] {
        let bb = p.bounds().unwrap();
        assert!(bb.min().x >= -1e-9 && bb.max().x <= 20.0 + 1e-9 && bb.max().y <= 12.0 + 1e-9);
    }
    recipe(&mut h, doc, TraceRecipeIntent::ClearCompare);
    assert!(outlines(&h, doc).is_empty());
}

#[test]
fn a_comparison_still_being_computed_has_no_outlines_and_closing_the_tracer_removes_them() {
    let (mut h, doc) = opened_scene();
    h.send(UiIntent::TraceRecipe {
        document: doc,
        intent: TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) },
    });
    assert!(recipes(&h, doc).compare.expect("started").computing);
    assert!(outlines(&h, doc).is_empty());
    h.run_tasks();
    assert_eq!(outlines(&h, doc).len(), 2);
    h.invoke(doc, burnburn_presentation::CommandKey::CloseTracer);
    assert!(outlines(&h, doc).is_empty());
    assert!(h.document(doc).unwrap().objects.is_empty(), "never document data");
}
