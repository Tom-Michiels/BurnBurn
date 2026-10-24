//! LF13: the one Windows form builder renders a `FormSpec` as control rectangles. Headless: the
//! rectangles are checked per DPI (96, 144, 192) for the label and field columns, the unit as a
//! suffix, the tabs, the notice and the button row in Windows order with the default button. The
//! text width comes from a fixed meter (`GetTextExtentPoint32W` in the real host).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_host_models::form::{Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec};
use burnburn_host_models::format::Quantity;
use burnburn_host_models::style::StatusTone;
use burnburn_windows::forms::layout::Rect;
use burnburn_windows::forms::spec_layout::{DluBase, SpecClass, SpecControl, SpecLayout, TextMeter, layout_spec};

const DPIS: [u32; 3] = [96, 144, 192];

/// `per_char` pixels per character at 96 dpi, scaled with the DPI.
struct Fixed {
    per_char: f64,
    dpi: u32,
}

impl TextMeter for Fixed {
    fn width(&self, text: &str, bold: bool) -> i32 {
        let w = self.per_char * f64::from(self.dpi) / 96.0 * if bold { 1.1 } else { 1.0 };
        (text.chars().count() as f64 * w).round() as i32
    }
}

fn build(spec: &FormSpec, dpi: u32, per_char: f64) -> SpecLayout {
    layout_spec(spec, DluBase::segoe_ui_9(dpi), &Fixed { per_char, dpi })
}

fn number(id: u32, label: &str, unit: Option<Quantity>) -> FormRow {
    let row = FormRow::new(id, label, Control::Number { value: "10".into() });
    match unit {
        Some(u) => row.unit(u),
        None => row,
    }
}

fn text_row(id: u32, label: &str) -> FormRow {
    FormRow::new(id, label, Control::Text { value: String::new(), placeholder: String::new() })
}

fn sample() -> FormSpec {
    FormSpec {
        title: "Snij-instellingen".into(),
        sections: vec![FormSection::new(
            Some("Snijden"),
            vec![
                number(1, "Snelheid", Some(Quantity::MillimetersPerSecond)),
                number(2, "Vermogen", Some(Quantity::Percent)),
                FormRow::new(
                    3,
                    "Laagsoort",
                    Control::Choice { options: vec!["Snijden".into(), "Vullen".into()], selected: 0 },
                ),
                FormRow::new(4, "Zichtbaar", Control::Check { on: true }),
            ],
        )],
        buttons: DialogButtons {
            cancel: Some(FormButton::new(11, "Annuleren")),
            primary: Some(FormButton::new(10, "OK")),
            destructive: Some(FormButton::new(12, "Standaard")),
            help: Some(FormButton::new(13, "Help")),
        },
        ..FormSpec::default()
    }
}

fn of(layout: &SpecLayout, class: fn(&SpecClass) -> bool) -> Vec<&SpecControl> {
    layout.controls.iter().filter(|c| class(&c.class)).collect()
}

fn by_id(layout: &SpecLayout, id: u32) -> &SpecControl {
    layout.controls.iter().find(|c| c.id == Some(FormId(id)) && !matches!(c.class, SpecClass::Suffix)).unwrap()
}

fn cy(r: &Rect) -> i32 {
    r.y + r.h / 2
}

#[test]
fn dialog_units_become_pixels_with_the_font_at_each_dpi() {
    // Segoe UI 9 pt: 7 by 15 pixels at 96 dpi, so 8 horizontal DLU are 14 px and 16 vertical 30 px.
    for (dpi, x, y) in [(96, 14, 30), (144, 21, 45), (192, 28, 60)] {
        let b = DluBase::segoe_ui_9(dpi);
        assert_eq!((b.x(8.0), b.y(16.0)), (x, y), "{dpi} dpi");
    }
    // A measured font (GetTextMetrics) replaces the table.
    let measured = DluBase { char_width: 8.0, char_height: 16.0, dpi: 96 };
    assert_eq!((measured.x(4.0), measured.y(8.0)), (8, 16));
}

