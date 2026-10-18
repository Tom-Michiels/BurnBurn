//! The layer palette, layer table and Cut Settings sheet, headless (C01–C05): what the widgets
//! show comes from presentation's view model, and every click or OK becomes one intent with the
//! typed text unparsed (DR-05). No AppKit here; `tests/native/macos/layer_palette.rs` drives the
//! real widgets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_job::{MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_macos::adapter::{NativeEvent, Reaction, flags, translate};
use burnburn_macos::l10n::{Locale, message};
use burnburn_macos::panels::{
    LAYER_COLUMNS, LayerColumn, SHEET_MODES, cut_settings_panel, kerf_applies, layer_label, operations_panel,
    palette_buttons, palette_slot, sheet_mode,
};
use burnburn_presentation::{
    CommandKey, CutSettingsInput, Feature, HeadlessEditorHarness, LAYER_SLOTS, MessageKey, OperationField,
    OperationKind, SelectMode, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{DocumentId, StableId};

/// One 40 × 20 rectangle, selected.
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

fn intent(h: &HeadlessEditorHarness, event: NativeEvent) -> Option<UiIntent> {
    match translate(event, &h.view()) {
        Reaction::Intent(i) => Some(i),
        _ => None,
    }
}

/// Translate and send, as the host does.
fn act(h: &mut HeadlessEditorHarness, event: NativeEvent) -> Option<UiIntent> {
    let i = intent(h, event)?;
    h.send(i.clone());
    Some(i)
}

#[test]
fn palette_click_becomes_assign_layer_and_shift_adds() {
    let (h, doc) = rectangle();
    let click = |tag, flags| intent(&h, NativeEvent::PaletteClicked { document: doc, tag, flags });
    assert_eq!(click(3, 0), Some(UiIntent::AssignLayer { document: doc, slot: 3, add: false }));
    assert_eq!(click(3, flags::SHIFT), Some(UiIntent::AssignLayer { document: doc, slot: 3, add: true }));
    assert_eq!(click(isize::from(LAYER_SLOTS), 0), None, "no button beyond the palette");
    assert_eq!(click(-1, 0), None);
    assert_eq!((palette_slot(0), palette_slot(29), palette_slot(30)), (Some(0), Some(29), None));
    let other = <DocumentId as StableId>::from_raw(999);
    assert_eq!(intent(&h, NativeEvent::PaletteClicked { document: other, tag: 1, flags: 0 }), None);
}

#[test]
fn palette_buttons_name_each_colour_and_its_layer() {
    let (mut h, doc) = rectangle();
    act(&mut h, NativeEvent::PaletteClicked { document: doc, tag: 1, flags: 0 }).unwrap();
    let buttons = palette_buttons(&h.view(), doc, Locale::Nl);
    assert_eq!(buttons.len(), usize::from(LAYER_SLOTS));
    assert_eq!(buttons.iter().map(|b| b.slot).collect::<Vec<_>>(), (0..LAYER_SLOTS).collect::<Vec<_>>());
    assert_eq!((layer_label(1), layer_label(29)), ("C01".to_owned(), "C29".to_owned()));
    let used = &buttons[1];
    assert_eq!((used.label.as_str(), used.used), ("C01", true));
    assert!(used.help.contains("C01") && used.help.contains("1 object"), "help {:?}", used.help);
    let empty = &buttons[0];
    assert!(!empty.used);
    assert!(empty.help.starts_with("C00") && empty.help.contains("nog leeg"), "help {:?}", empty.help);
    let en = palette_buttons(&h.view(), doc, Locale::En);
    assert!(en[0].help.contains("empty"));
}

#[test]
fn layer_rows_show_colour_mode_and_both_toggles_and_toggles_become_separate_intents() {
    let (mut h, doc) = rectangle();
    act(&mut h, NativeEvent::PaletteClicked { document: doc, tag: 2, flags: 0 }).unwrap();
    let panel = operations_panel(&h.view(), doc, Locale::Nl);
    let row = &panel.rows[0];
    assert_eq!((row.slot, row.layer.as_str(), row.kind.as_str(), row.passes.as_str()), (2, "C02", "Lijn", "1"));
    assert!(row.output && row.visible);
    let ids: Vec<_> = LAYER_COLUMNS.iter().map(|c| c.id()).collect();
    assert_eq!(ids, ["layer", "name", "kind", "speed", "power", "passes", "targets", "output", "visible"]);
    for c in LAYER_COLUMNS {
        assert_eq!(LayerColumn::from_id(c.id()), Some(c));
        assert!(!c.caption(Locale::Nl).is_empty() && !c.caption(Locale::En).is_empty(), "{c:?} caption");
    }
    assert_eq!(
        LAYER_COLUMNS.map(|c| c.field()),
        [
            None,
            Some(OperationField::Name),
            None,
            Some(OperationField::Speed),
            Some(OperationField::PowerMax),
            Some(OperationField::Passes),
            None,
            None,
            None
        ]
    );
    // The power column is the maximum: editing it keeps a minimum set in Cut Settings.
    assert!(LayerColumn::Output.is_toggle() && LayerColumn::Visible.is_toggle() && !LayerColumn::Name.is_toggle());
    assert_eq!(
        (row.text(LayerColumn::Layer), row.text(LayerColumn::Speed), row.text(LayerColumn::Targets)),
        ("C02".to_owned(), "100".to_owned(), "1 object".to_owned())
    );

    let op = h.document(doc).unwrap().operations[0].id;
    let toggle = |h: &HeadlessEditorHarness, column, on| {
        intent(h, NativeEvent::LayerToggled { document: doc, row: 0, column, on })
    };
    assert_eq!(
        toggle(&h, LayerColumn::Visible, false),
        Some(UiIntent::SetOperationVisible { document: doc, operation: op, visible: false })
    );
    assert_eq!(
        toggle(&h, LayerColumn::Output, false),
        Some(UiIntent::SetOperationOutput { document: doc, operation: op, enabled: false })
    );
    assert_eq!(toggle(&h, LayerColumn::Output, true), None, "already on: nothing");
    assert_eq!(toggle(&h, LayerColumn::Name, false), None, "not a checkbox column");
    act(&mut h, NativeEvent::LayerToggled { document: doc, row: 0, column: LayerColumn::Visible, on: false }).unwrap();
    let row = &operations_panel(&h.view(), doc, Locale::Nl).rows[0];
    assert_eq!((row.output, row.visible), (true, false));
    assert_eq!(
        intent(&h, NativeEvent::LayerToggled { document: doc, row: 5, column: LayerColumn::Output, on: false }),
        None
    );
}

#[test]
fn double_click_opens_cut_settings_and_ok_sends_the_typed_text() {
    let (mut h, doc) = rectangle();
    act(&mut h, NativeEvent::PaletteClicked { document: doc, tag: 1, flags: 0 }).unwrap();
    let op = h.document(doc).unwrap().operations[0].id;
    assert_eq!(intent(&h, NativeEvent::LayerOpened { document: doc, row: -1 }), None);
    assert_eq!(
        act(&mut h, NativeEvent::LayerOpened { document: doc, row: 0 }),
        Some(UiIntent::OpenCutSettings { document: doc, operation: op })
    );
    let sheet = cut_settings_panel(&h.view(), doc, Locale::Nl).expect("sheet");
    assert_eq!((sheet.operation, sheet.slot), (op, 1));
    assert!(sheet.title.contains("Snij-instellingen") && sheet.title.contains("C01"), "{}", sheet.title);
    let modes: Vec<_> = sheet.modes.iter().map(|m| (m.title.as_str(), m.enabled)).collect();
    assert_eq!(modes, [("Lijn", true), ("Vulling", true), ("Offsetvulling", true)]);
    assert_eq!(
        (sheet.mode_index, sheet.speed.as_str(), sheet.power_min.as_str(), sheet.power_max.as_str()),
        (Some(0), "100", "20", "20")
    );
    assert_eq!((sheet.passes.as_str(), sheet.kerf.as_str(), sheet.kerf_enabled), ("1", "0", true));
    assert!(sheet.output && sheet.visible && sheet.error.is_empty());
    assert_eq!(
        SHEET_MODES.iter().enumerate().map(|(i, _)| sheet_mode(i as isize)).collect::<Vec<_>>(),
        [Some(OperationKind::Line), Some(OperationKind::Fill), Some(OperationKind::OffsetFill)]
    );

    let ok = |mode_index, speed: &str, kerf: &str| NativeEvent::CutSettingsConfirmed {
        document: doc,
        mode_index,
        speed: speed.into(),
        power_min: "10".into(),
        power_max: "35,5 %".into(),
        passes: "2".into(),
        kerf: kerf.into(),
        output: false,
        visible: true,
    };
    assert_eq!(
        intent(&h, ok(0, "250 mm/s", "0,15 mm")),
        Some(UiIntent::ApplyCutSettings {
            document: doc,
            operation: op,
            settings: CutSettingsInput {
                mode: OperationKind::Line,
                speed: "250 mm/s".into(),
                power_min: "10".into(),
                power_max: "35,5 %".into(),
                passes: "2".into(),
                kerf: "0,15 mm".into(),
                output_enabled: false,
                visible: true,
            },
        }),
        "raw text, unparsed"
    );
    act(&mut h, ok(0, "snel", "")).unwrap();
    let sheet = cut_settings_panel(&h.view(), doc, Locale::Nl).expect("still open after an error");
    assert!(sheet.error.contains("snelheid"), "error {:?}", sheet.error);
    act(&mut h, ok(1, "250", "")).unwrap();
    assert_eq!(cut_settings_panel(&h.view(), doc, Locale::Nl), None, "valid OK closes it");
    let row = &operations_panel(&h.view(), doc, Locale::Nl).rows[0];
    assert_eq!((row.kind.as_str(), row.speed.as_str(), row.output), ("Vulling", "250", false));
    assert_eq!(intent(&h, ok(0, "1", "")), None, "no sheet open: nothing");
    act(&mut h, NativeEvent::LayerOpened { document: doc, row: 0 }).unwrap();
    assert!(!cut_settings_panel(&h.view(), doc, Locale::Nl).unwrap().kerf_enabled, "kerf is for Line only");
    // Choosing a mode in the open sheet enables the kerf field for Line only (before OK).
    assert_eq!([0, 1, 2, -1].map(kerf_applies), [true, false, false, false]);
    assert_eq!(
        act(&mut h, NativeEvent::CutSettingsCancelled { document: doc }),
        Some(UiIntent::CancelCutSettings { document: doc })
    );
    assert_eq!(cut_settings_panel(&h.view(), doc, Locale::Nl), None);
}

#[test]
fn blocked_mode_is_listed_disabled_with_the_missing_capability() {
    let (mut h, doc) = rectangle();
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
    .with_capabilities(MachineCapabilities::conservative().with(Feature::OffsetFill, Support::Unsupported))
    .unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    act(&mut h, NativeEvent::PaletteClicked { document: doc, tag: 0, flags: 0 }).unwrap();
    act(&mut h, NativeEvent::LayerOpened { document: doc, row: 0 }).unwrap();
    let sheet = cut_settings_panel(&h.view(), doc, Locale::Nl).unwrap();
    let offset = &sheet.modes[2];
    assert!(!offset.enabled, "a blocked mode cannot be picked");
    assert!(
        offset.title.starts_with("Offsetvulling") && offset.title.to_lowercase().contains("ontbrekende capability"),
        "title {:?}",
        offset.title
    );
    // If it still arrives (keyboard, scripting), presentation refuses it and the sheet says why.
    act(
        &mut h,
        NativeEvent::CutSettingsConfirmed {
            document: doc,
            mode_index: 2,
            speed: "100".into(),
            power_min: "20".into(),
            power_max: "20".into(),
            passes: "1".into(),
            kerf: String::new(),
            output: true,
            visible: true,
        },
    )
    .unwrap();
    let sheet = cut_settings_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(sheet.error.to_lowercase().contains("ontbrekende capability"), "error {:?}", sheet.error);
    assert_eq!(h.document(doc).unwrap().operations[0].kind, OperationKind::Line);
}

#[test]
fn layer_message_keys_have_nl_and_en() {
    let mut keys = vec![MessageKey::SelectObjectsFirst];
    keys.extend(
        [OperationField::PowerMin, OperationField::PowerMax, OperationField::Passes, OperationField::Kerf]
            .map(MessageKey::InvalidNumber),
    );
    keys.extend(Feature::ALL.map(MessageKey::MissingCapability));
    for key in keys {
        for locale in [Locale::Nl, Locale::En] {
            assert!(message(key, locale).is_some_and(|t| !t.trim().is_empty()), "{key:?} {locale:?}");
        }
    }
    let text = message(MessageKey::MissingCapability(Feature::OffsetFill), Locale::En).unwrap();
    assert!(text.to_lowercase().contains("missing capability") && text.to_lowercase().contains("offset fill"));
}
