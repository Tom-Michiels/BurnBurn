//! Product recognition as a panel, headless (G7-werk, W06): the menu item, learning a template from
//! the camera picture, the matches with their similarity (never millimetres) and their place from the
//! calibration, the operator's decision per match and the placement. A fake camera shows rendered
//! frames; raw text goes to presentation unparsed (DR-05).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::camera as rig;

use std::sync::Arc;

use burnburn_image::SourceImage;
use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_opens_by_itself, g5_panel};
use burnburn_presentation::{
    CameraWorkspaceIntent, CandidateDecision, HeadlessEditorHarness, RecognitionIntent as R, UiIntent,
};
use burnburn_types::DocumentId;
use rig::*;

const A: &str = "cam-A";
const W: u32 = 160;
const H: u32 = 120;

fn product(dx: f64, dy: f64) -> f64 {
    90.0 * (-(dx * dx + dy * dy) / 50.0).exp() - 40.0 * (-((dx + 7.0).powi(2) + (dy - 4.0).powi(2)) / 18.0).exp()
        + 55.0 * (-((dx - 8.0).powi(2) + (dy + 6.0).powi(2)) / 8.0).exp()
}

fn photo(at: &[(f64, f64)]) -> SourceImage {
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let mut v = 128.0 + ((x * 7 + y * 13) % 5) as f64 - 2.0;
            for (cx, cy) in at {
                let (dx, dy) = (f64::from(x) - cx, f64::from(y) - cy);
                if dx.abs() <= 15.0 && dy.abs() <= 15.0 {
                    v += product(dx, dy);
                }
            }
            let g = v.clamp(0.0, 255.0).round() as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    SourceImage::from_rgba8(W, H, rgba).unwrap()
}

struct Rig {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    cam: Arc<FakeCam>,
}

fn rig() -> Rig {
    let cam = FakeCam::new(&[(A, &[mode(W, H)])]);
    cam.authorize();
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    let doc = h.new_document().unwrap();
    Rig { h, doc, cam }
}

fn start_camera(r: &mut Rig, at: &[(f64, f64)]) {
    r.cam.show(A, mode(W, H), photo(at));
    r.h.send(UiIntent::CameraWorkspace(CameraWorkspaceIntent::UseCamera {
        device: dev(A),
        mode: mode(W, H),
        lens: "standard".into(),
    }));
}

fn lines(r: &Rig, locale: Locale) -> Vec<G5Line> {
    g5_panel(&r.h.view(), r.doc, G5Panel::Recognition, locale, &[]).expect("the recognition panel").lines
}

