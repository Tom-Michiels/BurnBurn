//! The Mac form builder (LF5, LF6a): turns a platform-free [`FormSpec`] into an `NSGridView` inside a
//! plain view, with the dialog buttons in a row below it. Captions are right-aligned in the first
//! column and share the first baseline of their field (the grid's row alignment), the unit is a
//! suffix label in the last column (small buttons beside a row stand there too), help and errors
//! sit under the field, the reason a section is not available is said once under its heading, and
//! all spacing comes from the 4/8-pt grid of `burnburn_host_models::style::Space`. The primary
//! button is rightmost and answers Return, the cancel button answers Escape.
//!
//! The builder decides nothing about what the form says. [`FormView::update`] copies a new
//! description of the same shape into the widgets in place (never over a field the user is typing
//! in); a changed shape asks the caller to build again. A row can be hidden and an empty help,
//! error, reason or read-only text shows nothing, so a form that keeps its shape can still come and
//! go; a section with nothing to show loses its heading. A form can be capped in height: the rows
//! then scroll above the button row.

use burnburn_host_models::form::{
    ButtonRole, Control, FormId, FormRow, FormSection, FormSpec, Key, Platform,
};
use burnburn_host_models::style::{Space, StatusTone, TextRole};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSAccessibility, NSButton, NSColor, NSControl, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSGridCell,
    NSGridCellPlacement, NSGridRow, NSGridRowAlignment, NSGridView, NSPopUpButton, NSScrollView, NSStackView,
    NSTextAlignment, NSTextField, NSTextView, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRange, NSSize, NSString};

use crate::appkit::panels::widgets::{push_button, rect};

/// The height of a multi-line text row.
const MEMO_HEIGHT: f64 = 160.0;
/// The least width of a dialog button.
const BUTTON_MIN_WIDTH: f64 = 80.0;
/// The height of the button row.
const BUTTON_HEIGHT: f64 = 24.0;
/// The width of a small button beside a row.
const TOOL_WIDTH: f64 = 34.0;
/// The widest the caption column gets, as a share of the form.
const CAPTION_SHARE: f64 = 0.4;

/// Where the controls report to: the target object and the action it answers.
#[derive(Clone, Copy)]
pub struct Wiring<'a> {
    pub target: &'a AnyObject,
    pub action: Sel,
}

/// How tall a form is allowed to get.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormLayout {
    /// As tall as the rows need.
    Natural,
    /// As tall as the rows need, up to this height; the rows scroll above the button row beyond it.
    Capped(f64),
    /// Always this tall; the rows scroll above the button row when they do not fit.
    Fixed(f64),
}

/// What a row holds, for reading and updating it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Choice,
    Check,
    Memo,
    Action,
    ReadOnly,
}

/// The widgets of one row.
struct RowWidgets {
    id: FormId,
    kind: Kind,
    /// The view the user works in (the text view of a memo).
    control: Retained<NSView>,
    caption: Option<Retained<NSTextField>>,
    unit: Option<Retained<NSTextField>>,
    help: Option<Retained<NSTextField>>,
    error: Option<Retained<NSTextField>>,
    tools: Vec<(FormId, Retained<NSButton>)>,
    /// The grid rows of the control and of its help and error lines.
    row: Retained<NSGridRow>,
    help_row: Option<Retained<NSGridRow>>,
    error_row: Option<Retained<NSGridRow>>,
    /// The section the row belongs to.
    section: usize,
    /// What the model last put into the control.
    pushed: String,
}

/// The heading and the reason line of one section, with their grid rows.
#[derive(Default)]
struct SectionWidgets {
    heading: Option<(Retained<NSTextField>, Retained<NSGridRow>)>,
    reason: Option<(Retained<NSTextField>, Retained<NSGridRow>)>,
}

/// Where the grid lies.
enum Body {
    /// Straight in the root view.
    Direct,
    /// In a scroll view above the button row.
    Scroll { scroll: Retained<NSScrollView>, document: Retained<NSView> },
}

/// A built form.
pub struct FormView {
    root: Retained<NSView>,
    grid: Retained<NSGridView>,
    body: Body,
    layout: FormLayout,
    rows: Vec<RowWidgets>,
    sections: Vec<SectionWidgets>,
    buttons: Vec<(ButtonRole, FormId, Retained<NSButton>)>,
    shape: Vec<String>,
    size: NSSize,
    margin: f64,
    inner: f64,
    button_row: f64,
}

fn as_view<T: AsRef<NSView>>(v: &T) -> &NSView {
    v.as_ref()
}

