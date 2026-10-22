//! G5-windows: keyboard use of the panels (tab order, keys the controls keep), the reasons shown
//! for disabled controls and the unselectable items of a pop-up, headless.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_windows::forms::ChoiceItem;
use burnburn_windows::forms::choice::ChoiceList;
use burnburn_windows::host_panels::keys::{VK_RETURN, VK_TAB, canvas_wants_key, control_wants_key};
use burnburn_windows::host_panels::{
    ControlKind, ControlSpec, Family, Metrics, PanelSpec, Row, disabled_reason_at, layout,
};

fn spec() -> PanelSpec {
    PanelSpec {
        title: "t".into(),
        rows: vec![
            Row::one(ControlSpec::heading(1, "Kop")),
            Row::of(vec![ControlSpec::edit(2, "12", true), ControlSpec::button(3, "OK", true)]),
            Row::one(ControlSpec::label(4, "uitleg")),
            Row::one(ControlSpec::button(5, "Start", false).tip("Geen machine verbonden")),
            Row::one(ControlSpec::check(6, "Frame", false, true)),
            Row::one(ControlSpec::button(7, "Verborgen", true).hidden_if(true)),
            Row::one(ControlSpec::report(8, 3, "log")),
            Row::one(ControlSpec::combo(9, vec!["a".into()], Some(0), true)),
        ],
    }
}

#[test]
fn tab_visits_enabled_visible_input_controls_in_reading_order() {
    // Heading, label and report are skipped, the disabled button and the hidden one too.
    assert_eq!(spec().tab_order(), vec![2, 3, 6, 9]);
}

#[test]
fn only_controls_that_take_the_focus_are_tab_stops() {
    let stops = |k: ControlKind| k.is_tab_stop();
    assert!(stops(ControlKind::Button) && stops(ControlKind::HoldButton) && stops(ControlKind::Edit));
    assert!(stops(ControlKind::Check { checked: false }));
    assert!(stops(ControlKind::Combo { items: vec![], selected: None }));
    assert!(stops(ControlKind::List { items: vec![], selected: None, lines: 3 }));
    assert!(stops(ControlKind::Slider { min: 0, max: 1, pos: 0 }));
    assert!(!stops(ControlKind::Label) && !stops(ControlKind::Heading));
    assert!(!stops(ControlKind::Report { lines: 2 }));
    assert!(!stops(ControlKind::Bitmap { image: None, height_px: 10 }));
}

#[test]
fn an_edit_keeps_return_but_never_tab() {
    assert!(control_wants_key(Family::Files, &ControlKind::Edit, false, VK_RETURN));
    assert!(!control_wants_key(Family::Files, &ControlKind::Edit, false, VK_TAB));
    assert!(!control_wants_key(Family::Files, &ControlKind::Button, false, VK_RETURN), "Return presses a button");
}

#[test]
fn the_laser_window_keeps_the_arrow_keys_for_jogging_and_other_windows_do_not() {
    for vk in [0x25, 0x26, 0x27, 0x28] {
        assert!(control_wants_key(Family::Laser, &ControlKind::Button, false, vk), "{vk:#x}");
        assert!(!control_wants_key(Family::Image, &ControlKind::Button, false, vk), "{vk:#x}");
    }
    assert!(!control_wants_key(Family::Laser, &ControlKind::Button, false, VK_TAB));
}

#[test]
fn while_a_shortcut_is_recorded_every_key_including_tab_and_escape_is_the_answer() {
    for vk in [VK_TAB, VK_RETURN, 0x1B, 0x41] {
        assert!(control_wants_key(Family::App, &ControlKind::Button, true, vk), "{vk:#x}");
    }
}

#[test]
fn the_canvas_keeps_arrows_return_escape_and_delete_but_tab_moves_on() {
    for vk in [0x25, 0x26, 0x27, 0x28, VK_RETURN, 0x1B, 0x2E, 0x08, 0x41] {
        assert!(canvas_wants_key(vk), "{vk:#x}");
    }
    assert!(!canvas_wants_key(VK_TAB));
}

fn places(spec: &PanelSpec) -> Vec<burnburn_windows::host_panels::Placement> {
    layout(spec, &Metrics::base(400)).0
}

fn centre(p: &burnburn_windows::host_panels::Placement) -> (i32, i32) {
    (p.x + p.w / 2, p.y + p.h / 2)
}

#[test]
fn the_reason_of_a_disabled_control_is_found_under_the_pointer() {
    let s = spec();
    let pl = places(&s);
    let start = pl.iter().find(|p| p.id == 5).unwrap();
    assert_eq!(disabled_reason_at(&s, &pl, 0, centre(start)).as_deref(), Some("Geen machine verbonden"));
}

#[test]
fn enabled_controls_and_controls_without_a_reason_give_none() {
    let s = spec();
    let pl = places(&s);
    for id in [2, 3, 6, 9] {
        let p = pl.iter().find(|p| p.id == id).unwrap();
        assert_eq!(disabled_reason_at(&s, &pl, 0, centre(p)), None, "control {id}");
    }
    assert_eq!(disabled_reason_at(&s, &pl, 0, (-5, -5)), None, "outside every control");
}

