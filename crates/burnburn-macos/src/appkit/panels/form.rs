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
//!
//! LF6c adds: tabs (sections with the same `tab` share a grid in an `NSTabView`; a form with tabs
//! is as tall as its tallest tab), one notice for the whole form above the buttons, read-only
//! scrolling text (`Control::Log`), pictures, a refused value in the error colour, wrapping check
//! boxes, and buttons wider than the control column that take the columns beside them.
//!
//! LF6b adds: a note (`Control::Note`, a wrapping text that keeps room for its lines), a report, a
//! preview picture and a row of buttons; rows and sections hidden by the host at run time
//! ([`FormView::set_hidden`], [`FormView::set_section_hidden`]) for panels that keep their own
//! widgets and copy a view model into them; and [`FormView::relayout`] to fit the form again.

use burnburn_host_models::form::{ButtonRole, Control, FormId, FormRow, FormSection, FormSpec, Key, Platform};
use burnburn_host_models::style::{Space, StatusTone, TextRole, toned_text};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility, NSButton, NSColor, NSControl, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSGridCell,
    NSGridCellPlacement, NSGridRow, NSGridRowAlignment, NSGridView, NSImageFrameStyle, NSImageScaling, NSImageView,
    NSLayoutAttribute, NSLayoutConstraint, NSLayoutRelation, NSLineBreakMode, NSPopUpButton, NSScrollView, NSStackView,
    NSTabView, NSTabViewItem, NSTextAlignment, NSTextField, NSTextView, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRange, NSSize, NSString};

use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::image::show_bitmap;
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
/// Between the edge of the tab view and its content: the border of the tab view plus air.
const TAB_INSET: f64 = 16.0;
/// A tab page is this much taller than its grid, above and below.
const TAB_PAD: f64 = 12.0;
/// The room kept for a notice that comes and goes: two lines of caption.
const NOTICE_SLOT_HEIGHT: f64 = 32.0;
/// The least height of a check box row.
const CHECK_HEIGHT: f64 = 18.0;
/// The height of a read-only scrolling text row.
const LOG_HEIGHT: f64 = 120.0;

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
    Log,
    Picture,
    /// A wrapping read-only text (status, hint, issue).
    Note,
    /// A read-only multi-line text with a scroll bar.
    Report,
    /// A picture the host fills in.
    Preview,
    /// Several buttons in one row.
    Actions,
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
    /// The buttons of a row of buttons, and the view that places them.
    button_row: Option<(Retained<NSView>, Vec<Retained<NSButton>>)>,
    /// Hidden by the host at run time, on top of what the description says.
    forced_hidden: bool,
    /// Whether the description hides the row.
    spec_hidden: bool,
}

/// The heading and the reason line of one section, with their grid rows.
#[derive(Default)]
struct SectionWidgets {
    heading: Option<(Retained<NSTextField>, Retained<NSGridRow>)>,
    reason: Option<(Retained<NSTextField>, Retained<NSGridRow>)>,
    /// Hidden by the host at run time: heading, reason and every row.
    forced_hidden: bool,
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
    /// The grids: one, or one per tab.
    grids: Vec<Retained<NSGridView>>,
    body: Body,
    /// The one notice of the form.
    notice: Option<Retained<NSTextField>>,
    notice_slot: bool,
    tab_view: Option<Retained<NSTabView>>,
    chrome_h: f64,
    inner_g: f64,
    mtm: MainThreadMarker,
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

/// The height of one line of the system font at `size`.
fn line_height(size: f64) -> f64 {
    let font = NSFont::systemFontOfSize(size);
    (font.ascender() - font.descender() + font.leading()).ceil()
}

#[allow(unsafe_code)]
fn constrain(view: &NSView, attribute: NSLayoutAttribute, relation: NSLayoutRelation, value: f64) {
    // SAFETY: a one-item constraint on a view that outlives it (the constraint belongs to the view).
    let constraint = unsafe {
        NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
            view.as_ref(),
            attribute,
            relation,
            None,
            NSLayoutAttribute::NotAnAttribute,
            1.0,
            value,
        )
    };
    constraint.setActive(true);
}

/// Asks `view` to be at least `height` points high (a note or a report keeps room for the lines it
/// is promised, and grows with a longer text).
fn at_least_high(view: &NSView, height: f64) {
    constrain(view, NSLayoutAttribute::Height, NSLayoutRelation::GreaterThanOrEqual, height);
}

/// Asks `view` to be exactly `width` points wide.
fn exactly_wide(view: &NSView, width: f64) {
    constrain(view, NSLayoutAttribute::Width, NSLayoutRelation::Equal, width);
}

/// Asks `view` to be exactly `width` by `height` points.
fn exactly_sized(view: &NSView, width: f64, height: f64) {
    exactly_wide(view, width);
    constrain(view, NSLayoutAttribute::Height, NSLayoutRelation::Equal, height);
}

fn tone_color(tone: StatusTone) -> Retained<NSColor> {
    match tone {
        StatusTone::Neutral => NSColor::secondaryLabelColor(),
        StatusTone::Warning => NSColor::systemOrangeColor(),
        StatusTone::Error => NSColor::systemRedColor(),
    }
}

