//! The one Windows form builder (LF13): turns a platform-free [`FormSpec`] into control rectangles
//! in device pixels. Nothing here knows Win32; the Win32 layer creates one child window per
//! [`SpecControl`] at its rectangle (`win32/spec_form.rs`).
//!
//! Sizes are dialog units (DLU) as the Windows user-interface guidelines give them (margin 7,
//! edit and button 14 high, button 50 wide, gaps 4) and become pixels through the font's base
//! units ([`DluBase`]). The width of a text comes from an injectable [`TextMeter`]
//! (`GetTextExtentPoint32W` in the real host, a fixed table in tests).

use burnburn_host_models::form::{
    ButtonRole, Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec, Platform,
};

use super::layout::Rect;

/// The width of a text in device pixels.
pub trait TextMeter {
    /// The width of `text` in the dialog font (`bold` for headings).
    fn width(&self, text: &str, bold: bool) -> i32;
}

/// The base units of the dialog font: the average character width and the height in pixels
/// (`tmAveCharWidth`, `tmHeight` of the font at the window's DPI). One horizontal DLU is a quarter
/// of the width, one vertical DLU an eighth of the height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DluBase {
    pub char_width: f64,
    pub char_height: f64,
    pub dpi: u32,
}

impl DluBase {
    /// Segoe UI 9 pt at `dpi` (96 dpi: 7 by 15 pixels), for a host that has not measured its font.
    pub fn segoe_ui_9(dpi: u32) -> DluBase {
        let scale = f64::from(dpi.max(1)) / 96.0;
        DluBase { char_width: 7.0 * scale, char_height: 15.0 * scale, dpi }
    }

    /// Horizontal dialog units to pixels.
    pub fn x(&self, dlu: f64) -> i32 {
        (dlu * self.char_width / 4.0).round() as i32
    }

    /// Vertical dialog units to pixels.
    pub fn y(&self, dlu: f64) -> i32 {
        (dlu * self.char_height / 8.0).round() as i32
    }

    /// Points of the form (a preview's size) to pixels at this DPI.
    pub fn points(&self, points: f64) -> i32 {
        (points * f64::from(self.dpi) / 96.0).round() as i32
    }
}

/// What a control is, in Win32 terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecClass {
    /// The tab strip (`SysTabControl32`).
    Tabs,
    /// A section heading (bold static text).
    Heading,
    /// The caption of a row.
    Caption,
    /// A single-line edit; `numeric` right-aligns it.
    Edit { numeric: bool },
    /// A drop-down list.
    Combo,
    Check,
    /// A multi-line edit.
    Memo,
    /// A push button inside the form.
    Button,
    /// The unit after a control.
    Suffix,
    /// Help under a control, or the reason of a section.
    Help,
    /// The error of a row.
    Error,
    /// A read-only single-line value.
    Value,
    /// A wrapping read-only text.
    Note,
    /// A read-only multi-line edit with a scroll bar.
    Report,
    Preview,
    Picture,
    /// A small button after a control.
    Tool,
    /// The notice of the whole form, above the buttons.
    Notice,
    /// A dialog button; `default` is the one Return answers.
    DialogButton { default: bool },
}

/// One control at its place.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecControl {
    pub class: SpecClass,
    /// The row, tool or button this control reports; `None` for captions and decoration.
    pub id: Option<FormId>,
    pub rect: Rect,
    pub text: String,
    pub enabled: bool,
    /// `false` for a hidden row: it keeps its control but takes no room.
    pub visible: bool,
    /// The tab page the control lies on; `None` outside the pages.
    pub tab: Option<usize>,
}

/// The built form.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecLayout {
    pub title: String,
    pub controls: Vec<SpecControl>,
    /// Width and height of the client area.
    pub client: (i32, i32),
    pub tabs: Vec<String>,
    /// The button Return answers.
    pub default_button: Option<FormId>,
    /// The button Escape answers.
    pub cancel_button: Option<FormId>,
}

