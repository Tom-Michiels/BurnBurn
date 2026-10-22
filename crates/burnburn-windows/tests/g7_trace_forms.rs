//! The golf-7 trace sections as Windows forms, headless (G7-trace): lens, pointing, protect and keep,
//! node goal, dropped pieces, own recipes, A/B comparison and batch. Same lines, words and intents as
//! the macOS panel (`tests/g7_trace_panels.rs` there): a control reports its value, presentation
//! decides what changes. The Win32 realisation is only type-checked from macOS, never run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-macos/tests/g5_fixtures/mod.rs"]
mod g5_fixtures;

use burnburn_presentation::{
    HeadlessEditorHarness, MaskMode, RecipeChoice, TracePick, TracePrepIntent, TracePreset, TraceRecipeIntent, UiIntent,
};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::l10n_g7_trace::{G7Text, g7_text};
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
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

fn shown() -> PanelState {
    PanelState { g5_shown: vec![P::TracePrep], ..PanelState::default() }
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
    let vm = h.view();
    g5::forms(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &shown(), &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::TracePrep))
        .expect("the preparation form")
}

fn flat(f: &Form) -> Vec<&FormLine> {
    f.lines
        .iter()
        .flat_map(|l| match l {
            FormLine::Row(inner) => inner.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

fn text_of(f: &Form, part: &str) -> bool {
    flat(f).iter().any(|l| matches!(l, FormLine::Text(t) if t.contains(part)))
}

fn react(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, set: &[(C, ControlValue)]) -> UiIntent {
    let f = form(h, doc);
    let mut values = f.values();
    for (control, value) in set {
        match values.iter_mut().find(|(id, _)| *id == ControlId::G5(*control)) {
            Some(slot) => slot.1 = value.clone(),
            None => values.push((ControlId::G5(*control), value.clone())),
        }
    }
    let vm = h.view();
    let outcome = g5::react(
        P::TracePrep,
        &ControlId::G5(c),
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    let FormOutcome::Intent(intent) = outcome else { panic!("an intent for {c:?}: {outcome:?}") };
    h.send(intent.clone());
    h.run_tasks();
    intent
}

#[test]
fn the_form_has_the_controls_of_the_new_sections() {
    let (h, doc) = opened();
    let f = form(&h, doc);
    for c in [
        C::LensK1,
        C::LensK2,
        C::LensApply,
        C::PickCorners,
        C::BrushRadius,
        C::PickBrushKeep,
        C::PickBrushRemove,
        C::NodeLimit,
        C::NodeTolerance,
        C::NodeApply,
        C::RecipeName,
        C::RecipeSave,
        C::RecipePreset,
        C::RecipeComparePreset,
        C::BatchFiles,
        C::BatchRun,
    ] {
        assert!(f.find(&ControlId::G5(c)).is_some(), "{c:?}");
    }
    let rows = h.document(doc).unwrap().trace_prep.unwrap().contours;
    for row in rows {
        assert!(f.find(&ControlId::G5(C::ContourProtect { id: row.id })).is_some());
        assert!(f.find(&ControlId::G5(C::ContourKeep { id: row.id })).is_some());
    }
    assert!(
        flat(&f).iter().any(|l| matches!(l, FormLine::Memo { id, .. } if *id == ControlId::G5(C::BatchFiles))),
        "the files of a batch are a memo"
    );
}

#[test]
fn lens_pointing_and_a_check_send_their_intents_and_the_form_follows() {
    let (mut h, doc) = opened();
    let intent = react(
        &mut h,
        doc,
        C::LensApply,
        &[(C::LensK1, ControlValue::Text("-0.1".into())), (C::LensK2, ControlValue::Text("0".into()))],
    );
    assert_eq!(
        intent,
        UiIntent::TracePrep { document: doc, intent: TracePrepIntent::UseLens { k1: "-0.1".into(), k2: "0".into() } }
    );
    assert!(text_of(&form(&h, doc), "Lens correction in use: k1 -0.1, k2 0."));

    let intent = react(&mut h, doc, C::PickBrushKeep, &[(C::BrushRadius, ControlValue::Text("6".into()))]);
    assert_eq!(
        intent,
        UiIntent::TracePick {
            document: doc,
            pick: TracePick::BeginBrushTyped { mode: MaskMode::Keep, radius: "6".into() }
        }
    );
    assert!(text_of(&form(&h, doc), "Brush in hand: keep, radius 6 px."));
    react(&mut h, doc, C::PickEnd, &[]);

    let hole = h.document(doc).unwrap().trace_prep.unwrap().contours.iter().find(|c| c.hole).unwrap().id;
    let intent = react(
        &mut h,
        doc,
        C::ContourProtect { id: hole },
        &[(C::ContourProtect { id: hole }, ControlValue::Toggle(true))],
    );
    assert_eq!(
        intent,
        UiIntent::TracePrep { document: doc, intent: TracePrepIntent::ProtectContour { id: hole, on: true } }
    );
    let f = form(&h, doc);
    assert!(flat(&f).iter().any(
        |l| matches!(l, FormLine::Check { id, on: true, .. } if *id == ControlId::G5(C::ContourProtect { id: hole }))
    ));
}

#[test]
fn recipes_comparison_and_batch_work_from_the_form() {
    let (mut h, doc) = opened();
    let intent = react(&mut h, doc, C::RecipeSave, &[(C::RecipeName, ControlValue::Text("Ring".into()))]);
    assert_eq!(
        intent,
        UiIntent::TraceRecipe { document: doc, intent: TraceRecipeIntent::Save { name: "Ring".into() } }
    );
    assert!(text_of(&form(&h, doc), "Recipe: Ring (contour)"));

    let index = TracePreset::ALL.iter().position(|p| *p == TracePreset::Logo).unwrap();
    let intent = react(&mut h, doc, C::RecipeComparePreset, &[(C::RecipePreset, ControlValue::Choice(index))]);
    assert_eq!(
        intent,
        UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Logo) }
        }
    );
    let f = form(&h, doc);
    assert!(text_of(&f, "A (current):"));
    assert!(flat(&f).iter().any(|l| matches!(l, FormLine::Image(_))), "the picture of both outlines");

    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/two.png"), &ring_png()).unwrap();
    let intent = react(&mut h, doc, C::BatchRun, &[(C::BatchFiles, ControlValue::Text("/work/two.png".into()))]);
    assert_eq!(
        intent,
        UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::RunBatch { paths: vec!["/work/two.png".into()] }
        }
    );
    assert!(text_of(&form(&h, doc), "1 file(s), 0 with something to look at."));
}

#[test]
fn the_words_of_the_new_sections_are_the_ones_of_the_mac_in_both_languages() {
    // The Windows host compiles the very same text file; spot-check that it is there in both languages.
    assert_eq!(g7_text(G7Text::LensHeading, Locale::Nl), "Lensvervorming");
    assert_eq!(g7_text(G7Text::LensHeading, Locale::En), "Lens distortion");
}
