//! Product recognition, golf-8 additions, headless: the choices for rotation and scale, the angle
//! and scale of a match, the templates kept in the library (use, delete) and pointing at the box in
//! the camera picture. A fake camera shows rendered frames; raw text goes to presentation unparsed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-presentation/tests/common/camera.rs"]
mod rig;

use std::sync::Arc;

use burnburn_image::SourceImage;
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{CameraWorkspaceIntent, HeadlessEditorHarness, RecognitionIntent as R, UiIntent};
use burnburn_types::DocumentId;
use rig::*;

const A: &str = "cam-A";
const W: u32 = 160;
const H: u32 = 120;

fn product(u: f64, v: f64) -> f64 {
    90.0 * (-(u * u + v * v) / 50.0).exp() - 40.0 * (-((u + 7.0).powi(2) + (v - 4.0).powi(2)) / 18.0).exp()
        + 55.0 * (-((u - 8.0).powi(2) + (v + 6.0).powi(2)) / 8.0).exp()
}

/// Products at `(x, y, angle)` (degrees, y down).
fn photo(at: &[(f64, f64, f64)]) -> SourceImage {
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let mut v = 128.0 + ((x * 7 + y * 13) % 5) as f64 - 2.0;
            for (cx, cy, angle) in at {
                let (dx, dy) = (f64::from(x) - cx, f64::from(y) - cy);
                let a = angle.to_radians();
                let (u, w) = (a.cos() * dx + a.sin() * dy, -a.sin() * dx + a.cos() * dy);
                if u.abs() <= 24.0 && w.abs() <= 24.0 {
                    v += product(u, w);
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

fn show(r: &mut Rig, at: &[(f64, f64, f64)]) {
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
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Heading(t) => Some(t),
            G5Line::Button { title, .. } => Some(format!("[{title}]")),
            G5Line::Check { caption, on, .. } => Some(format!("({}) {caption}", if on { "x" } else { " " })),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn intent(r: &Rig, control: C, draft: &[(C, String)]) -> Option<UiIntent> {
    g5_intent(&r.h.view(), r.doc, G5Panel::Recognition, control, draft)
}

fn learn(r: &mut Rig, name: &str) {
    r.h.send(UiIntent::Recognition(R::Learn {
        device: dev(A),
        x: "46".into(),
        y: "46".into(),
        width: "29".into(),
        height: "29".into(),
        name: name.into(),
    }));
}

fn button(r: &Rig, control: C) -> Option<bool> {
    lines(r, Locale::En).into_iter().find_map(|l| match l {
        G5Line::Button { control: c, enabled, .. } if c == control => Some(enabled),
        _ => None,
    })
}

#[test]
fn rotation_and_scale_are_two_choices_that_start_off_and_send_the_pair_of_choices() {
    let mut r = rig();
    show(&mut r, &[(60.0, 60.0, 0.0)]);
    let nl = texts(&r, Locale::Nl);
    assert!(nl.contains("( ) Gedraaide producten herkennen") && nl.contains("( ) Grotere en kleinere producten herkennen"), "{nl}");
    let en = texts(&r, Locale::En);
    assert!(en.contains("( ) Find turned products") && en.contains("( ) Find larger and smaller products"), "{en}");

    // Switching rotation on keeps the other choice as it is.
    let draft = vec![(C::RecogRotation, "1".to_owned())];
    assert_eq!(
        intent(&r, C::RecogRotation, &draft),
        Some(UiIntent::Recognition(R::SetSearch { rotation: true, scale: false }))
    );
    r.h.send(intent(&r, C::RecogRotation, &draft).unwrap());
    assert!(texts(&r, Locale::En).contains("(x) Find turned products"));
    let draft = vec![(C::RecogRotation, "1".to_owned()), (C::RecogScale, "1".to_owned())];
    assert_eq!(
        intent(&r, C::RecogScale, &draft),
        Some(UiIntent::Recognition(R::SetSearch { rotation: true, scale: true }))
    );
}

#[test]
fn a_match_says_how_far_it_is_turned_and_how_large_it_is() {
    let mut r = rig();
    show(&mut r, &[(60.0, 60.0, 0.0)]);
    learn(&mut r, "Kaartje");
    r.h.send(UiIntent::Recognition(R::SetSearch { rotation: true, scale: false }));
    show(&mut r, &[(100.0, 70.0, 40.0)]);
    r.h.send(UiIntent::Recognition(R::Find { min_percent: "90".into() }));
    r.h.run_tasks();
    let nl = texts(&r, Locale::Nl);
    assert!(nl.contains("Treffer 1: gedraaid") && nl.contains("schaal 100 %"), "{nl}");
    let en = texts(&r, Locale::En);
    assert!(en.contains("Match 1: turned") && en.contains("scale 100 %"), "{en}");

    // An upright match of an upright search says nothing about a turn.
    let mut plain = rig();
    show(&mut plain, &[(60.0, 60.0, 0.0)]);
    learn(&mut plain, "Kaartje");
    show(&mut plain, &[(100.0, 70.0, 0.0)]);
    plain.h.send(UiIntent::Recognition(R::Find { min_percent: "80".into() }));
    plain.h.run_tasks();
    assert!(!texts(&plain, Locale::En).contains("Match 1: turned"));
}

#[test]
fn pointing_at_the_box_has_a_button_with_the_chosen_camera_and_name_and_a_stop() {
    let mut r = rig();
    assert_eq!(button(&r, C::RecogPoint), Some(false), "without a running camera there is nothing to point at");
    show(&mut r, &[(60.0, 60.0, 0.0)]);
    assert_eq!(button(&r, C::RecogPoint), Some(true));
    let draft = vec![(C::RecogCamera, "0".to_owned()), (C::RecogName, "Kaartje".to_owned())];
    let point = intent(&r, C::RecogPoint, &draft).unwrap();
    assert_eq!(point, UiIntent::Recognition(R::PointBox { device: dev(A), name: "Kaartje".into() }));
    assert!(button(&r, C::RecogEndPoint).is_none());
    r.h.send(point);
    assert!(texts(&r, Locale::Nl).contains("Klik op twee tegenoverliggende hoeken"), "{}", texts(&r, Locale::Nl));
    assert!(texts(&r, Locale::En).contains("Click two opposite corners"));
    assert_eq!(button(&r, C::RecogEndPoint), Some(true));
    assert_eq!(intent(&r, C::RecogEndPoint, &[]), Some(UiIntent::Recognition(R::EndPoint)));
    r.h.send(intent(&r, C::RecogEndPoint, &[]).unwrap());
    assert!(button(&r, C::RecogEndPoint).is_none());
}

#[test]
fn saved_templates_are_listed_with_use_and_delete_and_the_one_in_use_is_marked() {
    let mut r = rig();
    show(&mut r, &[(60.0, 60.0, 0.0)]);
    assert!(!texts(&r, Locale::En).contains("Saved templates"), "no heading for an empty library");
    learn(&mut r, "Kaartje");
    let en = texts(&r, Locale::En);
    assert!(en.contains("Saved templates") && en.contains("Kaartje: 29 × 29 px (in use)"), "{en}");
    assert!(en.contains("Kept in the library."), "{en}");
    let nl = texts(&r, Locale::Nl);
    assert!(nl.contains("Bewaarde sjablonen") && nl.contains("Kaartje: 29 × 29 px (in gebruik)"), "{nl}");

    let id = r.h.document(r.doc).unwrap().recognition.saved[0].id;
    assert_eq!(intent(&r, C::RecogUse { index: 0 }, &[]), Some(UiIntent::Recognition(R::Use { id })));
    assert_eq!(intent(&r, C::RecogDelete { index: 0 }, &[]), Some(UiIntent::Recognition(R::Delete { id })));
    assert_eq!(intent(&r, C::RecogUse { index: 5 }, &[]), None, "a row that is not there");
    r.h.send(intent(&r, C::RecogDelete { index: 0 }, &[]).unwrap());
    assert!(!texts(&r, Locale::En).contains("Saved templates"), "the library is empty again");
    assert!(texts(&r, Locale::En).contains("Not kept in the library"), "the template in use is no longer kept");
}

#[test]
fn a_template_without_a_name_says_it_is_not_kept_and_a_refused_step_is_said_in_words() {
    let mut r = rig();
    show(&mut r, &[(60.0, 60.0, 0.0)]);
    learn(&mut r, "");
    assert!(texts(&r, Locale::En).contains("Not kept in the library: give it a name to keep it."));
    r.h.send(UiIntent::Recognition(R::Use { id: 99 }));
    assert!(texts(&r, Locale::En).contains("This saved template does not exist."));
    assert!(texts(&r, Locale::Nl).contains("Dit bewaarde sjabloon bestaat niet."));
}
