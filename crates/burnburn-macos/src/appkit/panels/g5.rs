//! The golf-5 panels (G5-native): project notes, offset and repair, centre-line decisions and the
//! structure panel, each a floating utility panel that draws the lines of
//! `crate::panels::g5_panel` — headings, texts, a multi-line note, fields, pop-ups, checkboxes and
//! buttons — and reports each use of a control, with the panel's current values, as a
//! `NativeEvent::G5Control`. It decides nothing: what is shown and what a control means is the
//! headless model's. The widgets are rebuilt when the shape of the panel changes and otherwise only
//! updated in place, never a field the user is typing in. The content scrolls when it is taller than
//! [`MAX_HEIGHT`] (a structure with many instances).

use std::cell::RefCell;
use std::rc::Weak;

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSApplication, NSBackingStoreType, NSButton, NSColor, NSControl, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSImageView, NSLineBreakMode, NSPanel, NSPopUpButton, NSScrollView, NSTextField,
    NSTextView, NSView, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSSize, NSString};

use crate::adapter::NativeEvent;
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::image::{image_view, show_bitmap};
use crate::appkit::panels::widgets::{field, label, push_button, rect, set_button_title, set_text};
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{G5Control, G5Line, G5Panel, g5_opens_by_itself, g5_panel};
use crate::panels_image::BitmapImage;

const PANEL_WIDTH: f64 = 440.0;
const MARGIN: f64 = 12.0;
const CAPTION_WIDTH: f64 = 200.0;
/// The caption of a field in the narrow inspector column.
const NARROW_CAPTION_WIDTH: f64 = 96.0;
const ROW_HEIGHT: f64 = 28.0;
const HEADING_HEIGHT: f64 = 24.0;
const TEXT_LINE_HEIGHT: f64 = 16.0;
const MEMO_HEIGHT: f64 = 160.0;
/// A preview picture is drawn this high and wide (the thumbnail is 64 x 64 pixels).
const IMAGE_SIZE: f64 = 96.0;
/// The panel never grows beyond this; the content scrolls instead.
const MAX_HEIGHT: f64 = 640.0;
/// The stack of the selection in the inspector column is at most this high; its content scrolls.
const STACK_HEIGHT: f64 = 340.0;
/// Space around the stack in the inspector column.
const INSPECTOR_GAP: f64 = 8.0;

/// The widget of one line, and what was last copied into it.
enum Widget {
    Label(Retained<NSTextField>),
    Memo { scroll: Retained<NSScrollView>, text: Retained<NSTextView>, pushed: Option<String> },
    Field { caption: Retained<NSTextField>, field: Retained<NSTextField>, pushed: String },
    Choice { caption: Retained<NSTextField>, popup: Retained<NSPopUpButton>, pushed: usize },
    Check { button: Retained<NSButton> },
    Button { button: Retained<NSButton> },
    Image { view: Retained<NSImageView>, shown: Option<BitmapImage> },
}

/// What the lines look like without their values: a change rebuilds the widgets.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shape {
    Heading,
    Text,
    Problem,
    Memo(G5Control),
    Field(G5Control),
    Choice(G5Control, usize),
    Check(G5Control),
    Button(G5Control),
    Image,
}

fn shape_of(line: &G5Line) -> Shape {
    match line {
        G5Line::Heading(_) => Shape::Heading,
        G5Line::Text(_) => Shape::Text,
        G5Line::Problem(_) => Shape::Problem,
        G5Line::Memo { control, .. } => Shape::Memo(*control),
        G5Line::Field { control, .. } => Shape::Field(*control),
        G5Line::Choice { control, options, .. } => Shape::Choice(*control, options.len()),
        G5Line::Check { control, .. } => Shape::Check(*control),
        G5Line::Button { control, .. } => Shape::Button(*control),
        G5Line::Image(_) => Shape::Image,
    }
}

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    panel: G5Panel,
    /// The controls with their widgets, in panel order (the target reads their values).
    controls: RefCell<Vec<(G5Control, Retained<NSView>)>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnG5Target"]
    #[ivars = TargetIvars]
    pub(crate) struct G5Target;

    impl G5Target {
        /// A field committed (Return or end of editing), a pop-up chosen, a checkbox toggled or a
        /// button clicked: the sender identifies the control.
        #[unsafe(method(g5Control:))]
        fn g5_control(&self, sender: Option<&AnyObject>) {
            ffi_guard("g5Control:", (), || {
                let Some(sender) = sender else { return };
                let ivars = self.ivars();
                let (control, values) = {
                    let Ok(controls) = ivars.controls.try_borrow() else { return };
                    let Some((control, _)) = controls.iter().find(|(_, v)| {
                        let ptr: *const AnyObject = (&**v as *const NSView).cast();
                        std::ptr::eq(ptr, sender)
                    }) else {
                        return;
                    };
                    (*control, read_values(&controls))
                };
                if let Some(app) = ivars.app.upgrade() {
                    app.native(NativeEvent::G5Control { document: ivars.document, panel: ivars.panel, control, values });
                }
            });
        }
    }
);