/// The sizes of the Windows guidelines, in dialog units.
mod dlu {
    pub const MARGIN: f64 = 7.0;
    pub const PAGE_PAD: f64 = 7.0;
    pub const TAB_HEADER: f64 = 14.0;
    pub const ROW: f64 = 14.0;
    pub const ROW_GAP: f64 = 4.0;
    pub const TEXT: f64 = 8.0;
    pub const CHECK: f64 = 10.0;
    pub const HEADING: f64 = 10.0;
    pub const SECTION_GAP: f64 = 7.0;
    pub const COLUMN_GAP: f64 = 4.0;
    pub const SUFFIX_GAP: f64 = 3.0;
    pub const FIELD: f64 = 100.0;
    pub const FIELD_MAX: f64 = 180.0;
    pub const MIN_INNER: f64 = 200.0;
    pub const BUTTON_W: f64 = 50.0;
    pub const BUTTON_H: f64 = 14.0;
    pub const BUTTON_PAD: f64 = 12.0;
    pub const BUTTON_GAP: f64 = 4.0;
    pub const TOOL_W: f64 = 14.0;
    pub const TOOL_PAD: f64 = 8.0;
    pub const TOOL_GAP: f64 = 2.0;
    pub const MEMO: f64 = 50.0;
    pub const LOG: f64 = 60.0;
    pub const REPORT_EXTRA: f64 = 6.0;
    pub const COMBO_ARROW: f64 = 20.0;
    pub const CHECK_BOX: f64 = 16.0;
    pub const NOTICE_LINES: usize = 2;
}

/// How many lines `text` takes in `width` pixels: words are packed greedily, a word wider than the
/// line takes as many lines as it needs, and a line break starts a new line.
pub fn wrap_lines(text: &str, width: i32, bold: bool, meter: &dyn TextMeter) -> usize {
    let width = width.max(1);
    let space = meter.width(" ", bold);
    let mut lines = 0;
    for paragraph in text.split('\n') {
        let mut used = 0;
        let mut any = false;
        let mut count = 1;
        for word in paragraph.split_whitespace() {
            let w = meter.width(word, bold);
            if any && used + space + w > width {
                count += 1;
                used = 0;
            } else if any {
                used += space;
            }
            if w > width {
                count += usize::try_from((w - 1) / width).unwrap_or(0);
                used = w % width;
            } else {
                used += w;
            }
            any = true;
        }
        lines += count;
    }
    lines.max(1)
}

/// What the form needs in width, found before anything is placed so that every tab shares the
/// same columns.
struct Columns {
    caption: i32,
    field: i32,
    inner: i32,
}

struct Builder<'a> {
    base: DluBase,
    meter: &'a dyn TextMeter,
    out: Vec<SpecControl>,
    cols: Columns,
    /// Left edge of the content (margin plus the page padding).
    x0: i32,
    tab: Option<usize>,
}

fn row_shown(row: &FormRow) -> bool {
    !row.hidden && !matches!(&row.control, Control::ReadOnly { value } if value.is_empty() && row.label.is_empty())
}

fn has_caption(row: &FormRow) -> bool {
    !matches!(row.control, Control::Check { .. }) && !row.label.is_empty()
}

/// A row without a caption that needs the width: it runs under the captions too.
fn spans(row: &FormRow) -> bool {
    row.label.is_empty()
        && matches!(
            row.control,
            Control::Note { .. }
                | Control::Preview { .. }
                | Control::Report { .. }
                | Control::Actions { .. }
                | Control::ReadOnly { .. }
                | Control::Log { .. }
                | Control::Picture { .. }
        )
}

