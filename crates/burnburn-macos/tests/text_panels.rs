//! The text toolbar, the text-on-path sheet, the variable-text panel and the barcode dialog,
//! headless (W3-UI-tekst): what the widgets show comes from presentation's view models in the
//! window language, and every control becomes at most one intent with typed text unparsed
//! (DR-05). `tests/native/macos/text_panels.rs` drives the real widgets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{Slot, SlotValue, TextPanels, text_panels};
use burnburn_presentation::{
    AdvancePolicy, Align, BarcodeEdit, CommandKey, Distort, HeadlessEditorHarness, SelectMode, SerialField,
    SymbolInputMode, Symbology, TextEdit, TextPathEdit, UiIntent, VAlign, VariableEdit,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;

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

fn panels(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> TextPanels {
    text_panels(&h.view(), doc, locale)
}

/// What a control becomes, through the adapter exactly as the app does it.
fn intent(h: &HeadlessEditorHarness, doc: DocumentId, slot: Slot, value: SlotValue) -> Option<UiIntent> {
    match translate(NativeEvent::TextPanel { document: doc, slot, value }, &h.view()) {
        Reaction::Intent(i) => Some(i),
        _ => None,
    }
}

fn text(s: &str) -> SlotValue {
    SlotValue::Text(s.into())
}

#[test]
fn slots_have_unique_tags_that_round_trip_and_zero_is_nobody() {
    let mut seen = std::collections::HashSet::new();
    for slot in Slot::ALL {
        assert!(slot.tag() > 0, "{slot:?}");
        assert!(seen.insert(slot.tag()), "{slot:?} shares its tag");
        assert_eq!(Slot::from_tag(slot.tag()), Some(slot));
    }
    assert_eq!((Slot::from_tag(0), Slot::from_tag(-3), Slot::from_tag(10_000)), (None, None, None));
}

#[test]
fn every_slot_reports_itself_in_one_way() {
    use burnburn_macos::panels::SlotKind;
    let count = |kind| Slot::ALL.iter().filter(|s| s.kind() == kind).count();
    assert_eq!(count(SlotKind::Csv), 1, "only the CSV button opens a file panel");
    assert_eq!(
        (count(SlotKind::Text), count(SlotKind::Toggle), count(SlotKind::Choice), count(SlotKind::Press)),
        (19, 7, 9, 11)
    );
}

#[test]
fn the_toolbar_shows_the_selected_text_in_the_window_language() {
    let (h, doc) = text_window("HIT");
    assert!(panels(&h, doc, Locale::Nl).path.is_none() && panels(&h, doc, Locale::Nl).barcode.is_none());
    let nl = panels(&h, doc, Locale::Nl).toolbar.expect("a text is selected");
    assert_eq!(nl.families, ["BIZ UDPGothic", "Liberation Sans"]);
    assert_eq!(nl.family, 1);
    assert!(nl.bold.enabled && !nl.bold.on && nl.bold.tip.is_empty());
    assert!(!nl.italic.enabled && nl.italic.tip.contains("cursieve"), "greyed with the reason: {:?}", nl.italic.tip);
    let en = panels(&h, doc, Locale::En).toolbar.unwrap();
    assert!(en.italic.tip.contains("italic"), "{:?}", en.italic.tip);
    assert_eq!((nl.align, nl.valign), (0, 0), "left, and the baseline is the first of the vertical choices");
    assert_eq!((nl.h_space.as_str(), nl.v_space.as_str(), nl.height.as_str()), ("0", "0", "10"));
    assert!(nl.welded.enabled && nl.convert_enabled && nl.note.is_empty());

    // No text, no toolbar.
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert_eq!(panels(&h, doc, Locale::Nl), TextPanels::default());
}

#[test]
fn toolbar_controls_become_one_text_edit_and_a_repeat_is_none() {
    let (h, doc) = text_window("HIT");
    let send = |slot, value| intent(&h, doc, slot, value);
    let edit = |e: TextEdit| Some(UiIntent::Text { document: doc, edit: e });
    assert_eq!(send(Slot::Family, SlotValue::Choice(0)), edit(TextEdit::Family("BIZ UDPGothic".into())));
    assert_eq!(send(Slot::Family, SlotValue::Choice(1)), None, "the family it already has");
    assert_eq!(send(Slot::Family, SlotValue::Choice(9)), None, "a row that does not exist");
    assert_eq!(send(Slot::Bold, SlotValue::Toggle(true)), edit(TextEdit::Bold(true)));
    assert_eq!(send(Slot::Italic, SlotValue::Toggle(true)), edit(TextEdit::Italic(true)));
    assert_eq!(send(Slot::Uppercase, SlotValue::Toggle(true)), edit(TextEdit::Uppercase(true)));
    assert_eq!(send(Slot::Welded, SlotValue::Toggle(true)), edit(TextEdit::Welded(true)));
    assert_eq!(send(Slot::SingleLine, SlotValue::Toggle(true)), edit(TextEdit::SingleLine(true)));
    assert_eq!(send(Slot::Align, SlotValue::Choice(1)), edit(TextEdit::Align(Align::Center)));
    assert_eq!(send(Slot::Align, SlotValue::Choice(2)), edit(TextEdit::Align(Align::Right)));
    for (index, v) in [(1, VAlign::Top), (2, VAlign::Middle), (3, VAlign::Bottom)] {
        assert_eq!(send(Slot::VAlign, SlotValue::Choice(index)), edit(TextEdit::VAlign(v)), "{index}");
    }
    assert_eq!(send(Slot::VAlign, SlotValue::Choice(0)), None, "baseline is what it has");
    assert_eq!(send(Slot::HSpace, text("2,5")), edit(TextEdit::HSpace("2,5".into())), "typed text goes unparsed");
    assert_eq!(send(Slot::VSpace, text("1")), edit(TextEdit::VSpace("1".into())));
    assert_eq!(send(Slot::Height, text("12")), edit(TextEdit::Height("12".into())));
    assert_eq!(send(Slot::HSpace, text("0")), None, "Return and the end of editing are one commit");
    assert_eq!(
        send(Slot::Convert, SlotValue::Press),
        Some(UiIntent::Invoke { document: Some(doc), command: CommandKey::ShapesConvertToPath })
    );
}

#[test]
fn a_refused_field_keeps_its_text_and_a_retry_of_the_same_text_still_goes() {
    let (mut h, doc) = text_window("HIT");
    h.send(UiIntent::Text { document: doc, edit: TextEdit::Height("veel".into()) });
    let nl = panels(&h, doc, Locale::Nl).toolbar.unwrap();
    assert!(nl.note.to_lowercase().contains("hoogte"), "the refusal names the field: {:?}", nl.note);
    assert!(panels(&h, doc, Locale::En).toolbar.unwrap().note.to_lowercase().contains("height"));
    assert_eq!(
        intent(&h, doc, Slot::Height, text("10")),
        Some(UiIntent::Text { document: doc, edit: TextEdit::Height("10".into()) }),
        "the shown value equals the typed one, but the field is in error"
    );
}

#[test]
fn the_text_on_path_sheet_follows_the_dialog_and_forwards_its_choices() {
    let (mut h, doc) = text_window("HIT");
    let outlines: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::ToolCircle);
    h.drag(doc, (100.0, 100.0), (140.0, 100.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let circle = h.document(doc).unwrap().objects.iter().map(|o| o.id).find(|i| !outlines.contains(i)).unwrap();
    let mut both = outlines;
    both.push(circle);
    h.send(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
    assert!(panels(&h, doc, Locale::Nl).path.is_none(), "closed until the command");
    h.invoke(doc, CommandKey::TextOnPath);
    let p = panels(&h, doc, Locale::Nl).path.expect("open");
    assert_eq!(
        (p.distort, p.start.as_str(), p.offset.as_str(), p.reverse, p.apply_enabled),
        (0, "0", "0", false, true)
    );
    assert!(p.note.is_empty());

    let path = |e: TextPathEdit| Some(UiIntent::TextPath { document: doc, edit: e });
    assert_eq!(intent(&h, doc, Slot::PathDistort, SlotValue::Choice(1)), path(TextPathEdit::Distort(Distort::Warp)));
    assert_eq!(intent(&h, doc, Slot::PathDistort, SlotValue::Choice(0)), None, "Rigid is what the sheet shows");
    assert_eq!(intent(&h, doc, Slot::PathStart, text("12,5")), path(TextPathEdit::Start("12,5".into())));
    assert_eq!(intent(&h, doc, Slot::PathOffset, text("-1")), path(TextPathEdit::Offset("-1".into())));
    assert_eq!(intent(&h, doc, Slot::PathReverse, SlotValue::Toggle(true)), path(TextPathEdit::Reverse(true)));
    assert_eq!(intent(&h, doc, Slot::PathApply, SlotValue::Press), path(TextPathEdit::Apply));
    assert_eq!(intent(&h, doc, Slot::PathCancel, SlotValue::Press), path(TextPathEdit::Cancel));

    // Without a guide the sheet says so and Apply is off.
    let first: Vec<_> = h.document(doc).unwrap().objects.iter().take(1).map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids: first, mode: SelectMode::Replace });
    let p = panels(&h, doc, Locale::En).path.unwrap();
    assert!(!p.apply_enabled && p.note.to_lowercase().contains("one text and one"), "{:?}", p.note);
}

#[test]
fn the_variable_panel_shows_serial_csv_preview_and_red_reasons() {
    let (mut h, doc) = text_window("{csv:naam} #{serial}");
    for e in [VariableEdit::SetVariable(true), VariableEdit::Serial(SerialField::Digits, "4".into())] {
        h.send(UiIntent::Variable { document: doc, edit: e });
    }
    let p = panels(&h, doc, Locale::Nl).variable.expect("a text is selected");
    assert!(p.variable && p.csv_line.is_empty() && !p.csv_loaded);
    assert_eq!((p.current.as_str(), p.start.as_str(), p.digits.as_str(), p.wrap, p.policy), ("1", "1", "4", true, 0));
    assert!(p.preview_is_error && p.preview.to_lowercase().contains("gegevens"), "no data: {:?}", p.preview);

    h.send(UiIntent::Variable {
        document: doc,
        edit: VariableEdit::LoadCsv { name: "k.csv".into(), bytes: b"naam;nummer\nJansen;7\nPietersen;\n".to_vec() },
    });
    let p = panels(&h, doc, Locale::Nl).variable.unwrap();
    assert_eq!((p.preview.as_str(), p.preview_is_error), ("Jansen #0001", false));
    assert!(p.csv_loaded && p.csv_line.contains("k.csv") && p.csv_line.contains('2'), "{:?}", p.csv_line);
    assert_eq!(p.row, "1");

    h.send(UiIntent::Variable { document: doc, edit: VariableEdit::Row("9".into()) });
    let p = panels(&h, doc, Locale::En).variable.unwrap();
    assert!(p.problem.to_lowercase().contains("row"), "{:?}", p.problem);
    h.send(UiIntent::Variable { document: doc, edit: VariableEdit::Test });
    h.send(UiIntent::Variable { document: doc, edit: VariableEdit::SetVariable(true) });
    let p = panels(&h, doc, Locale::En).variable.unwrap();
    assert!(p.test.is_empty(), "a change clears the old Test");
    h.send(UiIntent::Variable { document: doc, edit: VariableEdit::Test });
    let p = panels(&h, doc, Locale::En).variable.unwrap();
    assert!(p.test.contains("{csv:naam} #{serial}"), "{:?}", p.test);
}

#[test]
fn a_missing_value_names_column_and_row_in_the_window_language() {
    let (mut h, doc) = text_window("{csv:nummer}");
    for e in [
        VariableEdit::SetVariable(true),
        VariableEdit::LoadCsv { name: "k.csv".into(), bytes: b"naam;nummer\nJansen;7\nPietersen;\n".to_vec() },
        VariableEdit::Row("2".into()),
    ] {
        h.send(UiIntent::Variable { document: doc, edit: e });
    }
    let nl = panels(&h, doc, Locale::Nl).variable.unwrap();
    assert!(nl.preview_is_error && nl.preview.contains("nummer") && nl.preview.contains('2'), "{:?}", nl.preview);
    assert!(nl.preview.to_lowercase().contains("waarde"), "{:?}", nl.preview);
    let en = panels(&h, doc, Locale::En).variable.unwrap();
    assert!(en.preview.to_lowercase().contains("value") && en.preview.contains("nummer"), "{:?}", en.preview);
}

#[test]
fn variable_controls_become_variable_edits() {
    let (h, doc) = text_window("{serial}");
    let send = |slot, value| intent(&h, doc, slot, value);
    let var = |e: VariableEdit| Some(UiIntent::Variable { document: doc, edit: e });
    assert_eq!(send(Slot::VarKind, SlotValue::Choice(1)), var(VariableEdit::SetVariable(true)));
    assert_eq!(send(Slot::VarKind, SlotValue::Choice(0)), None, "it is Normal already");
    assert_eq!(send(Slot::VarOffset, text("3")), var(VariableEdit::Offset("3".into())));
    assert_eq!(send(Slot::SerialStart, text("10")), var(VariableEdit::Serial(SerialField::Start, "10".into())));
    assert_eq!(send(Slot::SerialEnd, text("99")), var(VariableEdit::Serial(SerialField::End, "99".into())));
    assert_eq!(send(Slot::SerialBy, text("5")), var(VariableEdit::Serial(SerialField::AdvanceBy, "5".into())));
    assert_eq!(send(Slot::SerialDigits, text("4")), var(VariableEdit::Serial(SerialField::Digits, "4".into())));
    assert_eq!(send(Slot::SerialCurrent, text("7")), var(VariableEdit::Serial(SerialField::Current, "7".into())));
    assert_eq!(send(Slot::SerialStart, text("1")), None, "the value it shows");
    assert_eq!(send(Slot::SerialWrap, SlotValue::Toggle(false)), var(VariableEdit::Wrap(false)));
    assert_eq!(
        send(Slot::VarPolicy, SlotValue::Choice(1)),
        var(VariableEdit::Policy(AdvancePolicy::OnUploadConfirmed))
    );
    assert_eq!(send(Slot::VarPolicy, SlotValue::Choice(2)), var(VariableEdit::Policy(AdvancePolicy::OnJobStarted)));
    assert_eq!(send(Slot::VarPolicy, SlotValue::Choice(0)), None, "manual is the default it shows");
    assert_eq!(send(Slot::VarPrevious, SlotValue::Press), var(VariableEdit::Previous));
    assert_eq!(send(Slot::VarNext, SlotValue::Press), var(VariableEdit::Next));
    assert_eq!(send(Slot::VarReset, SlotValue::Press), var(VariableEdit::Reset));
    assert_eq!(send(Slot::VarTest, SlotValue::Press), var(VariableEdit::Test));
    assert_eq!(send(Slot::VarBake, SlotValue::Press), var(VariableEdit::Bake));
    assert_eq!(send(Slot::VarClearCsv, SlotValue::Press), var(VariableEdit::ClearCsv));
    assert_eq!(send(Slot::VarRow, text("2")), var(VariableEdit::Row("2".into())));
    assert_eq!(
        send(Slot::VarCsv, SlotValue::Csv(Some(("k.csv".into(), b"a\n1\n".to_vec())))),
        var(VariableEdit::LoadCsv { name: "k.csv".into(), bytes: b"a\n1\n".to_vec() })
    );
    assert_eq!(send(Slot::VarCsv, SlotValue::Csv(None)), None, "a cancelled file panel changes nothing");
}

fn open_barcode() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::BarcodeNew);
    (h, doc)
}