#[allow(unsafe_code)]
fn wire(control: &NSControl, wiring: Option<Wiring<'_>>) {
    if let Some(w) = wiring {
        // SAFETY: the target is weak; the owner of the form keeps it and clears the targets
        // before it goes (`FormView::clear_targets`). `action` takes the sender (`v@:@`).
        unsafe { control.setTarget(Some(w.target)) };
        // SAFETY: as above.
        unsafe { control.setAction(Some(w.action)) };
    }
}

fn wrapping(mtm: MainThreadMarker, text: &str, width: f64, role: TextRole) -> Retained<NSTextField> {
    let l = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    l.setSelectable(false);
    l.setPreferredMaxLayoutWidth(width);
    let size = role.size();
    let font = if role.bold() { NSFont::boldSystemFontOfSize(size) } else { NSFont::systemFontOfSize(size) };
    l.setFont(Some(&font));
    l
}

fn error_text(text: &str) -> String {
    if text.is_empty() { String::new() } else { format!("{} {text}", StatusTone::Error.glyph()) }
}

fn text_width(label: &NSTextField) -> f64 {
    label.cell().map_or(0.0, |c| c.cellSize().width).ceil()
}

fn set_text(label: &NSTextField, text: &str) {
    if label.stringValue().to_string() != text {
        label.setStringValue(&NSString::from_str(text));
    }
}

fn set_hidden(row: &NSGridRow, hidden: bool) {
    if row.isHidden() != hidden {
        row.setHidden(hidden);
    }
}

/// A row that shows nothing: hidden by its description, or a read-only text without words.
fn row_hidden(row: &FormRow) -> bool {
    row.hidden || matches!(&row.control, Control::ReadOnly { value } if value.is_empty() && row.label.is_empty())
}

/// A section shows its heading when one of its rows shows something.
fn section_visible(section: &FormSection) -> bool {
    section.rows.is_empty()
        || section.rows.iter().any(|r| !row_hidden(r) || (!r.hidden && r.error.as_ref().is_some_and(|e| !e.is_empty())))
}

fn section_reason(section: &FormSection) -> &str {
    section.unavailable.as_deref().or(section.reason.as_deref()).unwrap_or("")
}

/// Builds the form for a view `width` points wide. `wiring` is set on every control and button.
pub fn build_form(mtm: MainThreadMarker, spec: &FormSpec, width: f64, wiring: Option<Wiring<'_>>) -> FormView {
    build_form_with(mtm, spec, width, wiring, FormLayout::Natural)
}