#[test]
fn captions_share_one_column_and_fields_start_after_the_widest_caption_at_every_dpi() {
    for dpi in DPIS {
        let l = build(&sample(), dpi, 6.0);
        let meter = Fixed { per_char: 6.0, dpi };
        let captions = of(&l, |c| matches!(c, SpecClass::Caption));
        assert_eq!(captions.len(), 3, "the check box carries its own text: {captions:?}");
        assert!(captions.iter().all(|c| c.rect.x == captions[0].rect.x), "{dpi}: one caption column");
        let widest = captions.iter().map(|c| meter.width(&c.text, false)).max().unwrap();
        let fields: Vec<&SpecControl> = vec![by_id(&l, 1), by_id(&l, 2), by_id(&l, 3)];
        assert!(fields.iter().all(|f| f.rect.x == fields[0].rect.x), "{dpi}: one field column");
        assert!(fields[0].rect.x >= captions[0].rect.x + widest, "{dpi}: fields clear the widest caption");
        // the check box sits in the field column
        let check = by_id(&l, 4);
        assert_eq!((check.class, check.text.as_str(), check.rect.x), (SpecClass::Check, "Zichtbaar", fields[0].rect.x));
        // reading order: each row lies below the previous one
        let ys: Vec<i32> = [1, 2, 3, 4].iter().map(|i| by_id(&l, *i).rect.y).collect();
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{dpi}: {ys:?}");
    }
}

#[test]
fn a_wider_caption_widens_the_caption_column_and_the_dpi_scales_the_form() {
    let wide = FormSpec {
        sections: vec![FormSection::new(None, vec![number(1, "Een heel lange bijschrift voor dit veld", None)])],
        ..FormSpec::default()
    };
    let short =
        FormSpec { sections: vec![FormSection::new(None, vec![number(1, "Kort", None)])], ..FormSpec::default() };
    assert!(by_id(&build(&wide, 96, 6.0), 1).rect.x > by_id(&build(&short, 96, 6.0), 1).rect.x + 100);
    // the same form at twice the DPI is twice as large (to the pixel rounding of each unit)
    let (a, b) = (build(&sample(), 96, 6.0), build(&sample(), 192, 6.0));
    assert!((b.client.0 - 2 * a.client.0).abs() <= 6, "{:?} {:?}", a.client, b.client);
    assert!((b.client.1 - 2 * a.client.1).abs() <= 12, "{:?} {:?}", a.client, b.client);
    assert!((by_id(&b, 3).rect.x - 2 * by_id(&a, 3).rect.x).abs() <= 4);
}

#[test]
fn the_unit_is_a_suffix_after_its_field_on_the_same_line_and_never_in_the_caption() {
    for dpi in DPIS {
        let l = build(&sample(), dpi, 6.0);
        let suffixes = of(&l, |c| matches!(c, SpecClass::Suffix));
        assert_eq!(suffixes.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), vec!["mm/s", "%"]);
        for (suffix, id) in suffixes.iter().zip([1, 2]) {
            let field = by_id(&l, id);
            assert!(suffix.rect.x >= field.rect.right(), "{dpi}: suffix after the field");
            assert!((cy(&suffix.rect) - cy(&field.rect)).abs() <= 1, "{dpi}: one line");
            assert_eq!(suffix.rect.x, suffixes[0].rect.x, "{dpi}: suffixes line up");
        }
        assert!(of(&l, |c| matches!(c, SpecClass::Caption)).iter().all(|c| !c.text.contains("mm")));
    }
}

