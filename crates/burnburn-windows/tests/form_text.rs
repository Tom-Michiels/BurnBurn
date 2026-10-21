//! The text panels of the Windows window, headless (W4-L8a): text toolbar, text on a path,
//! variable text and the barcode dialog as floating forms. Rows, words and intents come from the
//! same view models and slot logic as the macOS panels (`text_panels.rs` there); a control reports
//! its value and at most one intent follows, with typed text unparsed (DR-05).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_presentation::{
    BarcodeEdit, CommandKey, Distort, HeadlessEditorHarness, SelectMode, SerialField, Symbology, TextEdit,
    TextPathEdit, UiIntent, VariableEdit,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, FilePick, Form, FormContext, FormKind, FormLine, FormOutcome, Placement, text,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::Slot;

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, data.clone());
    src.insert("Liberation Sans", FontStyle::Bold, data);
    let biz = Arc::new(FontData::from_bytes(read_fixture("fonts", "BIZUDPGothic-Regular.ttf").unwrap()).unwrap());
    src.insert("BIZ UDPGothic", FontStyle::Regular, biz);
    (Arc::new(src), regular)
}

fn text_window(content: &str) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.controller().attach_clock(Arc::new(|| burnburn_presentation::JobClock::new(1_781_751_845, -300).ok()));
    h.send(UiIntent::InsertText { document: doc, text: content.into() });
    (h, doc)
}