/// [`build_form`] with a limit on the height.
pub fn build_form_with(
    mtm: MainThreadMarker,
    spec: &FormSpec,
    width: f64,
    wiring: Option<Wiring<'_>>,
    layout: FormLayout,
) -> FormView {
    let margin = Space::Group.points();
    let inner = width - 2.0 * margin;
    let col_gap = Space::Tight.points();

    // Column widths: the widest caption (capped), the widest unit or row of small buttons, the rest
    // for the controls.
    let caption_probe = |text: &str| {
        let l = NSTextField::labelWithString(&NSString::from_str(text), mtm);
        l.setFont(Some(&NSFont::systemFontOfSize(TextRole::Label.size())));
        text_width(&l)
    };
    let caption_w = spec
        .sections
        .iter()
        .flat_map(|s| &s.rows)
        .filter(|r| !matches!(r.control, Control::Check { .. }))
        .map(|r| caption_probe(&r.label))
        .fold(0.0_f64, f64::max)
        .min(inner * CAPTION_SHARE);
    let tools_width = |n: usize| if n == 0 { 0.0 } else { n as f64 * TOOL_WIDTH + (n as f64 - 1.0) * Space::Hairline.points() };
    let unit_w = spec
        .sections
        .iter()
        .flat_map(|s| &s.rows)
        .map(|r| r.suffix().map_or(0.0, caption_probe).max(tools_width(r.tools.len())))
        .fold(0.0_f64, f64::max);
    let has_captions = caption_w > 0.0;
    let has_units = unit_w > 0.0;
    // Text labels and fields draw 2 pt outside their alignment rect: the grid keeps that inside.
    let edge_pad = 2.0;
    let control_w = inner
        - edge_pad
        - if has_captions { caption_w + col_gap } else { 0.0 }
        - if has_units { unit_w + col_gap } else { 0.0 };

    let grid = NSGridView::gridViewWithNumberOfColumns_rows(3, 0, mtm);
    grid.setRowSpacing(Space::Hairline.points());
    grid.setColumnSpacing(col_gap);
    grid.setRowAlignment(NSGridRowAlignment::FirstBaseline);

    let mut rows: Vec<RowWidgets> = Vec::new();
    let mut sections: Vec<SectionWidgets> = Vec::new();
    let empty = || NSGridCell::emptyContentView(mtm);
    let mut first = true;
    for (si, section) in spec.sections.iter().enumerate() {
        let gap = if first { 0.0 } else { Space::Group.points() };
        let mut widgets = SectionWidgets::default();
        if let Some(text) = &section.heading {
            let l = wrapping(mtm, text, inner, TextRole::Heading);
            let row = grid.addRowWithViews(&NSArray::from_slice(&[as_view(&l), &*empty(), &*empty()]));
            row.mergeCellsInRange(NSRange::new(0, 3));
            row.setRowAlignment(NSGridRowAlignment::None);
            row.setTopPadding(gap + if first { 0.0 } else { Space::Hairline.points() });
            first = false;
            widgets.heading = Some((l, row));
        }
        if section.unavailable.is_some() || section.reason.is_some() {
            let l = wrapping(mtm, section_reason(section), inner, TextRole::Caption);
            l.setTextColor(Some(&NSColor::secondaryLabelColor()));
            let row = grid.addRowWithViews(&NSArray::from_slice(&[as_view(&l), &*empty(), &*empty()]));
            row.mergeCellsInRange(NSRange::new(0, 3));
            row.setRowAlignment(NSGridRowAlignment::None);
            widgets.reason = Some((l, row));
        }
        sections.push(widgets);
        for row in &section.rows {
            let mut caption = None;
            let (control, kind): (Retained<NSView>, Kind) = match &row.control {
                Control::Text { value, placeholder } => {
                    let f = text_field(mtm, value, wiring);
                    f.setPlaceholderString(Some(&NSString::from_str(placeholder)));
                    (Retained::into_super(Retained::into_super(f)), Kind::Text)
                }
                Control::Number { value } => {
                    let f = text_field(mtm, value, wiring);
                    f.setAlignment(NSTextAlignment::Right);
                    (Retained::into_super(Retained::into_super(f)), Kind::Text)
                }
                Control::Choice { options, selected } => {
                    let p = NSPopUpButton::initWithFrame_pullsDown(
                        NSPopUpButton::alloc(mtm),
                        rect(0.0, 0.0, control_w, 26.0),
                        false,
                    );
                    // Greyed entries are the model's to say, not AppKit's.
                    p.setAutoenablesItems(false);
                    for o in options {
                        p.addItemWithTitle(&NSString::from_str(o));
                    }
                    p.selectItemAtIndex(*selected as isize);
                    wire(&p, wiring);
                    (Retained::into_super(Retained::into_super(Retained::into_super(p))), Kind::Choice)
                }
                Control::Check { on } => {
                    let b = check_box(mtm, &row.label);
                    b.setState(if *on { NSControlStateValueOn } else { NSControlStateValueOff });
                    wire(&b, wiring);
                    (Retained::into_super(Retained::into_super(b)), Kind::Check)
                }
                Control::Memo { text } => {
                    let scroll = NSTextView::scrollableTextView(mtm);
                    scroll.setHasVerticalScroller(true);
                    scroll.setBorderType(objc2_app_kit::NSBorderType::BezelBorder);
                    if let Some(tv) = memo_text_view(&scroll) {
                        tv.setEditable(true);
                        tv.setRichText(false);
                        tv.setFont(Some(&NSFont::systemFontOfSize(TextRole::Body.size())));
                        tv.setString(&NSString::from_str(text));
                    }
                    (Retained::into_super(scroll), Kind::Memo)
                }
                Control::Action { title } => {
                    let b = push_button(mtm, title);
                    wire(&b, wiring);
                    (Retained::into_super(Retained::into_super(b)), Kind::Action)
                }
                Control::ReadOnly { value } => {
                    // Words without a caption run over the whole width of the form.
                    let spans = row.label.is_empty() && has_captions;
                    let l = wrapping(mtm, value, if spans { inner - edge_pad } else { control_w }, TextRole::Body);
                    (Retained::into_super(Retained::into_super(l)), Kind::ReadOnly)
                }
            };
            let spans = kind == Kind::ReadOnly && row.label.is_empty() && has_captions;
            let caption_view: Retained<NSView> = if matches!(kind, Kind::Check) || row.label.is_empty() {
                empty()
            } else {
                let l = wrapping(mtm, &row.label, caption_w, TextRole::Label);
                l.setAlignment(NSTextAlignment::Right);
                let v = Retained::into_super(Retained::into_super(l.clone()));
                caption = Some(l);
                v
            };
            // Label for the screen reader and the tooltip, on the control that is not a checkbox.
            if !matches!(kind, Kind::Check | Kind::ReadOnly | Kind::Action) && !row.label.is_empty() {
                control.setAccessibilityLabel(Some(&NSString::from_str(&row.label)));
            }
            if let Some(tip) = &row.tooltip {
                control.setToolTip(Some(&NSString::from_str(tip)));
            }
            let mut unit_label = None;
            let mut tools: Vec<(FormId, Retained<NSButton>)> = Vec::new();
            let unit_view: Retained<NSView> = if !row.tools.is_empty() {
                for t in &row.tools {
                    let b = push_button(mtm, &t.title);
                    b.setFrame(rect(0.0, 0.0, TOOL_WIDTH, BUTTON_HEIGHT));
                    b.setAccessibilityLabel(Some(&NSString::from_str(&t.label)));
                    b.setEnabled(t.enabled);
                    wire(&b, wiring);
                    tools.push((t.id, b));
                }
                let views: Vec<&NSView> = tools.iter().map(|(_, b)| as_view(&**b)).collect();
                let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(&views), mtm);
                stack.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
                stack.setSpacing(Space::Hairline.points());
                Retained::into_super(stack)
            } else {
                match row.suffix() {
                    Some(suffix) => {
                        let l = NSTextField::labelWithString(&NSString::from_str(suffix), mtm);
                        l.setTextColor(Some(&NSColor::secondaryLabelColor()));
                        unit_label = Some(l.clone());
                        Retained::into_super(Retained::into_super(l))
                    }
                    None => empty(),
                }
            };
            let views: [&NSView; 3] = if spans {
                [&control, &*empty(), &*empty()]
            } else {
                [&caption_view, &control, &unit_view]
            };
            let grid_row = grid.addRowWithViews(&NSArray::from_slice(&views));
            grid_row.setTopPadding(if first { 0.0 } else { Space::Hairline.points() });
            first = false;
            if spans {
                grid_row.mergeCellsInRange(NSRange::new(0, 3));
                grid_row.setRowAlignment(NSGridRowAlignment::None);
                if let Some(cell) = grid.cellForView(&control) {
                    cell.setXPlacement(NSGridCellPlacement::Leading);
                }
            } else {
                if matches!(kind, Kind::Memo) {
                    grid_row.setRowAlignment(NSGridRowAlignment::None);
                    grid_row.setHeight(MEMO_HEIGHT);
                }
                if let Some(cell) = grid.cellForView(&caption_view) {
                    cell.setXPlacement(NSGridCellPlacement::Trailing);
                    if matches!(kind, Kind::Memo) {
                        cell.setYPlacement(NSGridCellPlacement::Top);
                    }
                }
                if let Some(cell) = grid.cellForView(&control) {
                    match kind {
                        Kind::Text | Kind::Choice => cell.setXPlacement(NSGridCellPlacement::Fill),
                        Kind::Memo => {
                            cell.setXPlacement(NSGridCellPlacement::Fill);
                            cell.setYPlacement(NSGridCellPlacement::Fill);
                        }
                        Kind::Check | Kind::Action | Kind::ReadOnly => cell.setXPlacement(NSGridCellPlacement::Leading),
                    }
                }
                if let Some(cell) = grid.cellForView(&unit_view) {
                    cell.setXPlacement(NSGridCellPlacement::Leading);
                }
            }
            if !section.row_enabled(row) {
                set_enabled(&control, kind, false);
            }
            let mut notes: [Option<(Retained<NSTextField>, Retained<NSGridRow>)>; 2] = [None, None];
            for (slot, text) in [(0, row.help.clone()), (1, row.error.as_ref().map(|e| error_text(e)))] {
                let Some(text) = text else { continue };
                let l = wrapping(mtm, &text, control_w, TextRole::Caption);
                let color = if slot == 1 { NSColor::systemRedColor() } else { NSColor::secondaryLabelColor() };
                l.setTextColor(Some(&color));
                let note = grid.addRowWithViews(&NSArray::from_slice(&[&*empty(), as_view(&l), &*empty()]));
                note.mergeCellsInRange(NSRange::new(1, 2));
                note.setRowAlignment(NSGridRowAlignment::None);
                if let Some(cell) = grid.cellForView(as_view(&l)) {
                    cell.setXPlacement(NSGridCellPlacement::Leading);
                }
                notes[slot] = Some((l, note));
            }
            let [help, error] = notes;
            rows.push(RowWidgets {
                id: row.id,
                kind,
                pushed: pushed_of(&row.control),
                control: match kind {
                    Kind::Memo => memo_view(&control),
                    _ => control,
                },
                caption,
                unit: unit_label,
                help: help.as_ref().map(|(l, _)| l.clone()),
                error: error.as_ref().map(|(l, _)| l.clone()),
                tools,
                row: grid_row,
                help_row: help.map(|(_, r)| r),
                error_row: error.map(|(_, r)| r),
                section: si,
            });
        }
    }

    // Columns.
    if grid.numberOfColumns() == 3 {
        let (c0, c1, c2) = (grid.columnAtIndex(0), grid.columnAtIndex(1), grid.columnAtIndex(2));
        c0.setWidth(caption_w);
        c1.setWidth(control_w);
        c2.setWidth(unit_w);
        c0.setHidden(!has_captions);
        (if has_captions { &c0 } else { &c1 }).setLeadingPadding(edge_pad);
        c2.setHidden(!has_units);
    }

    // Dialog buttons: help alone at the left, the rest right-aligned, primary rightmost.
    let ordered = spec.buttons.order(Platform::Mac);
    let mut buttons: Vec<(ButtonRole, FormId, Retained<NSButton>)> = Vec::new();
    for (role, spec_button) in &ordered {
        let b = push_button(mtm, &spec_button.title);
        b.setEnabled(spec_button.enabled);
        if let Some(tip) = &spec_button.tooltip {
            b.setToolTip(Some(&NSString::from_str(tip)));
        }
        wire(&b, wiring);
        match role.key() {
            Some(Key::Return) => b.setKeyEquivalent(&NSString::from_str("\r")),
            Some(Key::Escape) => b.setKeyEquivalent(&NSString::from_str("\u{1b}")),
            None => {}
        }
        b.sizeToFit();
        let w = b.frame().size.width.max(BUTTON_MIN_WIDTH);
        b.setFrame(rect(0.0, 0.0, w, BUTTON_HEIGHT));
        buttons.push((*role, spec_button.id, b));
    }
    let button_row = if buttons.is_empty() { 0.0 } else { BUTTON_HEIGHT + Space::Group.points() };

    let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, 1.0));
    let body = if layout == FormLayout::Natural {
        root.addSubview(&grid);
        Body::Direct
    } else {
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, width, 1.0));
        scroll.setHasVerticalScroller(true);
        scroll.setHasHorizontalScroller(false);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, 1.0));
        document.addSubview(&grid);
        scroll.setDocumentView(Some(&document));
        root.addSubview(&scroll);
        Body::Scroll { scroll, document }
    };
    // Right to left from the edge; the help button keeps the left edge.
    let mut right = width - margin;
    for (role, _, b) in buttons.iter().rev() {
        let w = b.frame().size.width;
        if *role == ButtonRole::Help {
            b.setFrameOrigin(NSPoint::new(margin, margin));
        } else {
            right -= w;
            b.setFrameOrigin(NSPoint::new(right, margin));
            right -= Space::Control.points();
        }
        root.addSubview(b);
    }
    let mut view = FormView {
        root,
        grid,
        body,
        layout,
        rows,
        sections,
        buttons,
        shape: spec.shape(),
        size: NSSize::new(width, 1.0),
        margin,
        inner,
        button_row,
    };
    view.apply_state(spec, true);
    view.relayout();
    view.scroll_to_top();
    view
}