impl Builder<'_> {
    fn field_x(&self) -> i32 {
        self.x0 + if self.cols.caption > 0 { self.cols.caption + self.base.x(dlu::COLUMN_GAP) } else { 0 }
    }

    fn push(&mut self, class: SpecClass, id: Option<FormId>, rect: Rect, text: &str, enabled: bool, visible: bool) {
        let rect = if visible { rect } else { Rect { h: 0, ..rect } };
        self.out.push(SpecControl { class, id, rect, text: text.to_owned(), enabled, visible, tab: self.tab });
    }

    fn button_width(&self, title: &str) -> i32 {
        self.base.x(dlu::BUTTON_W).max(self.meter.width(title, false) + self.base.x(dlu::BUTTON_PAD))
    }

    fn lines(&self, text: &str, width: i32, bold: bool) -> i32 {
        let n = if text.is_empty() { 1 } else { wrap_lines(text, width, bold, self.meter) };
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    /// Places `section` from `y`; returns the y below it.
    fn section(&mut self, section: &FormSection, mut y: i32, first: bool) -> i32 {
        if !section.rows.iter().any(row_shown) {
            // nothing to show: the rows keep their controls, hidden, and take no room
            for row in &section.rows {
                self.row(section, row, y, false);
            }
            return y;
        }
        if !first {
            y += self.base.y(dlu::SECTION_GAP);
        }
        let inner = self.cols.inner;
        if let Some(heading) = &section.heading {
            let r = Rect { x: self.x0, y, w: inner, h: self.base.y(dlu::HEADING) };
            self.push(SpecClass::Heading, None, r, heading, true, true);
            y += r.h + self.base.y(dlu::ROW_GAP);
        }
        if let Some(reason) = section.unavailable.as_ref().or(section.reason.as_ref()) {
            let h = self.lines(reason, inner, false) * self.base.y(dlu::TEXT);
            let r = Rect { x: self.x0, y, w: inner, h };
            self.push(SpecClass::Help, None, r, reason, true, true);
            y += h + self.base.y(dlu::ROW_GAP);
        }
        for row in &section.rows {
            y = self.row(section, row, y, row_shown(row));
        }
        y
    }

    /// Places one row at `y`; returns the y of the next one.
    fn row(&mut self, section: &FormSection, row: &FormRow, y: i32, shown: bool) -> i32 {
        let base = self.base;
        let enabled = section.row_enabled(row);
        let id = Some(row.id);
        let (row_h, text_h) = (base.y(dlu::ROW), base.y(dlu::TEXT));
        let cx = if spans(row) { self.x0 } else { self.field_x() };
        let avail = (self.x0 + self.cols.inner - cx).max(1);
        let field = self.cols.field.min(avail);
        let mut h = row_h;
        let right;
        if has_caption(row) {
            let r = Rect { x: self.x0, y: y + (row_h - text_h) / 2, w: self.cols.caption, h: text_h };
            self.push(SpecClass::Caption, None, r, &row.label, enabled, shown);
        }
        match &row.control {
            Control::Text { value, .. } | Control::Number { value } => {
                let numeric = matches!(row.control, Control::Number { .. });
                self.push(SpecClass::Edit { numeric }, id, Rect { x: cx, y, w: field, h: row_h }, value, enabled, shown);
                right = cx + field;
            }
            Control::Choice { options, selected } => {
                let text = options.get(*selected).map_or("", String::as_str);
                self.push(SpecClass::Combo, id, Rect { x: cx, y, w: field, h: row_h }, text, enabled, shown);
                right = cx + field;
            }
            Control::Check { .. } => {
                h = base.y(dlu::CHECK);
                let w = (self.meter.width(&row.label, false) + base.x(dlu::CHECK_BOX)).min(avail);
                self.push(SpecClass::Check, id, Rect { x: cx, y, w, h }, &row.label, enabled, shown);
                right = cx + w;
            }
            Control::Memo { text } => {
                h = base.y(dlu::MEMO);
                self.push(SpecClass::Memo, id, Rect { x: cx, y, w: avail, h }, text, enabled, shown);
                right = cx + avail;
            }
            Control::Action { title } => {
                let w = self.button_width(title).min(avail);
                self.push(SpecClass::Button, id, Rect { x: cx, y, w, h: row_h }, title, enabled, shown);
                right = cx + w;
            }
            Control::Actions { titles } => {
                // Buttons that do not fit the width go on the next line.
                let (mut x, mut line_y) = (cx, y);
                let gap = base.x(dlu::BUTTON_GAP);
                for (n, title) in titles.iter().enumerate() {
                    let w = self.button_width(title).min(avail);
                    if x > cx && x + w > cx + avail {
                        x = cx;
                        line_y += row_h + base.y(dlu::ROW_GAP);
                    }
                    let id = Some(FormId(row.id.0 + u32::try_from(n).unwrap_or(0)));
                    self.push(SpecClass::Button, id, Rect { x, y: line_y, w, h: row_h }, title, enabled, shown);
                    x += w + gap;
                }
                h = line_y + row_h - y;
                right = x - gap;
            }
            Control::ReadOnly { value } => {
                self.push(SpecClass::Value, id, Rect { x: cx, y, w: avail, h: row_h }, value, enabled, shown);
                right = cx + avail;
            }
            Control::Note { value, lines, strong, .. } => {
                let needed = self.lines(value, avail, *strong);
                h = needed.max(i32::from(*lines)) * text_h;
                self.push(SpecClass::Note, id, Rect { x: cx, y, w: avail, h }, value, enabled, shown);
                right = cx + avail;
            }
            Control::Report { text, lines } => {
                h = i32::from(*lines).max(1) * text_h + base.y(dlu::REPORT_EXTRA);
                self.push(SpecClass::Report, id, Rect { x: cx, y, w: avail, h }, text, enabled, shown);
                right = cx + avail;
            }
            Control::Log { text } => {
                h = base.y(dlu::LOG);
                self.push(SpecClass::Report, id, Rect { x: cx, y, w: avail, h }, text, enabled, shown);
                right = cx + avail;
            }
            Control::Preview { width, height } => {
                let w = base.points(f64::from(*width)).min(avail).max(1);
                h = base.points(f64::from(*height)).max(1);
                self.push(SpecClass::Preview, id, Rect { x: cx, y, w, h }, "", enabled, shown);
                right = cx + w;
            }
            Control::Picture { image } => {
                let iw = base.points(f64::from(image.width)).max(1);
                let ih = base.points(f64::from(image.height)).max(1);
                let w = iw.min(avail);
                h = i32::try_from(i64::from(ih) * i64::from(w) / i64::from(iw)).unwrap_or(ih).max(1);
                self.push(SpecClass::Picture, id, Rect { x: cx, y, w, h }, "", enabled, shown);
                right = cx + w;
            }
        }
        let mut right = right;
        if !row.tools.is_empty() {
            h = h.max(row_h);
        }
        if let Some(unit) = row.suffix() {
            let x = right + base.x(dlu::SUFFIX_GAP);
            let w = self.meter.width(unit, false).max(1);
            let r = Rect { x, y: y + (row_h - text_h) / 2, w, h: text_h };
            self.push(SpecClass::Suffix, id, r, unit, enabled, shown);
            right = x + w;
        }
        let mut tool_x = right + base.x(dlu::SUFFIX_GAP);
        for tool in &row.tools {
            let w = base.x(dlu::TOOL_W).max(self.meter.width(&tool.title, false) + base.x(dlu::TOOL_PAD));
            let r = Rect { x: tool_x, y, w, h: row_h };
            self.push(SpecClass::Tool, Some(tool.id), r, &tool.title, enabled && tool.enabled, shown);
            tool_x += w + base.x(dlu::TOOL_GAP);
        }
        if !shown {
            return y;
        }
        let mut next = y + h;
        let width = (self.x0 + self.cols.inner - cx).max(1);
        for (class, text) in [(SpecClass::Help, &row.help), (SpecClass::Error, &row.error)] {
            if let Some(text) = text {
                let r = Rect { x: cx, y: next + base.y(1.0), w: width, h: self.lines(text, width, false) * text_h };
                self.push(class, None, r, text, true, true);
                next = r.bottom();
            }
        }
        next + base.y(dlu::ROW_GAP)
    }
}

