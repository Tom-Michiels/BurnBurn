//! Product recognition as a Windows form, headless (G7-werk, W06): same lines, words and intents as
//! the macOS panel (`burnburn-macos/tests/g7_werk_recognition.rs`); the form waits for its menu item.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::camera as rig;

use burnburn_presentation::{CameraWorkspaceIntent, HeadlessEditorHarness, RecognitionIntent, UiIntent};
use burnburn_windows::adapter::{MenuItem, command_id};
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, ControlValue, FormContext, FormKind, FormLine, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use rig::*;

#[test]
fn the_form_waits_for_its_menu_item_and_learns_from_the_chosen_camera_with_the_typed_box() {
    let cam = FakeCam::new(&[("cam-A", &[mode(160, 120)])]);
    cam.authorize();
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    let doc = h.new_document().unwrap();
    cam.show("cam-A", mode(160, 120), grey(160, 120, 128));
    h.send(UiIntent::CameraWorkspace(CameraWorkspaceIntent::UseCamera {
        device: dev("cam-A"),
        mode: mode(160, 120),
        lens: "standard".into(),
    }));

    let vm = h.view();
    let context = FormContext { vm: &vm, document: doc, locale: Locale::En };
    assert!(
        g5::forms(&context, &PanelState::default(), &Vec::new()).iter().all(|f| f.kind != FormKind::G5(P::Recognition))
    );
    let shown = PanelState { g5_shown: vec![P::Recognition], ..PanelState::default() };
    let form = g5::forms(&context, &shown, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::Recognition))
        .expect("the recognition form");
    let has = |needle: &str| {
        form.lines.iter().any(|l| match l {
            FormLine::Text(t) | FormLine::Heading(t) | FormLine::Problem(t) => t.contains(needle),
            FormLine::Buttons(b) => b.iter().any(|b| b.title.contains(needle)),
            _ => false,
        })
    };
    assert!(has("No template learned yet.") && has("Learn template"));

    let mut values = form.values();
    let typed = [C::RecogX, C::RecogY, C::RecogWidth, C::RecogHeight, C::RecogName];
    values.retain(|(id, _)| !matches!(id, ControlId::G5(c) if typed.contains(c)));
    for (control, text) in
        [(C::RecogX, "46"), (C::RecogY, "46"), (C::RecogWidth, "29"), (C::RecogHeight, "29"), (C::RecogName, "Card")]
    {
        values.push((ControlId::G5(control), ControlValue::Text(text.into())));
    }
    let outcome = g5::react(P::Recognition, &ControlId::G5(C::RecogLearn), &values, &context);
    let FormOutcome::Intent(intent) = outcome else { panic!("{outcome:?}") };
    assert_eq!(
        intent,
        UiIntent::Recognition(RecognitionIntent::Learn {
            device: dev("cam-A"),
            x: "46".into(),
            y: "46".into(),
            width: "29".into(),
            height: "29".into(),
            name: "Card".into(),
        })
    );
    assert_eq!(command_id(MenuItem::RecognitionPanel), 1071);
}