fn text_field(mtm: MainThreadMarker, value: &str, wiring: Option<Wiring<'_>>) -> Retained<NSTextField> {
    let f = NSTextField::textFieldWithString(&NSString::from_str(value), mtm);
    // Commit on Return and on end of editing, as every other field of the host does.
    if let Some(cell) = f.cell() {
        cell.setSendsActionOnEndEditing(true);
    }
    wire(&f, wiring);
    f
}

#[allow(unsafe_code)]
fn check_box(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    // SAFETY: no target and no action yet; `wire` sets both.
    unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(title), None, None, mtm) }
}

fn memo_text_view(scroll: &NSScrollView) -> Option<Retained<NSTextView>> {
    scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok())
}

/// The text view inside the scroll view of a memo row (the view the user types in).
fn memo_view(scroll: &Retained<NSView>) -> Retained<NSView> {
    scroll
        .clone()
        .downcast::<NSScrollView>()
        .ok()
        .and_then(|s| memo_text_view(&s))
        .map_or_else(|| scroll.clone(), |t| Retained::into_super(Retained::into_super(t)))
}

fn pushed_of(control: &Control) -> String {
    match control {
        Control::Text { value, .. } | Control::Number { value } | Control::ReadOnly { value } => value.clone(),
        Control::Choice { selected, .. } => selected.to_string(),
        Control::Check { on } => if *on { "1" } else { "0" }.to_owned(),
        Control::Memo { text } => text.clone(),
        Control::Action { title } => title.clone(),
    }
}

