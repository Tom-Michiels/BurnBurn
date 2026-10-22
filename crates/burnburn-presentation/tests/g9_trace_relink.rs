//! G9-trace: a local correction that no longer binds on variant B of the A/B comparison can be linked to
//! B's contours from the comparison, and unlinked again. A stays what the tracer shows.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_presentation::{
    CompareCorrectionRow, CompareCorrectionState, CompareViewModel, HeadlessEditorHarness, RecipeChoice, TracePreset,
    TraceRecipeIntent, TraceRecipesViewModel, UiIntent,
};
use burnburn_types::DocumentId;
use g7_support::*;

const REGION: [&str; 4] = ["30", "40", "90", "70"];

fn recipe(h: &mut HeadlessEditorHarness, doc: DocumentId, intent: TraceRecipeIntent) {
    h.send(UiIntent::TraceRecipe { document: doc, intent });
    h.run_tasks();
}

fn compare(h: &HeadlessEditorHarness, doc: DocumentId) -> CompareViewModel {
    let r: TraceRecipesViewModel = vm(h, doc).trace_recipes.expect("the recipe panel");
    r.compare.expect("a comparison")
}

/// The scene of `g8_trace_compare.rs`: a protected counter and a correction traced at 200 in `REGION`; the
/// photo preset sees other contours, so the correction does not bind on B.
fn unbound_on_b() -> (HeadlessEditorHarness, DocumentId) {
    let (mut h, doc) = opened_scene();
    let id = prep(&h, doc).contours.iter().find(|c| c.hole).map(|c| c.id).expect("the counter");
    send(&mut h, doc, burnburn_presentation::TracePrepIntent::ProtectContour { id, on: true });
    retrace(&mut h, doc, REGION, "200", "");
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) });
    (h, doc)
}

#[test]
fn a_correction_that_does_not_bind_on_b_is_listed_and_can_be_linked_and_unlinked() {
    let (mut h, doc) = unbound_on_b();
    let before = compare(&h, doc);
    assert_eq!(before.corrections.map(|k| (k.stored, k.a_applied, k.b_applied)), Some((1, 1, 0)));
    assert_eq!(before.b_corrections.len(), 1);
    assert_eq!(before.b_corrections[0].index, 0);
    assert!(matches!(before.b_corrections[0].state, CompareCorrectionState::Unbound { lost, .. } if lost >= 1));

    recipe(&mut h, doc, TraceRecipeIntent::RelinkCorrection { index: 0, on: true });
    let linked = compare(&h, doc);
    assert_eq!(linked.corrections.map(|k| (k.stored, k.a_applied, k.b_applied)), Some((1, 1, 1)));
    assert_eq!(linked.b_corrections, [CompareCorrectionRow { index: 0, state: CompareCorrectionState::Relinked }]);
    assert_eq!(linked.a, before.a, "A is what the tracer shows, linked or not");
    assert_ne!(linked.b, before.b, "B now carries the correction");
    assert_ne!(linked.b_outline, before.b_outline);
    assert!(!linked.computing);

    recipe(&mut h, doc, TraceRecipeIntent::RelinkCorrection { index: 0, on: false });
    let unlinked = compare(&h, doc);
    assert_eq!(unlinked.b, before.b);
    assert_eq!(unlinked.corrections.map(|k| k.b_applied), Some(0));
    assert_eq!(unlinked.b_corrections, before.b_corrections);
}

#[test]
fn linking_a_correction_that_is_not_in_the_list_or_without_a_comparison_changes_nothing() {
    let (mut h, doc) = unbound_on_b();
    let before = compare(&h, doc);
    recipe(&mut h, doc, TraceRecipeIntent::RelinkCorrection { index: 7, on: true });
    assert_eq!(compare(&h, doc), before);
    recipe(&mut h, doc, TraceRecipeIntent::ClearCompare);
    recipe(&mut h, doc, TraceRecipeIntent::RelinkCorrection { index: 0, on: true });
    assert!(vm(&h, doc).trace_recipes.expect("the recipe panel").compare.is_none());
}

#[test]
fn a_comparison_without_corrections_lists_none() {
    let (mut h, doc) = opened_scene();
    recipe(&mut h, doc, TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) });
    assert!(compare(&h, doc).b_corrections.is_empty());
}
