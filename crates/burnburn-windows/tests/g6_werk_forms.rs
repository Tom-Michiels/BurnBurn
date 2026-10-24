// G6-werk forms (merged under this name: G6-editor also added a g6_forms.rs).
//! The golf-6 workshop panels as Windows forms, headless (G6-werk): preparing an image for tracing.
//! Same lines, words and intents as the macOS panel (`tests/g6_werk_panels.rs` there): a control
//! reports its value, presentation decides what changes. The Win32 realisation is only type-checked
//! from macOS, never run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::g5 as g5_fixtures;

use burnburn_presentation::{HeadlessEditorHarness, MaskMode, TracePrepIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::adapter::{
    Control, MenuItem, MessageContext, Reaction, command_id, menu_item, menu_title, msg, notify, on_message,
};
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use g5_fixtures::t_png;

fn opened() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), std::path::Path::new("/work/in.png"), &t_png()).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: std::path::PathBuf::from("/work/in.png") });
    h.run_tasks();
    (h, doc)
}

fn shown() -> PanelState {
    PanelState { g5_shown: vec![P::TracePrep], ..PanelState::default() }
}

fn forms(h: &HeadlessEditorHarness, doc: DocumentId, state: &PanelState) -> Vec<Form> {
    let vm = h.view();
    g5::forms(&FormContext { vm: &vm, document: doc, locale: Locale::En }, state, &Vec::new())
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

#[test]
fn the_preparation_form_waits_for_its_menu_item_and_a_tracer() {
    let (h, doc) = opened();
    assert!(forms(&h, doc, &PanelState::default()).iter().all(|f| f.kind != FormKind::G5(P::TracePrep)));
    let with = forms(&h, doc, &shown());
    assert!(with.iter().any(|f| f.kind == FormKind::G5(P::TracePrep)));

    let mut empty = HeadlessEditorHarness::new();
    let doc2 = empty.new_document().unwrap();
    assert!(forms(&empty, doc2, &shown()).iter().all(|f| f.kind != FormKind::G5(P::TracePrep)), "no tracer, no form");
}

#[test]
fn the_form_has_the_fields_and_buttons_of_the_mac_panel_and_a_button_sends_the_typed_text() {
    let (mut h, doc) = opened();
    let form = forms(&h, doc, &shown()).into_iter().find(|f| f.kind == FormKind::G5(P::TracePrep)).unwrap();
    for c in [C::PrepRadius, C::PrepOffset, C::PrepAreaX, C::PrepAreaWidth, C::PrepRegionThreshold, C::PrepAnchors] {
        assert!(form.find(&ControlId::G5(c)).is_some(), "{c:?}");
    }
    assert!(
        flat(&form).iter().any(|l| matches!(l, FormLine::Choice { id, .. } if *id == ControlId::G5(C::PrepMaskKind)))
    );

    let mut values = form.values();
    let set = |values: &mut Vec<(ControlId, ControlValue)>, c: C, v: ControlValue| match values
        .iter_mut()
        .find(|(id, _)| *id == ControlId::G5(c))
    {
        Some(slot) => slot.1 = v,
        None => values.push((ControlId::G5(c), v)),
    };
    set(&mut values, C::PrepAreaX, ControlValue::Text("2".into()));
    set(&mut values, C::PrepAreaY, ControlValue::Text("3".into()));
    set(&mut values, C::PrepAreaWidth, ControlValue::Text("20".into()));
    set(&mut values, C::PrepAreaHeight, ControlValue::Text("10".into()));
    set(&mut values, C::PrepMaskKind, ControlValue::Choice(1));
    let vm = h.view();
    let outcome = g5::react(
        P::TracePrep,
        &ControlId::G5(C::PrepAddMask),
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    let FormOutcome::Intent(intent) = outcome else { panic!("an intent: {outcome:?}") };
    assert_eq!(
        intent,
        UiIntent::TracePrep {
            document: doc,
            intent: TracePrepIntent::AddMask {
                mode: MaskMode::Remove,
                x: "2".into(),
                y: "3".into(),
                width: "20".into(),
                height: "10".into()
            }
        }
    );
    h.send(intent);
    h.run_tasks();
    let form = forms(&h, doc, &shown()).into_iter().find(|f| f.kind == FormKind::G5(P::TracePrep)).unwrap();
    assert!(
        flat(&form).iter().any(|l| matches!(l, FormLine::Text(t) if t == "Mask 1: remove, 2, 3, 20 × 10 px")),
        "the mask is listed"
    );
}

#[test]
fn the_menu_item_has_the_macos_tag_and_toggles_the_panel() {
    let (h, doc) = opened();
    let vm = h.view();
    assert_eq!(command_id(MenuItem::TracePrepPanel), 1032);
    assert_eq!(menu_item(1032), Some(MenuItem::TracePrepPanel));
    assert_eq!(menu_title(MenuItem::TracePrepPanel, &vm, Locale::Nl), "Beeld voorbereiden");
    let ctx = MessageContext {
        document: doc,
        control: Control::Frame,
        dpi: 96,
        text: "",
        selected_rows: &[],
        alt: false,
        cursor_px: [0, 0],
        time_ms: 0,
    };
    let wm = |id: u16| (id as usize) | ((notify::MENU as usize) << 16);
    assert_eq!(
        on_message(&ctx, msg::WM_COMMAND, wm(command_id(MenuItem::TracePrepPanel)), 0, &vm),
        Reaction::ToggleG5Panel(P::TracePrep)
    );
}

#[test]
fn the_structure_form_offers_making_a_part_and_a_reference_from_the_selection() {
    use burnburn_presentation::CommandKey;
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    let state = PanelState { g5_shown: vec![P::Structure], ..PanelState::default() };
    let form = forms(&h, doc, &state).into_iter().find(|f| f.kind == FormKind::G5(P::Structure)).unwrap();
    for c in [C::StructDefineName, C::StructDefine, C::RefEdge, C::RefAdd, C::RefName] {
        assert!(form.find(&ControlId::G5(c)).is_some(), "{c:?}");
    }
    let mut values = form.values();
    let name = values.iter_mut().find(|(id, _)| *id == ControlId::G5(C::StructDefineName)).unwrap();
    name.1 = ControlValue::Text("Beugel".into());
    let vm = h.view();
    let context = FormContext { vm: &vm, document: doc, locale: Locale::En };
    let outcome = g5::react(P::Structure, &ControlId::G5(C::StructDefine), &values, &context);
    let FormOutcome::Intent(UiIntent::Structure { edit, .. }) = outcome else { panic!("{outcome:?}") };
    assert_eq!(edit, burnburn_presentation::StructureEdit::DefinePart { name: "Beugel".into() });
}