fn texts(r: &Rig, locale: Locale) -> String {
    lines(r, locale)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Unavailable(t) | G5Line::Heading(t) => Some(t),
            // LF17: no template yet is an empty state (symbol and sentence).
            G5Line::EmptyState { text, .. } => Some(text),
            G5Line::Button { title, .. } => Some(format!("[{title}]")),
            G5Line::Field { caption, .. } => Some(format!("<{caption}>")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn learn(r: &mut Rig) {
    r.h.send(UiIntent::Recognition(R::Learn {
        device: dev(A),
        x: "46".into(),
        y: "46".into(),
        width: "29".into(),
        height: "29".into(),
        name: "Kaartje".into(),
    }));
}

#[test]
fn the_menu_item_has_its_tag_its_titles_and_toggles_the_panel() {
    let r = rig();
    let vm = r.h.view();
    assert_eq!(menu_tag(MenuItem::RecognitionPanel), 1071);
    assert_eq!(menu_item(1071), Some(MenuItem::RecognitionPanel));
    assert!(menu_enabled(MenuItem::RecognitionPanel, &vm));
    assert_eq!(menu_title(MenuItem::RecognitionPanel, &vm, Locale::Nl), "Herkenning…");
    assert_eq!(menu_title(MenuItem::RecognitionPanel, &vm, Locale::En), "Recognition…");
    assert_eq!(
        translate(NativeEvent::Menu(MenuItem::RecognitionPanel), &vm),
        Reaction::ToggleG5Panel(G5Panel::Recognition)
    );
    assert!(!g5_opens_by_itself(G5Panel::Recognition), "it waits for its menu item");
}

#[test]
fn without_a_running_camera_the_panel_says_to_start_one_and_with_one_it_offers_it() {
    let mut r = rig();
    let nl = texts(&r, Locale::Nl);
    assert!(nl.contains("Start eerst een camera (Camerabeheer)."), "{nl}");
    assert!(nl.contains("Nog geen sjabloon om te herkennen. Leer een product aan met de camera."), "{nl}");
    start_camera(&mut r, &[(60.0, 60.0)]);
    let choice = lines(&r, Locale::Nl).into_iter().find_map(|l| match l {
        G5Line::Choice { control: C::RecogCamera, options, selected, .. } => Some((options, selected)),
        _ => None,
    });
    assert_eq!(choice, Some((vec!["Camera cam-A".to_owned()], 0)));
}

#[test]
fn the_learn_button_sends_the_typed_box_unparsed_for_the_chosen_camera() {
    let mut r = rig();
    start_camera(&mut r, &[(60.0, 60.0)]);
    let draft = vec![
        (C::RecogCamera, "0".to_owned()),
        (C::RecogX, "46".to_owned()),
        (C::RecogY, "46".to_owned()),
        (C::RecogWidth, "29".to_owned()),
        (C::RecogHeight, "29 px".to_owned()),
        (C::RecogName, "Kaartje".to_owned()),
    ];
    assert_eq!(
        g5_intent(&r.h.view(), r.doc, G5Panel::Recognition, C::RecogLearn, &draft),
        Some(UiIntent::Recognition(R::Learn {
            device: dev(A),
            x: "46".into(),
            y: "46".into(),
            width: "29".into(),
            height: "29 px".into(),
            name: "Kaartje".into(),
        }))
    );
    // Without a running camera there is nobody to learn from.
    let idle = rig();
    assert_eq!(g5_intent(&idle.h.view(), idle.doc, G5Panel::Recognition, C::RecogLearn, &draft), None);
}

#[test]
fn matches_show_their_similarity_their_place_and_wait_for_a_decision_per_match() {
    let mut r = rig();
    start_camera(&mut r, &[(60.0, 60.0)]);
    learn(&mut r);
    let mut store = burnburn_image::calib::CalibrationStore::default();
    store.insert_alignment(aligned(A, mode(W, H), 0.0, Mapping { mm_per_px: 0.5, origin_mm: [100.0, 300.0] }));
    r.h.controller().attach_calibrations(store);
    start_camera(&mut r, &[(100.0, 70.0), (30.0, 95.0)]);

    let draft = vec![(C::RecogMinPercent, "80".to_owned())];
    let intent = g5_intent(&r.h.view(), r.doc, G5Panel::Recognition, C::RecogFind, &draft);
    assert_eq!(intent, Some(UiIntent::Recognition(R::Find { min_percent: "80".into() })));
    r.h.send(intent.unwrap());
    assert!(texts(&r, Locale::Nl).contains("Bezig met zoeken…"));
    r.h.run_tasks();

    let nl = texts(&r, Locale::Nl);
    assert!(nl.contains("Sjabloon Kaartje: 29 × 29 px"), "{nl}");
    assert!(nl.contains("Meerdere treffers: beslis over elke treffer"), "{nl}");
    assert!(nl.contains("Treffer 1: gelijkenis") && nl.contains("Treffer 2: gelijkenis"), "{nl}");
    assert!(nl.contains("op het bed (") && nl.contains("wacht op jouw keuze"), "{nl}");
    assert!(nl.contains("geen nauwkeurigheid in millimeters"), "{nl}");
    assert!(nl.contains("[Accepteer treffer 1]") && nl.contains("[Wijs treffer 2 af]"), "{nl}");
    let en = texts(&r, Locale::En);
    assert!(en.contains("not an accuracy in millimetres") && en.contains("waiting for your choice"), "{en}");

    // The place button is off until every match is decided and one is accepted.
    let place_enabled = |r: &Rig| {
        lines(r, Locale::Nl)
            .into_iter()
            .any(|l| matches!(l, G5Line::Button { control: C::RecogPlace, enabled: true, .. }))
    };
    assert!(!place_enabled(&r));
    let vm = r.h.view();
    let accept = g5_intent(&vm, r.doc, G5Panel::Recognition, C::RecogAccept { index: 0 }, &[]);
    assert_eq!(accept, Some(UiIntent::Recognition(R::Decide { index: 0, decision: CandidateDecision::Accepted })));
    r.h.send(accept.unwrap());
    assert!(!place_enabled(&r), "the other match is still waiting");
    let reject = g5_intent(&r.h.view(), r.doc, G5Panel::Recognition, C::RecogReject { index: 1 }, &[]);
    assert_eq!(reject, Some(UiIntent::Recognition(R::Decide { index: 1, decision: CandidateDecision::Rejected })));
    r.h.send(reject.unwrap());
    assert!(place_enabled(&r));
    assert!(texts(&r, Locale::Nl).contains("geaccepteerd") && texts(&r, Locale::Nl).contains("afgewezen"));
    assert_eq!(
        g5_intent(&r.h.view(), r.doc, G5Panel::Recognition, C::RecogPlace, &[]),
        Some(UiIntent::Recognition(R::Place { document: r.doc }))
    );
}

#[test]
fn a_refused_step_is_said_in_words_and_an_uncalibrated_match_has_no_millimetres() {
    let mut r = rig();
    start_camera(&mut r, &[(60.0, 60.0)]);
    r.h.send(UiIntent::Recognition(R::Find { min_percent: String::new() }));
    assert!(texts(&r, Locale::Nl).contains("Leer eerst een sjabloon."));
    learn(&mut r);
    start_camera(&mut r, &[(100.0, 70.0)]);
    r.h.send(UiIntent::Recognition(R::Find { min_percent: "80".into() }));
    r.h.run_tasks();
    let nl = texts(&r, Locale::Nl);
    assert!(nl.contains("niet gekalibreerd: geen plaats in mm"), "{nl}");
    assert!(!nl.contains("Meerdere treffers"), "one match needs no review text: {nl}");
    r.h.send(UiIntent::Recognition(R::Place { document: r.doc }));
    assert!(texts(&r, Locale::En).contains("Decide about each match first."));
}
