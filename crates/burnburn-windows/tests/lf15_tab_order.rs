//! LF15: the Tab order of a Windows form is the reading order. Win32 walks the `WS_TABSTOP`
//! controls in creation order, so `SpecLayout::tab_order` (the controls the Win32 layer creates
//! with the tab-stop style, in creation order) must equal the order a reader meets the controls in:
//! row by row, left to right, captions skipped, the tab strip before its page, the dialog buttons
//! last. The oracle sorts the control rectangles by position; it does not look at creation order.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_host_models::form::{
    Control, DialogButtons, FormButton, FormRow, FormSection, FormSpec, FormTool,
};
use burnburn_windows::forms::spec_layout::{DluBase, SpecClass, SpecLayout, TextMeter, layout_spec};
use burnburn_windows::forms::spec_win32::win_spec;

struct Fixed(i32);

impl TextMeter for Fixed {
    fn width(&self, text: &str, _bold: bool) -> i32 {
        i32::try_from(text.chars().count()).unwrap_or(0) * self.0
    }
}

fn text_row(id: u32, label: &str) -> FormRow {
    FormRow::new(id, label, Control::Text { value: String::new(), placeholder: String::new() })
}

fn layout(spec: &FormSpec, dpi: u32) -> SpecLayout {
    layout_spec(spec, DluBase::segoe_ui_9(dpi), &Fixed(6))
}

/// Indices of the controls a reader can Tab to, sorted by where they stand.
fn reading_order(l: &SpecLayout) -> Vec<usize> {
    let strip_y = l.controls.iter().find(|c| c.class == SpecClass::Tabs).map(|c| c.rect.y);
    let mut stops: Vec<usize> = (0..l.controls.len())
        .filter(|&i| {
            let c = &l.controls[i];
            c.visible && c.enabled && win_spec(c.class, true).style & 0x0001_0000 != 0
        })
        .collect();
    // A control on a tab page sorts at the strip's height, then by page, then by position.
    let key = |i: &usize| {
        let c = &l.controls[*i];
        match (c.tab, strip_y) {
            (Some(page), Some(sy)) => (sy, 1 + page, c.rect.y, c.rect.x),
            _ => (c.rect.y, 0, c.rect.y, c.rect.x),
        }
    };
    stops.sort_by_key(key);
    stops
}

fn mixed() -> FormSpec {
    FormSpec {
        sections: vec![
            FormSection::new(
                Some("Eerste"),
                vec![
                    text_row(1, "Naam"),
                    FormRow::new(2, "Soort", Control::Choice { options: vec!["A".into(), "B".into()], selected: 0 }),
                    FormRow::new(3, "Aan", Control::Check { on: true }).tool(FormTool::new(4, "+", "Meer")),
                    FormRow::new(5, "Notitie", Control::Text { value: String::new(), placeholder: String::new() })
                        .help("Hulp onder het veld"),
                ],
            ),
            FormSection::new(None, vec![text_row(6, "Laatste")]),
        ],
        buttons: DialogButtons {
            primary: Some(FormButton::new(90, "OK")),
            cancel: Some(FormButton::new(91, "Annuleer")),
            ..DialogButtons::default()
        },
        ..FormSpec::default()
    }
}

fn tabbed() -> FormSpec {
    let page = |tab: &str, first: u32| {
        FormSection::new(None, (0..3).map(|i| text_row(first + i, &format!("Veld {i}"))).collect()).on_tab(tab)
    };
    FormSpec {
        sections: vec![FormSection::new(None, vec![text_row(50, "Boven")]), page("Een", 1), page("Twee", 10)],
        buttons: DialogButtons { primary: Some(FormButton::new(99, "OK")), ..DialogButtons::default() },
        ..FormSpec::default()
    }
}

#[test]
fn tab_order_is_the_reading_order_of_every_form_at_every_dpi() {
    for spec in [mixed(), tabbed()] {
        for dpi in [96, 144, 192] {
            let l = layout(&spec, dpi);
            let expected = reading_order(&l);
            assert!(!expected.is_empty(), "{dpi}: the oracle found no tab stops");
            assert_eq!(l.tab_order(), expected, "{dpi}: Tab does not follow the reading order");
        }
    }
}

#[test]
fn tab_order_skips_captions_hints_and_disabled_controls_and_ends_on_the_dialog_buttons() {
    let l = layout(&mixed(), 96);
    let order = l.tab_order();
    let ids: Vec<u32> = order.iter().filter_map(|&i| l.controls[i].id.map(|id| id.0)).collect();
    assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 90, 91].into_iter().filter(|i| *i != 0).collect::<Vec<_>>());
    assert!(order.iter().all(|&i| !matches!(l.controls[i].class, SpecClass::Caption | SpecClass::Help)));
    let last = &l.controls[*order.last().unwrap()];
    assert!(matches!(last.class, SpecClass::DialogButton { .. }));
}