/// Lays `buttons` out left to right at their fitted width (never under the least width), one gap
/// apart; when they do not fit in `width` they share it equally.
fn layout_buttons(buttons: &[Retained<NSButton>], width: f64) {
    let gap = Space::Tight.points();
    let mut widths: Vec<f64> = buttons
        .iter()
        .map(|b| {
            b.sizeToFit();
            b.frame().size.width.max(BUTTON_MIN_WIDTH)
        })
        .collect();
    let total = widths.iter().sum::<f64>() + gap * buttons.len().saturating_sub(1) as f64;
    if total > width && !buttons.is_empty() {
        let each = ((width - gap * (buttons.len() - 1) as f64) / buttons.len() as f64).floor();
        widths.iter_mut().for_each(|w| *w = each);
    }
    let mut x = 0.0;
    for (b, w) in buttons.iter().zip(widths) {
        b.setFrame(rect(x, 0.0, w, BUTTON_HEIGHT));
        x += w + gap;
    }
}

fn as_any(v: &NSView) -> &AnyObject {
    v.as_ref()
}

fn caption_color(invalid: bool) -> Retained<NSColor> {
    if invalid { NSColor::systemRedColor() } else { NSColor::labelColor() }
}

/// The text of a note: in a warning or error tone it starts with the tone's glyph, so the colour
/// is never the only carrier (LF15). A bold title note keeps the label colour and needs none.
fn note_text(tone: StatusTone, strong: bool, text: &str) -> String {
    if strong { text.to_owned() } else { toned_text(tone, text) }
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
    // A form with tabs draws its grids inside the tab view (and is as tall as its tallest tab).
    let tabbed = !spec.tabs().is_empty();
    let inner_g = if tabbed { inner - 2.0 * TAB_INSET } else { inner };
    let layout = if tabbed { FormLayout::Natural } else { layout };

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
        .filter(|r| !matches!(r.control, Control::Check { .. }) && !r.label.is_empty())
        .map(|r| caption_probe(&r.label))
        .fold(0.0_f64, f64::max)
        .min(inner_g * CAPTION_SHARE);
    let tools_width =
        |n: usize| if n == 0 { 0.0 } else { n as f64 * TOOL_WIDTH + (n as f64 - 1.0) * Space::Hairline.points() };
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
    let control_w = inner_g
        - edge_pad
        - if has_captions { caption_w + col_gap } else { 0.0 }
        - if has_units { unit_w + col_gap } else { 0.0 };

    let new_grid = || {
        let grid = NSGridView::gridViewWithNumberOfColumns_rows(3, 0, mtm);
        grid.setRowSpacing(Space::Hairline.points());
        grid.setColumnSpacing(col_gap);
        grid.setRowAlignment(NSGridRowAlignment::FirstBaseline);
        grid
    };

    let mut rows: Vec<RowWidgets> = Vec::new();
    let mut sections: Vec<SectionWidgets> = Vec::new();
    let empty = || NSGridCell::emptyContentView(mtm);
    // A row that runs under the captions is one merged cell. Its content lives in the first cell of
    // the range, and a hidden column hides its cells' content: without captions the first column
    // is hidden, so the range starts at the control column then.
    let first_col: usize = if has_captions { 0 } else { 1 };
    let merged_views = |view: &NSView| -> Vec<Retained<NSView>> {
        let owned = Some(view.retain());
        let (a, b) = (empty(), empty());
        match owned {
            Some(v) if first_col == 0 => vec![v, a, b],
            Some(v) => vec![a, v, b],
            None => vec![a, b],
        }
    };
    // Consecutive sections of one tab share a grid; a form without tabs has one.
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for (si, section) in spec.sections.iter().enumerate() {
        match groups.last_mut() {
            Some((tab, ids)) if *tab == section.tab => ids.push(si),
            _ => groups.push((section.tab.clone(), vec![si])),
        }
    }
    if groups.is_empty() {
        groups.push((None, Vec::new()));
    }
    let mut grids: Vec<(Option<String>, Retained<NSGridView>)> = Vec::new();
    for (tab_title, indices) in groups {
        let grid = new_grid();
        let mut first = true;
        for si in indices {
            let section = &spec.sections[si];
            let gap = if first { 0.0 } else { Space::Group.points() };
            let mut widgets = SectionWidgets::default();
            if let Some(text) = &section.heading {
                let l = wrapping(mtm, text, inner_g, TextRole::Heading);
                let views = merged_views(as_view(&l));
                let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
                let row = grid.addRowWithViews(&NSArray::from_slice(&refs));
                row.mergeCellsInRange(NSRange::new(first_col, 3 - first_col));
                row.setRowAlignment(NSGridRowAlignment::None);
                row.setTopPadding(gap + if first { 0.0 } else { Space::Hairline.points() });
                first = false;
                widgets.heading = Some((l, row));
            }
            if section.unavailable.is_some() || section.reason.is_some() {
                let l = wrapping(mtm, section_reason(section), inner_g, TextRole::Caption);
                l.setTextColor(Some(&NSColor::secondaryLabelColor()));
                let views = merged_views(as_view(&l));
                let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
                let row = grid.addRowWithViews(&NSArray::from_slice(&refs));
                row.mergeCellsInRange(NSRange::new(first_col, 3 - first_col));
                row.setRowAlignment(NSGridRowAlignment::None);
                widgets.reason = Some((l, row));
            }
            sections.push(widgets);
            for row in &section.rows {
                let mut caption = None;
                // A note, picture, report or button row without a caption runs under the captions
                // too. A merged cell keeps its content at its own size, so a report and a button
                // row are given the width of the span.
                let wide = row.label.is_empty()
                    && matches!(
                        row.control,
                        Control::Note { .. }
                            | Control::Preview { .. }
                            | Control::Report { .. }
                            | Control::Actions { .. }
                    );
                let cw = if wide { inner_g - edge_pad } else { control_w };
                let mut button_row = None;
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
                        // A wrapping label draws 2 pt outside its alignment rect on both sides.
                        let l = wrapping(
                            mtm,
                            value,
                            if spans { inner_g - edge_pad - 4.0 } else { control_w - 4.0 },
                            TextRole::Body,
                        );
                        (Retained::into_super(Retained::into_super(l)), Kind::ReadOnly)
                    }
                    Control::Log { text } => {
                        let scroll = NSTextView::scrollableTextView(mtm);
                        scroll.setHasVerticalScroller(true);
                        scroll.setBorderType(objc2_app_kit::NSBorderType::BezelBorder);
                        if let Some(tv) = memo_text_view(&scroll) {
                            tv.setEditable(false);
                            tv.setSelectable(true);
                            tv.setFont(Some(&NSFont::systemFontOfSize(TextRole::Body.size())));
                            tv.setString(&NSString::from_str(text));
                        }
                        (Retained::into_super(scroll), Kind::Log)
                    }
                    Control::Note { value, lines, tone, strong } => {
                        let size = TextRole::Body.size();
                        let l = wrapping(mtm, &note_text(*tone, *strong, value), cw - 4.0, TextRole::Body);
                        if *strong {
                            l.setFont(Some(&NSFont::boldSystemFontOfSize(size)));
                        }
                        let color = if *strong { NSColor::labelColor() } else { tone_color(*tone) };
                        l.setTextColor(Some(&color));
                        at_least_high(&l, f64::from(*lines) * line_height(size));
                        (Retained::into_super(Retained::into_super(l)), Kind::Note)
                    }
                    Control::Report { text, lines } => {
                        let scroll = NSTextView::scrollableTextView(mtm);
                        scroll.setHasVerticalScroller(true);
                        scroll.setBorderType(objc2_app_kit::NSBorderType::BezelBorder);
                        if let Some(tv) = memo_text_view(&scroll) {
                            tv.setEditable(false);
                            tv.setSelectable(true);
                            tv.setRichText(false);
                            tv.setFont(Some(&NSFont::systemFontOfSize(TextRole::Body.size())));
                            tv.setString(&NSString::from_str(text));
                        }
                        at_least_high(&scroll, f64::from(*lines) * line_height(TextRole::Body.size()) + 8.0);
                        if wide {
                            exactly_wide(&scroll, cw);
                        }
                        (Retained::into_super(scroll), Kind::Report)
                    }
                    Control::Preview { width, height } => {
                        let (w, h) = (f64::from(*width), f64::from(*height));
                        let v = NSImageView::initWithFrame(NSImageView::alloc(mtm), rect(0.0, 0.0, w, h));
                        v.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                        v.setImageFrameStyle(NSImageFrameStyle::GrayBezel);
                        // The gray bezel takes 3 pt on every side out of the alignment rect.
                        exactly_sized(&v, w - 6.0, h - 6.0);
                        (Retained::into_super(Retained::into_super(v)), Kind::Preview)
                    }
                    Control::Actions { titles } => {
                        let container = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, cw, BUTTON_HEIGHT));
                        let buttons: Vec<Retained<NSButton>> = titles
                            .iter()
                            .map(|title| {
                                let b = push_button(mtm, title);
                                wire(&b, wiring);
                                container.addSubview(&b);
                                b
                            })
                            .collect();
                        layout_buttons(&buttons, cw);
                        if wide {
                            exactly_wide(&container, cw);
                        }
                        button_row = Some((container.clone(), buttons));
                        (container, Kind::Actions)
                    }
                    Control::Picture { image } => {
                        let v = NSImageView::initWithFrame(NSImageView::alloc(mtm), rect(0.0, 0.0, 96.0, 96.0));
                        v.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                        v.setImageFrameStyle(objc2_app_kit::NSImageFrameStyle::GrayBezel);
                        show_bitmap(&v, &mut None, Some(image));
                        (Retained::into_super(Retained::into_super(v)), Kind::Picture)
                    }
                };
                let spans = kind == Kind::ReadOnly && row.label.is_empty() && has_captions;
                let caption_view: Retained<NSView> = if matches!(kind, Kind::Check) || row.label.is_empty() {
                    empty()
                } else {
                    let l = wrapping(mtm, &row.label, caption_w, TextRole::Label);
                    l.setAlignment(NSTextAlignment::Right);
                    l.setTextColor(Some(&caption_color(row.invalid)));
                    let v = Retained::into_super(Retained::into_super(l.clone()));
                    caption = Some(l);
                    v
                };
                // A row without a caption is named by its section's heading, else by the form's title.
                let accessible_name = if row.label.is_empty() {
                    section.heading.clone().unwrap_or_else(|| spec.title.clone())
                } else {
                    row.label.clone()
                };
                // Label for the screen reader and the tooltip, on the control that is not a checkbox.
                if !matches!(
                    kind,
                    Kind::Check
                        | Kind::ReadOnly
                        | Kind::Action
                        | Kind::Log
                        | Kind::Picture
                        | Kind::Note
                        | Kind::Report
                        | Kind::Preview
                        | Kind::Actions
                ) && !accessible_name.is_empty()
                {
                    // A memo is read through its text view, not the scroll view around it. The
                    // caption is its title element as well, so VoiceOver ties the two (LF15).
                    let named = if kind == Kind::Memo { memo_view(&control) } else { control.clone() };
                    named.setAccessibilityLabel(Some(&NSString::from_str(&accessible_name)));
                    if let Some(c) = &caption {
                        let title: &AnyObject = c.as_ref();
                        // SAFETY: the link is unretained; the caption lives in the same row as the control.
                        unsafe { named.setAccessibilityTitleUIElement(Some(title)) };
                    }
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
                // A button wider than both columns beside its caption takes the whole row.
                let beside_w = control_w + if has_units { col_gap + unit_w } else { 0.0 };
                let action_need = if kind == Kind::Action { control.fittingSize().width.ceil() } else { 0.0 };
                let whole_row = action_need > beside_w;
                let check_need = if kind == Kind::Check {
                    as_any(&control)
                        .downcast_ref::<NSButton>()
                        .and_then(|b| b.cell())
                        .map_or(0.0, |c| c.cellSize().width.ceil())
                } else {
                    0.0
                };
                let merged;
                let views: [&NSView; 3] = if wide {
                    merged = merged_views(&control);
                    [&*merged[0], &*merged[1], &*merged[2]]
                } else if spans || whole_row {
                    [&control, &*empty(), &*empty()]
                } else {
                    [&caption_view, &control, &unit_view]
                };
                let grid_row = grid.addRowWithViews(&NSArray::from_slice(&views));
                grid_row.setTopPadding(if first { 0.0 } else { Space::Hairline.points() });
                first = false;
                if wide {
                    grid_row.mergeCellsInRange(NSRange::new(first_col, 3 - first_col));
                    grid_row.setRowAlignment(NSGridRowAlignment::None);
                    if matches!(kind, Kind::Actions) {
                        grid_row.setHeight(BUTTON_HEIGHT);
                    }
                    if let Some(cell) = grid.cellForView(&control) {
                        cell.setXPlacement(NSGridCellPlacement::Leading);
                        cell.setYPlacement(if matches!(kind, Kind::Report | Kind::Actions) {
                            NSGridCellPlacement::Fill
                        } else {
                            NSGridCellPlacement::Top
                        });
                    }
                } else if whole_row {
                    grid_row.mergeCellsInRange(NSRange::new(0, 3));
                    grid_row.setRowAlignment(NSGridRowAlignment::None);
                    if let Some(cell) = grid.cellForView(&control) {
                        cell.setXPlacement(NSGridCellPlacement::Leading);
                    }
                    if action_need > inner_g
                        && let Some(b) = as_any(&control).downcast_ref::<NSButton>()
                    {
                        // A title wider than the whole form is cut with a tooltip, never silently.
                        b.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
                        b.setToolTip(Some(&b.title()));
                    }
                } else if spans {
                    grid_row.mergeCellsInRange(NSRange::new(0, 3));
                    grid_row.setRowAlignment(NSGridRowAlignment::None);
                    if let Some(cell) = grid.cellForView(&control) {
                        cell.setXPlacement(NSGridCellPlacement::Leading);
                    }
                } else {
                    if matches!(kind, Kind::Note | Kind::Report | Kind::Preview | Kind::Actions) {
                        grid_row.setRowAlignment(NSGridRowAlignment::None);
                        if kind == Kind::Actions {
                            grid_row.setHeight(BUTTON_HEIGHT);
                        }
                    } else if matches!(kind, Kind::Memo | Kind::Log) {
                        grid_row.setRowAlignment(NSGridRowAlignment::None);
                        grid_row.setHeight(if kind == Kind::Log { LOG_HEIGHT } else { MEMO_HEIGHT });
                    } else if kind == Kind::Picture {
                        grid_row.setRowAlignment(NSGridRowAlignment::None);
                    } else if kind == Kind::Action && action_need > control_w {
                        grid_row.mergeCellsInRange(NSRange::new(1, 2));
                    } else if kind == Kind::Check && check_need > control_w - 2.0 {
                        // A title that does not fit takes the control and the unit column and wraps there.
                        if let Some(b) = as_any(&control).downcast_ref::<NSButton>() {
                            b.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
                            if let Some(cell) = b.cell() {
                                cell.setWraps(true);
                            }
                        }
                        grid_row.mergeCellsInRange(NSRange::new(1, 2));
                        let wanted = as_any(&control)
                            .downcast_ref::<NSButton>()
                            .and_then(|b| b.cell())
                            .map_or(0.0, |c| c.cellSizeForBounds(rect(0.0, 0.0, beside_w, 10_000.0)).height.ceil());
                        grid_row.setHeight(wanted.max(CHECK_HEIGHT));
                    }
                    if let Some(cell) = grid.cellForView(&caption_view) {
                        cell.setXPlacement(NSGridCellPlacement::Trailing);
                        if matches!(kind, Kind::Memo | Kind::Log | Kind::Picture | Kind::Report | Kind::Preview) {
                            cell.setYPlacement(NSGridCellPlacement::Top);
                        }
                    }
                    if let Some(cell) = grid.cellForView(&control) {
                        match kind {
                            Kind::Text | Kind::Choice => cell.setXPlacement(NSGridCellPlacement::Fill),
                            Kind::Memo | Kind::Log | Kind::Report | Kind::Actions => {
                                cell.setXPlacement(NSGridCellPlacement::Fill);
                                cell.setYPlacement(NSGridCellPlacement::Fill);
                            }
                            Kind::Note | Kind::Preview => {
                                cell.setXPlacement(NSGridCellPlacement::Leading);
                                cell.setYPlacement(NSGridCellPlacement::Top);
                            }
                            Kind::Picture => {
                                cell.setXPlacement(NSGridCellPlacement::Leading);
                                cell.setYPlacement(NSGridCellPlacement::Top);
                            }
                            Kind::Check if check_need > control_w - 2.0 => {
                                cell.setXPlacement(NSGridCellPlacement::Fill)
                            }
                            Kind::Check | Kind::Action | Kind::ReadOnly => {
                                cell.setXPlacement(NSGridCellPlacement::Leading)
                            }
                        }
                    }
                    if let Some(cell) = grid.cellForView(&unit_view) {
                        cell.setXPlacement(NSGridCellPlacement::Leading);
                    }
                }
                if !section.row_enabled(row) {
                    set_enabled(&control, kind, false);
                    if let Some((_, buttons)) = &button_row {
                        buttons.iter().for_each(|b| b.setEnabled(false));
                    }
                }
                let mut notes: [Option<(Retained<NSTextField>, Retained<NSGridRow>)>; 2] = [None, None];
                for (slot, text) in [(0, row.help.clone()), (1, row.error.as_ref().map(|e| error_text(e)))] {
                    let Some(text) = text else { continue };
                    let l = wrapping(mtm, &text, control_w - 4.0, TextRole::Caption);
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
                        Kind::Memo | Kind::Log | Kind::Report => memo_view(&control),
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
                    button_row,
                    forced_hidden: false,
                    spec_hidden: false,
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
        grids.push((tab_title, grid));
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

    // The notice of the form: above the buttons, whatever tab is open. With a slot the room for
    // two lines is kept while there is no notice.
    let notice = (spec.notice.is_some() || spec.notice_slot).then(|| {
        let text = spec.notice.as_deref().map(error_text).unwrap_or_default();
        let l = wrapping(mtm, &text, inner, TextRole::Caption);
        l.setTextColor(Some(&NSColor::systemRedColor()));
        l
    });

    let root = FormRoot::new(mtm, rect(0.0, 0.0, width, 1.0));
    let mut chrome_h = 0.0;
    let mut tab_view = None;
    let body = if tabbed {
        let tv = NSTabView::initWithFrame(NSTabView::alloc(mtm), rect(0.0, 0.0, inner, 300.0));
        for (title, _) in &grids {
            // SAFETY: a fresh item without an identifier object.
            let item = unsafe { NSTabViewItem::initWithIdentifier(NSTabViewItem::alloc(), None) };
            item.setLabel(&NSString::from_str(title.as_deref().unwrap_or("")));
            item.setView(Some(&NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 1.0, 1.0))));
            tv.addTabViewItem(&item);
        }
        // The border and the strip of tabs around the content, measured on the real control.
        chrome_h = 300.0 - tv.contentRect().size.height;
        let page_w = tv.contentRect().size.width;
        for ((_, grid), item) in grids.iter().zip(tv.tabViewItems().iter()) {
            let page = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, page_w, 1.0));
            page.addSubview(grid);
            item.setView(Some(&page));
        }
        root.addSubview(&tv);
        tab_view = Some(tv);
        Body::Direct
    } else if layout == FormLayout::Natural {
        if let Some((_, grid)) = grids.first() {
            root.addSubview(grid);
        }
        Body::Direct
    } else {
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, width, 1.0));
        scroll.setHasVerticalScroller(true);
        scroll.setHasHorizontalScroller(false);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        let document = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, 1.0));
        if let Some((_, grid)) = grids.first() {
            document.addSubview(grid);
        }
        scroll.setDocumentView(Some(&document));
        root.addSubview(&scroll);
        Body::Scroll { scroll, document }
    };
    if let Some(l) = &notice {
        root.addSubview(l);
    }
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
        root: Retained::into_super(root),
        grids: grids.into_iter().map(|(_, g)| g).collect(),
        body,
        notice,
        notice_slot: spec.notice_slot,
        tab_view,
        chrome_h,
        inner_g,
        mtm,
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
    view.relayout(0.0);
    view.scroll_to_top();
    view.link_key_views();
    view
}