#[test]
fn nothing_overlaps_and_everything_is_inside_the_client_area_at_every_dpi() {
    for dpi in DPIS {
        let l = build(&sample(), dpi, 6.0);
        let shown: Vec<&SpecControl> = l.controls.iter().filter(|c| c.visible).collect();
        assert!(!shown.is_empty());
        for c in &shown {
            assert!(c.rect.x >= 0 && c.rect.y >= 0, "{dpi}: {c:?}");
            assert!(c.rect.right() <= l.client.0 && c.rect.bottom() <= l.client.1, "{dpi}: {c:?} in {:?}", l.client);
            assert!(c.rect.w > 0 && c.rect.h > 0, "{dpi}: {c:?}");
        }
        for (i, a) in shown.iter().enumerate() {
            for b in &shown[i + 1..] {
                assert!(!a.rect.overlaps(&b.rect), "{dpi}: {a:?} overlaps {b:?}");
            }
        }
    }
}

#[test]
fn the_buttons_run_ok_cancel_then_the_rest_on_one_line_at_the_right_with_ok_as_default() {
    for dpi in DPIS {
        let l = build(&sample(), dpi, 6.0);
        let buttons = of(&l, |c| matches!(c, SpecClass::DialogButton { .. }));
        let order: Vec<(&str, u32)> = buttons.iter().map(|b| (b.text.as_str(), b.id.unwrap().0)).collect();
        assert_eq!(order, vec![("OK", 10), ("Annuleren", 11), ("Standaard", 12), ("Help", 13)], "{dpi}");
        assert!(buttons.windows(2).all(|w| w[0].rect.right() < w[1].rect.x), "{dpi}: left to right with a gap");
        assert!(buttons.iter().all(|b| b.rect.y == buttons[0].rect.y && b.rect.h == buttons[0].rect.h));
        let margin = l.client.0 - buttons.last().unwrap().rect.right();
        assert!(margin > 0 && margin <= DluBase::segoe_ui_9(dpi).x(14.0), "{dpi}: right aligned, margin {margin}");
        let defaults: Vec<u32> = buttons
            .iter()
            .filter(|b| matches!(b.class, SpecClass::DialogButton { default: true }))
            .map(|b| b.id.unwrap().0)
            .collect();
        assert_eq!(defaults, vec![10], "{dpi}");
        assert_eq!((l.default_button, l.cancel_button), (Some(FormId(10)), Some(FormId(11))));
        // every button is at least the standard 50 by 14 DLU
        let base = DluBase::segoe_ui_9(dpi);
        assert!(buttons.iter().all(|b| b.rect.w >= base.x(50.0) && b.rect.h >= base.y(14.0)));
        // the buttons lie below the content
        assert!(buttons[0].rect.y > by_id(&l, 4).rect.bottom());
    }
}

#[test]
fn a_disabled_primary_button_is_not_the_default_and_a_missing_cancel_has_no_escape() {
    let mut spec = sample();
    if let Some(p) = spec.buttons.primary.as_mut() {
        p.enabled = false;
    }
    spec.buttons.cancel = None;
    let l = build(&spec, 96, 6.0);
    assert_eq!((l.default_button, l.cancel_button), (None, None));
    assert!(of(&l, |c| matches!(c, SpecClass::DialogButton { default: true })).is_empty());
    let ok = l.controls.iter().find(|c| c.text == "OK").unwrap();
    assert!(!ok.enabled);
}

