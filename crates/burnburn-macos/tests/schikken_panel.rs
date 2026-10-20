//! The Schikken panel, headless (W3-UI-schikken): what it shows comes from presentation's view
//! model in the window language, and a control becomes one intent with the text unparsed (DR-05).
//! `tests/native/macos/schikken_panel.rs` drives the real widgets.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{MenuItem, NativeEvent, Reaction, menu_enabled, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{
    ARRANGE_TOOLBAR, SchikkenControl as C, SchikkenLine, SchikkenText, schikken_panel, schikken_text,
};
use burnburn_presentation::{
    CenterFinderIntent, CommandKey, HeadlessEditorHarness, NestRotations, PlaceOptionsInput, PrintAndCutIntent,
    SchikkenIntent, SlotChoice, SlotsInput, UiIntent,
};
use burnburn_types::DocumentId;

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

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<SchikkenLine> {
    schikken_panel(&h.view(), doc, locale).expect("a panel").lines
}

fn control(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> Option<SchikkenLine> {
    lines(h, doc, Locale::En).into_iter().find(|l| l.control() == Some(c))
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, values: Vec<(C, String)>) -> Option<UiIntent> {
    match translate(NativeEvent::SchikkenControl { document: doc, control: c, values }, &h.view()) {
        Reaction::Intent(i) => {
            h.send(i.clone());
            Some(i)
        }
        _ => None,
    }
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<String> {
    lines(h, doc, Locale::En)
        .into_iter()
        .filter_map(|l| match l {
            SchikkenLine::Text(t) | SchikkenLine::Problem(t) | SchikkenLine::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn without_a_flow_the_panel_offers_the_dock_options_and_a_change_sends_the_options_with_that_change() {
    let (mut h, doc) = window();
    let panel = schikken_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(!panel.flow, "the options panel opens only when asked for");
    assert_eq!(panel.title, "Schikken");
    assert!(matches!(&panel.lines[0], SchikkenLine::Heading(t) if t == "Docken en samenschuiven"));
    let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, C::PlaceGap) else { panic!("gap field") };
    assert_eq!(text, "0");
    let sent = press(&mut h, doc, C::PlaceGap, vec![(C::PlaceGap, "2,5".into())]).unwrap();
    assert_eq!(
        sent,
        UiIntent::Schikken(SchikkenIntent::SetPlaceOptions {
            document: doc,
            options: PlaceOptionsInput { gap: "2,5".into(), ..PlaceOptionsInput::default() }
        })
    );
    let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, C::PlaceGap) else { panic!("gap field") };
    assert_eq!(text, "2,5");
    // A choice keeps the typed gap.
    press(&mut h, doc, C::PlaceContact, vec![(C::PlaceContact, "1".into())]).unwrap();
    let options = h.document(doc).unwrap().schikken.place.input;
    assert_eq!((options.gap.as_str(), options.contact), ("2,5", burnburn_presentation::DockContactChoice::Outline));
    // A wrong gap is shown on its field.
    press(&mut h, doc, C::PlaceGap, vec![(C::PlaceGap, "abc".into())]);
    let Some(SchikkenLine::Field { invalid, .. }) = control(&h, doc, C::PlaceGap) else { panic!("gap field") };
    assert!(invalid);
    assert!(texts(&h, doc).iter().any(|t| t.contains("0 or more")), "{:?}", texts(&h, doc));
}

#[test]
fn the_slots_dialog_is_a_flow_and_ok_sends_every_field_as_typed() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::ResizeSlots);
    let panel = schikken_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.flow);
    let values = vec![
        (C::SlotsFrom, "3".to_owned()),
        (C::SlotsTo, "3,2".to_owned()),
        (C::SlotsTolerance, "0,01".to_owned()),
        (C::SlotsSelect, "2".to_owned()),
    ];
    let sent = press(&mut h, doc, C::SlotsOk, values).unwrap();
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
    assert_eq!(press(&mut h, doc, C::SlotsFrom, vec![(C::SlotsFrom, "4".into())]), None, "typing alone sends nothing");
    press(&mut h, doc, C::SlotsCancel, vec![]).unwrap();
    assert!(!schikken_panel(&h.view(), doc, Locale::En).unwrap().flow);
}

