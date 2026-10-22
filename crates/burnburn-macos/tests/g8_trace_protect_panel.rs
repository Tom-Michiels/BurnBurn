//! G8-trace, T05 and T06: the preparation panel offers protecting by clicking (contours and corners),
//! says what the clicks do, lists the protected corners with a button to take each back, and says
//! why a click did nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::g5 as g5_fixtures;

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{HeadlessEditorHarness, TracePick, TracePrepIntent, UiIntent};
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

fn button(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale, c: C) -> Option<(String, bool)> {
    lines(h, doc, locale).into_iter().find_map(|l| match l {
        G5Line::Button { control, title, enabled } if control == c => Some((title, enabled)),
        _ => None,
    })
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    lines(h, doc, locale)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C) -> Option<UiIntent> {
    let intent = g5_intent(&h.view(), doc, G5Panel::TracePrep, c, &[])?;
    h.send(intent.clone());
    h.run_tasks();
    Some(intent)
}

#[test]
fn two_buttons_start_protecting_contours_or_corners_and_say_what_a_click_does() {
    let (mut h, doc) = opened();
    assert_eq!(
        button(&h, doc, Locale::En, C::PickProtectContours),
        Some(("Click contours to protect".to_owned(), true))
    );
    assert_eq!(
        button(&h, doc, Locale::Nl, C::PickProtectCorners),
        Some(("Hoeken aanklikken om te beschermen".to_owned(), true))
    );

    assert_eq!(
        press(&mut h, doc, C::PickProtectContours),
        Some(UiIntent::TracePick { document: doc, pick: TracePick::BeginProtectContours })
    );
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.contains("click a contour to protect it or take it back")));
    assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t.contains("klik op een contour")));
    press(&mut h, doc, C::PickEnd).unwrap();

    assert_eq!(
        press(&mut h, doc, C::PickProtectCorners),
        Some(UiIntent::TracePick { document: doc, pick: TracePick::BeginProtectCorners })
    );
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.contains("click near a corner to keep a node on it")));
}

#[test]
fn a_protected_corner_is_listed_with_a_button_that_takes_it_back() {
    let (mut h, doc) = opened();
    press(&mut h, doc, C::PickProtectCorners).unwrap();
    // Nothing protected yet: no list.
    assert!(!texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Protected corner 1")));
    // A click at the top of the ring (outer radius 20 px around (32, 32)): the sharpest vertex near it.
    h.send(UiIntent::TracePick { document: doc, pick: TracePick::ProtectAt { x: 32.0, y: 12.0, reach_px: 3.0 } });
    h.run_tasks();
    let en = texts(&h, doc, Locale::En);
    assert!(en.iter().any(|t| t.starts_with("Protected corner 1: at (")), "{en:?}");
    assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t.starts_with("Beschermde hoek 1: op (")));
    assert_eq!(button(&h, doc, Locale::En, C::CornerRemove { index: 0 }).map(|b| b.1), Some(true));
    assert_eq!(
        press(&mut h, doc, C::CornerRemove { index: 0 }),
        Some(UiIntent::TracePrep { document: doc, intent: TracePrepIntent::RemoveProtectedCorner { index: 0 } })
    );
    assert!(!texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Protected corner 1")));
}

#[test]
fn a_click_that_did_nothing_says_why() {
    let (mut h, doc) = opened();
    press(&mut h, doc, C::PickProtectContours).unwrap();
    h.send(UiIntent::TracePick { document: doc, pick: TracePick::ProtectAt { x: 2.0, y: 2.0, reach_px: 1.5 } });
    h.run_tasks();
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "There is no contour at the place you clicked."));
    assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t == "Op de aangeklikte plek zit geen contour."));
    h.send(UiIntent::TracePrep { document: doc, intent: TracePrepIntent::RemoveProtectedCorner { index: 4 } });
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "This protected corner is not in the list."));
}

#[test]
fn without_a_contour_result_the_buttons_are_off() {
    let (mut h, doc) = opened();
    let edit = |h: &mut HeadlessEditorHarness| {
        let mut input = h.document(doc).unwrap().tracer.unwrap().form.input;
        input.mode = burnburn_presentation::TraceMode::Centerline;
        h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
        h.run_tasks();
    };
    edit(&mut h);
    assert_eq!(button(&h, doc, Locale::En, C::PickProtectContours).map(|b| b.1), Some(false));
    assert_eq!(button(&h, doc, Locale::En, C::PickProtectCorners).map(|b| b.1), Some(false));
}