fn is_check(control: G5Control) -> bool {
    use crate::panels::EditorControl as E;
    matches!(
        control,
        G5Control::OffsetKeepSources
            | G5Control::ContourProtect { .. }
            | G5Control::ContourKeep { .. }
            | G5Control::Editor(E::ModStepOn { .. } | E::TextPathReverse | E::ModNewPathOrient)
    )
}

/// Every value-carrying control's value now.
fn read_values(controls: &[(G5Control, Retained<NSView>)]) -> Vec<(G5Control, String)> {
    controls
        .iter()
        .filter_map(|(control, view)| {
            let any: &AnyObject = view.as_ref();
            if let Some(f) = any.downcast_ref::<NSTextField>() {
                Some((*control, f.stringValue().to_string()))
            } else if let Some(p) = any.downcast_ref::<NSPopUpButton>() {
                Some((*control, p.indexOfSelectedItem().to_string()))
            } else if let Some(t) = any.downcast_ref::<NSTextView>() {
                Some((*control, t.string().to_string()))
            } else if let Some(b) = any.downcast_ref::<NSButton>() {
                // A checkbox reports its state; a push button has no value.
                is_check(*control)
                    .then(|| (*control, if b.state() == NSControlStateValueOn { "1" } else { "0" }.to_owned()))
            } else {
                None
            }
        })
        .collect()
}

/// One panel and its widgets.
pub(crate) struct G5Section {
    panel_kind: G5Panel,
    target: Retained<G5Target>,
    panel: Retained<NSPanel>,
    scroll: Retained<NSScrollView>,
    content: Retained<NSView>,
    shape: Vec<Shape>,
    widgets: Vec<Widget>,
    /// The menu item asked for the panel (notes and centre-line decisions open by themselves).
    pub(crate) requested: bool,
    pub(crate) shown: bool,
    /// The width the lines are laid out for: the panel's, or the inspector column's.
    width: f64,
    /// The width of the caption beside a field or pop-up.
    caption_width: f64,
    /// `Some`: the panel is a section of the inspector column of the document window (the stack of
    /// the selection, G9-editor, E06): where it lies, and not a floating panel.
    embedded: Option<Embedded>,
}

/// Where an embedded section lies in its host view.
#[derive(Debug, Clone, Copy)]
struct Embedded {
    x: f64,
    y: f64,
    max_height: f64,
}

pub(crate) fn g5_section(
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    panel_kind: G5Panel,
) -> G5Section {
    // Only the panels that wait for a menu item can be closed with the close box; the others close
    // with their own button or with the flow they belong to.
    let mut style = NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow;
    if !g5_opens_by_itself(panel_kind) {
        style |= NSWindowStyleMask::Closable;
    }
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect(0.0, 0.0, PANEL_WIDTH, 120.0),
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, PANEL_WIDTH, 120.0));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, PANEL_WIDTH, 120.0));
    scroll.setDocumentView(Some(&content));
    panel.setContentView(Some(&scroll));
    let ivars = TargetIvars { app, document, panel: panel_kind, controls: RefCell::new(Vec::new()) };
    let this = G5Target::alloc(mtm).set_ivars(ivars);
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<G5Target> = unsafe { msg_send![super(this), init] };
    G5Section {
        panel_kind,
        target,
        panel,
        scroll,
        content,
        shape: Vec::new(),
        widgets: Vec::new(),
        requested: false,
        shown: false,
        width: PANEL_WIDTH,
        caption_width: CAPTION_WIDTH,
        embedded: None,
    }
}

