//! G9-trace: the comparison section lists a correction that does not bind on variant B and offers a button
//! to link it to B's contours (and to take the link back), with the words of the two languages.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::g5::compared_with_unbound_correction;

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{HeadlessEditorHarness, TraceRecipeIntent, UiIntent};
use burnburn_types::DocumentId;

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
fn an_unbound_correction_is_said_and_has_a_button_that_links_it_to_b() {
    let (mut h, doc) = compared_with_unbound_correction();
    assert!(
        texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Correction 1 does not fit B: ")),
        "{:?}",
        texts(&h, doc, Locale::En)
    );
    assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t.starts_with("Correctie 1 past niet op B: ")));
    assert_eq!(
        button(&h, doc, Locale::En, C::CompareRelink { index: 0 }),
        Some(("Link correction 1 to B anyway".to_owned(), true))
    );
    assert_eq!(
        button(&h, doc, Locale::Nl, C::CompareRelink { index: 0 }),
        Some(("Koppel correctie 1 toch aan B".to_owned(), true))
    );

    assert_eq!(
        press(&mut h, doc, C::CompareRelink { index: 0 }),
        Some(UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::RelinkCorrection { index: 0, on: true }
        })
    );
    // Linked: the line says so and the same button takes the link back.
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Correction 1 is linked to B")));
    assert_eq!(
        button(&h, doc, Locale::En, C::CompareRelink { index: 0 }),
        Some(("Take correction 1 off B again".to_owned(), true))
    );
    assert_eq!(
        press(&mut h, doc, C::CompareRelink { index: 0 }),
        Some(UiIntent::TraceRecipe {
            document: doc,
            intent: TraceRecipeIntent::RelinkCorrection { index: 0, on: false }
        })
    );
}

#[test]
fn a_correction_that_is_not_listed_has_no_button_and_no_intent() {
    let (h, doc) = compared_with_unbound_correction();
    assert_eq!(button(&h, doc, Locale::En, C::CompareRelink { index: 3 }), None);
    assert_eq!(g5_intent(&h.view(), doc, G5Panel::TracePrep, C::CompareRelink { index: 3 }, &[]), None);
}