/// The widths every tab shares.
fn columns(spec: &FormSpec, base: DluBase, meter: &dyn TextMeter, buttons_w: i32, tabbed: bool) -> Columns {
    let rows = || spec.sections.iter().flat_map(|s| &s.rows).filter(|r| row_shown(r));
    let caption = rows().filter(|r| has_caption(r)).map(|r| meter.width(&r.label, false)).max().unwrap_or(0);
    let mut field = base.x(dlu::FIELD);
    for row in rows() {
        if let Control::Choice { options, .. } = &row.control {
            let widest = options.iter().map(|o| meter.width(o, false)).max().unwrap_or(0);
            field = field.max((widest + base.x(dlu::COMBO_ARROW)).min(base.x(dlu::FIELD_MAX)));
        }
    }
    let suffix = rows().filter_map(FormRow::suffix).map(|u| meter.width(u, false)).max().unwrap_or(0);
    let tools = rows()
        .map(|r| {
            r.tools
                .iter()
                .map(|t| base.x(dlu::TOOL_W).max(meter.width(&t.title, false) + base.x(dlu::TOOL_PAD)) + base.x(dlu::TOOL_GAP))
                .sum::<i32>()
        })
        .max()
        .unwrap_or(0);
    let gap = if caption > 0 { base.x(dlu::COLUMN_GAP) } else { 0 };
    let mut inner = caption + gap + field;
    if suffix > 0 {
        inner += base.x(dlu::SUFFIX_GAP) + suffix;
    }
    if tools > 0 {
        inner += base.x(dlu::SUFFIX_GAP) + tools;
    }
    let pad = if tabbed { 2 * base.x(dlu::PAGE_PAD) } else { 0 };
    let inner = inner.max(base.x(dlu::MIN_INNER)).max(buttons_w - pad);
    Columns { caption, field, inner }
}