#[test]
fn the_barcode_sheet_lists_29_types_with_unsupported_ones_greyed_and_reasoned() {
    let (h, doc) = open_barcode();
    let b = panels(&h, doc, Locale::Nl).barcode.expect("the dialog is open");
    assert_eq!(b.types.len(), 29);
    // W3-robuust added 2 of 5 (Matrix) and Han Xin; Grid Matrix and ArUco MIP stay unsupported with a reason.
    let greyed: Vec<&str> = b.types.iter().filter(|t| !t.enabled).map(|t| t.title.as_str()).collect();
    assert_eq!(greyed.len(), 2, "{greyed:?}");
    assert!(
        greyed.iter().any(|t| t.contains("Grid Matrix")) && greyed.iter().any(|t| t.contains("ArUco")),
        "{greyed:?}"
    );
    for t in &b.types {
        assert_eq!(t.enabled, t.tip.is_empty() || t.tip.starts_with("Bron"), "{}: {:?}", t.title, t.tip);
    }
    let grid = b.types.iter().find(|t| t.title.contains("Grid Matrix")).unwrap();
    assert!(!grid.enabled && grid.tip.contains("verified"), "the domain's reason is the tooltip: {:?}", grid.tip);
    assert_eq!(b.symbology, Symbology::ALL.iter().position(|s| *s == Symbology::QrCode).unwrap());
    assert!(b.height.is_none() && b.ecc.is_some() && b.input_modes.is_none() || b.input_modes.is_some());
    assert!(!b.create_enabled, "an empty payload cannot be made");
    assert!(b.problems.to_lowercase().contains("inhoud"), "{:?}", b.problems);
}