fn set_enabled(control: &NSView, kind: Kind, enabled: bool) {
    let any: &AnyObject = control.as_ref();
    if let Some(c) = any.downcast_ref::<NSControl>() {
        c.setEnabled(enabled);
    } else if let Some(t) = any.downcast_ref::<NSTextView>() {
        t.setEditable(enabled);
        t.setSelectable(enabled || kind == Kind::Memo);
    }
}

impl FormView {
    /// The widget of a row (a memo is its text view), of any kind.
    pub fn control(&self, id: FormId) -> Option<Retained<NSView>> {
        self.rows.iter().find(|r| r.id == id).map(|r| r.control.clone())
    }

    /// The help line under a row, even when it is empty and hidden.
    pub fn help_label(&self, id: FormId) -> Option<Retained<NSTextField>> {
        self.rows.iter().find(|r| r.id == id).and_then(|r| r.help.clone())
    }

    /// The error line under a row, even when it is empty and hidden.
    pub fn error_label(&self, id: FormId) -> Option<Retained<NSTextField>> {
        self.rows.iter().find(|r| r.id == id).and_then(|r| r.error.clone())
    }

    /// The heading of a section.
    pub fn heading_label(&self, section: usize) -> Option<Retained<NSTextField>> {
        self.sections.get(section).and_then(|s| s.heading.as_ref().map(|(l, _)| l.clone()))
    }

