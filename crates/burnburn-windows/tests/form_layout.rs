//! Form layout as pure geometry (W4-L8a): every control of every panel gets a place inside the
//! client width, nothing overlaps, the order is reading order and the content height is the
//! scroll range. The Win32 layer only creates windows at these rectangles, so this is what keeps
//! a panel readable on a real screen, at any DPI.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::layout::{Item, Layout, layout};
use burnburn_windows::forms::{
    ControlId, Form, FormButton, FormContext, FormLine, PanelState, cut_settings, inspector, layers, schikken,
};
use burnburn_windows::l10n::Locale;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    (h, doc)
}

fn some_forms() -> Vec<Form> {
    let (mut h, doc) = window();
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::En };
    let state = PanelState { schikken_shown: true, layer_row: Some(0) };
    vec![
        inspector::form(&ctx).unwrap(),
        layers::form(&ctx, &state).unwrap(),
        cut_settings::form(&ctx, &[]).unwrap(),
        schikken::form(&ctx, &state).unwrap(),
    ]
}

fn interactive(l: &Layout) -> Vec<ControlId> {
    l.items
        .iter()
        .filter_map(|p| match &p.item {
            Item::Field { id, .. } | Item::Check { id, .. } | Item::Choice { id, .. } | Item::List { id, .. } => {
                Some(id.clone())
            }
            Item::Button(b) => Some(b.id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_control_of_a_form_is_placed_once_in_reading_order() {
    for form in some_forms() {
        let l = layout(&form, 300, 1.0);
        let mut want = form.ids();
        let mut got = interactive(&l);
        assert_eq!(got.len(), want.len(), "{:?}", form.kind);
        // Same set; within a row the left control comes first.
        want.sort_by_key(|i| format!("{i:?}"));
        got.sort_by_key(|i| format!("{i:?}"));
        assert_eq!(got, want, "{:?}", form.kind);
    }
}

#[test]
fn nothing_leaves_the_client_width_overlaps_or_starts_above_the_previous_line() {
    for form in some_forms() {
        for width in [240, 300, 420] {
            let l = layout(&form, width, 1.0);
            for (i, a) in l.items.iter().enumerate() {
                assert!(a.rect.x >= 0 && a.rect.right() <= width, "{:?} item {i} {:?} in {width}", form.kind, a.rect);
                assert!(
                    a.rect.w > 0 && a.rect.h > 0 && a.rect.y >= 0 && a.rect.bottom() <= l.height,
                    "{:?} {i}",
                    form.kind
                );
                if let Some((_, c)) = &a.caption {
                    assert!(c.x >= 0 && c.right() <= width && c.w > 0, "{:?} caption {i}", form.kind);
                    assert!(!c.overlaps(&a.rect), "{:?}: caption over its control {i}", form.kind);
                }
                for (j, b) in l.items.iter().enumerate().skip(i + 1) {
                    assert!(
                        !a.rect.overlaps(&b.rect),
                        "{:?}: items {i} and {j} overlap: {:?} {:?}",
                        form.kind,
                        a.rect,
                        b.rect
                    );
                }
            }
        }
    }
}

#[test]
fn a_form_grows_with_its_lines_and_with_the_dpi() {
    let forms = some_forms();
    let inspector = &forms[0];
    let at100 = layout(inspector, 280, 1.0);
    let at200 = layout(inspector, 560, 2.0);
    assert!(at100.height > 200, "{}", at100.height);
    assert!(at200.height > at100.height * 3 / 2, "{} vs {}", at200.height, at100.height);
    let mut longer = inspector.clone();
    longer.lines.push(FormLine::Text("one more line of explanation".into()));
    assert!(layout(&longer, 280, 1.0).height > at100.height);
    // An empty form still has a height to show.
    let empty = Form { lines: vec![], ..inspector.clone() };
    assert_eq!(layout(&empty, 280, 1.0).items.len(), 0);
}

#[test]
fn a_long_text_wraps_into_more_height_on_a_narrow_panel() {
    let mut f = some_forms().remove(0);
    f.lines = vec![FormLine::Text("word ".repeat(60))];
    let wide = layout(&f, 600, 1.0).height;
    let narrow = layout(&f, 200, 1.0).height;
    assert!(narrow > wide, "{narrow} vs {wide}");
}

#[test]
fn a_grid_is_square_buttons_in_columns_and_a_row_shares_the_width() {
    let mut f = some_forms().remove(0);
    let buttons: Vec<FormButton> = (0..9).map(|n| FormButton::new(ControlId::RefPoint(n), "x", true)).collect();
    f.lines = vec![FormLine::Grid { columns: 3, buttons }];
    let l = layout(&f, 300, 1.0);
    assert_eq!(l.items.len(), 9);
    let xs: std::collections::BTreeSet<i32> = l.items.iter().map(|p| p.rect.x).collect();
    let ys: std::collections::BTreeSet<i32> = l.items.iter().map(|p| p.rect.y).collect();
    assert_eq!((xs.len(), ys.len()), (3, 3));
    assert!(l.items.iter().all(|p| p.rect.w == p.rect.h), "square cells");
    // Two fields in a row sit side by side, the same width.
    let mut row = some_forms().remove(0);
    row.lines = vec![FormLine::Row(vec![
        FormLine::field(ControlId::SketchParamName, "a", ""),
        FormLine::field(ControlId::SketchParamText, "b", ""),
    ])];
    let l = layout(&row, 300, 1.0);
    assert_eq!(l.items.len(), 2);
    assert_eq!(l.items[0].rect.y, l.items[1].rect.y);
    assert_eq!(l.items[0].rect.w, l.items[1].rect.w);
    assert!(l.items[0].rect.right() <= l.items[1].rect.x);
}
