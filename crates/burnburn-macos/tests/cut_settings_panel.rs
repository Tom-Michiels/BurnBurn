//! The option rows of the Cut Settings sheet, headless (C03–C05, W2-L1-cut): what the rows show
//! comes from presentation's view model and the live values on the sheet, every OK sends the raw
//! option values as one intent, and what the machine profile blocks stays visible with its
//! reason. No AppKit here; `tests/native/macos/cut_settings_options.rs` drives the real widgets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::{
    DitherMode, ImageRender, ImageSettings, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange,
    ProcessSettings, Speed,
};
use burnburn_job::{MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::{Locale, message};
use burnburn_macos::panels::{OptionRow, RowControl, choice_label, cut_settings_panel, option_caption, option_rows};
use burnburn_presentation::{
    CommandKey, CutChoice, CutExtras, CutField, CutSection, CutSettingsViewModel, CutValue, Feature,
    HeadlessEditorHarness, MessageKey, OperationKind, SelectMode, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{ContentHash, DocumentId, HashDomain, Length};

fn rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    (h, doc)
}

fn open_line_sheet() -> (HeadlessEditorHarness, DocumentId, CutSettingsViewModel) {
    open_line_sheet_with(MachineCapabilities::conservative().with(Feature::DotMode, Support::Supported))
}

fn open_line_sheet_with(caps: MachineCapabilities) -> (HeadlessEditorHarness, DocumentId, CutSettingsViewModel) {
    let (mut h, doc) = rectangle();
    with_caps(&mut h, caps);
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let sheet = h.document(doc).unwrap().cut_settings.unwrap();
    (h, doc, sheet)
}

fn with_caps(h: &mut HeadlessEditorHarness, caps: MachineCapabilities) {
    let profile = MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(caps)
    .unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
}

fn row(rows: &[OptionRow], field: CutField) -> &OptionRow {
    rows.iter().find(|r| r.field == field).unwrap_or_else(|| panic!("no row for {field:?}"))
}

fn fields(rows: &[OptionRow]) -> Vec<CutField> {
    rows.iter().map(|r| r.field).collect()
}

#[test]
fn every_field_and_every_choice_has_a_caption_in_both_languages() {
    for locale in [Locale::Nl, Locale::En] {
        for field in CutField::ALL {
            assert!(!option_caption(field, locale).trim().is_empty(), "{field:?} {locale:?}");
            for choice in field.choices() {
                assert!(!choice_label(*choice, locale).trim().is_empty(), "{choice:?} {locale:?}");
            }
        }
    }
    assert_ne!(option_caption(CutField::TabsEnabled, Locale::Nl), option_caption(CutField::TabsEnabled, Locale::En));
    // The messages the sheet can show are in the catalogue as well.
    for key in [
        MessageKey::CutOptionsConflict,
        MessageKey::InvalidCutField(CutField::TabsValue),
        MessageKey::CutOptionNotForMode(CutField::ImageAngle),
    ] {
        for locale in [Locale::Nl, Locale::En] {
            assert!(message(key, locale).is_some_and(|t| !t.trim().is_empty()), "{key:?} {locale:?}");
        }
    }
    assert!(message(MessageKey::InvalidCutField(CutField::TabsValue), Locale::En).unwrap().contains("spacing"));
}

#[test]
fn a_line_sheet_has_the_line_rows_with_the_right_controls_and_greys_what_is_off() {
    let (h, doc, sheet) = open_line_sheet();
    let panel = cut_settings_panel(&h.view(), doc, Locale::Nl).unwrap();
    let line: Vec<CutField> = CutField::ALL.into_iter().filter(|f| f.section() == CutSection::Line).collect();
    assert_eq!(fields(&panel.options), line, "the Line rows, in sheet order");
    let tabs = row(&panel.options, CutField::TabsEnabled);
    assert_eq!((tabs.control.clone(), tabs.enabled), (RowControl::Check(false), true));
    assert!(tabs.caption.to_lowercase().contains("tab"), "{}", tabs.caption);
    let value = row(&panel.options, CutField::TabsValue);
    assert_eq!(value.control, RowControl::Text("4".into()));
    assert!(!value.enabled && value.note.is_empty(), "greyed until its switch is on, and that needs no explanation");
    match &row(&panel.options, CutField::LeadInShape).control {
        RowControl::Popup { items, selected } => {
            assert_eq!(items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Lijn", "Boog"]);
            assert_eq!(*selected, 0);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(row(&panel.options, CutField::Overcut).control, RowControl::Text("0".into()));
    assert!(row(&panel.options, CutField::Overcut).enabled);
    // The Fill mode of the pop-up shows the Fill rows instead.
    let fill = option_rows(&sheet, OperationKind::Fill, &sheet.extras, Locale::En);
    // The Z offset belongs to the operation, whatever its mode: it closes every sheet's rows.
    let mut expected: Vec<CutField> = CutField::ALL.into_iter().filter(|f| f.section() == CutSection::Fill).collect();
    expected.push(CutField::ZOffset);
    assert_eq!(fields(&fill), expected);
    assert_eq!(row(&fill, CutField::FillInterval).control, RowControl::Text("0.1".into()));
    assert_eq!(row(&fill, CutField::FillBidirectional).control, RowControl::Check(true));
    // OffsetFill has no options.
    assert!(option_rows(&sheet, OperationKind::OffsetFill, &sheet.extras, Locale::En).is_empty());
}

#[test]
fn live_values_decide_the_greying_and_dot_mode_says_why() {
    let (_, _, sheet) = open_line_sheet();
    let mut live = sheet.extras.clone();
    live.set_flag(CutField::TabsEnabled, true);
    let rows = option_rows(&sheet, OperationKind::Line, &live, Locale::Nl);
    assert!(row(&rows, CutField::TabsValue).enabled, "tabs on: their values can be edited");
    live.set_flag(CutField::DotEnabled, true);
    let rows = option_rows(&sheet, OperationKind::Line, &live, Locale::Nl);
    for f in [CutField::TabsEnabled, CutField::TabsValue, CutField::LeadInEnabled, CutField::Overcut] {
        let r = row(&rows, f);
        assert!(!r.enabled, "{f:?}");
        assert!(r.note.to_lowercase().contains("dot"), "{f:?}: {:?}", r.note);
    }
    assert!(row(&rows, CutField::DotSpacing).enabled && row(&rows, CutField::DotEnabled).enabled);
}

#[test]
fn what_the_profile_blocks_is_visible_greyed_with_its_reason() {
    let (mut h, doc) = rectangle();
    with_caps(&mut h, MachineCapabilities::conservative().with(Feature::Tabs, Support::Unsupported));
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let panel = cut_settings_panel(&h.view(), doc, Locale::Nl).unwrap();
    for f in [CutField::TabsEnabled, CutField::TabsMode, CutField::TabsValue, CutField::TabsWidth] {
        let r = row(&panel.options, f);
        assert!(!r.enabled, "{f:?}");
        assert!(r.note.to_lowercase().contains("ontbrekende capability"), "{f:?}: {:?}", r.note);
    }
    let sheet = h.document(doc).unwrap().cut_settings.unwrap();
    // Dot is Unverified in the conservative matrix: visible, greyed, reason.
    let dot = row(&panel.options, CutField::DotEnabled);
    assert!(!dot.enabled && !dot.note.is_empty());
    // Perforation is supported and stays editable.
    assert!(row(&panel.options, CutField::PerforationEnabled).enabled);
    // Fill: the ramp is greyed with its reason; the angle stays editable but carries the advice.
    let fill = option_rows(&sheet, OperationKind::Fill, &sheet.extras, Locale::En);
    let ramp = row(&fill, CutField::FillRamp);
    assert!(!ramp.enabled && ramp.note.contains("Missing capability"), "{:?}", ramp.note);
    let angle = row(&fill, CutField::FillAngle);
    assert!(angle.enabled && angle.note.contains("Missing capability"), "{:?}", angle.note);
    assert!(row(&fill, CutField::FillCrossHatch).enabled);
}

fn image_sheet(h: &mut HeadlessEditorHarness, doc: DocumentId) -> CutSettingsViewModel {
    let pitch = Length::from_mm(0.1).unwrap();
    let image = ImageSettings::new(
        ContentHash::of(HashDomain::SourcePixels, b"pixels"),
        pitch,
        pitch,
        ImageRender::Dither(DitherMode::Threshold { level: 128 }),
    )
    .unwrap();
    let p = PowerPercent::new(30.0).unwrap();
    let draft = OperationDraft {
        name: "Foto".into(),
        mode: OperationMode::Image(image),
        settings: ProcessSettings {
            speed: Speed::mm_per_s(200.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    h.send(UiIntent::AddOperation { document: doc, draft });
    let op = h.document(doc).unwrap().operations.iter().find(|o| o.name == "Foto").unwrap().id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    h.document(doc).unwrap().cut_settings.unwrap()
}

#[test]
fn an_image_sheet_lists_the_modes_and_shows_only_the_rows_of_the_chosen_one() {
    let (mut h, doc) = rectangle();
    let sheet = image_sheet(&mut h, doc);
    let panel = cut_settings_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.modes.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(), ["Image"]);
    assert_eq!(panel.mode_index, Some(0));
    let mode = row(&panel.options, CutField::ImageMode);
    let RowControl::Popup { items, selected } = &mode.control else { panic!("{:?}", mode.control) };
    assert_eq!(items.len(), 11);
    assert_eq!((items[*selected].title.as_str(), items[*selected].enabled), ("Threshold", true));
    let grayscale = items.iter().find(|i| i.title.starts_with("Grayscale")).unwrap();
    assert!(!grayscale.enabled && grayscale.title.contains("Missing capability"), "{:?}", grayscale.title);
    assert!(fields(&panel.options).contains(&CutField::ImageLevel));
    assert!(!fields(&panel.options).contains(&CutField::ScreenCell));
    // Choosing Halftone on the sheet (live) swaps the level for the screen rows.
    let live = sheet.extras.clone().with_choice(CutField::ImageMode, CutChoice::Halftone);
    let rows = option_rows(&sheet, OperationKind::Image, &live, Locale::En);
    assert!(!fields(&rows).contains(&CutField::ImageLevel));
    assert!(fields(&rows).contains(&CutField::ScreenCell) && fields(&rows).contains(&CutField::ScreenAngle));
    // Fill and Line rows are not part of an image sheet.
    assert!(!fields(&rows).contains(&CutField::TabsEnabled) && !fields(&rows).contains(&CutField::FillAngle));
}

#[test]
fn ok_sends_the_option_values_raw_and_one_command_reaches_the_document() {
    let (mut h, doc, sheet) = open_line_sheet();
    let confirm = |options| NativeEvent::CutSettingsConfirmed {
        document: doc,
        mode_index: 0,
        speed: "100".into(),
        power_min: "20".into(),
        power_max: "20".into(),
        passes: "1".into(),
        kerf: "0".into(),
        output: true,
        visible: true,
        options,
    };
    let options = vec![
        (CutField::TabsEnabled, CutValue::Flag(true)),
        (CutField::TabsMode, CutValue::Choice(1)),
        (CutField::TabsValue, CutValue::Text("25 mm".into())),
        (CutField::TabsWidth, CutValue::Text("1,5".into())),
    ];
    let Reaction::Intent(intent) = translate(confirm(options), &h.view()) else { panic!("no intent") };
    let UiIntent::ApplyCutSettings { settings, .. } = &intent else { panic!("{intent:?}") };
    assert_eq!(settings.extras.text(CutField::TabsValue), "25 mm", "unparsed");
    assert_eq!(settings.extras.choice(CutField::TabsMode), Some(CutChoice::TabsBySpacing));
    assert_eq!(settings.extras.get(CutField::Overcut), None, "fields the host did not send stay out");
    let rev = h.document(doc).unwrap().revision.raw();
    h.send(intent);
    assert_eq!(h.document(doc).unwrap().revision.raw(), rev + 1, "one command");
    let op = sheet.operation;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let reopened = h.document(doc).unwrap().cut_settings.unwrap();
    assert_eq!(reopened.extras.text(CutField::TabsValue), "25");
    assert_eq!(reopened.extras.choice(CutField::TabsMode), Some(CutChoice::TabsBySpacing));
    // The same sheet again sends nothing new.
    let again = confirm(vec![]);
    let Reaction::Intent(intent) = translate(again, &h.view()) else { panic!("no intent") };
    h.send(intent);
    assert_eq!(h.document(doc).unwrap().revision.raw(), rev + 1);
    let _ = (CutExtras::new(), MachineCapabilities::conservative());
}

#[test]
fn the_mode_popup_of_an_image_sheet_maps_its_only_item_to_image() {
    let (mut h, doc) = rectangle();
    let sheet = image_sheet(&mut h, doc);
    let confirm = NativeEvent::CutSettingsConfirmed {
        document: doc,
        mode_index: 0,
        speed: "200".into(),
        power_min: "30".into(),
        power_max: "30".into(),
        passes: "1".into(),
        kerf: "0".into(),
        output: true,
        visible: true,
        options: vec![(CutField::ImageNegative, CutValue::Flag(true))],
    };
    let Reaction::Intent(UiIntent::ApplyCutSettings { settings, operation, .. }) = translate(confirm, &h.view()) else {
        panic!("no intent")
    };
    assert_eq!((settings.mode, operation), (OperationKind::Image, sheet.operation));
    assert!(settings.extras.flag(CutField::ImageNegative));
}