/// G6-editor: a generated barcode opens again and the sheet's button says Apply instead of Create.
#[test]
fn the_barcode_sheet_button_says_apply_when_a_generated_barcode_is_changed() {
    let (mut h, doc) = open_barcode();
    assert_eq!(panels(&h, doc, Locale::Nl).barcode.unwrap().create_title, "Maak");
    h.send(UiIntent::Barcode { document: doc, edit: BarcodeEdit::Payload("HALLO".into()) });
    h.send(UiIntent::Barcode { document: doc, edit: BarcodeEdit::Create });
    assert!(panels(&h, doc, Locale::Nl).barcode.is_none(), "created: the sheet is gone");
    h.invoke(doc, CommandKey::GeneratorEdit);
    let nl = panels(&h, doc, Locale::Nl).barcode.expect("opened again");
    assert_eq!((nl.create_title.as_str(), nl.payload.as_str()), ("Toepassen", "HALLO"));
    assert_eq!(panels(&h, doc, Locale::En).barcode.unwrap().create_title, "Apply");
}

#[test]
fn barcode_controls_become_barcode_edits_and_a_greyed_type_does_nothing() {
    let (h, doc) = open_barcode();
    let b = panels(&h, doc, Locale::Nl).barcode.unwrap();
    let send = |slot, value| intent(&h, doc, slot, value);
    let bc = |e: BarcodeEdit| Some(UiIntent::Barcode { document: doc, edit: e });
    let index_of = |s: Symbology| Symbology::ALL.iter().position(|x| *x == s).unwrap();
    assert_eq!(
        send(Slot::BcType, SlotValue::Choice(index_of(Symbology::Code128))),
        bc(BarcodeEdit::Symbology(Symbology::Code128))
    );
    assert_eq!(send(Slot::BcType, SlotValue::Choice(index_of(Symbology::GridMatrix))), None, "greyed out");
    assert_eq!(send(Slot::BcType, SlotValue::Choice(b.symbology)), None, "QR is the chosen one already");
    assert_eq!(send(Slot::BcPayload, text("HELLO")), bc(BarcodeEdit::Payload("HELLO".into())));
    assert_eq!(send(Slot::BcModule, text("0,4")), bc(BarcodeEdit::Module("0,4".into())));
    assert_eq!(send(Slot::BcMinModule, text("0,25")), bc(BarcodeEdit::MinModule("0,25".into())));
    assert_eq!(send(Slot::BcQuiet, text("12")), bc(BarcodeEdit::Quiet("12".into())));
    assert_eq!(send(Slot::BcHeight, text("8")), None, "a QR code has no bar height");
    assert_eq!(send(Slot::BcColumns, text("3")), None, "and no columns");
    assert_eq!(send(Slot::BcWidth, text("30")), bc(BarcodeEdit::Width("30".into())));
    assert_eq!(send(Slot::BcEcc, SlotValue::Choice(2)), bc(BarcodeEdit::Ecc(2)));
    assert_eq!(send(Slot::BcCreate, SlotValue::Press), bc(BarcodeEdit::Create));
    assert_eq!(send(Slot::BcCancel, SlotValue::Press), bc(BarcodeEdit::Cancel));
    assert_eq!(send(Slot::BcModule, text(&b.module)), None, "the value it shows");
}