/// The first control of a form, which the root remembers (`previousKeyView` of that control is the
/// last control, so the root cannot hold it in a key-view link).
#[derive(Default)]
struct FirstStop(std::cell::RefCell<Option<Retained<NSView>>>);

define_class!(
    /// The root view of a built form (LF15). When the form arrives in a window that has no first
    /// responder yet, its first control becomes the window's `initialFirstResponder`: Tab then
    /// starts at the top of the form.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnFormRoot"]
    #[ivars = FirstStop]
    struct FormRoot;

    impl FormRoot {
        #[unsafe(method(viewDidMoveToWindow))]
        fn view_did_move_to_window(&self) {
            ffi_guard("viewDidMoveToWindow", (), || {
                // SAFETY: NSView's own implementation, no arguments.
                let _: () = unsafe { msg_send![super(self), viewDidMoveToWindow] };
                self.offer_initial();
            });
        }
    }
);

impl FormRoot {
    /// Offers the first control a user can reach as the window's `initialFirstResponder`, unless
    /// another form of the window already holds it. Rows come and go, so this is repeated when the
    /// form changes (`FormView::relayout`).
    fn offer_initial(&self) {
        let Some(window) = self.window() else { return };
        let first = self.ivars().0.borrow().clone();
        // AppKit may have picked a first responder by itself; the first form to arrive keeps
        // the window's, a later form of the same window does not take it over.
        let in_another_form = window.initialFirstResponder().filter(|c| c.window().is_some()).is_some_and(|c| {
            // SAFETY: reads of the view hierarchy on the main thread.
            let mut up = unsafe { c.superview() };
            while let Some(p) = up {
                if p.downcast_ref::<FormRoot>().is_some_and(|f| !std::ptr::eq(f, self)) {
                    return true;
                }
                // SAFETY: as above.
                up = unsafe { p.superview() };
            }
            false
        });
        // The first control a user can reach: rows may be hidden or switched off.
        let start = first.and_then(|first| {
            let mut at = first.clone();
            for _ in 0..400 {
                let usable =
                    !at.isHiddenOrHasHiddenAncestor() && at.downcast_ref::<NSControl>().is_none_or(|c| c.isEnabled());
                if usable {
                    return Some(at);
                }
                // SAFETY: a read of the key-view link between this form's own views.
                let next = unsafe { at.nextKeyView() }?;
                if std::ptr::eq(&*next, &*first) {
                    return None;
                }
                at = next;
            }
            None
        });
        if let (Some(start), false) = (start, in_another_form) {
            window.setInitialFirstResponder(Some(&start));
        }
    }

