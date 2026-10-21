//! The Schikken panel of the Windows window, headless (W4-L8a): dock / move-together options,
//! Resize Slots, nest, Print and Cut, Center Finder, Warp / Deform and Tangent Circle as one form
//! that follows the flow in progress. Same lines, words and intents as the macOS panel
//! (`schikken_panel.rs` there): a control reports its value, presentation decides what changes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{
    CenterFinderIntent, CommandKey, HeadlessEditorHarness, NestRotations, PlaceOptionsInput, PrintAndCutIntent,
    SchikkenIntent, SlotChoice, SlotsInput, UiIntent,
};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState, Placement, schikken,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::SchikkenControl as C;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.drag(doc, (40.0, 0.0), (60.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    (h, doc)
}

fn shown() -> PanelState {
    PanelState { schikken_shown: true, ..PanelState::default() }
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
    let vm = h.view();
    schikken::form(&FormContext { vm: &vm, document: doc, locale }, &shown()).expect("a panel")
}

/// Use a control as the host does: read every control, report one, send what comes back.
fn press(
    h: &mut HeadlessEditorHarness,
    doc: DocumentId,
    control: C,
    value: ControlValue,
    overrides: &[(C, ControlValue)],
) -> Option<UiIntent> {
    let mut values = form(h, doc, Locale::En).values();
    for (c, v) in overrides {
        match values.iter_mut().find(|(id, _)| *id == ControlId::Schikken(*c)) {
            Some(slot) => slot.1 = v.clone(),
            None => values.push((ControlId::Schikken(*c), v.clone())),
        }
    }
    let vm = h.view();
    let outcome =
        schikken::react(&ControlId::Schikken(control), &value, &values, &FormContext { vm: &vm, document: doc, locale: Locale::En });
    match outcome {
        FormOutcome::Intent(i) => {
            h.send(i.clone());
            Some(i)
        }
        FormOutcome::Nothing => None,
        other => panic!("{other:?}"),
    }
}

fn line(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> Option<FormLine> {
    form(h, doc, Locale::En).find(&ControlId::Schikken(c)).cloned()
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<String> {
    form(h, doc, Locale::En)
        .lines
        .into_iter()
        .filter_map(|l| match l {
            FormLine::Text(t) | FormLine::Problem(t) | FormLine::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn button_enabled(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> bool {
    match line(h, doc, c) {
        Some(FormLine::Buttons(b)) => b.iter().find(|b| b.id == ControlId::Schikken(c)).unwrap().enabled,
        other => panic!("{c:?}: {other:?}"),
    }
}

fn flow(h: &HeadlessEditorHarness, doc: DocumentId) -> bool {
    // Without the menu toggle the panel exists only while a flow is in progress.
    let vm = h.view();
    schikken::form(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &PanelState::default()).is_some()
}

#[test]
fn the_panel_floats_and_opens_by_itself_only_for_a_flow_or_when_asked_for() {
    let (mut h, doc) = window();
    assert!(!flow(&h, doc), "the options panel opens only when asked for");
    let f = form(&h, doc, Locale::Nl);
    assert_eq!((f.kind, f.placement), (FormKind::Schikken, Placement::Floating));
    assert_eq!(f.title, "Schikken");
    assert!(matches!(&f.lines[0], FormLine::Heading(t) if t == "Docken en samenschuiven"));
    h.invoke(doc, CommandKey::ResizeSlots);
    assert!(flow(&h, doc), "a flow opens the panel by itself");
}

#[test]
fn a_dock_option_change_sends_the_options_with_that_change_and_a_wrong_gap_is_said() {
    let (mut h, doc) = window();
    let Some(FormLine::Field { text, .. }) = line(&h, doc, C::PlaceGap) else { panic!("gap field") };
    assert_eq!(text, "0");
    let sent = press(&mut h, doc, C::PlaceGap, ControlValue::Text("2,5".into()), &[(C::PlaceGap, ControlValue::Text("2,5".into()))]).unwrap();
    assert_eq!(
        sent,
        UiIntent::Schikken(SchikkenIntent::SetPlaceOptions {
            document: doc,
            options: PlaceOptionsInput { gap: "2,5".into(), ..PlaceOptionsInput::default() }
        })
    );
    let Some(FormLine::Field { text, .. }) = line(&h, doc, C::PlaceGap) else { panic!("gap field") };
    assert_eq!(text, "2,5");
    // A choice keeps the typed gap.
    press(&mut h, doc, C::PlaceContact, ControlValue::Choice(1), &[(C::PlaceContact, ControlValue::Choice(1))]).unwrap();
    let options = h.document(doc).unwrap().schikken.place.input;
    assert_eq!((options.gap.as_str(), options.contact), ("2,5", burnburn_presentation::DockContactChoice::Outline));
    press(&mut h, doc, C::PlaceGap, ControlValue::Text("abc".into()), &[(C::PlaceGap, ControlValue::Text("abc".into()))]);
    assert!(matches!(line(&h, doc, C::PlaceGap), Some(FormLine::Field { invalid: true, .. })));
    assert!(texts(&h, doc).iter().any(|t| t.contains("0 or more")), "{:?}", texts(&h, doc));
}

#[test]
fn the_slots_dialog_is_a_flow_and_ok_sends_every_field_as_typed() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::ResizeSlots);
    let overrides = [
        (C::SlotsFrom, ControlValue::Text("3".into())),
        (C::SlotsTo, ControlValue::Text("3,2".into())),
        (C::SlotsTolerance, ControlValue::Text("0,01".into())),
        (C::SlotsSelect, ControlValue::Choice(2)),
    ];
    let sent = press(&mut h, doc, C::SlotsOk, ControlValue::Press, &overrides).unwrap();
    assert_eq!(
        sent,
        UiIntent::Schikken(SchikkenIntent::ApplySlots {
            document: doc,
            input: SlotsInput {
                from: "3".into(),
                to: "3,2".into(),
                tolerance: "0,01".into(),
                select: SlotChoice::Both
            }
        })
    );
    // Rectangles have no slots of that width: the dialog stays and says so.
    assert!(texts(&h, doc).iter().any(|t| t.contains("Nothing would change")), "{:?}", texts(&h, doc));
    assert_eq!(
        press(&mut h, doc, C::SlotsFrom, ControlValue::Text("4".into()), &[(C::SlotsFrom, ControlValue::Text("4".into()))]),
        None,
        "typing alone sends nothing"
    );
    press(&mut h, doc, C::SlotsCancel, ControlValue::Press, &[]).unwrap();
    assert!(!flow(&h, doc));
}

#[test]
fn the_nest_panel_shows_the_report_and_its_buttons_follow_the_run() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::QuickNest);
    assert!(button_enabled(&h, doc, C::NestStart) && !button_enabled(&h, doc, C::NestStop) && !button_enabled(&h, doc, C::NestApply));
    assert!(texts(&h, doc).iter().any(|t| t.contains("2 parts ready")), "{:?}", texts(&h, doc));
    press(&mut h, doc, C::NestRotations, ControlValue::Choice(3), &[(C::NestRotations, ControlValue::Choice(3))]).unwrap();
    assert_eq!(h.document(doc).unwrap().schikken.nest.unwrap().input.rotations, NestRotations::Eighth);
    press(&mut h, doc, C::NestStart, ControlValue::Press, &[]).unwrap();
    assert!(button_enabled(&h, doc, C::NestStop) && !button_enabled(&h, doc, C::NestStart), "running");
    h.run_tasks();
    assert!(button_enabled(&h, doc, C::NestApply) && !button_enabled(&h, doc, C::NestStop));
    assert!(texts(&h, doc).iter().any(|t| t.contains("2 placed")), "{:?}", texts(&h, doc));
    press(&mut h, doc, C::NestApply, ControlValue::Press, &[]).unwrap();
    assert!(texts(&h, doc).iter().any(|t| t.contains("Applied")));
    press(&mut h, doc, C::NestClose, ControlValue::Press, &[]).unwrap();
    assert!(!flow(&h, doc));
}

#[test]
fn print_and_cut_sends_both_coordinates_of_the_target_that_was_edited() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::PrintAndCut);
    let sent = press(
        &mut h,
        doc,
        C::PcX(1),
        ControlValue::Text("130".into()),
        &[(C::PcX(1), ControlValue::Text("130".into())), (C::PcY(1), ControlValue::Text("10".into()))],
    )
    .unwrap();
    assert_eq!(
        sent,
        UiIntent::Schikken(SchikkenIntent::PrintAndCut(PrintAndCutIntent::SetTarget {
            document: doc,
            index: 1,
            x: "130".into(),
            y: "10".into()
        }))
    );
    let pick = press(&mut h, doc, C::PcPick(0), ControlValue::Press, &[]).unwrap();
    assert!(matches!(
        pick,
        UiIntent::Schikken(SchikkenIntent::PrintAndCut(PrintAndCutIntent::PickTarget { index: 0, .. }))
    ));
    let Some(FormLine::Buttons(b)) = line(&h, doc, C::PcPick(0)) else { panic!("button") };
    assert!(b[0].title.contains("Click"), "the button says a click is awaited: {}", b[0].title);
    press(&mut h, doc, C::PcRegister, ControlValue::Press, &[]).unwrap();
    assert!(texts(&h, doc).iter().any(|t| t.contains("Choose both targets")), "{:?}", texts(&h, doc));
    // Committing what the field already shows changes nothing (Return, then focus loss).
    let again = press(
        &mut h,
        doc,
        C::PcY(1),
        ControlValue::Text("10".into()),
        &[(C::PcX(1), ControlValue::Text("130".into())), (C::PcY(1), ControlValue::Text("10".into()))],
    );
    assert_eq!(again, None);
}

#[test]
fn the_center_finder_adds_the_typed_point_and_shows_the_fit() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::CenterFinder);
    for (x, y) in [("70", "40"), ("50", "60"), ("30", "40")] {
        let sent = press(
            &mut h,
            doc,
            C::CfAdd,
            ControlValue::Press,
            &[(C::CfX, ControlValue::Text(x.into())), (C::CfY, ControlValue::Text(y.into()))],
        )
        .unwrap();
        assert_eq!(
            sent,
            UiIntent::Schikken(SchikkenIntent::CenterFinder(CenterFinderIntent::AddPoint {
                document: doc,
                x: x.into(),
                y: y.into()
            }))
        );
    }
    let all = texts(&h, doc);
    assert!(all.iter().any(|t| t.contains("Centre (50, 40) mm, radius 20 mm")), "{all:?}");
    assert_eq!(press(&mut h, doc, C::CfX, ControlValue::Text("1".into()), &[]), None);
}