/// Puts the stack of the selection into the inspector column of the document window (G9-editor,
/// E06): the column gets a scroll view so that the stack, which sits below everything the column
/// already holds, can be reached whatever the height of the window. Returns the view that replaces
/// the inspector in the split view.
#[allow(unsafe_code)]
pub(crate) fn embed_stack(
    mtm: MainThreadMarker,
    inspector: &NSView,
    sections: &mut [G5Section],
    column_width: f64,
    window_height: f64,
) -> Retained<NSScrollView> {
    // The lowest thing the column holds, then room below it for the stack.
    let lowest = inspector.subviews().iter().map(|v| v.frame().origin.y).fold(f64::INFINITY, f64::min);
    let lowest = if lowest.is_finite() { lowest } else { 0.0 };
    let needed = (window_height - lowest) + INSPECTOR_GAP + STACK_HEIGHT + INSPECTOR_GAP;
    let total = needed.max(window_height);
    let shift = total - window_height;
    if shift > 0.0 {
        // The column is laid out from the top: grow it at the bottom.
        for v in inspector.subviews().iter() {
            let f = v.frame();
            v.setFrameOrigin(NSPoint::new(f.origin.x, f.origin.y + shift));
        }
    }
    inspector.setFrameSize(NSSize::new(column_width, total));
    if let Some(section) = sections.iter_mut().find(|s| s.panel_kind == G5Panel::Stack) {
        section.embed(
            mtm,
            inspector,
            Embedded { x: INSPECTOR_GAP, y: INSPECTOR_GAP, max_height: STACK_HEIGHT },
            column_width - 2.0 * INSPECTOR_GAP,
        );
    }
    let wrapper = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, column_width, window_height));
    wrapper.setHasVerticalScroller(true);
    wrapper.setDrawsBackground(false);
    wrapper.setAutohidesScrollers(true);
    wrapper.setDocumentView(Some(inspector));
    // The column starts at its top.
    inspector.scrollPoint(NSPoint::new(0.0, total));
    wrapper
}

impl G5Section {
    /// Nil the targets (weak references) before the section goes: nothing can call a dead object.
    #[allow(unsafe_code)]
    pub(crate) fn clear_targets(&self) {
        let controls: Vec<&NSControl> = self
            .widgets
            .iter()
            .filter_map(|w| match w {
                Widget::Field { field, .. } => Some(&***field),
                Widget::Choice { popup, .. } => Some(&****popup),
                Widget::Check { button } | Widget::Button { button } => Some(&***button),
                Widget::Label(_) | Widget::Memo { .. } | Widget::Image { .. } => None,
            })
            .collect();
        for c in controls {
            // SAFETY: a nil target cannot dangle.
            unsafe { c.setTarget(None) };
        }
        let _ = &self.target;
    }

    pub(crate) fn panel_kind(&self) -> G5Panel {
        self.panel_kind
    }

    pub(crate) fn hide(&mut self) {
        if self.embedded.is_some() {
            self.shown = false;
            self.scroll.setHidden(true);
        } else if self.shown {
            self.shown = false;
            self.panel.orderOut(None);
        }
    }

