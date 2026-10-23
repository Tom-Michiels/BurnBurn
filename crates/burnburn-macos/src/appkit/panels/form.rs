//! The Mac form builder (LF5): turns a platform-free [`FormSpec`] into an `NSGridView` inside a
//! plain view, with the dialog buttons in a row below it. Captions are right-aligned in the first
//! column and share the first baseline of their field (the grid's row alignment), the unit is a
//! suffix label in the last column, help and errors sit under the field, the reason a section is
//! not available is said once under its heading, and all spacing comes from the 4/8-pt grid of
//! `burnburn_host_models::style::Space`. The primary button is rightmost and answers Return, the
//! cancel button answers Escape.
//!
//! The builder decides nothing about what the form says. [`FormView::update`] copies a new
//! description of the same shape into the widgets in place (never over a field the user is typing
//! in); a changed shape asks the caller to build again.

use burnburn_host_models::form::{ButtonRole, Control, FormId, FormSpec, Key, Platform};
use burnburn_host_models::style::{Space, StatusTone, TextRole};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSButton, NSColor, NSControl, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSGridCell,
    NSGridCellPlacement, NSGridRowAlignment, NSGridView, NSPopUpButton, NSScrollView, NSTextAlignment, NSTextField,
    NSTextView, NSView,
};
use objc2_foundation::{NSArray, NSRange, NSSize, NSString};

use crate::appkit::panels::widgets::{push_button, rect};

/// The height of a multi-line text row.
const MEMO_HEIGHT: f64 = 160.0;
/// The least width of a dialog button.
const BUTTON_MIN_WIDTH: f64 = 80.0;
/// The height of the button row.
const BUTTON_HEIGHT: f64 = 24.0;
/// The widest the caption column gets, as a share of the form.
const CAPTION_SHARE: f64 = 0.4;

/// Where the controls report to: the target object and the action it answers.
#[derive(Clone, Copy)]
pub struct Wiring<'a> {
    pub target: &'a AnyObject,
    pub action: Sel,
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
    help: Option<Retained<NSTextField>>,
    error: Option<Retained<NSTextField>>,
    /// The section the row belongs to.
    section: usize,
    /// What the model last put into the control.
    pushed: String,
}

/// A built form.
pub struct FormView {
    root: Retained<NSView>,
    rows: Vec<RowWidgets>,
    headings: Vec<Option<Retained<NSTextField>>>,
    /// Section reasons, by section.
    reasons: Vec<Option<Retained<NSTextField>>>,
    buttons: Vec<(ButtonRole, FormId, Retained<NSButton>)>,
    shape: Vec<String>,
    size: NSSize,
}

fn as_view<T: AsRef<NSView>>(v: &T) -> &NSView {
    v.as_ref()
}