    fn new(mtm: MainThreadMarker, frame: objc2_foundation::NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(FirstStop::default());
        // SAFETY: NSView's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
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
        Control::Log { text } => text.clone(),
        Control::Picture { image } => format!("{}x{}", image.width, image.height),
        Control::Note { value, .. } => value.clone(),
        Control::Report { text, .. } => text.clone(),
        Control::Preview { .. } | Control::Actions { .. } => String::new(),
    }
}

fn set_enabled(control: &NSView, kind: Kind, enabled: bool) {
    let any: &AnyObject = control.as_ref();
    if let Some(c) = any.downcast_ref::<NSControl>() {
        c.setEnabled(enabled);
    } else if let Some(t) = any.downcast_ref::<NSTextView>() {
        // A log or a report is read-only whatever the row says; its text stays selectable.
        t.setEditable(enabled && !matches!(kind, Kind::Log | Kind::Report));
        t.setSelectable(enabled || matches!(kind, Kind::Memo | Kind::Log | Kind::Report));
    }
}

impl FormView {
    /// The controls Tab visits, in reading order: row by row (a control, then the small buttons
    /// beside it; captions, notes, pictures and read-only texts are skipped), the dialog buttons
    /// last.
    pub fn key_stops(&self) -> Vec<Retained<NSView>> {
        let mut stops: Vec<Retained<NSView>> = Vec::new();
        for r in &self.rows {
            match r.kind {
                Kind::Text | Kind::Choice | Kind::Check | Kind::Memo | Kind::Action => {
                    stops.push(r.control.clone());
                }
                Kind::Actions => {
                    if let Some((_, buttons)) = &r.button_row {
                        stops.extend(buttons.iter().map(|b| Retained::into_super(Retained::into_super(b.clone()))));
                    }
                }
                Kind::ReadOnly | Kind::Log | Kind::Picture | Kind::Note | Kind::Report | Kind::Preview => {}
            }
            stops.extend(r.tools.iter().map(|(_, b)| Retained::into_super(Retained::into_super(b.clone()))));
        }
        stops.extend(self.buttons.iter().map(|(_, _, b)| Retained::into_super(Retained::into_super(b.clone()))));
        stops
    }