#[test]
fn the_nest_panel_shows_the_report_and_its_buttons_follow_the_run() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::QuickNest);
    let button = |h: &HeadlessEditorHarness, c: C| match control(h, doc, c) {
        Some(SchikkenLine::Button { enabled, .. }) => enabled,
        other => panic!("{c:?}: {other:?}"),
    };
    assert!(button(&h, C::NestStart) && !button(&h, C::NestStop) && !button(&h, C::NestApply));
    assert!(texts(&h, doc).iter().any(|t| t.contains("2 parts ready")), "{:?}", texts(&h, doc));
    press(&mut h, doc, C::NestRotations, vec![(C::NestRotations, "3".into())]).unwrap();
    assert_eq!(h.document(doc).unwrap().schikken.nest.unwrap().input.rotations, NestRotations::Eighth);
    press(&mut h, doc, C::NestStart, vec![]).unwrap();
    assert!(button(&h, C::NestStop) && !button(&h, C::NestStart), "running");
    h.run_tasks();
    assert!(button(&h, C::NestApply) && !button(&h, C::NestStop));
    assert!(texts(&h, doc).iter().any(|t| t.contains("2 placed")), "{:?}", texts(&h, doc));
    press(&mut h, doc, C::NestApply, vec![]).unwrap();
    assert!(texts(&h, doc).iter().any(|t| t.contains("Applied")));
    press(&mut h, doc, C::NestClose, vec![]).unwrap();
    assert!(!schikken_panel(&h.view(), doc, Locale::En).unwrap().flow);
}

#[test]
fn print_and_cut_sends_both_coordinates_of_the_target_that_was_edited() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::PrintAndCut);
    let sent = press(
        &mut h,
        doc,
        C::PcX(1),
        vec![(C::PcX(1), "130".into()), (C::PcY(1), "10".into()), (C::PcX(0), "ignored".into())],
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
    let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, C::PcX(1)) else { panic!("field") };
    assert_eq!(text, "130");
    let pick = press(&mut h, doc, C::PcPick(0), vec![]).unwrap();
    assert!(matches!(
        pick,
        UiIntent::Schikken(SchikkenIntent::PrintAndCut(PrintAndCutIntent::PickTarget { index: 0, .. }))
    ));
    let Some(SchikkenLine::Button { title, .. }) = control(&h, doc, C::PcPick(0)) else { panic!("button") };
    assert!(title.contains("Click"), "the button says a click is awaited: {title}");
    // Registering without measurements says what is missing.
    press(&mut h, doc, C::PcRegister, vec![]).unwrap();
    assert!(texts(&h, doc).iter().any(|t| t.contains("Choose both targets")), "{:?}", texts(&h, doc));
}

#[test]
fn the_center_finder_adds_the_typed_point_and_shows_the_fit() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::CenterFinder);
    for (x, y) in [("70", "40"), ("50", "60"), ("30", "40")] {
        let sent = press(&mut h, doc, C::CfAdd, vec![(C::CfX, x.into()), (C::CfY, y.into())]).unwrap();
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
    assert_eq!(press(&mut h, doc, C::CfX, vec![(C::CfX, "1".into())]), None);
}

