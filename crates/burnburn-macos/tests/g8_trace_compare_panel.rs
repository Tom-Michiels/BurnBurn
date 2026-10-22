//! G8-trace, T10: the comparison section of the preparation panel says how many of the tracer's local
//! corrections each variant carries, and that the two outlines also lie on the sheet.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod g5_fixtures;

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Line, G5Panel, g5_panel};
use burnburn_presentation::{
    HeadlessEditorHarness, RecipeChoice, TracePrepIntent, TracePreset, TraceRecipeIntent, UiIntent,
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

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    g5_panel(&h.view(), doc, G5Panel::TracePrep, locale, &[])
        .expect("the preparation panel")
        .lines
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn compare(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::TraceRecipe {
        document: doc,
        intent: TraceRecipeIntent::Compare { with: RecipeChoice::Preset(TracePreset::Photo) },
    });
    h.run_tasks();
}

#[test]
fn a_comparison_says_the_outlines_are_also_on_the_sheet_and_nothing_of_corrections_when_there_are_none() {
    let (mut h, doc) = opened();
    compare(&mut h, doc);
    let en = texts(&h, doc, Locale::En);
    assert!(!en.iter().any(|t| t.contains("Local corrections")), "{en:?}");
    assert!(en.iter().any(|t| t.contains("also on the sheet")), "{en:?}");
    let nl = texts(&h, doc, Locale::Nl);
    assert!(nl.iter().any(|t| t.contains("ook op het blad")), "{nl:?}");
}

#[test]
fn a_comparison_says_how_many_local_corrections_each_variant_carries() {
    let (mut h, doc) = opened();
    h.send(UiIntent::TracePrep {
        document: doc,
        intent: TracePrepIntent::RetraceRegion {
            x: "5".into(),
            y: "5".into(),
            width: "54".into(),
            height: "54".into(),
            threshold: "100".into(),
            anchors: String::new(),
        },
    });
    h.run_tasks();
    compare(&mut h, doc);
    let en = texts(&h, doc, Locale::En);
    assert!(en.iter().any(|t| t.starts_with("Local corrections: 1 kept; A applies 1, B applies ")), "{en:?}");
    let nl = texts(&h, doc, Locale::Nl);
    assert!(nl.iter().any(|t| t.starts_with("Lokale correcties: 1 bewaard; A past er 1 toe, B past er ")), "{nl:?}");
}
