//! Product recognition, golf-8 additions, as a Windows form (headless): rotation and scale choices,
//! pointing at the box, and the saved templates — same lines, words and intents as the macOS panel
//! (`burnburn-macos/tests/g8_werk_recognition.rs`, one shared panel model).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::camera as rig;

use burnburn_presentation::{CameraWorkspaceIntent, HeadlessEditorHarness, RecognitionIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use rig::*;

fn rig() -> (HeadlessEditorHarness, DocumentId) {
    let cam = FakeCam::new(&[("cam-A", &[mode(160, 120)])]);
    cam.authorize();
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    let doc = h.new_document().unwrap();
    // A picture with a pattern: a bright square on grey.
    let mut rgba = Vec::new();
    for y in 0..120u32 {
        for x in 0..160u32 {
            let g = if (50..80).contains(&x) && (40..70).contains(&y) { 230 } else { 90 + ((x + y) % 7) as u8 };
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    cam.show("cam-A", mode(160, 120), burnburn_image::SourceImage::from_rgba8(160, 120, rgba).unwrap());
    h.send(UiIntent::CameraWorkspace(CameraWorkspaceIntent::UseCamera {
        device: dev("cam-A"),
        mode: mode(160, 120),
        lens: "standard".into(),
    }));
    (h, doc)
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
    let vm = h.view();
    let context = FormContext { vm: &vm, document: doc, locale: Locale::En };
    let shown = PanelState { g5_shown: vec![P::Recognition], ..PanelState::default() };
    g5::forms(&context, &shown, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::Recognition))
        .expect("the recognition form")
}

fn react(h: &HeadlessEditorHarness, doc: DocumentId, control: C, value: ControlValue) -> FormOutcome {
    let vm = h.view();
    let context = FormContext { vm: &vm, document: doc, locale: Locale::En };
    let mut values = form(h, doc).values();
    values.retain(|(id, _)| *id != ControlId::G5(control));
    values.push((ControlId::G5(control), value.clone()));
    g5::react(P::Recognition, &ControlId::G5(control), &values, &context)
}

#[test]
fn the_form_has_the_two_choices_the_pointing_button_and_the_saved_templates() {
    let (mut h, doc) = rig();
    let f = form(&h, doc);
    assert!(matches!(f.find(&ControlId::G5(C::RecogRotation)), Some(FormLine::Check { on: false, .. })));
    assert!(matches!(f.find(&ControlId::G5(C::RecogScale)), Some(FormLine::Check { on: false, .. })));
    assert!(f.find(&ControlId::G5(C::RecogPoint)).is_some());
    assert!(f.find(&ControlId::G5(C::RecogUse { index: 0 })).is_none(), "nothing saved yet");

    let outcome = react(&h, doc, C::RecogRotation, ControlValue::Toggle(true));
    let FormOutcome::Intent(intent) = outcome else { panic!("{outcome:?}") };
    assert_eq!(intent, UiIntent::Recognition(RecognitionIntent::SetSearch { rotation: true, scale: false }));
    h.send(intent);
    assert!(matches!(form(&h, doc).find(&ControlId::G5(C::RecogRotation)), Some(FormLine::Check { on: true, .. })));

    // Learn a named template: it is listed, and Use / Delete send its id.
    h.send(UiIntent::Recognition(RecognitionIntent::Learn {
        device: dev("cam-A"),
        x: "40".into(),
        y: "30".into(),
        width: "50".into(),
        height: "50".into(),
        name: "Vierkant".into(),
    }));
    let id = h.document(doc).unwrap().recognition.saved[0].id;
    let f = form(&h, doc);
    assert!(f.find(&ControlId::G5(C::RecogUse { index: 0 })).is_some());
    let FormOutcome::Intent(use_it) = react(&h, doc, C::RecogUse { index: 0 }, ControlValue::Press) else {
        panic!("use")
    };
    assert_eq!(use_it, UiIntent::Recognition(RecognitionIntent::Use { id }));
    let FormOutcome::Intent(delete) = react(&h, doc, C::RecogDelete { index: 0 }, ControlValue::Press) else {
        panic!("delete")
    };
    assert_eq!(delete, UiIntent::Recognition(RecognitionIntent::Delete { id }));
}

#[test]
fn pointing_sends_the_chosen_camera_and_name_and_shows_its_stop_while_it_lasts() {
    let (mut h, doc) = rig();
    let outcome = react(&h, doc, C::RecogPoint, ControlValue::Press);
    let FormOutcome::Intent(intent) = outcome else { panic!("{outcome:?}") };
    assert!(
        matches!(&intent, UiIntent::Recognition(RecognitionIntent::PointBox { device, .. }) if *device == dev("cam-A"))
    );
    assert!(form(&h, doc).find(&ControlId::G5(C::RecogEndPoint)).is_none());
    h.send(intent);
    assert!(form(&h, doc).find(&ControlId::G5(C::RecogEndPoint)).is_some());
    let FormOutcome::Intent(stop) = react(&h, doc, C::RecogEndPoint, ControlValue::Press) else { panic!("stop") };
    assert_eq!(stop, UiIntent::Recognition(RecognitionIntent::EndPoint));
}