    /// The line that says why rows of a section are greyed.
    pub fn reason_label(&self, section: usize) -> Option<Retained<NSTextField>> {
        self.sections.get(section).and_then(|s| s.reason.as_ref().map(|(l, _)| l.clone()))
    }

    /// A small button beside a row.
    pub fn tool(&self, id: FormId) -> Option<Retained<NSButton>> {
        self.rows.iter().flat_map(|r| &r.tools).find(|(i, _)| *i == id).map(|(_, b)| b.clone())
    }

    /// The view to put in a window or panel.
    pub fn root(&self) -> &NSView {
        &self.root
    }

    pub fn size(&self) -> NSSize {
        self.size
    }

    /// The dialog buttons from left to right as they lie.
    pub fn buttons(&self) -> Vec<(ButtonRole, FormId, Retained<NSButton>)> {
        self.buttons.clone()
    }

    /// Every control and button with its id (a memo is its text view).
    pub fn controls(&self) -> Vec<(FormId, Retained<NSView>)> {
        let mut out: Vec<(FormId, Retained<NSView>)> =
            self.rows.iter().filter(|r| r.kind != Kind::ReadOnly).map(|r| (r.id, r.control.clone())).collect();
        out.extend(self.buttons.iter().map(|(_, id, b)| (*id, Retained::into_super(Retained::into_super(b.clone())))));
        out
    }

    /// Nil every target (they are weak) before the target object goes.
    #[allow(unsafe_code)]
    pub fn clear_targets(&self) {
        let mut controls: Vec<Retained<NSControl>> = Vec::new();
        for r in &self.rows {
            let any: &AnyObject = r.control.as_ref();
            if let Some(c) = any.downcast_ref::<NSControl>() {
                controls.push(Message::retain(c));
            }
            for (_, b) in &r.tools {
                controls.push(Retained::into_super(b.clone()));
            }
        }
        for (_, _, b) in &self.buttons {
            controls.push(Retained::into_super(b.clone()));
        }
        for c in controls {
            // SAFETY: a nil target cannot dangle.
            unsafe { c.setTarget(None) };
        }
    }

    /// Whether `spec` has the shape this form was built for.
    pub fn fits(&self, spec: &FormSpec) -> bool {
        spec.shape() == self.shape
    }

    /// Scrolls the rows to their top.
    pub fn scroll_to_top(&self) {
        if let Body::Scroll { document, .. } = &self.body {
            document.scrollPoint(NSPoint::new(0.0, document.frame().size.height));
        }
    }

    /// Copies `spec` into the widgets. Returns `false` (and changes nothing) when its shape differs.
    pub fn update(&mut self, spec: &FormSpec) -> bool {
        if !self.fits(spec) {
            return false;
        }
        self.apply_state(spec, false);
        self.relayout();
        true
    }

    /// Puts every value of `spec` into the controls, also over what the user typed (a dialog that
    /// opens again). Returns `false` and changes nothing when the shape differs.
    pub fn load(&mut self, spec: &FormSpec) -> bool {
        if !self.fits(spec) {
            return false;
        }
        self.apply_state(spec, true);
        self.relayout();
        true
    }