#[test]
fn bar_height_and_columns_belong_to_their_types() {
    let (mut h, doc) = open_barcode();
    h.send(UiIntent::Barcode { document: doc, edit: BarcodeEdit::Symbology(Symbology::Code128) });
    let bc = |e: BarcodeEdit| Some(UiIntent::Barcode { document: doc, edit: e });
    assert_eq!(intent(&h, doc, Slot::BcHeight, text("8")), bc(BarcodeEdit::Height("8".into())));
    assert!(panels(&h, doc, Locale::Nl).barcode.unwrap().ecc.is_none(), "no error-correction slider for Code 128");
    h.send(UiIntent::Barcode { document: doc, edit: BarcodeEdit::Symbology(Symbology::Pdf417) });
    assert_eq!(intent(&h, doc, Slot::BcColumns, text("3")), bc(BarcodeEdit::Columns("3".into())));
    h.send(UiIntent::Barcode { document: doc, edit: BarcodeEdit::Symbology(Symbology::DataMatrix) });
    let b = panels(&h, doc, Locale::En).barcode.unwrap();
    if let Some((titles, current)) = b.input_modes {
        let other = (current + 1) % titles.len();
        let mode = [SymbolInputMode::Data, SymbolInputMode::Unicode, SymbolInputMode::Gs1]
            .into_iter()
            .find(|m| format!("{m:?}") == titles[other].replace("GS1", "Gs1"))
            .unwrap();
        assert_eq!(intent(&h, doc, Slot::BcInputMode, SlotValue::Choice(other)), bc(BarcodeEdit::InputMode(mode)));
    }
}

