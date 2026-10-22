//! The golf-6 workshop panels, headless (G6-werk): preparing an image for tracing. What the panel
//! shows comes from presentation's view model in the window language; a button sends the typed text
//! of its fields as one intent (DR-05). The real widgets are drawn by the hosts from the same lines.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::g5 as g5_fixtures;

use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::Locale;
use burnburn_macos::l10n_g6_werk::{G6Text, g6_text};
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, G5PanelModel, g5_intent, g5_panel};
use burnburn_presentation::{
    HeadlessEditorHarness, MaskMode, TraceFormInput, TracePrepIntent, TracerViewModel, UiIntent,
};
use burnburn_types::DocumentId;
use g5_fixtures::t_png;

fn opened(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/in.png"), bytes).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: std::path::PathBuf::from("/work/in.png") });
    h.run_tasks();
    (h, doc)
}

fn panel(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Option<G5PanelModel> {
    g5_panel(&h.view(), doc, G5Panel::TracePrep, locale, &[])
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<G5Line> {
    panel(h, doc, Locale::En).expect("the preparation panel").lines
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    panel(h, doc, locale)
        .expect("the preparation panel")
        .lines
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn has(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> bool {
    lines(h, doc).iter().any(|l| l.control() == Some(c))
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, values: &[(C, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (*c, (*v).to_owned())).collect();
    let intent = g5_intent(&h.view(), doc, G5Panel::TracePrep, c, &values)?;
    h.send(intent.clone());
    h.run_tasks();
    Some(intent)
}

fn tracer(h: &HeadlessEditorHarness, doc: DocumentId) -> TracerViewModel {
    h.document(doc).unwrap().tracer.expect("tracer")
}

#[test]
fn the_panel_exists_only_while_a_tracer_is_open_and_the_menu_item_follows() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    assert!(panel(&h, doc, Locale::En).is_none());
    assert!(!menu_enabled(MenuItem::TracePrepPanel, &h.view()), "nothing to prepare yet");
    let (h, doc) = opened(&t_png());
    assert!(panel(&h, doc, Locale::En).is_some());
    assert!(menu_enabled(MenuItem::TracePrepPanel, &h.view()));
    assert_eq!(menu_item(menu_tag(MenuItem::TracePrepPanel)), Some(MenuItem::TracePrepPanel));
    assert_eq!(menu_title(MenuItem::TracePrepPanel, &h.view(), Locale::Nl), "Beeld voorbereiden…");
    assert_eq!(
        translate(NativeEvent::Menu(MenuItem::TracePrepPanel), &h.view()),
        Reaction::ToggleG5Panel(G5Panel::TracePrep)
    );
}

#[test]
fn the_local_threshold_button_sends_the_typed_window_and_the_panel_then_says_it_is_in_use() {
    let (mut h, doc) = opened(&t_png());
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.contains("global threshold of the tracer sheet")));
    let intent = press(&mut h, doc, C::PrepUseLocal, &[(C::PrepRadius, "9"), (C::PrepOffset, "12")]).unwrap();
    assert_eq!(
        intent,
        UiIntent::TracePrep {
            document: doc,
            intent: TracePrepIntent::UseLocalThreshold { radius: "9".into(), offset: "12".into() }
        }
    );
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "Local threshold in use: window 9 px, offset 12."));
    assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t == "Lokale drempel in gebruik: venster 9 px, verschil 12."));
    // A bad number is said where the panel is, and nothing changes.
    press(&mut h, doc, C::PrepUseLocal, &[(C::PrepRadius, "abc"), (C::PrepOffset, "12")]);
    assert!(
        texts(&h, doc, Locale::En).iter().any(|t| t.contains("Window: this is not a number")),
        "{:?}",
        texts(&h, doc, Locale::En)
    );
    let back = press(&mut h, doc, C::PrepUseGlobal, &[]);
    assert!(back.is_some());
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.contains("global threshold")));
}

