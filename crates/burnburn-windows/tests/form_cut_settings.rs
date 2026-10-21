//! The Cut Settings sheet of the Windows window, headless (W4-L8a): mode, speed, power, passes,
//! kerf, the two switches and the option rows (tabs, leads, overcut, perforation, dot, fill,
//! cross-hatch, scan angle, image, Z offset). Same rows, words and intents as the macOS sheet
//! (`layer_panels.rs`, `cut_settings_panel.rs` there); the sheet's draft is the controls' values.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::{
    DitherMode, ImageRender, ImageSettings, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange,
    ProcessSettings, Speed,
};
use burnburn_job::{MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_presentation::{
    CommandKey, CutExtras, CutField, CutSettingsInput, CutValue, Feature, HeadlessEditorHarness, OperationKind,
    SelectMode, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{ContentHash, DocumentId, HashDomain, Length};
use burnburn_windows::forms::{
    ChoiceItem, ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, Placement, cut_settings,
};
use burnburn_windows::l10n::Locale;

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

/// A layer on colour `slot` with its Cut Settings sheet open.
fn open_sheet(h: &mut HeadlessEditorHarness, doc: DocumentId, slot: u8) {
    h.send(UiIntent::AssignLayer { document: doc, slot, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
}

fn form(
    h: &HeadlessEditorHarness,
    doc: DocumentId,
    locale: Locale,
    live: &[(ControlId, ControlValue)],
) -> Option<Form> {
    let vm = h.view();
    cut_settings::form(&FormContext { vm: &vm, document: doc, locale }, live)
}

fn react(
    h: &HeadlessEditorHarness,
    doc: DocumentId,
    id: ControlId,
    value: ControlValue,
    values: &[(ControlId, ControlValue)],
) -> FormOutcome {
    let vm = h.view();
    cut_settings::react(&id, &value, values, &FormContext { vm: &vm, document: doc, locale: Locale::En })
}

fn set(values: &mut Vec<(ControlId, ControlValue)>, id: ControlId, value: ControlValue) {
    match values.iter_mut().find(|(i, _)| *i == id) {
        Some(slot) => slot.1 = value,
        None => values.push((id, value)),
    }
}

fn option_fields(form: &Form) -> Vec<CutField> {
    form.ids().into_iter().filter_map(|i| if let ControlId::CutOption(f) = i { Some(f) } else { None }).collect()
}

fn enabled(form: &Form, id: &ControlId) -> bool {
    match form.find(id) {
        Some(FormLine::Field { enabled, .. } | FormLine::Check { enabled, .. } | FormLine::Choice { enabled, .. }) => {
            *enabled
        }
        other => panic!("{id:?}: {other:?}"),
    }
}

#[test]
fn there_is_no_sheet_until_a_layer_is_opened() {
    let (h, doc) = rectangle();
    assert!(form(&h, doc, Locale::En, &[]).is_none());
}

#[test]
fn a_line_sheet_shows_the_modes_the_values_and_the_line_option_rows() {
    let (mut h, doc) = rectangle();
    open_sheet(&mut h, doc, 1);
    let f = form(&h, doc, Locale::Nl, &[]).expect("a sheet");
    assert_eq!((f.kind, f.placement), (FormKind::CutSettings, Placement::Floating));
    assert!(f.title.contains("Snij-instellingen") && f.title.contains("C01"), "{}", f.title);
    match f.find(&ControlId::CutMode) {
        Some(FormLine::Choice { items, selected, .. }) => {
            let titles: Vec<_> = items.iter().map(|i| (i.title.as_str(), i.enabled)).collect();
            assert_eq!(titles, [("Lijn", true), ("Vulling", true), ("Offsetvulling", true)]);
            assert_eq!(*selected, 0);
        }
        other => panic!("{other:?}"),
    }
    let text = |id: ControlId| match f.find(&id) {
        Some(FormLine::Field { text, .. }) => text.clone(),
        other => panic!("{id:?}: {other:?}"),
    };
    assert_eq!(
        (
            text(ControlId::CutSpeed).as_str(),
            text(ControlId::CutPowerMin).as_str(),
            text(ControlId::CutPowerMax).as_str(),
            text(ControlId::CutPasses).as_str(),
            text(ControlId::CutKerf).as_str()
        ),
        ("100", "20", "20", "1", "0")
    );
    assert!(matches!(f.find(&ControlId::CutOutput), Some(FormLine::Check { on: true, .. })));
    assert!(matches!(f.find(&ControlId::CutVisible), Some(FormLine::Check { on: true, .. })));
    assert!(enabled(&f, &ControlId::CutKerf), "kerf is for Line");
    let wanted: Vec<CutField> = {
        use burnburn_presentation::CutSection;
        CutField::ALL.into_iter().filter(|c| c.section() == CutSection::Line).collect()
    };
    assert_eq!(option_fields(&f), wanted, "the Line rows, in sheet order");
    assert!(enabled(&f, &ControlId::CutOption(CutField::TabsEnabled)));
    assert!(!enabled(&f, &ControlId::CutOption(CutField::TabsValue)), "greyed until its switch is on");
    match f.find(&ControlId::CutOption(CutField::LeadInShape)) {
        Some(FormLine::Choice { items, .. }) => {
            assert_eq!(items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Lijn", "Boog"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(f.ids().contains(&ControlId::CutOk) && f.ids().contains(&ControlId::CutCancel));
}

#[test]
fn choosing_fill_swaps_the_option_rows_greys_kerf_and_asks_for_a_redraw() {
    let (mut h, doc) = rectangle();
    open_sheet(&mut h, doc, 1);
    let fresh = form(&h, doc, Locale::En, &[]).unwrap();
    let mut values = fresh.values();
    set(&mut values, ControlId::CutMode, ControlValue::Choice(1));
    assert_eq!(react(&h, doc, ControlId::CutMode, ControlValue::Choice(1), &values), FormOutcome::Redraw);
    let fill = form(&h, doc, Locale::En, &values).unwrap();
    let mut expected: Vec<CutField> = {
        use burnburn_presentation::CutSection;
        CutField::ALL.into_iter().filter(|c| c.section() == CutSection::Fill).collect()
    };
    expected.push(CutField::ZOffset);
    assert_eq!(option_fields(&fill), expected);
    assert!(!enabled(&fill, &ControlId::CutKerf), "kerf is for Line only");
    assert!(matches!(fill.find(&ControlId::CutMode), Some(FormLine::Choice { selected: 1, .. })));
    // Text the user typed survives the redraw.
    set(&mut values, ControlId::CutSpeed, ControlValue::Text("250".into()));
    let again = form(&h, doc, Locale::En, &values).unwrap();
    assert!(matches!(again.find(&ControlId::CutSpeed), Some(FormLine::Field { text, .. }) if text == "250"));
    // OffsetFill has no options.
    set(&mut values, ControlId::CutMode, ControlValue::Choice(2));
    assert!(option_fields(&form(&h, doc, Locale::En, &values).unwrap()).iter().all(|f| *f == CutField::ZOffset));
}

#[test]
fn a_switch_decides_the_greying_of_its_row_and_dot_mode_says_why() {
    let (mut h, doc) = rectangle();
    with_caps(&mut h, MachineCapabilities::conservative().with(Feature::DotMode, Support::Supported));
    open_sheet(&mut h, doc, 1);
    let mut values = form(&h, doc, Locale::En, &[]).unwrap().values();
    set(&mut values, ControlId::CutOption(CutField::TabsEnabled), ControlValue::Toggle(true));
    assert_eq!(
        react(&h, doc, ControlId::CutOption(CutField::TabsEnabled), ControlValue::Toggle(true), &values),
        FormOutcome::Redraw
    );
    assert!(enabled(&form(&h, doc, Locale::En, &values).unwrap(), &ControlId::CutOption(CutField::TabsValue)));
    set(&mut values, ControlId::CutOption(CutField::DotEnabled), ControlValue::Toggle(true));
    let dot = form(&h, doc, Locale::En, &values).unwrap();
    for f in [CutField::TabsEnabled, CutField::LeadInEnabled, CutField::Overcut] {
        assert!(!enabled(&dot, &ControlId::CutOption(f)), "{f:?} is off in dot mode");
    }
    // The note says why, in the line under the row.
    assert!(
        dot.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.to_lowercase().contains("dot"))),
        "a note about dot mode"
    );
    // A text field edit changes no greying: nothing to redraw.
    assert_eq!(
        react(&h, doc, ControlId::CutOption(CutField::Overcut), ControlValue::Text("1".into()), &values),
        FormOutcome::Nothing
    );
}

#[test]
fn what_the_profile_blocks_stays_listed_greyed_with_its_reason() {
    let (mut h, doc) = rectangle();
    let profile_caps = MachineCapabilities::conservative().with(Feature::OffsetFill, Support::Unsupported);
    with_caps(&mut h, profile_caps);
    open_sheet(&mut h, doc, 0);
    let f = form(&h, doc, Locale::Nl, &[]).unwrap();
    match f.find(&ControlId::CutMode) {
        Some(FormLine::Choice { items, .. }) => {
            let offset: &ChoiceItem = &items[2];
            assert!(!offset.enabled, "a blocked mode cannot be picked");
            assert!(
                offset.title.starts_with("Offsetvulling")
                    && offset.title.to_lowercase().contains("ontbrekende capability")
            );
        }
        other => panic!("{other:?}"),
    }
    // If it arrives anyway (keyboard), presentation refuses it and the sheet says why.
    let mut values = f.values();
    set(&mut values, ControlId::CutMode, ControlValue::Choice(2));
    let outcome = react(&h, doc, ControlId::CutOk, ControlValue::Press, &values);
    let FormOutcome::Intent(intent) = outcome else { panic!("OK sends an intent") };
    h.send(intent);
    let after = form(&h, doc, Locale::Nl, &[]).expect("still open after the refusal");
    assert!(
        after
            .lines
            .iter()
            .any(|l| matches!(l, FormLine::Problem(p) if p.to_lowercase().contains("ontbrekende capability"))),
        "the reason is on the sheet"
    );
    assert_eq!(h.document(doc).unwrap().operations[0].kind, OperationKind::Line);
}

#[test]
fn ok_sends_every_field_as_typed_in_one_intent_and_a_valid_ok_closes_the_sheet() {
    let (mut h, doc) = rectangle();
    open_sheet(&mut h, doc, 1);
    let op = h.document(doc).unwrap().operations[0].id;
    let mut values = form(&h, doc, Locale::En, &[]).unwrap().values();
    set(&mut values, ControlId::CutSpeed, ControlValue::Text("250 mm/s".into()));
    set(&mut values, ControlId::CutPowerMin, ControlValue::Text("10".into()));
    set(&mut values, ControlId::CutPowerMax, ControlValue::Text("35,5 %".into()));
    set(&mut values, ControlId::CutPasses, ControlValue::Text("2".into()));
    set(&mut values, ControlId::CutKerf, ControlValue::Text("0,15 mm".into()));
    set(&mut values, ControlId::CutOutput, ControlValue::Toggle(false));
    set(&mut values, ControlId::CutOption(CutField::TabsEnabled), ControlValue::Toggle(true));
    set(&mut values, ControlId::CutOption(CutField::TabsValue), ControlValue::Text("6".into()));
    let FormOutcome::Intent(UiIntent::ApplyCutSettings { document, operation, settings }) =
        react(&h, doc, ControlId::CutOk, ControlValue::Press, &values)
    else {
        panic!("OK sends ApplyCutSettings")
    };
    assert_eq!((document, operation), (doc, op));
    let CutSettingsInput { mode, speed, power_min, power_max, passes, kerf, output_enabled, visible, extras } =
        settings.clone();
    assert_eq!(mode, OperationKind::Line);
    assert_eq!(
        (speed.as_str(), power_min.as_str(), power_max.as_str(), passes.as_str(), kerf.as_str()),
        ("250 mm/s", "10", "35,5 %", "2", "0,15 mm")
    );
    assert!(!output_enabled && visible);
    assert_eq!(extras.get(CutField::TabsEnabled), Some(&CutValue::Flag(true)));
    assert_eq!(extras.get(CutField::TabsValue), Some(&CutValue::Text("6".into())));
    // Only the rows on the sheet travel: a Fill row is not among them.
    assert!(extras.get(CutField::FillInterval).is_none());
    h.send(UiIntent::ApplyCutSettings { document, operation, settings });
    assert!(form(&h, doc, Locale::En, &[]).is_none(), "a valid OK closes the sheet");
    let row = &h.document(doc).unwrap().operations[0];
    assert_eq!((row.speed.as_str(), row.passes.as_str(), row.output_enabled), ("250", "2", false));
}

#[test]
fn a_refused_ok_keeps_the_sheet_open_with_the_reason_and_cancel_closes_it() {
    let (mut h, doc) = rectangle();
    open_sheet(&mut h, doc, 1);
    let mut values = form(&h, doc, Locale::En, &[]).unwrap().values();
    set(&mut values, ControlId::CutSpeed, ControlValue::Text("snel".into()));
    let FormOutcome::Intent(i) = react(&h, doc, ControlId::CutOk, ControlValue::Press, &values) else { panic!() };
    h.send(i);
    let after = form(&h, doc, Locale::En, &values).expect("still open");
    assert!(after.lines.iter().any(|l| matches!(l, FormLine::Problem(p) if p.to_lowercase().contains("speed"))));
    // What was typed is still on the sheet.
    assert!(matches!(after.find(&ControlId::CutSpeed), Some(FormLine::Field { text, .. }) if text == "snel"));
    assert_eq!(
        react(&h, doc, ControlId::CutCancel, ControlValue::Press, &values),
        FormOutcome::Intent(UiIntent::CancelCutSettings { document: doc })
    );
    h.send(UiIntent::CancelCutSettings { document: doc });
    assert_eq!(react(&h, doc, ControlId::CutCancel, ControlValue::Press, &values), FormOutcome::Nothing);
    assert_eq!(react(&h, doc, ControlId::CutOk, ControlValue::Press, &values), FormOutcome::Nothing, "no sheet open");
}

#[test]
fn an_image_layer_has_the_image_rows_and_the_z_offset() {
    let (mut h, doc) = rectangle();
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
    let f = form(&h, doc, Locale::En, &[]).unwrap();
    let fields = option_fields(&f);
    for needed in [CutField::ImageMode, CutField::ImageInterval, CutField::ImageAngle, CutField::ZOffset] {
        assert!(fields.contains(&needed), "{needed:?} in {fields:?}");
    }
    assert!(!fields.contains(&CutField::TabsEnabled), "no Line rows on an image layer");
    // OK on an untouched sheet sends the stored image values back unchanged.
    let FormOutcome::Intent(UiIntent::ApplyCutSettings { settings, .. }) =
        react(&h, doc, ControlId::CutOk, ControlValue::Press, &f.values())
    else {
        panic!()
    };
    let stored: CutExtras = h.document(doc).unwrap().cut_settings.unwrap().extras;
    for field in [CutField::ImageMode, CutField::ImageInterval] {
        assert_eq!(settings.extras.get(field), stored.get(field), "{field:?}");
    }
}