#[test]
fn barcode_errors_and_the_readability_warning_say_what_and_where_in_the_window_language() {
    let (mut h, doc) = open_barcode();
    let send = |h: &mut HeadlessEditorHarness, e: BarcodeEdit| {
        h.send(UiIntent::Barcode { document: doc, edit: e });
    };
    send(&mut h, BarcodeEdit::Symbology(Symbology::Ean));
    send(&mut h, BarcodeEdit::Payload("40063A133393".into()));
    let b = panels(&h, doc, Locale::En).barcode.unwrap();
    assert!(b.problems.contains('A') && b.problems.contains('6'), "character and position (from 1): {:?}", b.problems);
    send(&mut h, BarcodeEdit::Payload("4006381333932".into()));
    let b = panels(&h, doc, Locale::Nl).barcode.unwrap();
    assert!(
        b.problems.contains('1') && b.problems.contains('2') && b.problems.to_lowercase().contains("controlecijfer"),
        "{:?}",
        b.problems
    );
    send(&mut h, BarcodeEdit::Payload("400638133393".into()));
    let b = panels(&h, doc, Locale::Nl).barcode.unwrap();
    assert!(b.canonical_line.contains("4006381333931"), "{:?}", b.canonical_line);
    assert!(b.size_line.contains('×') || b.size_line.contains('x'), "{:?}", b.size_line);
    assert!(b.create_enabled && b.readability.is_empty());

    send(&mut h, BarcodeEdit::MinModule("0,3".into()));
    send(&mut h, BarcodeEdit::Module("0,1".into()));
    let b = panels(&h, doc, Locale::Nl).barcode.unwrap();
    assert!(b.readability.contains("0.1") && b.readability.contains("0.3"), "both numbers: {:?}", b.readability);
    assert!(!b.create_enabled);
    let en = panels(&h, doc, Locale::En).barcode.unwrap();
    assert!(en.readability.to_lowercase().contains("readable"), "{:?}", en.readability);
}
