//! Where each control of a [`Form`] goes, as pure geometry (no `windows` crate): the Win32 layer
//! creates one child window per [`Placed`] item at its rectangle. Units are device pixels; a
//! `scale` of 1.0 is 96 dpi.

use super::{ChoiceItem, ControlId, Form, FormButton, FormLine};

/// A rectangle in client pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }
}

/// One thing on the form.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Heading(String),
    Text(String),
    Problem(String),
    Field { id: ControlId, text: String, enabled: bool, invalid: bool, hint: String },
    Check { id: ControlId, caption: String, on: bool, enabled: bool },
    Choice { id: ControlId, items: Vec<ChoiceItem>, selected: usize, enabled: bool },
    Button(FormButton),
    List { id: ControlId, items: Vec<String>, selected: Vec<usize>, multi: bool },
}

/// An item at its place; `caption` is the label of a field or pop-up, drawn beside or above it.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub item: Item,
    pub rect: Rect,
    pub caption: Option<(String, Rect)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub items: Vec<Placed>,
    /// Total height of the content (the scroll range).
    pub height: i32,
}

/// Metrics in device pixels for one scale.
struct Metrics {
    margin: i32,
    gap: i32,
    row: i32,
    caption: i32,
    check: i32,
    button: i32,
    list: i32,
    line: i32,
    cell: i32,
    char_w: i32,
}

impl Metrics {
    fn new(scale: f64) -> Metrics {
        let px = |v: f64| (v * scale).round().max(1.0) as i32;
        Metrics {
            margin: px(8.0),
            gap: px(4.0),
            row: px(24.0),
            caption: px(14.0),
            check: px(22.0),
            button: px(26.0),
            list: px(120.0),
            line: px(15.0),
            cell: px(28.0),
            char_w: px(7.0),
        }
    }

    /// Height of `text` wrapped in `width` pixels.
    fn text_height(&self, text: &str, width: i32) -> i32 {
        let per_line = usize::try_from((width / self.char_w).max(1)).unwrap_or(1);
        let lines: usize = text.split('\n').map(|l| l.chars().count().div_ceil(per_line).max(1)).sum();
        i32::try_from(lines).unwrap_or(i32::MAX / 4).saturating_mul(self.line)
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w: w.max(1), h: h.max(1) }
}

/// Places one line at `(x, y)` in `w` pixels and returns its height. `stacked`: inside a row a
/// caption sits above its control instead of beside it.
fn place(line: &FormLine, x: i32, y: i32, w: i32, stacked: bool, m: &Metrics, out: &mut Vec<Placed>) -> i32 {
    let captioned = |out: &mut Vec<Placed>, caption: &str, item: Item| -> i32 {
        if stacked {
            out.push(Placed {
                item,
                rect: rect(x, y + m.caption, w, m.row),
                caption: Some((caption.to_owned(), rect(x, y, w, m.caption))),
            });
            m.caption + m.row
        } else {
            let cw = (f64::from(w) * 0.42) as i32;
            out.push(Placed {
                item,
                rect: rect(x + cw + m.gap, y, w - cw - m.gap, m.row),
                caption: Some((caption.to_owned(), rect(x, y + m.gap, cw, m.row - m.gap))),
            });
            m.row
        }
    };
    match line {
        FormLine::Heading(t) => {
            out.push(Placed { item: Item::Heading(t.clone()), rect: rect(x, y, w, m.line + 2), caption: None });
            m.line + 2
        }
        FormLine::Text(t) | FormLine::Problem(t) => {
            let h = m.text_height(t, w);
            let item =
                if matches!(line, FormLine::Problem(_)) { Item::Problem(t.clone()) } else { Item::Text(t.clone()) };
            out.push(Placed { item, rect: rect(x, y, w, h), caption: None });
            h
        }
        FormLine::Field { id, caption, text, enabled, invalid, hint } => captioned(
            out,
            caption,
            Item::Field {
                id: id.clone(),
                text: text.clone(),
                enabled: *enabled,
                invalid: *invalid,
                hint: hint.clone(),
            },
        ),
        FormLine::Choice { id, caption, items, selected, enabled } => captioned(
            out,
            caption,
            Item::Choice { id: id.clone(), items: items.clone(), selected: *selected, enabled: *enabled },
        ),
        FormLine::Check { id, caption, on, enabled } => {
            out.push(Placed {
                item: Item::Check { id: id.clone(), caption: caption.clone(), on: *on, enabled: *enabled },
                rect: rect(x, y, w, m.check),
                caption: None,
            });
            m.check
        }
        FormLine::Buttons(buttons) => {
            let n = i32::try_from(buttons.len().max(1)).unwrap_or(1);
            let bw = (w - (n - 1) * m.gap) / n;
            for (i, b) in buttons.iter().enumerate() {
                let i = i32::try_from(i).unwrap_or(0);
                out.push(Placed {
                    item: Item::Button(b.clone()),
                    rect: rect(x + i * (bw + m.gap), y, bw, m.button),
                    caption: None,
                });
            }
            m.button
        }
        FormLine::Grid { columns, buttons } => {
            let c = i32::try_from((*columns).max(1)).unwrap_or(1);
            let cell = m.cell.min((w - (c - 1) * m.gap) / c).max(1);
            for (i, b) in buttons.iter().enumerate() {
                let i = i32::try_from(i).unwrap_or(0);
                let (col, row) = (i % c, i / c);
                out.push(Placed {
                    item: Item::Button(b.clone()),
                    rect: rect(x + col * (cell + m.gap), y + row * (cell + m.gap), cell, cell),
                    caption: None,
                });
            }
            let rows = ((i32::try_from(buttons.len()).unwrap_or(0) + c - 1) / c).max(1);
            rows * (cell + m.gap) - m.gap
        }
        FormLine::List { id, items, selected, multi } => {
            out.push(Placed {
                item: Item::List { id: id.clone(), items: items.clone(), selected: selected.clone(), multi: *multi },
                rect: rect(x, y, w, m.list),
                caption: None,
            });
            m.list
        }
        FormLine::Row(children) => {
            let fixed = |l: &FormLine| match l {
                FormLine::Grid { columns, .. } => {
                    let c = i32::try_from((*columns).max(1)).unwrap_or(1);
                    Some(c * m.cell + (c - 1) * m.gap)
                }
                _ => None,
            };
            let n = i32::try_from(children.len().max(1)).unwrap_or(1);
            let fixed_total: i32 = children.iter().filter_map(fixed).sum();
            let flexible = children.iter().filter(|c| fixed(c).is_none()).count();
            let flexible = i32::try_from(flexible.max(1)).unwrap_or(1);
            let share = ((w - fixed_total - (n - 1) * m.gap) / flexible).max(1);
            let (mut cx, mut tallest) = (x, 0);
            for child in children {
                let cw = fixed(child).unwrap_or(share);
                tallest = tallest.max(place(child, cx, y, cw, true, m, out));
                cx += cw + m.gap;
            }
            tallest
        }
    }
}

/// Lay `form` out in a client area `width` pixels wide at `scale` (1.0 = 96 dpi).
pub fn layout(form: &Form, width: i32, scale: f64) -> Layout {
    let m = Metrics::new(scale);
    let inner = (width - 2 * m.margin).max(40);
    let mut items = Vec::new();
    let mut y = m.margin;
    for line in &form.lines {
        y += place(line, m.margin, y, inner, false, &m, &mut items) + m.gap;
    }
    Layout { items, height: y + m.margin - m.gap }
}