fn forms(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<Form> {
    let vm = h.view();
    text::forms(&FormContext { vm: &vm, document: doc, locale })
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, kind: FormKind) -> Option<Form> {
    forms(h, doc, Locale::En).into_iter().find(|f| f.kind == kind)
}

fn react(h: &HeadlessEditorHarness, doc: DocumentId, slot: Slot, value: ControlValue) -> FormOutcome {
    let vm = h.view();
    text::react(&ControlId::Text(slot), &value, &FormContext { vm: &vm, document: doc, locale: Locale::En })
}

fn intent(h: &HeadlessEditorHarness, doc: DocumentId, slot: Slot, value: ControlValue) -> Option<UiIntent> {
    match react(h, doc, slot, value) {
        FormOutcome::Intent(i) => Some(i),
        _ => None,
    }
}

fn typed(s: &str) -> ControlValue {
    ControlValue::Text(s.into())
}

#[test]
fn no_text_no_panels_and_a_selected_text_has_a_floating_toolbar() {
    let mut h = HeadlessEditorHarness::new();
    let empty = h.new_document().unwrap();
    assert!(forms(&h, empty, Locale::En).is_empty());
    let (h, doc) = text_window("HIT");
    let f = form(&h, doc, FormKind::TextToolbar).expect("a text is selected");
    assert_eq!(f.placement, Placement::Floating);
    assert!(form(&h, doc, FormKind::TextPath).is_none() && form(&h, doc, FormKind::Barcode).is_none());
    let nl = forms(&h, doc, Locale::Nl).into_iter().find(|f| f.kind == FormKind::TextToolbar).unwrap();
    assert_eq!(nl.title, "Tekst");
    match f.find(&ControlId::Text(Slot::Family)) {
        Some(FormLine::Choice { items, selected, .. }) => {
            assert_eq!(items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["BIZ UDPGothic", "Liberation Sans"]);
            assert_eq!(*selected, 1);
        }
        other => panic!("{other:?}"),
    }
    // Italic has no face: greyed, and the reason is under the toolbar's controls.
    assert!(matches!(f.find(&ControlId::Text(Slot::Italic)), Some(FormLine::Check { enabled: false, .. })));
    assert!(matches!(f.find(&ControlId::Text(Slot::Bold)), Some(FormLine::Check { enabled: true, on: false, .. })));
    let fields = |id| match f.find(&ControlId::Text(id)) {
        Some(FormLine::Field { text, .. }) => text.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!((fields(Slot::HSpace).as_str(), fields(Slot::VSpace).as_str(), fields(Slot::Height).as_str()), ("0", "0", "10"));
}

#[test]
fn toolbar_controls_become_one_text_edit_and_a_repeat_is_none() {
    let (h, doc) = text_window("HIT");
    let edit = |e: TextEdit| Some(UiIntent::Text { document: doc, edit: e });
    assert_eq!(intent(&h, doc, Slot::Family, ControlValue::Choice(0)), edit(TextEdit::Family("BIZ UDPGothic".into())));
    assert_eq!(intent(&h, doc, Slot::Family, ControlValue::Choice(1)), None, "the family it already has");
    assert_eq!(intent(&h, doc, Slot::Bold, ControlValue::Toggle(true)), edit(TextEdit::Bold(true)));
    assert_eq!(intent(&h, doc, Slot::Bold, ControlValue::Toggle(false)), None);
    assert_eq!(intent(&h, doc, Slot::HSpace, typed("2,5")), edit(TextEdit::HSpace("2,5".into())));
    assert_eq!(intent(&h, doc, Slot::HSpace, typed("0")), None);
    assert_eq!(
        intent(&h, doc, Slot::Convert, ControlValue::Press),
        Some(UiIntent::Invoke { document: Some(doc), command: CommandKey::ShapesConvertToPath })
    );
    // A control reporting the wrong kind of value sends nothing.
    assert_eq!(react(&h, doc, Slot::Bold, typed("x")), FormOutcome::Nothing);
}

#[test]
fn the_text_on_path_sheet_opens_with_the_command_and_forwards_its_choices() {
    let (mut h, doc) = text_window("HIT");
    let outlines: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::ToolCircle);
    h.drag(doc, (100.0, 100.0), (140.0, 100.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let circle = h.document(doc).unwrap().objects.iter().map(|o| o.id).find(|i| !outlines.contains(i)).unwrap();
    let mut both = outlines;
    both.push(circle);
    h.send(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
    assert!(form(&h, doc, FormKind::TextPath).is_none(), "closed until the command");
    h.invoke(doc, CommandKey::TextOnPath);
    let f = form(&h, doc, FormKind::TextPath).expect("open");
    assert_eq!(f.placement, Placement::Floating);
    assert!(matches!(f.find(&ControlId::Text(Slot::PathDistort)), Some(FormLine::Choice { selected: 0, items, .. }) if items.len() == 2));
    let path = |e: TextPathEdit| Some(UiIntent::TextPath { document: doc, edit: e });
    assert_eq!(intent(&h, doc, Slot::PathDistort, ControlValue::Choice(1)), path(TextPathEdit::Distort(Distort::Warp)));
    assert_eq!(intent(&h, doc, Slot::PathStart, typed("12,5")), path(TextPathEdit::Start("12,5".into())));
    assert_eq!(intent(&h, doc, Slot::PathReverse, ControlValue::Toggle(true)), path(TextPathEdit::Reverse(true)));
    assert_eq!(intent(&h, doc, Slot::PathApply, ControlValue::Press), path(TextPathEdit::Apply));
    assert_eq!(intent(&h, doc, Slot::PathCancel, ControlValue::Press), path(TextPathEdit::Cancel));
    // Without a guide the sheet says so and Apply is greyed.
    let first: Vec<_> = h.document(doc).unwrap().objects.iter().take(1).map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids: first, mode: SelectMode::Replace });
    let f = form(&h, doc, FormKind::TextPath).unwrap();
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Text(t) | FormLine::Problem(t) if t.to_lowercase().contains("one text and one"))));
    let apply = f
        .lines
        .iter()
        .filter_map(|l| if let FormLine::Buttons(b) = l { Some(b) } else { None })
        .flatten()
        .find(|b| b.id == ControlId::Text(Slot::PathApply))
        .unwrap();
    assert!(!apply.enabled);
}