#[test]
fn warp_shows_its_hint_and_the_apply_button_waits_for_a_change() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::ToolWarp);
    assert!(flow(&h, doc));
    let nl = form(&h, doc, Locale::Nl);
    assert!(matches!(&nl.lines[0], FormLine::Heading(t) if t == "Vervorm selectie (4 punten)"));
    assert!(!button_enabled(&h, doc, C::ReshapeApply));
    assert!(matches!(line(&h, doc, C::WarpBilinear), Some(FormLine::Check { on: false, .. })));
    press(&mut h, doc, C::WarpBilinear, ControlValue::Toggle(true), &[(C::WarpBilinear, ControlValue::Toggle(true))]).unwrap();
    assert!(matches!(line(&h, doc, C::WarpBilinear), Some(FormLine::Check { on: true, .. })));
    h.drag(doc, (60.0, 10.0), (50.0, 25.0));
    assert!(button_enabled(&h, doc, C::ReshapeApply));
    press(&mut h, doc, C::ReshapeApply, ControlValue::Press, &[]).unwrap();
    assert!(!flow(&h, doc), "the tool is done after applying");
    h.invoke(doc, CommandKey::ToolDeform);
    assert!(line(&h, doc, C::WarpBilinear).is_none(), "Deform has no warp kind");
}

