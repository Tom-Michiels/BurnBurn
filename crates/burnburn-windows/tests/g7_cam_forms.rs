//! G7-cam on Windows, headless: the Cut Settings sheet has the tab power row (greyed with its
//! reason without proof for a power per movement), and the nest form has the copies field, a row
//! per part, the two checks and sends what the Mac sends. Same model, same words.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_presentation::{
    CommandKey, CutField, HeadlessEditorHarness, SchikkenIntent, SelectMode, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormLine, FormOutcome, PanelState, cut_settings, schikken,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::SchikkenControl as C;

fn rectangle(tab_power: Support) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
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
    .with_capabilities(MachineCapabilities::conservative().with(Feature::TabCutPower, tab_power))
    .unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    (h, doc)
}

fn cut_form(h: &HeadlessEditorHarness, doc: DocumentId, live: &[(ControlId, ControlValue)]) -> Form {
    let vm = h.view();
    cut_settings::form(&FormContext { vm: &vm, document: doc, locale: Locale::En }, live).expect("a sheet")
}

fn open_sheet(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
}

#[test]
fn the_cut_sheet_has_the_tab_power_row_greyed_without_proof_and_editable_with_it() {
    let id = ControlId::CutOption(CutField::TabsPower);
    let tabs_on = [(ControlId::CutOption(CutField::TabsEnabled), ControlValue::Toggle(true))];

    let (mut h, doc) = rectangle(Support::Unverified { reason: "no RD form".into() });
    open_sheet(&mut h, doc);
    match cut_form(&h, doc, &tabs_on).find(&id) {
        Some(FormLine::Field { enabled, hint, text, .. }) => {
            assert!(!enabled, "greyed: nothing proves a power per movement");
            assert!(hint.to_lowercase().contains("missing capability"), "{hint}");
            assert_eq!(text, "0");
        }
        other => panic!("{other:?}"),
    }

    let (mut h, doc) = rectangle(Support::Supported);
    open_sheet(&mut h, doc);
    assert!(matches!(cut_form(&h, doc, &[]).find(&id), Some(FormLine::Field { enabled: false, .. })), "greyed until tabs are on");
    assert!(matches!(cut_form(&h, doc, &tabs_on).find(&id), Some(FormLine::Field { enabled: true, .. })));
}

fn nest_form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
    let vm = h.view();
    schikken::form(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &PanelState::default())
        .expect("the nest form")
}

#[test]
fn the_nest_form_has_copies_a_row_per_part_and_the_two_checks_and_sends_the_mac_intents() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.drag(doc, (40.0, 0.0), (60.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::QuickNest);

    let f = nest_form(&h, doc);
    for c in [C::NestCopies, C::NestPartCopies(0), C::NestPartCopies(1)] {
        assert!(matches!(f.find(&ControlId::Schikken(c)), Some(FormLine::Field { .. })), "{c:?}");
    }
    assert!(f.find(&ControlId::Schikken(C::NestPartCopies(2))).is_none());
    for (c, want) in [(C::NestHoles, true), (C::NestTight, false)] {
        assert!(matches!(f.find(&ControlId::Schikken(c)), Some(FormLine::Check { on, .. }) if *on == want), "{c:?}");
    }

    let react = |h: &HeadlessEditorHarness, c: C, value: ControlValue| {
        let vm = h.view();
        let mut values = nest_form(h, doc).values();
        match values.iter_mut().find(|(id, _)| *id == ControlId::Schikken(c)) {
            Some(slot) => slot.1 = value.clone(),
            None => values.push((ControlId::Schikken(c), value.clone())),
        }
        schikken::react(&ControlId::Schikken(c), &value, &values, &FormContext { vm: &vm, document: doc, locale: Locale::En })
    };
    let FormOutcome::Intent(UiIntent::Schikken(SchikkenIntent::SetNestOptions { input, .. })) =
        react(&h, C::NestCopies, ControlValue::Text("3".into()))
    else {
        panic!("copies send the options")
    };
    assert_eq!(input.copies, "3");
    let FormOutcome::Intent(UiIntent::Schikken(SchikkenIntent::SetNestOptions { input, .. })) =
        react(&h, C::NestTight, ControlValue::Toggle(true))
    else {
        panic!("tight sends the options")
    };
    assert!(input.tight);
}