#[allow(unsafe_code)]
fn wire(control: &NSControl, wiring: Option<Wiring<'_>>) {
    if let Some(w) = wiring {
        // SAFETY: the target is weak; the owner of the form keeps it and clears the targets
        // before it goes (`FormView::clear_targets`). `action` takes the sender (`v@:@`).
        unsafe {
            control.setTarget(Some(w.target));
            control.setAction(Some(w.action));
        }
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
    format!("{} {text}", StatusTone::Error.glyph())
}

fn text_width(label: &NSTextField) -> f64 {
    label.cell().map_or(0.0, |c| c.cellSize().width).ceil()
}

fn set_text(label: &NSTextField, text: &str) {
    if label.stringValue().to_string() != text {
        label.setStringValue(&NSString::from_str(text));
    }
}

/// Builds the form for a view `width` points wide. `wiring` is set on every control and button.
pub fn build_form(mtm: MainThreadMarker, spec: &FormSpec, width: f64, wiring: Option<Wiring<'_>>) -> FormView {
    let margin = Space::Group.points();
    let inner = width - 2.0 * margin;
    let col_gap = Space::Tight.points();

    // Column widths: the widest caption (capped), the widest unit, the rest for the controls.
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
    let unit_w = spec
        .sections
        .iter()
        .flat_map(|s| &s.rows)
        .filter_map(|r| r.suffix())
        .map(caption_probe)
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
    let mut headings = Vec::new();
    let mut reasons = Vec::new();
    let empty = || NSGridCell::emptyContentView(mtm);
    let mut first = true;
    for (si, section) in spec.sections.iter().enumerate() {
        let gap = if first { 0.0 } else { Space::Group.points() };
        let mut heading_label = None;
        if let Some(text) = &section.heading {
            let l = wrapping(mtm, text, inner, TextRole::Heading);
            let row = grid.addRowWithViews(&NSArray::from_slice(&[as_view(&l), &*empty(), &*empty()]));
            row.mergeCellsInRange(NSRange::new(0, 3));
            row.setRowAlignment(NSGridRowAlignment::None);
            row.setTopPadding(gap + if first { 0.0 } else { Space::Hairline.points() });
            first = false;
            heading_label = Some(l);
        }
        headings.push(heading_label);
        let mut reason_label = None;
        if let Some(text) = &section.unavailable {
            let l = wrapping(mtm, text, inner, TextRole::Caption);
            l.setTextColor(Some(&NSColor::secondaryLabelColor()));
            let row = grid.addRowWithViews(&NSArray::from_slice(&[as_view(&l), &*empty(), &*empty()]));
            row.mergeCellsInRange(NSRange::new(0, 3));
            row.setRowAlignment(NSGridRowAlignment::None);
            reason_label = Some(l);
        }
        reasons.push(reason_label);
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
                        // The grid shows the scroll view; the user works in its text view.
                        let _ = tv;
                    }
                    (Retained::into_super(scroll), Kind::Memo)
                }
                Control::Action { title } => {
                    let b = push_button(mtm, title);
                    wire(&b, wiring);
                    (Retained::into_super(Retained::into_super(b)), Kind::Action)
                }
                Control::ReadOnly { value } => {
                    let l = wrapping(mtm, value, control_w, TextRole::Body);
                    (Retained::into_super(Retained::into_super(l)), Kind::ReadOnly)
                }
            };
            let caption_view: Retained<NSView> = if matches!(kind, Kind::Check) || row.label.is_empty() {
                empty()
            } else {
                let l = wrapping(mtm, &row.label, caption_w, TextRole::Label);
                l.setAlignment(NSTextAlignment::Right);
                let v = Retained::into_super(Retained::into_super(l.clone()));
                caption = Some(l);
                v
            };
            let unit_view: Retained<NSView> = match row.suffix() {
                Some(suffix) => {
                    let l = NSTextField::labelWithString(&NSString::from_str(suffix), mtm);
                    l.setTextColor(Some(&NSColor::secondaryLabelColor()));
                    Retained::into_super(Retained::into_super(l))
                }
                None => empty(),
            };
            let grid_row = grid.addRowWithViews(&NSArray::from_slice(&[&*caption_view, &*control, &*unit_view]));
            grid_row.setTopPadding(if first { 0.0 } else { Space::Hairline.points() });
            first = false;
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
            if !section.row_enabled(row) {
                set_enabled(&control, kind, false);
            }
            let mut notes: [Option<Retained<NSTextField>>; 2] = [None, None];
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
                notes[slot] = Some(l);
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
                help,
                error,
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
    // Rows of a form without captions or units still leave the gap to the next column out.

    // Dialog buttons: help alone at the left, the rest right-aligned, primary rightmost.
    let ordered = spec.buttons.order(Platform::Mac);
    let mut buttons: Vec<(ButtonRole, FormId, Retained<NSButton>)> = Vec::new();
    for (role, spec_button) in &ordered {
        let b = push_button(mtm, &spec_button.title);
        b.setEnabled(spec_button.enabled);
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

    let fit = grid.fittingSize();
    let button_row = if buttons.is_empty() { 0.0 } else { BUTTON_HEIGHT + Space::Group.points() };
    let height = margin + button_row + fit.height + margin;
    let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, height));
    grid.setFrame(rect(margin, margin + button_row, inner, fit.height));
    root.addSubview(&grid);
    // Right to left from the edge; the help button keeps the left edge.
    let mut right = width - margin;
    for (role, _, b) in buttons.iter().rev() {
        let w = b.frame().size.width;
        if *role == ButtonRole::Help {
            b.setFrameOrigin(objc2_foundation::NSPoint::new(margin, margin));
        } else {
            right -= w;
            b.setFrameOrigin(objc2_foundation::NSPoint::new(right, margin));
            right -= Space::Control.points();
        }
        root.addSubview(b);
    }
    grid.layoutSubtreeIfNeeded();
    FormView { root, rows, headings, reasons, buttons, shape: spec.shape(), size: NSSize::new(width, height) }
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

    /// Copies `spec` into the widgets. Returns `false` (and changes nothing) when its shape differs.
    pub fn update(&mut self, spec: &FormSpec) -> bool {
        if !self.fits(spec) {
            return false;
        }
        for (si, section) in spec.sections.iter().enumerate() {
            if let (Some(Some(l)), Some(text)) = (self.headings.get(si), &section.heading) {
                set_text(l, text);
            }
            if let (Some(Some(l)), Some(text)) = (self.reasons.get(si), &section.unavailable) {
                set_text(l, text);
            }
        }
        let specs: Vec<(usize, &burnburn_host_models::form::FormRow)> =
            spec.sections.iter().enumerate().flat_map(|(si, s)| s.rows.iter().map(move |r| (si, r))).collect();
        for (widgets, (si, row)) in self.rows.iter_mut().zip(specs) {
            debug_assert_eq!(widgets.section, si);
            let enabled = spec.sections[si].row_enabled(row);
            set_enabled(&widgets.control, widgets.kind, enabled);
            if let Some(c) = &widgets.caption {
                set_text(c, &row.label);
            }
            if let (Some(l), Some(t)) = (&widgets.help, &row.help) {
                set_text(l, t);
            }
            if let (Some(l), Some(t)) = (&widgets.error, &row.error) {
                set_text(l, &error_text(t));
            }
            let new = pushed_of(&row.control);
            let any: &AnyObject = widgets.control.as_ref();
            match (&row.control, widgets.kind) {
                (Control::Text { value, .. } | Control::Number { value }, _) => {
                    if let Some(f) = any.downcast_ref::<NSTextField>()
                        && widgets.pushed != *value
                        && f.currentEditor().is_none()
                    {
                        f.setStringValue(&NSString::from_str(value));
                        widgets.pushed = new;
                    }
                }
                (Control::Choice { options, selected }, _) => {
                    if let Some(p) = any.downcast_ref::<NSPopUpButton>() {
                        for (i, o) in options.iter().enumerate() {
                            if let Some(item) = p.itemAtIndex(i as isize)
                                && item.title().to_string() != *o
                            {
                                item.setTitle(&NSString::from_str(o));
                            }
                        }
                        if widgets.pushed != new {
                            p.selectItemAtIndex(*selected as isize);
                            widgets.pushed = new;
                        }
                    }
                }
                (Control::Check { on }, _) => {
                    if let Some(b) = any.downcast_ref::<NSButton>() {
                        if widgets.pushed != new {
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
                        && widgets.pushed != *text
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
        }
        true
    }

    /// The texts the form shows (headings, reasons, captions, help, errors), for tests.
    pub fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let label = |l: &NSTextField| l.stringValue().to_string();
        for h in self.headings.iter().flatten() {
            out.push(label(h));
        }
        for r in self.reasons.iter().flatten() {
            out.push(label(r));
        }
        for row in &self.rows {
            for l in [&row.caption, &row.help, &row.error].into_iter().flatten() {
                out.push(label(l));
            }
        }
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