    /// Texts, values, enablement and what shows, from `spec`. `force` also replaces what the user
    /// typed.
    fn apply_state(&mut self, spec: &FormSpec, force: bool) {
        for (si, section) in spec.sections.iter().enumerate() {
            let visible = section_visible(section);
            let Some(widgets) = self.sections.get(si) else { continue };
            if let (Some((l, row)), Some(text)) = (&widgets.heading, &section.heading) {
                set_text(l, text);
                set_hidden(row, !visible);
            }
            if let Some((l, row)) = &widgets.reason {
                let text = section_reason(section);
                set_text(l, text);
                set_hidden(row, !visible || text.is_empty());
            }
        }
        let specs: Vec<(usize, &FormRow)> =
            spec.sections.iter().enumerate().flat_map(|(si, s)| s.rows.iter().map(move |r| (si, r))).collect();
        for (widgets, (si, row)) in self.rows.iter_mut().zip(specs) {
            debug_assert_eq!(widgets.section, si);
            let enabled = spec.sections[si].row_enabled(row);
            set_enabled(&widgets.control, widgets.kind, enabled);
            set_hidden(&widgets.row, row_hidden(row));
            if let Some(c) = &widgets.caption {
                set_text(c, &row.label);
            }
            if let (Some(l), Some(suffix)) = (&widgets.unit, row.suffix()) {
                set_text(l, suffix);
            }
            if let (Some(l), Some(r)) = (&widgets.help, &widgets.help_row) {
                let text = row.help.clone().unwrap_or_default();
                set_text(l, &text);
                set_hidden(r, row.hidden || text.is_empty());
            }
            if let (Some(l), Some(r)) = (&widgets.error, &widgets.error_row) {
                let text = row.error.as_deref().map(error_text).unwrap_or_default();
                set_text(l, &text);
                set_hidden(r, row.hidden || text.is_empty());
            }
            for (tool, (_, button)) in row.tools.iter().zip(&widgets.tools) {
                button.setEnabled(tool.enabled);
                if button.accessibilityLabel().map(|a| a.to_string()).as_deref() != Some(tool.label.as_str()) {
                    button.setAccessibilityLabel(Some(&NSString::from_str(&tool.label)));
                }
            }
            let tip = row.tooltip.as_deref().map(NSString::from_str);
            widgets.control.setToolTip(tip.as_deref());
            let new = pushed_of(&row.control);
            let any: &AnyObject = widgets.control.as_ref();
            match (&row.control, widgets.kind) {
                (Control::Text { value, .. } | Control::Number { value }, _) => {
                    if let Some(f) = any.downcast_ref::<NSTextField>()
                        && (force || (widgets.pushed != *value && f.currentEditor().is_none()))
                    {
                        if f.stringValue().to_string() != *value {
                            f.setStringValue(&NSString::from_str(value));
                        }
                        widgets.pushed = new;
                    }
                }
                (Control::Choice { options, selected }, _) => {
                    if let Some(p) = any.downcast_ref::<NSPopUpButton>() {
                        for (i, o) in options.iter().enumerate() {
                            if let Some(item) = p.itemAtIndex(i as isize) {
                                if item.title().to_string() != *o {
                                    item.setTitle(&NSString::from_str(o));
                                }
                                let disabled = row.disabled_options.iter().find(|(d, _)| *d == i);
                                item.setEnabled(disabled.is_none());
                                let tip = disabled.and_then(|(_, r)| r.as_deref()).map(NSString::from_str);
                                item.setToolTip(tip.as_deref());
                            }
                        }
                        if force || widgets.pushed != new {
                            p.selectItemAtIndex(*selected as isize);
                            widgets.pushed = new;
                        }
                    }
                }
                (Control::Check { on }, _) => {
                    if let Some(b) = any.downcast_ref::<NSButton>() {
                        if force || widgets.pushed != new {
                            b.setState(if *on { NSControlStateValueOn } else { NSControlStateValueOff });
                            widgets.pushed = new;
                        }
                        if b.title().to_string() != row.label {
                            b.setTitle(&NSString::from_str(&row.label));
                        }
                    }
                }
                (Control::Memo { text }, _) => {
                    // What the user typed stays until the stored text changes.
                    if let Some(t) = any.downcast_ref::<NSTextView>()
                        && (force || widgets.pushed != *text)
                    {
                        t.setString(&NSString::from_str(text));
                        widgets.pushed = new;
                    }
                }
                (Control::Action { title }, _) => {
                    if let Some(b) = any.downcast_ref::<NSButton>()
                        && b.title().to_string() != *title
                    {
                        b.setTitle(&NSString::from_str(title));
                    }
                }
                (Control::ReadOnly { value }, _) => {
                    if let Some(l) = any.downcast_ref::<NSTextField>() {
                        set_text(l, value);
                    }
                }
            }
        }
        for ((_, b), (_, _, widget)) in spec.buttons.order(Platform::Mac).into_iter().zip(&self.buttons) {
            if widget.title().to_string() != b.title {
                widget.setTitle(&NSString::from_str(&b.title));
            }
            widget.setEnabled(b.enabled);
            let tip = b.tooltip.as_deref().map(NSString::from_str);
            widget.setToolTip(tip.as_deref());
        }
    }