#[test]
fn tabs_put_each_page_in_the_same_area_and_the_form_is_as_high_as_its_tallest_page() {
    let page = |tab: &str, rows: u32, first: u32| {
        FormSection::new(None, (0..rows).map(|i| text_row(first + i, &format!("Veld {i}"))).collect()).on_tab(tab)
    };
    let spec = FormSpec {
        sections: vec![page("Algemeen", 2, 1), page("Canvas", 5, 10)],
        buttons: DialogButtons { primary: Some(FormButton::new(99, "OK")), ..DialogButtons::default() },
        ..FormSpec::default()
    };
    for dpi in DPIS {
        let l = build(&spec, dpi, 6.0);
        assert_eq!(l.tabs, vec!["Algemeen".to_owned(), "Canvas".to_owned()]);
        let strip = of(&l, |c| matches!(c, SpecClass::Tabs));
        assert_eq!(strip.len(), 1);
        assert!(strip[0].rect.w >= l.client.0 - 2 * DluBase::segoe_ui_9(dpi).x(7.0) - 2);
        assert_eq!(by_id(&l, 1).tab, Some(0));
        assert_eq!(by_id(&l, 10).tab, Some(1));
        assert_eq!(by_id(&l, 1).rect.y, by_id(&l, 10).rect.y, "{dpi}: pages start at the same place");
        assert!(by_id(&l, 1).rect.y >= strip[0].rect.y + 1, "{dpi}: pages lie in the strip");
        // the tallest page (5 rows) sets the height, so switching tabs never resizes the dialog
        let last = l.controls.iter().filter(|c| c.tab == Some(1)).map(|c| c.rect.bottom()).max().unwrap();
        let ok = l.controls.iter().find(|c| c.text == "OK").unwrap();
        assert!(ok.rect.y > last, "{dpi}");
        assert!(strip[0].rect.bottom() >= last, "{dpi}: the strip frames the pages");
        // controls on different pages may share room; on one page they never overlap
        for tab in 0..2 {
            let on: Vec<&SpecControl> = l.controls.iter().filter(|c| c.tab == Some(tab)).collect();
            for (i, a) in on.iter().enumerate() {
                assert!(on[i + 1..].iter().all(|b| !a.rect.overlaps(&b.rect)), "{dpi} tab {tab}");
            }
        }
    }
}

#[test]
fn the_notice_stands_above_the_buttons_and_its_room_is_kept_while_there_is_none() {
    let mut with = sample();
    with.notice = Some("Waarde buiten bereik".into());
    with.notice_slot = true;
    let mut empty = sample();
    empty.notice_slot = true;
    let none = sample();
    for dpi in DPIS {
        let (a, b, c) = (build(&with, dpi, 6.0), build(&empty, dpi, 6.0), build(&none, dpi, 6.0));
        let notice = a.controls.iter().find(|c| c.class == SpecClass::Notice).unwrap();
        assert_eq!(notice.text, "Waarde buiten bereik");
        let ok = a.controls.iter().find(|c| c.text == "OK").unwrap();
        assert!(notice.rect.bottom() <= ok.rect.y, "{dpi}: above the buttons");
        assert_eq!(a.client, b.client, "{dpi}: the slot keeps the shape");
        assert!(b.client.1 > c.client.1, "{dpi}: and takes room");
        assert!(b.controls.iter().find(|c| c.class == SpecClass::Notice).is_some_and(|n| n.text.is_empty()));
        assert!(c.controls.iter().all(|c| c.class != SpecClass::Notice));
    }
}

#[test]
fn a_hidden_row_takes_no_room_and_an_unavailable_section_says_why_once_and_greys_its_rows() {
    let mut spec = sample();
    spec.sections[0].rows[1].hidden = true;
    let shown = build(&sample(), 96, 6.0);
    let hidden = build(&spec, 96, 6.0);
    assert!(!by_id(&hidden, 2).visible);
    let gap = by_id(&shown, 2).rect.y - by_id(&shown, 1).rect.bottom();
    assert_eq!(by_id(&hidden, 3).rect.y, by_id(&hidden, 1).rect.bottom() + gap);
    assert!(hidden.client.1 < shown.client.1);

    let mut off = sample();
    off.sections[0] = std::mem::take(&mut off.sections[0]).unavailable("Kies eerst een machine");
    let l = build(&off, 96, 6.0);
    let reasons: Vec<_> = l.controls.iter().filter(|c| c.text == "Kies eerst een machine").collect();
    assert_eq!(reasons.len(), 1);
    assert!([1, 2, 3, 4].iter().all(|i| !by_id(&l, *i).enabled));
    assert!(by_id(&l, 1).rect.y > reasons[0].rect.bottom(), "the reason stands under the heading, above the rows");
}