    /// Makes the section part of `host` instead of a floating panel (G9-editor, E06).
    fn embed(&mut self, mtm: MainThreadMarker, host: &NSView, at: Embedded, width: f64) {
        self.width = width;
        self.caption_width = NARROW_CAPTION_WIDTH;
        self.embedded = Some(at);
        // The scroll view is the panel's content view until it moves into the host.
        self.panel.setContentView(Some(&NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 1.0, 1.0))));
        self.scroll.setFrame(rect(at.x, at.y, width, 1.0));
        self.scroll.setHidden(true);
        host.addSubview(&self.scroll);
    }

    /// The visible widgets of the panel, in line order, for tests that drive them like a user.
    pub(crate) fn controls(&self) -> Vec<(G5Control, Retained<NSView>)> {
        self.target.ivars().controls.try_borrow().map(|c| c.clone()).unwrap_or_default()
    }

    /// What the user has typed and chosen in the panel so far.
    pub(crate) fn values(&self) -> Vec<(G5Control, String)> {
        self.target.ivars().controls.try_borrow().map(|c| read_values(&c)).unwrap_or_default()
    }

    pub(crate) fn labels(&self) -> Vec<String> {
        self.widgets
            .iter()
            .filter_map(|w| match w {
                Widget::Label(l) => Some(l.stringValue().to_string()),
                _ => None,
            })
            .collect()
    }

    /// The control's widget as a plain view, if the panel has it now.
    pub(crate) fn control_view(&self, control: G5Control) -> Option<Retained<NSView>> {
        self.controls().into_iter().find(|(c, _)| *c == control).map(|(_, v)| v)
    }

    #[allow(unsafe_code)]
    fn rebuild(&mut self, mtm: MainThreadMarker, lines: &[G5Line]) {
        // What the user typed into a field survives a rebuild (a problem line appearing changes the
        // shape of the panel, not the field), together with what the model last put there.
        let mut kept: Vec<(G5Control, String, String)> = Vec::new();
        {
            let controls = self.target.ivars().controls.try_borrow().map(|c| c.clone()).unwrap_or_default();
            let mut controls = controls.iter();
            for w in &self.widgets {
                match w {
                    Widget::Label(_) => {}
                    Widget::Field { field, pushed, .. } => {
                        if let Some((c, _)) = controls.next() {
                            kept.push((*c, field.stringValue().to_string(), pushed.clone()));
                        }
                    }
                    Widget::Memo { text, pushed, .. } => {
                        if let Some((c, _)) = controls.next() {
                            kept.push((*c, text.string().to_string(), pushed.clone().unwrap_or_default()));
                        }
                    }
                    _ => {
                        controls.next();
                    }
                }
            }
        }
        for v in self.content.subviews().iter() {
            v.removeFromSuperview();
        }
        self.clear_targets();
        self.widgets.clear();
        let any: &AnyObject = self.target.as_ref();
        let mut controls: Vec<(G5Control, Retained<NSView>)> = Vec::new();
        let full = self.width - 2.0 * MARGIN;
        for line in lines {
            match line {
                G5Line::Heading(_) | G5Line::Text(_) | G5Line::Problem(_) => {
                    let l = label(mtm, "", rect(MARGIN, 0.0, full, TEXT_LINE_HEIGHT));
                    l.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
                    l.setPreferredMaxLayoutWidth(full);
                    if matches!(line, G5Line::Heading(_)) {
                        l.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
                    }
                    self.content.addSubview(&l);
                    self.widgets.push(Widget::Label(l));
                }
                G5Line::Memo { control, .. } => {
                    let scroll = NSTextView::scrollableTextView(mtm);
                    scroll.setFrame(rect(MARGIN, 0.0, full, MEMO_HEIGHT));
                    scroll.setHasVerticalScroller(true);
                    let text =
                        scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok()).unwrap_or_else(|| {
                            NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, full, MEMO_HEIGHT))
                        });
                    text.setEditable(true);
                    text.setRichText(false);
                    text.setFont(Some(&NSFont::systemFontOfSize(13.0)));
                    self.content.addSubview(&scroll);
                    controls.push((*control, Retained::into_super(Retained::into_super(text.clone()))));
                    let mut pushed = None;
                    if let Some((_, typed, was)) = kept.iter().find(|(c, _, _)| c == control) {
                        text.setString(&NSString::from_str(typed));
                        pushed = Some(was.clone());
                    }
                    self.widgets.push(Widget::Memo { scroll, text, pushed });
                }
                G5Line::Field { control, .. } => {
                    let caption = label(mtm, "", rect(MARGIN, 0.0, self.caption_width, 20.0));
                    caption.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
                    let f =
                        field(mtm, rect(MARGIN + self.caption_width + 6.0, 0.0, full - self.caption_width - 6.0, 24.0));
                    // SAFETY: the target is weak; the section keeps it and clears it before it goes.
                    unsafe { f.setTarget(Some(any)) };
                    // SAFETY: `g5Control:` takes the sender (`v@:@`).
                    unsafe { f.setAction(Some(sel!(g5Control:))) };
                    self.content.addSubview(&caption);
                    self.content.addSubview(&f);
                    controls.push((*control, Retained::into_super(Retained::into_super(f.clone()))));
                    let mut pushed = String::new();
                    if let Some((_, typed, was)) = kept.iter().find(|(c, _, _)| c == control) {
                        f.setStringValue(&NSString::from_str(typed));
                        pushed.clone_from(was);
                    }
                    self.widgets.push(Widget::Field { caption, field: f, pushed });
                }
                G5Line::Choice { control, options, .. } => {
                    let caption = label(mtm, "", rect(MARGIN, 0.0, self.caption_width, 20.0));
                    caption.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
                    let popup = NSPopUpButton::initWithFrame_pullsDown(
                        NSPopUpButton::alloc(mtm),
                        rect(MARGIN + self.caption_width + 6.0, 0.0, full - self.caption_width - 6.0, 26.0),
                        false,
                    );
                    for o in options {
                        popup.addItemWithTitle(&NSString::from_str(o));
                    }
                    // SAFETY: as above.
                    unsafe { popup.setTarget(Some(any)) };
                    // SAFETY: as above.
                    unsafe { popup.setAction(Some(sel!(g5Control:))) };
                    self.content.addSubview(&caption);
                    self.content.addSubview(&popup);
                    controls.push((
                        *control,
                        Retained::into_super(Retained::into_super(Retained::into_super(popup.clone()))),
                    ));
                    self.widgets.push(Widget::Choice { caption, popup, pushed: usize::MAX });
                }
                G5Line::Check { control, .. } => {
                    // SAFETY: no target and no action yet; both are set below.
                    let b =
                        unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(""), None, None, mtm) };
                    b.setFrame(rect(MARGIN, 0.0, full, 22.0));
                    // SAFETY: as above.
                    unsafe { b.setTarget(Some(any)) };
                    // SAFETY: as above.
                    unsafe { b.setAction(Some(sel!(g5Control:))) };
                    self.content.addSubview(&b);
                    controls.push((*control, Retained::into_super(Retained::into_super(b.clone()))));
                    self.widgets.push(Widget::Check { button: b });
                }
                G5Line::Image(_) => {
                    let view = image_view(mtm, &self.content, "", rect(MARGIN, 0.0, IMAGE_SIZE, IMAGE_SIZE));
                    self.widgets.push(Widget::Image { view, shown: None });
                }
                G5Line::Button { control, .. } => {
                    let b = push_button(mtm, "");
                    b.setFrame(rect(MARGIN, 0.0, full, 26.0));
                    // SAFETY: as above.
                    unsafe { b.setTarget(Some(any)) };
                    // SAFETY: as above.
                    unsafe { b.setAction(Some(sel!(g5Control:))) };
                    self.content.addSubview(&b);
                    controls.push((*control, Retained::into_super(Retained::into_super(b.clone()))));
                    self.widgets.push(Widget::Button { button: b });
                }
            }
        }
        if let Ok(mut slot) = self.target.ivars().controls.try_borrow_mut() {
            *slot = controls;
        }
    }

    /// Copies the lines into the widgets and lays them out from the top. Returns the content height.
    fn fill(&mut self, lines: &[G5Line], locale: Locale) -> f64 {
        let full = self.width - 2.0 * MARGIN;
        let wrapped = |t: &str, width: f64| {
            let chars_per_line = (width / 6.5).max(10.0) as usize;
            t.chars().count().div_ceil(chars_per_line).max(1) as f64
        };
        // Heights first (wrapped text), then positions from the top.
        let mut heights = Vec::with_capacity(lines.len());
        for line in lines {
            heights.push(match line {
                G5Line::Heading(_) => HEADING_HEIGHT,
                G5Line::Text(t) | G5Line::Problem(t) => wrapped(t, full) * TEXT_LINE_HEIGHT + 6.0,
                G5Line::Memo { .. } => MEMO_HEIGHT + 8.0,
                G5Line::Field { caption, .. } | G5Line::Choice { caption, .. } => {
                    (wrapped(caption, self.caption_width) * TEXT_LINE_HEIGHT + 8.0).max(ROW_HEIGHT)
                }
                G5Line::Check { .. } | G5Line::Button { .. } => ROW_HEIGHT,
                G5Line::Image(_) => IMAGE_SIZE + 8.0,
            });
        }
        let total: f64 = heights.iter().sum::<f64>() + 2.0 * MARGIN;
        let mut y = total - MARGIN;
        for ((line, widget), height) in lines.iter().zip(self.widgets.iter_mut()).zip(&heights) {
            y -= height;
            match (line, widget) {
                (G5Line::Heading(t) | G5Line::Text(t), Widget::Label(l)) => {
                    set_text(l, t);
                    l.setFrame(rect(MARGIN, y, full, *height));
                    l.setTextColor(Some(&NSColor::labelColor()));
                }
                (G5Line::Problem(t), Widget::Label(l)) => {
                    set_text(l, t);
                    l.setFrame(rect(MARGIN, y, full, *height));
                    l.setTextColor(Some(&NSColor::systemRedColor()));
                }
                (G5Line::Memo { text, .. }, Widget::Memo { scroll, text: view, pushed }) => {
                    scroll.setFrame(rect(MARGIN, y + 4.0, full, MEMO_HEIGHT));
                    // What the user typed stays until the stored note changes.
                    if pushed.as_deref() != Some(text.as_str()) {
                        view.setString(&NSString::from_str(text));
                        *pushed = Some(text.clone());
                    }
                    view.setAccessibilityLabel(Some(&NSString::from_str(crate::l10n_g5::g5_text(
                        crate::l10n_g5::G5Text::NotesTitle,
                        locale,
                    ))));
                }
                (G5Line::Field { caption, text, .. }, Widget::Field { caption: c, field, pushed }) => {
                    c.setFrame(rect(MARGIN, y + 2.0, self.caption_width, *height - 4.0));
                    field.setFrame(rect(
                        MARGIN + self.caption_width + 6.0,
                        y + (*height - 24.0) / 2.0,
                        full - self.caption_width - 6.0,
                        24.0,
                    ));
                    set_text(c, caption);
                    // What the user typed stays until the model changes; a field being edited is left alone.
                    if *pushed != *text && field.currentEditor().is_none() {
                        field.setStringValue(&NSString::from_str(text));
                        *pushed = text.clone();
                    }
                    field.setAccessibilityLabel(Some(&NSString::from_str(caption)));
                }
                (G5Line::Choice { caption, options, selected, .. }, Widget::Choice { caption: c, popup, pushed }) => {
                    c.setFrame(rect(MARGIN, y + 2.0, self.caption_width, *height - 4.0));
                    popup.setFrame(rect(
                        MARGIN + self.caption_width + 6.0,
                        y + (*height - 26.0) / 2.0,
                        full - self.caption_width - 6.0,
                        26.0,
                    ));
                    set_text(c, caption);
                    for (i, o) in options.iter().enumerate() {
                        if let Some(item) = popup.itemAtIndex(i as isize)
                            && item.title().to_string() != *o
                        {
                            item.setTitle(&NSString::from_str(o));
                        }
                    }
                    if *pushed != *selected {
                        popup.selectItemAtIndex(*selected as isize);
                        *pushed = *selected;
                    }
                    popup.setAccessibilityLabel(Some(&NSString::from_str(caption)));
                }
                (G5Line::Check { caption, on, .. }, Widget::Check { button }) => {
                    button.setFrame(rect(MARGIN, y, full, 22.0));
                    set_button_title(button, caption);
                    button.setState(if *on { NSControlStateValueOn } else { NSControlStateValueOff });
                }
                (G5Line::Image(image), Widget::Image { view, shown }) => {
                    view.setFrame(rect(MARGIN, y + 4.0, IMAGE_SIZE, IMAGE_SIZE));
                    show_bitmap(view, shown, Some(image));
                }
                (G5Line::Button { title, enabled, .. }, Widget::Button { button }) => {
                    button.setFrame(rect(MARGIN, y, full, 26.0));
                    set_button_title(button, title);
                    button.setEnabled(*enabled);
                }
                _ => {}
            }
        }
        total
    }
}