#[test]
fn a_hidden_control_has_no_reason_and_the_scroll_offset_is_taken_into_account() {
    let mut s = spec();
    let pl = places(&s);
    let start = pl.iter().find(|p| p.id == 5).unwrap();
    // Scrolled down by 40 px, the control sits 40 px higher in the window.
    let (x, y) = centre(start);
    assert_eq!(disabled_reason_at(&s, &pl, 40, (x, y - 40)).as_deref(), Some("Geen machine verbonden"));
    assert_eq!(disabled_reason_at(&s, &pl, 40, (x, y)), None, "that spot now holds something else");
    // A hidden control never explains anything.
    if let Some(c) = s.rows[3].controls.first_mut() {
        c.visible = false;
    }
    assert_eq!(disabled_reason_at(&s, &pl, 0, centre(start)), None);
}

fn items() -> Vec<ChoiceItem> {
    vec![
        ChoiceItem::plain("Vector"),
        ChoiceItem { title: "Beeld \u{2014} geen beeldlaag".into(), enabled: false },
        ChoiceItem::plain("Uitsnijden"),
        ChoiceItem { title: "Vul \u{2014} niet voor dit profiel".into(), enabled: false },
    ]
}

#[test]
fn blocked_items_are_not_offered_and_positions_map_back_to_the_form_items() {
    let list = ChoiceList::new(&items(), 0);
    assert_eq!(list.titles, ["Vector", "Uitsnijden"]);
    assert_eq!(list.item_at(0), Some(0));
    assert_eq!(list.item_at(1), Some(2), "the second shown item is the form's third");
    assert_eq!(list.item_at(2), None);
    assert_eq!(list.selected, Some(0));
}

#[test]
fn a_currently_selected_blocked_item_stays_listed_so_the_control_can_show_it() {
    let list = ChoiceList::new(&items(), 1);
    assert_eq!(list.titles, ["Vector", "Beeld \u{2014} geen beeldlaag", "Uitsnijden"]);
    assert_eq!(list.selected, Some(1));
    assert_eq!(list.item_at(1), Some(1));
}

#[test]
fn the_tooltip_names_what_is_blocked_and_is_empty_when_nothing_is() {
    let list = ChoiceList::new(&items(), 0);
    assert!(list.blocked_note.contains("geen beeldlaag") && list.blocked_note.contains("niet voor dit profiel"));
    assert!(!list.blocked_note.contains("Vector"));
    let plain = [ChoiceItem::plain("a"), ChoiceItem::plain("b")];
    assert_eq!(ChoiceList::new(&plain, 1).blocked_note, "");
    assert_eq!(ChoiceList::new(&plain, 1).selected, Some(1));
}

#[test]
fn a_selection_outside_the_items_selects_nothing() {
    assert_eq!(ChoiceList::new(&items(), 9).selected, None);
    assert_eq!(ChoiceList::new(&[], 0).titles.len(), 0);
}

// ---- disabled fields of a form and disabled strip buttons ------------------------------------

use burnburn_windows::forms::layout::{disabled_tip_at as form_tip_at, layout as form_layout};
use burnburn_windows::forms::toolstrip::{StripGroup, StripPanel, disabled_tip_at as strip_tip_at, strip_layout};
use burnburn_windows::forms::{ControlId, Form, FormKind, FormLine, Placement};

fn form() -> Form {
    Form {
        kind: FormKind::Inspector,
        title: "t".into(),
        placement: Placement::DockRight,
        lines: vec![
            FormLine::Field {
                id: ControlId::GeometryValue,
                caption: "Snelheid".into(),
                text: "10".into(),
                enabled: false,
                invalid: false,
                hint: "Niet voor dit modus".into(),
            },
            FormLine::Field {
                id: ControlId::SketchValue,
                caption: "Kracht".into(),
                text: "5".into(),
                enabled: true,
                invalid: false,
                hint: "Procent".into(),
            },
        ],
    }
}

#[test]
fn a_disabled_field_gives_its_hint_under_the_pointer_and_an_enabled_one_does_not() {
    let lay = form_layout(&form(), 300, 1.0);
    let at = |i: usize| {
        let r = lay.items[i].rect;
        (r.x + r.w / 2, r.y + r.h / 2)
    };
    assert_eq!(form_tip_at(&lay, 0, at(0)).as_deref(), Some("Niet voor dit modus"));
    assert_eq!(form_tip_at(&lay, 0, at(1)), None, "an enabled field shows its own tooltip");
    assert_eq!(form_tip_at(&lay, 0, (0, 0)), None);
    let (x, y) = at(0);
    assert_eq!(form_tip_at(&lay, 30, (x, y - 30)).as_deref(), Some("Niet voor dit modus"), "scrolled");
}

#[test]
fn a_disabled_strip_button_gives_its_tooltip_with_the_reason_under_the_pointer() {
    use burnburn_presentation::HeadlessEditorHarness;
    use burnburn_windows::forms::toolstrip::strip;
    use burnburn_windows::l10n::Locale;
    let h = HeadlessEditorHarness::new(); // no window: tools are off
    let panels: Vec<StripPanel> = strip(&h.view(), Locale::En);
    let lay = strip_layout(&panels, 900, 1.0);
    let off = lay.controls.iter().find(|c| !c.enabled).expect("a disabled button without a window");
    let at = (off.rect.x + off.rect.w / 2, off.rect.y + off.rect.h / 2);
    let tip = strip_tip_at(&lay, at).expect("tooltip");
    assert_eq!(tip, off.tooltip);
    assert!(tip.contains('\u{2014}'), "label and reason: {tip}");
    let on = lay.controls.iter().find(|c| c.enabled);
    if let Some(on) = on {
        assert_eq!(strip_tip_at(&lay, (on.rect.x + 1, on.rect.y + 1)), None, "enabled buttons have their own tooltip");
    }
    let _ = StripGroup { group: None, button: panels[0].groups[0].button.clone(), variants: vec![] };
}