#[test]
fn a_note_keeps_room_for_its_lines_and_a_long_help_wraps_inside_the_field_column() {
    let note = |text: &str| FormSpec {
        sections: vec![FormSection::new(
            None,
            vec![
                FormRow::new(
                    1,
                    "",
                    Control::Note { value: text.into(), lines: 3, tone: StatusTone::Warning, strong: false },
                ),
                number(2, "Naam", None),
            ],
        )],
        ..FormSpec::default()
    };
    assert_eq!(build(&note("Kort"), 96, 6.0).client, build(&note("Een andere, langere tekst"), 96, 6.0).client);
    let l = build(&note("x"), 96, 6.0);
    let n = by_id(&l, 1);
    let base = DluBase::segoe_ui_9(96);
    assert!(n.rect.h >= 3 * base.y(8.0), "{n:?}");
    assert!(by_id(&l, 2).rect.y >= n.rect.bottom(), "the next row does not move into the note's lines");
    assert!(of(&l, |c| matches!(c, SpecClass::Caption)).iter().all(|c| !c.text.is_empty()), "no empty caption");

    let helped = |help: &str| FormSpec {
        sections: vec![FormSection::new(None, vec![number(1, "Afstand", None).help(help)])],
        ..FormSpec::default()
    };
    let one = build(&helped("Kort"), 96, 6.0);
    let many = build(&helped(&"Een lange uitleg die niet op een regel past. ".repeat(6)), 96, 6.0);
    let help = |l: &SpecLayout| l.controls.iter().find(|c| c.class == SpecClass::Help).unwrap().rect;
    assert!(help(&many).h > help(&one).h, "wraps onto more lines");
    assert_eq!(help(&many).x, by_id(&many, 1).rect.x, "help stands under the field");
    assert!(help(&many).right() <= many.client.0);
    assert!(help(&many).y >= by_id(&many, 1).rect.bottom());
}

/// Regression (found by the panel tests): tools beside a check box overlapped its error line, and
/// a row of action buttons wider than the form ran out of it.
#[test]
fn tools_make_their_row_tall_enough_and_action_buttons_wrap_inside_the_form() {
    use burnburn_host_models::form::FormTool;
    let spec = FormSpec {
        sections: vec![FormSection::new(
            None,
            vec![
                FormRow::new(1, "Volgorde", Control::Check { on: true })
                    .tool(FormTool::new(2, "\u{2191}", "Omhoog"))
                    .tool(FormTool::new(3, "\u{2193}", "Omlaag"))
                    .error(""),
                FormRow::new(
                    10,
                    "",
                    Control::Actions {
                        titles: vec![
                            "Bijwerken en toepassen".into(),
                            "Een tweede lange knop".into(),
                            "Derde knop".into(),
                        ],
                    },
                ),
            ],
        )],
        ..FormSpec::default()
    };
    for dpi in DPIS {
        let l = build(&spec, dpi, 6.0);
        let shown: Vec<&SpecControl> = l.controls.iter().filter(|c| c.visible).collect();
        for (i, a) in shown.iter().enumerate() {
            assert!(shown[i + 1..].iter().all(|b| !a.rect.overlaps(&b.rect)), "{dpi}: {a:?}");
            assert!(a.rect.right() <= l.client.0, "{dpi}: {a:?} runs out of {:?}", l.client);
        }
        let buttons: Vec<&SpecControl> = l.controls.iter().filter(|c| c.class == SpecClass::Button).collect();
        assert_eq!(buttons.iter().map(|b| b.id.unwrap().0).collect::<Vec<_>>(), vec![10, 11, 12]);
        assert!(buttons.iter().any(|b| b.rect.y > buttons[0].rect.y), "{dpi}: the buttons wrap");
    }
}