impl DocWindow {
    /// Shows, fills or hides every golf-5 panel as the model says: notes and centre-line decisions
    /// open by themselves, offset/repair and structure wait for their menu item, and all belong to
    /// the active window only.
    pub(crate) fn refresh_g5(&mut self, vm: &AppViewModel, document: DocumentId) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let locale = self.locale;
        let window_frame = self.window.frame();
        let app_active = NSApplication::sharedApplication(mtm).isActive();
        for (slot, section) in self.g5.iter_mut().enumerate() {
            let kind = section.panel_kind;
            // The user closed it with the close box: the menu item shows it again.
            if section.shown && app_active && !section.panel.isVisible() && !g5_opens_by_itself(kind) {
                section.shown = false;
                section.requested = false;
            }
            if let Some(at) = section.embedded {
                let model = if vm.active == Some(document) { g5_panel(vm, document, kind, locale, &[]) } else { None };
                let Some(model) = model else {
                    section.hide();
                    continue;
                };
                // The title leads the lines: there is no panel title to carry it.
                let mut lines = vec![G5Line::Heading(model.title.clone())];
                lines.extend(model.lines);
                let shape: Vec<Shape> = lines.iter().map(shape_of).collect();
                if shape != section.shape {
                    section.rebuild(mtm, &lines);
                    section.shape = shape;
                }
                let total = section.fill(&lines, locale);
                let height = total.min(at.max_height);
                let was_shown = section.shown;
                section.shown = true;
                let (scroll, content) = (section.scroll.clone(), section.content.clone());
                let width = section.width;
                if let Err(e) = crate::appkit::objc_try("showing the stack in the inspector", || {
                    content.setFrameSize(NSSize::new(width, total));
                    scroll.setFrame(rect(at.x, at.y, width, height));
                    scroll.setHidden(false);
                    if !was_shown {
                        content.scrollPoint(NSPoint::new(0.0, total));
                    }
                    // A stack that appears with the selection is brought into view in the column.
                    if !was_shown {
                        scroll.scrollRectToVisible(scroll.bounds());
                    }
                }) {
                    eprintln!("burnburn: {e}");
                }
                continue;
            }
            let wanted = vm.active == Some(document) && (g5_opens_by_itself(kind) || section.requested);
            // A panel nobody asked for is not even built (the structure lines are not cheap).
            let model = if wanted { g5_panel(vm, document, kind, locale, &section.values()) } else { None };
            let Some(model) = model else {
                section.hide();
                continue;
            };
            let shape: Vec<Shape> = model.lines.iter().map(shape_of).collect();
            if shape != section.shape {
                section.rebuild(mtm, &model.lines);
                section.shape = shape;
            }
            let total = section.fill(&model.lines, locale);
            let height = total.min(MAX_HEIGHT);
            let was_shown = section.shown;
            section.shown = true;
            let panel = section.panel.clone();
            let scroll = section.scroll.clone();
            let content = section.content.clone();
            if panel.title().to_string() != model.title {
                panel.setTitle(&NSString::from_str(&model.title));
            }
            if let Err(e) = crate::appkit::objc_try("showing a golf-5 panel", || {
                content.setFrameSize(NSSize::new(PANEL_WIDTH, total));
                scroll.setFrame(rect(0.0, 0.0, PANEL_WIDTH, height));
                panel.setContentSize(NSSize::new(PANEL_WIDTH, height));
                if !was_shown {
                    let stagger = 28.0 * slot as f64;
                    panel.setFrameTopLeftPoint(NSPoint::new(
                        window_frame.origin.x + window_frame.size.width - PANEL_WIDTH - 24.0 - stagger,
                        window_frame.origin.y + window_frame.size.height - 60.0 - stagger,
                    ));
                    panel.orderFront(None);
                    // Show the top of a tall panel.
                    content.scrollPoint(NSPoint::new(0.0, total));
                }
            }) {
                eprintln!("burnburn: {e}");
            }
        }
    }

    /// Venster ▸ Offset en reparatie / Structuur: asks for (or dismisses) that panel.
    pub(crate) fn toggle_g5(&mut self, panel: G5Panel) -> bool {
        match self.g5.iter_mut().find(|s| s.panel_kind == panel) {
            Some(section) => {
                section.requested = !section.requested;
                section.requested
            }
            None => false,
        }
    }
}