#[test]
fn the_variable_panel_shows_serial_csv_and_the_preview_and_its_controls_send_variable_edits() {
    let (mut h, doc) = text_window("{csv:naam} #{serial}");
    h.send(UiIntent::Variable { document: doc, edit: VariableEdit::SetVariable(true) });
    let f = form(&h, doc, FormKind::Variable).expect("a text is selected");
    assert!(matches!(f.find(&ControlId::Text(Slot::VarKind)), Some(FormLine::Choice { selected: 1, .. })));
    let cur = match f.find(&ControlId::Text(Slot::SerialCurrent)) {
        Some(FormLine::Field { text, .. }) => text.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(cur, "1");
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Problem(p) if p.to_lowercase().contains("data"))), "no data yet");
    let var = |e: VariableEdit| Some(UiIntent::Variable { document: doc, edit: e });
    assert_eq!(intent(&h, doc, Slot::SerialEnd, typed("99")), var(VariableEdit::Serial(SerialField::End, "99".into())));
    assert_eq!(intent(&h, doc, Slot::SerialStart, typed("1")), None, "the value it shows");
    assert_eq!(intent(&h, doc, Slot::VarNext, ControlValue::Press), var(VariableEdit::Next));
    assert_eq!(intent(&h, doc, Slot::VarBake, ControlValue::Press), var(VariableEdit::Bake));
    // The CSV button asks for a file; the chosen file completes the intent, a cancel changes nothing.
    assert_eq!(react(&h, doc, Slot::VarCsv, ControlValue::Press), FormOutcome::PickFile(FilePick::Csv));
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::En };
    assert_eq!(text::file_chosen(None, &ctx), FormOutcome::Nothing);
    let bytes = b"naam;nummer\nJansen;7\n".to_vec();
    let outcome = text::file_chosen(Some(("k.csv".into(), bytes.clone())), &ctx);
    assert_eq!(outcome, FormOutcome::Intent(UiIntent::Variable { document: doc, edit: VariableEdit::LoadCsv { name: "k.csv".into(), bytes } }));
    if let FormOutcome::Intent(i) = outcome {
        h.send(i);
    }
    let f = form(&h, doc, FormKind::Variable).unwrap();
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.contains("Jansen #0001") || t.contains("Jansen #1"))), "{:?}", f.lines);
}

fn open_barcode() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::BarcodeNew);
    (h, doc)
}

#[test]
fn the_barcode_dialog_lists_the_types_with_unsupported_ones_greyed_and_only_the_fields_of_the_type() {
    let (mut h, doc) = open_barcode();
    let f = form(&h, doc, FormKind::Barcode).expect("the dialog is open");
    match f.find(&ControlId::Text(Slot::BcType)) {
        Some(FormLine::Choice { items, selected, .. }) => {
            assert_eq!(items.len(), 29);
            assert_eq!(items.iter().filter(|i| !i.enabled).count(), 2, "Grid Matrix and ArUco MIP stay unsupported");
            assert_eq!(*selected, Symbology::ALL.iter().position(|s| *s == Symbology::QrCode).unwrap());
        }
        other => panic!("{other:?}"),
    }
    assert!(f.ids().contains(&ControlId::Text(Slot::BcEcc)) && !f.ids().contains(&ControlId::Text(Slot::BcHeight)));
    let create = f
        .lines
        .iter()
        .filter_map(|l| if let FormLine::Buttons(b) = l { Some(b) } else { None })
        .flatten()
        .find(|b| b.id == ControlId::Text(Slot::BcCreate))
        .unwrap();
    assert!(!create.enabled, "an empty payload cannot be made");
    h.send(UiIntent::Barcode { document: doc, edit: BarcodeEdit::Symbology(Symbology::Code128) });
    let f = form(&h, doc, FormKind::Barcode).unwrap();
    assert!(f.ids().contains(&ControlId::Text(Slot::BcHeight)) && !f.ids().contains(&ControlId::Text(Slot::BcEcc)));
    let bc = |e: BarcodeEdit| Some(UiIntent::Barcode { document: doc, edit: e });
    assert_eq!(intent(&h, doc, Slot::BcPayload, typed("HELLO")), bc(BarcodeEdit::Payload("HELLO".into())));
    assert_eq!(intent(&h, doc, Slot::BcHeight, typed("8")), bc(BarcodeEdit::Height("8".into())));
    assert_eq!(intent(&h, doc, Slot::BcCreate, ControlValue::Press), bc(BarcodeEdit::Create));
    let grid = Symbology::ALL.iter().position(|s| *s == Symbology::GridMatrix).unwrap();
    assert_eq!(intent(&h, doc, Slot::BcType, ControlValue::Choice(grid)), None, "a greyed type does nothing");
    assert_eq!(
        intent(&h, doc, Slot::BcType, ControlValue::Choice(Symbology::ALL.iter().position(|s| *s == Symbology::QrCode).unwrap())),
        bc(BarcodeEdit::Symbology(Symbology::QrCode))
    );
}