#[test]
fn the_four_corners_are_typed_and_the_transform_is_listed_after_applying() {
    let (mut h, doc) = opened(&t_png());
    let corners = [
        (C::PrepCorner { index: 0, axis: 0 }, "8"),
        (C::PrepCorner { index: 0, axis: 1 }, "6"),
        (C::PrepCorner { index: 1, axis: 0 }, "56"),
        (C::PrepCorner { index: 1, axis: 1 }, "6"),
        (C::PrepCorner { index: 2, axis: 0 }, "60"),
        (C::PrepCorner { index: 2, axis: 1 }, "58"),
        (C::PrepCorner { index: 3, axis: 0 }, "4"),
        (C::PrepCorner { index: 3, axis: 1 }, "58"),
    ];
    let mut values: Vec<(C, &str)> = corners.to_vec();
    values.extend([(C::PrepWidth, "48"), (C::PrepHeight, "52")]);
    let sent = press(&mut h, doc, C::PrepApplyCorners, &values).unwrap();
    let UiIntent::TracePrep { intent: TracePrepIntent::ApplyPerspective { corners, width, height }, .. } = sent else {
        panic!("one ApplyPerspective");
    };
    assert_eq!(corners[2], ["60".to_owned(), "58".to_owned()]);
    assert_eq!((width.as_str(), height.as_str()), ("48", "52"));
    let shown = texts(&h, doc, Locale::En);
    assert!(shown.iter().any(|t| t.starts_with("Plane transform (photo to corrected): [")), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains("largest corner error 0 px")), "four corners fit exactly: {shown:?}");
    assert!(has(&h, doc, C::PrepClearCorners));
    press(&mut h, doc, C::PrepClearCorners, &[]).unwrap();
    assert!(!texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Plane transform")));
}

#[test]
fn a_mask_is_added_from_the_area_fields_with_the_chosen_kind_and_can_be_removed_again() {
    let (mut h, doc) = opened(&t_png());
    let area = [(C::PrepAreaX, "0"), (C::PrepAreaY, "40"), (C::PrepAreaWidth, "64"), (C::PrepAreaHeight, "24")];
    let mut values: Vec<(C, &str)> = area.to_vec();
    values.push((C::PrepMaskKind, "1"));
    let sent = press(&mut h, doc, C::PrepAddMask, &values).unwrap();
    assert_eq!(
        sent,
        UiIntent::TracePrep {
            document: doc,
            intent: TracePrepIntent::AddMask {
                mode: MaskMode::Remove,
                x: "0".into(),
                y: "40".into(),
                width: "64".into(),
                height: "24".into()
            }
        }
    );
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "Mask 1: remove, 0, 40, 64 × 24 px"));
    assert!(has(&h, doc, C::PrepRemoveMask { index: 0 }));
    press(&mut h, doc, C::PrepRemoveMask { index: 0 }, &[]).unwrap();
    assert!(!has(&h, doc, C::PrepRemoveMask { index: 0 }));
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "No masks."));
}

#[test]
fn a_correction_is_listed_with_its_status_and_the_report_names_the_cost_of_simplifying() {
    let (mut h, doc) = opened(&t_png());
    let mut input: TraceFormInput = tracer(&h, doc).form.input;
    input.ignore_below_mm2 = "0".into();
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
    let values = [
        (C::PrepAreaX, "0"),
        (C::PrepAreaY, "0"),
        (C::PrepAreaWidth, "64"),
        (C::PrepAreaHeight, "32"),
        (C::PrepRegionThreshold, "100"),
        (C::PrepAnchors, "10, 12; 20, 14"),
    ];
    let sent = press(&mut h, doc, C::PrepRetrace, &values).unwrap();
    assert!(matches!(
        sent,
        UiIntent::TracePrep { intent: TracePrepIntent::RetraceRegion { ref threshold, ref anchors, .. }, .. }
            if threshold == "100" && anchors == "10, 12; 20, 14"
    ));
    let shown = texts(&h, doc, Locale::En);
    assert!(
        shown
            .iter()
            .any(|t| t.starts_with("Correction 1: area 0, 0, 64 × 32 px, threshold 100, 2 fixed point(s): applied")),
        "{shown:?}"
    );
    assert!(shown.iter().any(|t| t.starts_with("Fit tolerance ") && t.contains("largest deviation")), "{shown:?}");
    assert!(shown.iter().any(|t| t.starts_with("Deviation ") && t.contains(" mm at (")), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains("parts and") && t.contains("holes in the trace")), "{shown:?}");
    let nl = texts(&h, doc, Locale::Nl);
    assert!(nl.iter().any(|t| t.starts_with("Correctie 1: gebied 0, 0, 64 × 32 px, drempel 100")), "{nl:?}");
    assert!(has(&h, doc, C::PrepRemoveCorrection { index: 0 }));
    press(&mut h, doc, C::PrepRemoveCorrection { index: 0 }, &[]).unwrap();
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "No local corrections."));
}

#[test]
fn every_text_has_both_languages_with_the_same_placeholders() {
    fn placeholders(s: &str) -> Vec<String> {
        let mut out: Vec<String> =
            s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n.to_owned())).collect();
        out.sort();
        out
    }
    for t in G6Text::ALL {
        let (nl, en) = (g6_text(*t, Locale::Nl), g6_text(*t, Locale::En));
        assert!(!nl.is_empty() && !en.is_empty(), "{t:?}");
        assert_eq!(placeholders(nl), placeholders(en), "{t:?}");
        // Cognates such as "Machine" or "Orders" read the same in both languages.
    }
}
