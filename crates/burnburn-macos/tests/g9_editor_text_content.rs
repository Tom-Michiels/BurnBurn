//! G9-editor E07 panel, headless on the Mac: the content of the selected texts (insert and delete in
//! the middle, for several texts at once). Field text goes to presentation unparsed (DR-05). The
//! Windows host shows the same lines (`burnburn-windows/tests/g9_editor_text_content.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{
    EditorControl as E, G5Control as C, G5Line, G5Panel, g5_intent, g5_opens_by_itself, g5_panel,
};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, TextEdit, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), regular)
}

/// The texts "eerste" and "tweede", both selected.
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

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Option<Vec<G5Line>> {
    g5_panel(&h.view(), doc, G5Panel::TextContent, locale, &[]).map(|m| m.lines)
}

fn find(all: &[G5Line], control: E) -> Option<&G5Line> {
    all.iter().find(|l| l.control() == Some(C::Editor(control)))
}

fn intent(h: &HeadlessEditorHarness, doc: DocumentId, control: E, values: &[(E, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (C::Editor(*c), (*v).to_owned())).collect();
    g5_intent(&h.view(), doc, G5Panel::TextContent, C::Editor(control), &values)
}

#[test]
fn the_panel_opens_by_itself_for_texts_and_lists_what_it_will_change() {
    assert!(g5_opens_by_itself(G5Panel::TextContent));
    let (h, doc) = two_selected();
    let all = lines(&h, doc, Locale::En).expect("the panel for two texts");
    for c in [E::TextEditPosition, E::TextEditInsert, E::TextEditInsertGo, E::TextEditCount, E::TextEditDeleteGo] {
        assert!(find(&all, c).is_some(), "{c:?}");
    }
    let shown: Vec<&str> =
        all.iter().filter_map(|l| if let G5Line::Text(t) = l { Some(t.as_str()) } else { None }).collect();
    assert!(shown.iter().any(|t| t.contains("eerste")) && shown.iter().any(|t| t.contains("tweede")), "{shown:?}");
    let nl = lines(&h, doc, Locale::Nl).unwrap();
    assert!(matches!(find(&nl, E::TextEditInsertGo), Some(G5Line::Button { title, .. }) if title.contains("Voeg")));
}

#[test]
fn there_is_no_panel_without_a_text_selected() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(lines(&h, doc, Locale::En).is_none());
    let (mut h, doc) = two_selected();
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(lines(&h, doc, Locale::En).is_none());
}

#[test]
fn the_buttons_send_the_typed_text_unparsed_and_the_result_shows_in_both_texts() {
    let (mut h, doc) = two_selected();
    let go = intent(&h, doc, E::TextEditInsertGo, &[(E::TextEditPosition, " 2 "), (E::TextEditInsert, "!")]).unwrap();
    assert_eq!(
        go,
        UiIntent::Text { document: doc, edit: TextEdit::InsertAt { position: " 2 ".into(), text: "!".into() } }
    );
    h.send(go);
    assert_eq!(h.text_contents(doc), ["ee!rste", "tw!eede"]);
    let del = intent(&h, doc, E::TextEditDeleteGo, &[(E::TextEditPosition, "-3"), (E::TextEditCount, "2")]).unwrap();
    assert_eq!(
        del,
        UiIntent::Text { document: doc, edit: TextEdit::DeleteAt { position: "-3".into(), count: "2".into() } }
    );
    h.send(del);
    assert_eq!(h.text_contents(doc), ["ee!re", "tw!ee"]);
    // The panel shows the new content.
    let all = lines(&h, doc, Locale::En).unwrap();
    for wanted in ["ee!re", "tw!ee"] {
        assert!(all.iter().any(|l| matches!(l, G5Line::Text(t) if t.contains(wanted))), "{wanted}");
    }
    // A bad number is a problem line, in words.
    h.send(intent(&h, doc, E::TextEditInsertGo, &[(E::TextEditPosition, "twee"), (E::TextEditInsert, "?")]).unwrap());
    let all = lines(&h, doc, Locale::En).unwrap();
    assert!(all.iter().any(|l| matches!(l, G5Line::Problem(t) if t.contains("Position"))), "{all:?}");
}

#[test]
fn a_variable_text_shows_its_sample_beside_the_template_in_both_languages() {
    use burnburn_presentation::{JobClock, SerialField, VariableEdit};
    let (mut h, doc) = two_selected();
    h.controller().attach_clock(Arc::new(|| Some(JobClock::new(1_781_751_845, -300).unwrap())));
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::InsertText { document: doc, text: "SN-{serial}".into() });
    h.send(UiIntent::Variable { document: doc, edit: VariableEdit::SetVariable(true) });
    for (field, text) in [(SerialField::Start, "7"), (SerialField::End, "95"), (SerialField::Digits, "3")] {
        h.send(UiIntent::Variable { document: doc, edit: VariableEdit::Serial(field, text.into()) });
    }
    for locale in [Locale::En, Locale::Nl] {
        let all = lines(&h, doc, locale).expect("the panel");
        assert!(
            all.iter().any(|l| matches!(l, G5Line::Text(t) if t.contains("SN-{serial}") && t.contains("SN-007"))),
            "{locale:?}: {all:?}"
        );
    }
}