/// The dialog buttons in Windows order: whether the button is the default one, the button, its width.
fn button_row<'a>(
    buttons: &'a DialogButtons,
    base: DluBase,
    meter: &dyn TextMeter,
) -> Vec<(bool, &'a FormButton, i32)> {
    buttons
        .order(Platform::Windows)
        .into_iter()
        .map(|(role, b)| {
            let w = base.x(dlu::BUTTON_W).max(meter.width(&b.title, false) + base.x(dlu::BUTTON_PAD));
            (role == ButtonRole::Primary && b.enabled, b, w)
        })
        .collect()
}

/// Builds the layout of `spec`.
pub fn layout_spec(spec: &FormSpec, base: DluBase, meter: &dyn TextMeter) -> SpecLayout {
    let buttons = button_row(&spec.buttons, base, meter);
    let gap = base.x(dlu::BUTTON_GAP);
    let buttons_w =
        buttons.iter().map(|b| b.2).sum::<i32>() + gap * i32::try_from(buttons.len().saturating_sub(1)).unwrap_or(0);
    let tabs = spec.tabs();
    let tabbed = !tabs.is_empty();
    let margin = base.x(dlu::MARGIN);
    let pad = if tabbed { base.x(dlu::PAGE_PAD) } else { 0 };
    let cols = columns(spec, base, meter, buttons_w, tabbed);
    let client_w = 2 * margin + 2 * pad + cols.inner;
    let mut b = Builder { base, meter, out: Vec::new(), cols, x0: margin, tab: None };
    let mut y = margin;

    // sections that lie on no tab come first
    let mut first = true;
    for section in spec.sections.iter().filter(|s| s.tab.is_none()) {
        let below = b.section(section, y, first);
        first &= below == y;
        y = below;
    }
    if !first && tabbed {
        y += base.y(dlu::SECTION_GAP);
    }
    if tabbed {
        let strip_y = y;
        b.out.push(SpecControl {
            class: SpecClass::Tabs,
            id: None,
            rect: Rect { x: margin, y: strip_y, w: client_w - 2 * margin, h: 0 },
            text: String::new(),
            enabled: true,
            visible: true,
            tab: None,
        });
        let strip_index = b.out.len() - 1;
        let page_y = strip_y + base.y(dlu::TAB_HEADER) + base.y(dlu::PAGE_PAD);
        b.x0 = margin + pad;
        let mut page_bottom = page_y;
        for (index, title) in tabs.iter().enumerate() {
            b.tab = Some(index);
            let mut y = page_y;
            let mut first = true;
            for section in spec.sections.iter().filter(|s| s.tab.as_deref() == Some(title.as_str())) {
                let below = b.section(section, y, first);
                first &= below == y;
                y = below;
            }
            page_bottom = page_bottom.max(y);
        }
        b.tab = None;
        b.x0 = margin;
        let bottom = page_bottom + pad;
        b.out[strip_index].rect.h = bottom - strip_y;
        y = bottom + base.y(dlu::ROW_GAP);
    }

    if spec.notice_slot || spec.notice.is_some() {
        let text = spec.notice.clone().unwrap_or_default();
        let needed = wrap_lines(&text, client_w - 2 * margin, false, meter).max(dlu::NOTICE_LINES);
        let h = i32::try_from(needed).unwrap_or(i32::MAX) * base.y(dlu::TEXT);
        let r = Rect { x: margin, y, w: client_w - 2 * margin, h };
        b.push(SpecClass::Notice, None, r, &text, true, true);
        y += h + base.y(dlu::ROW_GAP);
    }

    let (mut default_button, mut cancel_button) = (None, None);
    if !buttons.is_empty() {
        y += base.y(dlu::ROW_GAP);
        let h = base.y(dlu::BUTTON_H);
        let mut x = client_w - margin - buttons_w;
        for (default, button, w) in &buttons {
            if *default {
                default_button = Some(button.id);
            }
            let class = SpecClass::DialogButton { default: *default };
            b.push(class, Some(button.id), Rect { x, y, w: *w, h }, &button.title, button.enabled, true);
            x += w + gap;
        }
        cancel_button = spec.buttons.cancel.as_ref().filter(|c| c.enabled).map(|c| c.id);
        y += h;
    }
    SpecLayout {
        title: spec.title.clone(),
        controls: b.out,
        client: (client_w, y + margin),
        tabs,
        default_button,
        cancel_button,
    }
}