    fn offer_initial(&self) {
        if let Some(root) = self.root.downcast_ref::<FormRoot>() {
            root.offer_initial();
        }
    }

    /// Links the stops into a loop (`nextKeyView`, the last back to the first) and tells the root
    /// which is the first. Hidden and disabled controls stay in the loop; AppKit skips them.
    #[allow(unsafe_code)]
    fn link_key_views(&self) {
        let stops = self.key_stops();
        let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return };
        for pair in stops.windows(2) {
            // SAFETY: the links are unretained; every stop is a view of this form, which its rows
            // and buttons keep alive as long as the form exists.
            unsafe { pair[0].setNextKeyView(Some(&pair[1])) };
        }
        // SAFETY: as above.
        unsafe { last.setNextKeyView(Some(first)) };
        if let Some(root) = self.root.downcast_ref::<FormRoot>() {
            *root.ivars().0.borrow_mut() = Some(first.clone());
        }
    }

    /// The widget of a row (a memo is its text view), of any kind.
    pub fn control(&self, id: FormId) -> Option<Retained<NSView>> {
        if let Some(r) = self.rows.iter().find(|r| r.id == id) {
            return match &r.button_row {
                Some((_, buttons)) => buttons.first().map(|b| Retained::into_super(Retained::into_super(b.clone()))),
                None => Some(r.control.clone()),
            };
        }
        // A row of buttons answers its first button as `id` and the next ones as `id + 1`, ….
        self.rows.iter().find_map(|r| {
            let (_, buttons) = r.button_row.as_ref()?;
            let index = usize::try_from(id.0.checked_sub(r.id.0)?).ok()?;
            buttons.get(index).map(|b| Retained::into_super(Retained::into_super(b.clone())))
        })
    }

    /// The caption of a row, when it has one.
    pub fn caption(&self, id: FormId) -> Option<Retained<NSTextField>> {
        self.rows.iter().find(|r| r.id == id).and_then(|r| r.caption.clone())
    }

    /// Hides or shows a row with its caption, unit, help and error, on top of what the description
    /// says. The space comes back only after [`FormView::relayout`].
    pub fn set_hidden(&mut self, id: FormId, hidden: bool) {
        if let Some(r) = self.rows.iter_mut().find(|r| r.id == id) {
            r.forced_hidden = hidden;
        }
        self.apply_forced();
    }

    /// Hides or shows a whole section: heading, reason and rows.
    pub fn set_section_hidden(&mut self, section: usize, hidden: bool) {
        if let Some(s) = self.sections.get_mut(section) {
            s.forced_hidden = hidden;
        }
        self.apply_forced();
    }

    /// Puts the run-time hiding over the grid rows, and gives a row back what its description says.
    fn apply_forced(&self) {
        self.offer_initial();
        for (si, s) in self.sections.iter().enumerate() {
            let any_shown = self.rows.iter().filter(|r| r.section == si).any(|r| !r.spec_hidden);
            if let Some((_, row)) = &s.heading {
                set_hidden(row, s.forced_hidden || !any_shown);
            }
            if let Some((l, row)) = &s.reason {
                set_hidden(row, s.forced_hidden || !any_shown || l.stringValue().to_string().is_empty());
            }
        }
        for r in &self.rows {
            let section_hidden = self.sections.get(r.section).is_some_and(|s| s.forced_hidden);
            let hide = r.forced_hidden || section_hidden;
            set_hidden(&r.row, hide || r.spec_hidden);
            if let (Some(l), Some(row)) = (&r.help, &r.help_row) {
                set_hidden(row, hide || l.stringValue().to_string().is_empty());
            }
            if let (Some(l), Some(row)) = (&r.error, &r.error_row) {
                set_hidden(row, hide || l.stringValue().to_string().is_empty());
            }
        }
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
        let mut out: Vec<(FormId, Retained<NSView>)> = self
            .rows
            .iter()
            .filter(|r| {
                !matches!(
                    r.kind,
                    Kind::ReadOnly
                        | Kind::Log
                        | Kind::Picture
                        | Kind::Note
                        | Kind::Report
                        | Kind::Preview
                        | Kind::Actions
                )
            })
            .map(|r| (r.id, r.control.clone()))
            .collect();
        for r in &self.rows {
            if let Some((_, buttons)) = &r.button_row {
                out.extend(buttons.iter().enumerate().map(|(i, b)| {
                    (
                        FormId(r.id.0 + u32::try_from(i).unwrap_or(0)),
                        Retained::into_super(Retained::into_super(b.clone())),
                    )
                }));
            }
        }
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
            if let Some((_, buttons)) = &r.button_row {
                controls.extend(buttons.iter().map(|b| Retained::into_super(b.clone())));
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
        self.relayout(0.0);
        true
    }

    /// Puts every value of `spec` into the controls, also over what the user typed (a dialog that
    /// opens again). Returns `false` and changes nothing when the shape differs.
    pub fn load(&mut self, spec: &FormSpec) -> bool {
        if !self.fits(spec) {
            return false;
        }
        self.apply_state(spec, true);
        self.relayout(0.0);
        true
    }

    /// Texts, values, enablement and what shows, from `spec`. `force` also replaces what the user
    /// typed.
    fn apply_state(&mut self, spec: &FormSpec, force: bool) {
        if let Some(l) = &self.notice {
            set_text(l, &spec.notice.as_deref().map(error_text).unwrap_or_default());
        }
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
            widgets.spec_hidden = row_hidden(row);
            let forced = widgets.forced_hidden || self.sections.get(si).is_some_and(|s| s.forced_hidden);
            set_hidden(&widgets.row, widgets.spec_hidden || forced);
            if let Some(c) = &widgets.caption {
                set_text(c, &row.label);
                c.setTextColor(Some(&caption_color(row.invalid)));
            }
            if let (Some(l), Some(suffix)) = (&widgets.unit, row.suffix()) {
                set_text(l, suffix);
            }
            if let (Some(l), Some(r)) = (&widgets.help, &widgets.help_row) {
                let text = row.help.clone().unwrap_or_default();
                set_text(l, &text);
                set_hidden(r, row.hidden || text.is_empty() || forced);
            }
            if let (Some(l), Some(r)) = (&widgets.error, &widgets.error_row) {
                let text = row.error.as_deref().map(error_text).unwrap_or_default();
                set_text(l, &text);
                set_hidden(r, row.hidden || text.is_empty() || forced);
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
                    // A refused value stays visible in its field until it is accepted.
                    if let Some(f) = any.downcast_ref::<NSTextField>()
                        && (force || (widgets.pushed != *value && f.currentEditor().is_none() && !row.invalid))
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
                (Control::Log { text }, _) => {
                    if let Some(t) = any.downcast_ref::<NSTextView>()
                        && (force || widgets.pushed != *text)
                    {
                        t.setString(&NSString::from_str(text));
                        widgets.pushed = new;
                    }
                }
                (Control::Picture { .. }, _) => {}
                (Control::Note { value, tone, strong, .. }, _) => {
                    if let Some(l) = any.downcast_ref::<NSTextField>() {
                        set_text(l, &note_text(*tone, *strong, value));
                    }
                }
                (Control::Report { text, .. }, _) => {
                    if let Some(t) = any.downcast_ref::<NSTextView>()
                        && t.string().to_string() != *text
                    {
                        t.setString(&NSString::from_str(text));
                    }
                }
                (Control::Preview { .. }, _) => {}
                (Control::Actions { titles }, _) => {
                    if let Some((_, buttons)) = &widgets.button_row {
                        for (b, title) in buttons.iter().zip(titles) {
                            if b.title().to_string() != *title {
                                b.setTitle(&NSString::from_str(title));
                            }
                        }
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
        self.offer_initial();
    }

    /// Sizes the grids to their rows and the root to the grids (or to its limit); the rows scroll
    /// when they do not fit. With tabs the tab view is as tall as its tallest tab.
    pub fn relayout(&mut self, min_height: f64) -> NSSize {
        for r in &self.rows {
            if let Some((container, buttons)) = &r.button_row {
                layout_buttons(buttons, container.frame().size.width);
            }
        }
        let (margin, width) = (self.margin, self.root.frame().size.width);
        let notice_h = match (&self.notice, self.notice_slot) {
            (None, _) => 0.0,
            (Some(_), true) => NOTICE_SLOT_HEIGHT + Space::Control.points(),
            (Some(l), false) => {
                l.setPreferredMaxLayoutWidth(self.inner);
                l.fittingSize().height.ceil() + Space::Control.points()
            }
        };
        let bottom = margin + self.button_row + notice_h;
        if let Some(l) = &self.notice {
            l.setFrame(rect(
                margin,
                margin + self.button_row,
                self.inner,
                (notice_h - Space::Control.points()).max(0.0),
            ));
        }
        if let Some(tv) = &self.tab_view {
            let grid_w = self.inner_g;
            for g in &self.grids {
                g.setFrame(rect(0.0, 0.0, grid_w, 1.0));
                g.layoutSubtreeIfNeeded();
            }
            let fit_h = self.grids.iter().map(|g| g.fittingSize().height).fold(0.0_f64, f64::max);
            let page_h = fit_h + 2.0 * TAB_PAD;
            for (grid, item) in self.grids.iter().zip(tv.tabViewItems().iter()) {
                let Some(page) = item.view(self.mtm) else { continue };
                let page_w = page.frame().size.width;
                page.setFrameSize(NSSize::new(page_w, page_h));
                let gh = grid.fittingSize().height;
                grid.setFrame(rect((page_w - grid_w) / 2.0, page_h - TAB_PAD - gh, grid_w, gh));
            }
            let tv_h = page_h + self.chrome_h;
            tv.setFrame(rect(margin, bottom, self.inner, tv_h));
            let height = (bottom + tv_h + margin).max(min_height);
            self.root.setFrameSize(NSSize::new(width, height));
            self.size = NSSize::new(width, height);
            return self.size;
        }
        let Some(grid) = self.grids.first() else { return self.size };
        grid.setFrame(rect(margin, bottom, self.inner, 1.0));
        grid.layoutSubtreeIfNeeded();
        let fit = grid.fittingSize().height;
        let natural = bottom + fit + margin;
        let height = match self.layout {
            FormLayout::Natural => natural.max(min_height),
            FormLayout::Capped(max) => natural.min(max),
            FormLayout::Fixed(h) => h,
        };
        self.root.setFrameSize(NSSize::new(width, height));
        match &self.body {
            // Content at the top, buttons at the bottom: the same when the form is as tall as its rows.
            Body::Direct => grid.setFrame(rect(margin, height - margin - fit, self.inner, fit)),
            Body::Scroll { scroll, document } => {
                let visible = height - bottom;
                scroll.setFrame(rect(0.0, bottom, width, visible));
                let doc_h = (fit + margin).max(visible);
                document.setFrame(rect(0.0, 0.0, width, doc_h));
                grid.setFrame(rect(margin, doc_h - margin - fit, self.inner, fit));
            }
        }
        grid.layoutSubtreeIfNeeded();
        self.size = NSSize::new(width, height);
        self.size
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
        if let Some(n) = &self.notice {
            out.push(label(n));
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

    /// The tab view of a form with tabs.
    pub fn tab_view(&self) -> Option<Retained<NSTabView>> {
        self.tab_view.clone()
    }

    /// Chooses item `index` in the pop-up row `id` of a rebuilt form.
    pub fn restore_choice(&mut self, id: FormId, index: usize) {
        let Some(row) = self.rows.iter_mut().find(|r| r.id == id) else { return };
        let any: &AnyObject = row.control.as_ref();
        if let Some(p) = any.downcast_ref::<NSPopUpButton>()
            && (index as isize) < p.numberOfItems()
        {
            p.selectItemAtIndex(index as isize);
            row.pushed = index.to_string();
        }
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
                    Kind::Action
                    | Kind::ReadOnly
                    | Kind::Log
                    | Kind::Picture
                    | Kind::Note
                    | Kind::Report
                    | Kind::Preview
                    | Kind::Actions => None,
                }
            })
            .collect()
    }
}

/// Puts `view` in `panel`: as the content view when it fits `max_height`, else in a scroll view
/// of that height that starts at the top. Returns the content size the panel needs.
pub fn install_form(mtm: MainThreadMarker, panel: &objc2_app_kit::NSPanel, view: &FormView, max_height: f64) -> NSSize {
    let size = view.size();
    if size.height <= max_height {
        panel.setContentView(Some(view.root()));
        return size;
    }
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, size.width, max_height));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    scroll.setDocumentView(Some(view.root()));
    view.root().scrollPoint(NSPoint::new(0.0, size.height));
    panel.setContentView(Some(&scroll));
    NSSize::new(size.width, max_height)
}
