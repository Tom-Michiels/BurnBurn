//! LF17: the Windows host shows the same empty states as the Mac, headless: the empty-state row of
//! a form becomes a wrapping note with the sentence (the glyph is the native drawing's job, LF14),
//! the golf-5 panels turn the empty-state line into text, and the shared progress rows give the
//! Windows window the same running tasks with the same cancel intent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_host_models::empty_state::progress_rows;
use burnburn_host_models::form::{Control, FormId, FormRow, FormSection, FormSpec};
use burnburn_host_models::l10n::Locale;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_windows::forms::spec_layout::{DluBase, SpecClass, TextMeter, layout_spec};
use burnburn_windows::forms::{FormContext, FormKind, FormLine, PanelState, g5};
use burnburn_windows::panels::G5Panel;

struct Fixed;

impl TextMeter for Fixed {
    fn width(&self, text: &str, _bold: bool) -> i32 {
        i32::try_from(text.chars().count()).unwrap() * 6
    }
}

#[test]
fn an_empty_state_row_is_a_wrapping_note_with_its_sentence_at_every_dpi() {
    let spec = FormSpec {
        sections: vec![FormSection::new(
            Some("Restplaten"),
            vec![FormRow::new(
                7,
                "",
                Control::EmptyState {
                    symbol: "rectangle.dashed".into(),
                    text: "Nog geen restplaten bewaard. Selecteer de omtrek van een restplaat om hem te bewaren."
                        .into(),
                },
            )],
        )],
        ..FormSpec::default()
    };
    for dpi in [96, 144, 192] {
        let layout = layout_spec(&spec, DluBase::segoe_ui_9(dpi), &Fixed);
        let note = layout.controls.iter().find(|c| c.id == Some(FormId(7))).expect("the empty state");
        assert_eq!(note.class, SpecClass::Note, "{dpi}");
        assert!(note.text.starts_with("Nog geen restplaten"), "{dpi}");
        assert!(note.rect.w > 0 && note.rect.h > 0, "{dpi}");
        assert!(note.rect.x + note.rect.w <= layout.client.0, "{dpi}: inside the form");
    }
}

#[test]
fn the_remnants_form_says_the_empty_sentence_as_text() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let vm = h.view();
    let context = FormContext { vm: &vm, document: doc, locale: Locale::Nl };
    let shown = PanelState { g5_shown: vec![G5Panel::Remnants], ..PanelState::default() };
    let form = g5::forms(&context, &shown, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(G5Panel::Remnants))
        .expect("the remnant form");
    let says =
        form.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.starts_with("Nog geen restplaten bewaard.")));
    assert!(says, "{:?}", form.lines);
}

#[test]
fn the_windows_window_gets_the_same_progress_row_and_cancel_intent_for_a_running_compile() {
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    let profile = MachineProfile::new(
        "bed",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
    let mut h = HeadlessEditorHarness::new();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
    h.run_tasks();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: burnburn_presentation::SelectMode::Replace });
    let p = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    h.send(UiIntent::AddOperation { document: doc, draft });
    h.invoke(doc, CommandKey::Compile);
    let rows = progress_rows(&h.document(doc).unwrap(), Locale::Nl);
    assert_eq!(rows.len(), 1, "the running compile");
    let cancel = rows[0].cancel.as_ref().expect("a compile can be cancelled");
    assert_eq!(cancel.intent, UiIntent::CancelTask { document: doc, task: rows[0].task });
    assert_eq!(cancel.label, "Annuleer");
    h.run_tasks();
    assert_eq!(progress_rows(&h.document(doc).unwrap(), Locale::Nl), Vec::new());
}
