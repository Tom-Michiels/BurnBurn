//! G9-editor E07 form, headless on the Windows side: the content of the selected texts. The lines
//! come from the same model as the Mac panel (`burnburn-macos/tests/g9_editor_text_content.rs`);
//! here only the form mapping and the intents.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, TextEdit, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, ControlValue, FormContext, FormKind, FormLine, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{EditorControl as E, G5Control as C, G5Panel as P};

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), regular)
}

fn two_selected() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.send(UiIntent::InsertText { document: doc, text: "eerste".into() });
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    h.send(UiIntent::InsertText { document: doc, text: "tweede".into() });
    let ids: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
    (h, doc)
}

#[test]
fn the_text_content_form_has_the_fields_and_sends_the_same_intents_as_the_mac_panel() {
    let (mut h, doc) = two_selected();
    let vm = h.view();
    let state = PanelState { g5_shown: vec![P::TextContent], ..PanelState::default() };
    let form = g5::forms(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &state, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::TextContent))
        .expect("the text content form");
    for c in [E::TextEditPosition, E::TextEditInsert, E::TextEditInsertGo, E::TextEditCount, E::TextEditDeleteGo] {
        assert!(form.find(&ControlId::G5(C::Editor(c))).is_some(), "{c:?}");
    }
    assert!(form.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.contains("eerste"))));

    let values = vec![
        (ControlId::G5(C::Editor(E::TextEditPosition)), ControlValue::Text("1".into())),
        (ControlId::G5(C::Editor(E::TextEditInsert)), ControlValue::Text("-".into())),
    ];
    let out = g5::react(
        P::TextContent,
        &ControlId::G5(C::Editor(E::TextEditInsertGo)),
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    let FormOutcome::Intent(intent) = out else { panic!("an intent") };
    assert_eq!(
        intent,
        UiIntent::Text { document: doc, edit: TextEdit::InsertAt { position: "1".into(), text: "-".into() } }
    );
    h.send(intent);
    assert_eq!(h.text_contents(doc), ["e-erste", "t-weede"]);
}