#[test]
fn warp_shows_its_hint_and_the_apply_button_waits_for_a_change() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::ToolWarp);
    let panel = schikken_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(panel.flow);
    assert!(matches!(&panel.lines[0], SchikkenLine::Heading(t) if t == "Vervorm selectie (4 punten)"));
    assert!(matches!(control(&h, doc, C::ReshapeApply), Some(SchikkenLine::Button { enabled: false, .. })));
    assert!(matches!(control(&h, doc, C::WarpBilinear), Some(SchikkenLine::Check { on: false, .. })));
    press(&mut h, doc, C::WarpBilinear, vec![(C::WarpBilinear, "1".into())]).unwrap();
    assert!(matches!(control(&h, doc, C::WarpBilinear), Some(SchikkenLine::Check { on: true, .. })));
    h.drag(doc, (60.0, 10.0), (50.0, 25.0));
    assert!(matches!(control(&h, doc, C::ReshapeApply), Some(SchikkenLine::Button { enabled: true, .. })));
    press(&mut h, doc, C::ReshapeApply, vec![]).unwrap();
    assert!(!schikken_panel(&h.view(), doc, Locale::En).unwrap().flow, "the tool is done after applying");
    h.invoke(doc, CommandKey::ToolDeform);
    assert!(control(&h, doc, C::WarpBilinear).is_none(), "Deform has no warp kind");
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
fn a_control_without_its_section_sends_nothing() {
    let (mut h, doc) = window();
    for c in [C::SlotsOk, C::NestApply, C::PcRegister, C::CfAdd, C::ReshapeApply] {
        let sent =
            match translate(NativeEvent::SchikkenControl { document: doc, control: c, values: vec![] }, &h.view()) {
                Reaction::Intent(i) => Some(i),
                _ => None,
            };
        // Intents for a missing section are harmless to presentation (they find nothing to act on).
        if let Some(i) = sent {
            let revision = h.document(doc).unwrap().revision;
            h.send(i);
            assert_eq!(h.document(doc).unwrap().revision, revision, "{c:?}");
        }
    }
}

#[test]
fn the_texts_exist_in_both_languages_and_the_menu_commands_follow_the_view_model() {
    for t in SchikkenText::ALL {
        assert!(!schikken_text(*t, Locale::Nl).trim().is_empty(), "{t:?} nl");
        assert!(!schikken_text(*t, Locale::En).trim().is_empty(), "{t:?} en");
    }
    assert_ne!(schikken_text(SchikkenText::MenuDock, Locale::Nl), schikken_text(SchikkenText::MenuDock, Locale::En));
    let (h, _) = window();
    let vm = h.view();
    for key in [
        CommandKey::DockLeft,
        CommandKey::MoveTogetherHorizontal,
        CommandKey::QuickNest,
        CommandKey::NestSelected,
        CommandKey::ToolWarp,
        CommandKey::ResizeSlots,
        CommandKey::PrintAndCut,
        CommandKey::CenterFinder,
    ] {
        assert_eq!(menu_enabled(MenuItem::Command(key), &vm), vm.commands[&key].enabled, "{key:?}");
    }
    assert!(vm.commands[&CommandKey::DockLeft].enabled && vm.commands[&CommandKey::NestSelected].enabled);
}

#[test]
fn the_toolbar_control_carries_dock_and_move_together_after_align_and_distribute() {
    assert_eq!(
        &ARRANGE_TOOLBAR[8..],
        &[
            CommandKey::DockLeft,
            CommandKey::DockRight,
            CommandKey::DockUp,
            CommandKey::DockDown,
            CommandKey::MoveTogetherHorizontal,
            CommandKey::MoveTogetherVertical,
        ]
    );
}

#[test]
fn committing_what_a_print_and_cut_field_already_shows_changes_nothing() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::PrintAndCut);
    let set = |h: &mut HeadlessEditorHarness, c: C, x: &str, y: &str| {
        let values = vec![
            (C::PcX(0), x.to_owned()),
            (C::PcY(0), y.to_owned()),
            (c, if matches!(c, C::PcX(_)) { x } else { y }.to_owned()),
        ];
        press(h, doc, c, values)
    };
    assert!(set(&mut h, C::PcX(0), "10", "10").is_some());
    h.send(UiIntent::Schikken(SchikkenIntent::PrintAndCut(PrintAndCutIntent::SetMeasured {
        document: doc,
        index: 0,
        x: "5".into(),
        y: "6".into(),
    })));
    // Return and the end of editing are one commit: the second one carries the same text.
    assert_eq!(set(&mut h, C::PcY(0), "10", "10"), None);
    let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, C::PcMeasuredX(0)) else { panic!("field") };
    assert_eq!(text, "5", "the measurement survived the repeated commit");
}