#[test]
fn tangent_circle_tells_what_to_pick_next() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolLine);
    h.drag(doc, (0.0, 0.0), (100.0, 0.0));
    h.drag(doc, (0.0, 0.0), (0.0, 100.0));
    h.send(UiIntent::SelectTool { document: Some(doc), id: "shape.tangent_circle".into() });
    h.click(doc, (50.0, 0.3), burnburn_editor::Modifiers::default());
    h.click(doc, (0.3, 50.0), burnburn_editor::Modifiers::default());
    let all = texts(&h, doc);
    assert!(all.iter().any(|t| t.contains("(2/3)")), "{all:?}");
    assert!(all.iter().any(|t| t.contains("Type the diameter")), "{all:?}");
}

#[test]
fn a_control_without_its_section_changes_nothing_in_the_document() {
    let (mut h, doc) = window();
    for c in [C::SlotsOk, C::NestApply, C::PcRegister, C::CfAdd, C::ReshapeApply] {
        let revision = h.document(doc).unwrap().revision;
        let _ = press(&mut h, doc, c, ControlValue::Press, &[]);
        assert_eq!(h.document(doc).unwrap().revision, revision, "{c:?}");
    }
}

#[test]
fn a_window_that_is_gone_has_no_panel() {
    let (h, _) = window();
    let vm = h.view();
    let gone = <DocumentId as burnburn_types::StableId>::from_raw(999);
    assert!(schikken::form(&FormContext { vm: &vm, document: gone, locale: Locale::En }, &shown()).is_none());
}