    /// Sizes the grid to its rows and the root to the grid (or to its limit); the rows scroll when
    /// they do not fit.
    fn relayout(&mut self) {
        let (margin, width) = (self.margin, self.root.frame().size.width);
        let bottom = margin + self.button_row;
        self.grid.setFrame(rect(margin, bottom, self.inner, 1.0));
        self.grid.layoutSubtreeIfNeeded();
        let fit = self.grid.fittingSize().height;
        let natural = bottom + fit + margin;
        let height = match self.layout {
            FormLayout::Natural => natural,
            FormLayout::Capped(max) => natural.min(max),
            FormLayout::Fixed(h) => h,
        };
        self.root.setFrameSize(NSSize::new(width, height));
        match &self.body {
            Body::Direct => self.grid.setFrame(rect(margin, bottom, self.inner, fit)),
            Body::Scroll { scroll, document } => {
                let visible = height - bottom;
                scroll.setFrame(rect(0.0, bottom, width, visible));
                let doc_h = (fit + margin).max(visible);
                document.setFrame(rect(0.0, 0.0, width, doc_h));
                self.grid.setFrame(rect(margin, doc_h - margin - fit, self.inner, fit));
            }
        }
        self.size = NSSize::new(width, height);
    }

    /// The texts the form shows (headings, reasons, captions, help, errors), for tests.
    pub fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let label = |l: &NSTextField| l.stringValue().to_string();
        for s in &self.sections {
            if let Some((h, _)) = &s.heading {
                out.push(label(h));
            }
        }
        for s in &self.sections {
            if let Some((r, _)) = &s.reason {
                out.push(label(r));
            }
        }
        for row in &self.rows {
            for l in [&row.caption, &row.help, &row.error].into_iter().flatten() {
                out.push(label(l));
            }
        }
        out.retain(|t| !t.is_empty());
        out
    }

    /// For each text or memo row the text now in the control and what the model last put there.
    pub fn edits(&self) -> Vec<(FormId, String, String)> {
        let now = self.values();
        self.rows
            .iter()
            .filter(|r| matches!(r.kind, Kind::Text | Kind::Memo))
            .filter_map(|r| now.iter().find(|(id, _)| *id == r.id).map(|(_, v)| (r.id, v.clone(), r.pushed.clone())))
            .collect()
    }

    /// Puts what the user typed into a text or memo row of a rebuilt form, and remembers what the
    /// model had put there, so the next `update` leaves the typing alone.
    pub fn restore(&mut self, id: FormId, typed: &str, pushed: &str) {
        let Some(row) = self.rows.iter_mut().find(|r| r.id == id) else { return };
        let any: &AnyObject = row.control.as_ref();
        if let Some(f) = any.downcast_ref::<NSTextField>() {
            f.setStringValue(&NSString::from_str(typed));
        } else if let Some(t) = any.downcast_ref::<NSTextView>() {
            t.setString(&NSString::from_str(typed));
        } else {
            return;
        }
        row.pushed = pushed.to_owned();
    }

    /// What the user has typed and chosen: text, the chosen index, `1`/`0` for a checkbox.
    pub fn values(&self) -> Vec<(FormId, String)> {
        self.rows
            .iter()
            .filter_map(|r| {
                let any: &AnyObject = r.control.as_ref();
                match r.kind {
                    Kind::Text => any.downcast_ref::<NSTextField>().map(|f| (r.id, f.stringValue().to_string())),
                    Kind::Choice => {
                        any.downcast_ref::<NSPopUpButton>().map(|p| (r.id, p.indexOfSelectedItem().to_string()))
                    }
                    Kind::Memo => any.downcast_ref::<NSTextView>().map(|t| (r.id, t.string().to_string())),
                    Kind::Check => any
                        .downcast_ref::<NSButton>()
                        .map(|b| (r.id, if b.state() == NSControlStateValueOn { "1" } else { "0" }.to_owned())),
                    Kind::Action | Kind::ReadOnly => None,
                }
            })
            .collect()
    }
}
