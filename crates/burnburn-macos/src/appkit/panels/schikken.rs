//! The Schikken panel (W3-UI-schikken): a floating utility panel that draws the lines of
//! `crate::panels::schikken_panel` — headings, texts, fields, pop-ups, checkboxes and buttons — and
//! reports each use of a control, with the panel's current values, as a `NativeEvent`. It decides
//! nothing: what is shown and what a control means is the headless model's. The widgets are rebuilt
//! when the shape of the panel changes (another flow, another set of lines) and otherwise only
//! updated in place, never a field the user is typing in.

use std::cell::RefCell;
use std::rc::Weak;

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSLineBreakMode, NSPanel, NSPopUpButton, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSSize, NSString};

use crate::adapter::NativeEvent;
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{field, label, push_button, rect, set_button_title, set_text};
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{SchikkenControl, SchikkenLine, schikken_panel};

const PANEL_WIDTH: f64 = 380.0;
const MARGIN: f64 = 12.0;
const CAPTION_WIDTH: f64 = 170.0;
const ROW_HEIGHT: f64 = 28.0;
const HEADING_HEIGHT: f64 = 24.0;
const TEXT_LINE_HEIGHT: f64 = 16.0;

/// The widget of one line, and what was last copied into it.
enum Widget {
    Label(Retained<NSTextField>),
    Field { caption: Retained<NSTextField>, field: Retained<NSTextField>, pushed: String },
    Choice { caption: Retained<NSTextField>, popup: Retained<NSPopUpButton>, pushed: usize },
    Check { button: Retained<NSButton> },
    Button { button: Retained<NSButton> },
}

/// What the lines look like without their values: a change rebuilds the widgets.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shape {
    Heading,
    Text,
    Problem,
    Field(SchikkenControl),
    Choice(SchikkenControl, usize),
    Check(SchikkenControl),
    Button(SchikkenControl),
}

fn shape_of(line: &SchikkenLine) -> Shape {
    match line {
        SchikkenLine::Heading(_) => Shape::Heading,
        SchikkenLine::Text(_) => Shape::Text,
        SchikkenLine::Problem(_) => Shape::Problem,
        SchikkenLine::Field { control, .. } => Shape::Field(*control),
        SchikkenLine::Choice { control, options, .. } => Shape::Choice(*control, options.len()),
        SchikkenLine::Check { control, .. } => Shape::Check(*control),
        SchikkenLine::Button { control, .. } => Shape::Button(*control),
    }
}

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    /// The controls with their widgets, in panel order (the target reads their values).
    controls: RefCell<Vec<(SchikkenControl, Retained<NSView>)>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnSchikkenTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct SchikkenTarget;

    impl SchikkenTarget {
        /// A field committed (Return or end of editing), a pop-up chosen, a checkbox toggled or a
        /// button clicked: the sender identifies the control.
        #[unsafe(method(schikkenControl:))]
        fn schikken_control(&self, sender: Option<&AnyObject>) {
            ffi_guard("schikkenControl:", (), || {
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
                    app.native(NativeEvent::SchikkenControl { document: ivars.document, control, values });
                }
            });
        }
    }
);

fn is_check(control: SchikkenControl) -> bool {
    matches!(
        control,
        SchikkenControl::PlaceByGroup
            | SchikkenControl::WarpBilinear
            | SchikkenControl::NestMirror
            | SchikkenControl::NestHoles
            | SchikkenControl::NestTight
            | SchikkenControl::NestCommonLine
    )
}

/// Every value-carrying control's value now.
fn read_values(controls: &[(SchikkenControl, Retained<NSView>)]) -> Vec<(SchikkenControl, String)> {
    controls
        .iter()
        .filter_map(|(control, view)| {
            let any: &AnyObject = view.as_ref();
            if let Some(f) = any.downcast_ref::<NSTextField>() {
                Some((*control, f.stringValue().to_string()))
            } else if let Some(p) = any.downcast_ref::<NSPopUpButton>() {
                Some((*control, p.indexOfSelectedItem().to_string()))
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

/// The panel and its widgets.
pub(crate) struct SchikkenSection {
    target: Retained<SchikkenTarget>,
    panel: Retained<NSPanel>,
    content: Retained<NSView>,
    shape: Vec<Shape>,
    widgets: Vec<Widget>,
    /// Schikken ▸ Schikkenpaneel was chosen: the options panel shows even without a flow.
    pub(crate) requested: bool,
    pub(crate) shown: bool,
}

pub(crate) fn schikken_section(
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> SchikkenSection {
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect(0.0, 0.0, PANEL_WIDTH, 120.0),
        NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow | NSWindowStyleMask::Closable,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(&crate::panels::schikken_text(crate::panels::SchikkenText::PanelTitle, locale)));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, PANEL_WIDTH, 120.0));
    panel.setContentView(Some(&content));
    let ivars = TargetIvars { app, document, controls: RefCell::new(Vec::new()) };
    let this = SchikkenTarget::alloc(mtm).set_ivars(ivars);
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<SchikkenTarget> = unsafe { msg_send![super(this), init] };
    SchikkenSection { target, panel, content, shape: Vec::new(), widgets: Vec::new(), requested: false, shown: false }
}

impl SchikkenSection {
    /// Nil the targets (weak references) before the section goes: nothing can call a dead object.
    #[allow(unsafe_code)]
    pub(crate) fn clear_targets(&self) {
        // Every control is an `NSControl`; a nil target cannot dangle.
        let controls: Vec<&objc2_app_kit::NSControl> = self
            .widgets
            .iter()
            .filter_map(|w| match w {
                Widget::Field { field, .. } => Some(&***field),
                Widget::Choice { popup, .. } => Some(&****popup),
                Widget::Check { button } | Widget::Button { button } => Some(&***button),
                Widget::Label(_) => None,
            })
            .collect();
        for c in controls {
            // SAFETY: a nil target cannot dangle.
            unsafe { c.setTarget(None) };
        }
        let _ = &self.target;
    }

    pub(crate) fn hide(&mut self) {
        if self.shown {
            self.shown = false;
            self.panel.orderOut(None);
        }
    }

    /// The visible widgets of the panel, in line order, for tests that drive them like a user.
    pub(crate) fn controls(&self) -> Vec<(SchikkenControl, Retained<NSView>)> {
        self.target.ivars().controls.try_borrow().map(|c| c.clone()).unwrap_or_default()
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
    pub(crate) fn control_view(&self, control: SchikkenControl) -> Option<Retained<NSView>> {
        self.controls().into_iter().find(|(c, _)| *c == control).map(|(_, v)| v)
    }

    #[allow(unsafe_code)]
    fn rebuild(&mut self, mtm: MainThreadMarker, lines: &[SchikkenLine]) {
        // What the user typed into a field survives a rebuild (a problem line appearing changes the
        // shape of the panel, not the field), together with what the model last put there.
        let mut kept: Vec<(SchikkenControl, String, String)> = Vec::new();
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
        let mut controls: Vec<(SchikkenControl, Retained<NSView>)> = Vec::new();
        let full = PANEL_WIDTH - 2.0 * MARGIN;
        for line in lines {
            match line {
                SchikkenLine::Heading(_) | SchikkenLine::Text(_) | SchikkenLine::Problem(_) => {
                    let l = label(mtm, "", rect(MARGIN, 0.0, full, TEXT_LINE_HEIGHT));
                    l.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
                    l.setPreferredMaxLayoutWidth(full);
                    if matches!(line, SchikkenLine::Heading(_)) {
                        l.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
                    }
                    self.content.addSubview(&l);
                    self.widgets.push(Widget::Label(l));
                }
                SchikkenLine::Field { control, .. } => {
                    let caption = label(mtm, "", rect(MARGIN, 0.0, CAPTION_WIDTH, 20.0));
                    let f = field(mtm, rect(MARGIN + CAPTION_WIDTH + 6.0, 0.0, full - CAPTION_WIDTH - 6.0, 24.0));
                    // SAFETY: the target is weak; the section keeps it and clears it before it goes.
                    unsafe { f.setTarget(Some(any)) };
                    // SAFETY: `schikkenControl:` takes the sender (`v@:@`).
                    unsafe { f.setAction(Some(sel!(schikkenControl:))) };
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
                SchikkenLine::Choice { control, options, .. } => {
                    let caption = label(mtm, "", rect(MARGIN, 0.0, CAPTION_WIDTH, 20.0));
                    let popup = NSPopUpButton::initWithFrame_pullsDown(
                        NSPopUpButton::alloc(mtm),
                        rect(MARGIN + CAPTION_WIDTH + 6.0, 0.0, full - CAPTION_WIDTH - 6.0, 26.0),
                        false,
                    );
                    for o in options {
                        popup.addItemWithTitle(&NSString::from_str(o));
                    }
                    // SAFETY: as above.
                    unsafe { popup.setTarget(Some(any)) };
                    // SAFETY: as above.
                    unsafe { popup.setAction(Some(sel!(schikkenControl:))) };
                    self.content.addSubview(&caption);
                    self.content.addSubview(&popup);
                    controls.push((
                        *control,
                        Retained::into_super(Retained::into_super(Retained::into_super(popup.clone()))),
                    ));
                    self.widgets.push(Widget::Choice { caption, popup, pushed: usize::MAX });
                }
                SchikkenLine::Check { control, .. } => {
                    // SAFETY: no target and no action yet; both are set below.
                    let b =
                        unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(""), None, None, mtm) };
                    b.setFrame(rect(MARGIN, 0.0, full, 22.0));
                    // SAFETY: as above.
                    unsafe { b.setTarget(Some(any)) };
                    // SAFETY: as above.
                    unsafe { b.setAction(Some(sel!(schikkenControl:))) };
                    self.content.addSubview(&b);
                    controls.push((*control, Retained::into_super(Retained::into_super(b.clone()))));
                    self.widgets.push(Widget::Check { button: b });
                }
                SchikkenLine::Button { control, .. } => {
                    let b = push_button(mtm, "");
                    b.setFrame(rect(MARGIN, 0.0, full, 26.0));
                    // SAFETY: as above.
                    unsafe { b.setTarget(Some(any)) };
                    // SAFETY: as above.
                    unsafe { b.setAction(Some(sel!(schikkenControl:))) };
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

    /// Copies the lines into the widgets and lays them out from the top. Returns the panel height.
    fn fill(&mut self, lines: &[SchikkenLine]) -> f64 {
        let full = PANEL_WIDTH - 2.0 * MARGIN;
        // Heights first (wrapped text), then positions from the top.
        let mut heights = Vec::with_capacity(lines.len());
        for (line, widget) in lines.iter().zip(&self.widgets) {
            heights.push(match (line, widget) {
                (SchikkenLine::Heading(t), Widget::Label(l))
                | (SchikkenLine::Text(t) | SchikkenLine::Problem(t), Widget::Label(l)) => {
                    set_text(l, t);
                    let chars_per_line = (full / 6.5).max(10.0) as usize;
                    let rows = t.chars().count().div_ceil(chars_per_line).max(1) as f64;
                    if matches!(line, SchikkenLine::Heading(_)) {
                        HEADING_HEIGHT
                    } else {
                        rows * TEXT_LINE_HEIGHT + 6.0
                    }
                }
                (
                    SchikkenLine::Field { .. }
                    | SchikkenLine::Choice { .. }
                    | SchikkenLine::Button { .. }
                    | SchikkenLine::Check { .. },
                    _,
                ) => ROW_HEIGHT,
                _ => ROW_HEIGHT,
            });
        }
        let total: f64 = heights.iter().sum::<f64>() + 2.0 * MARGIN;
        let mut y = total - MARGIN;
        for ((line, widget), height) in lines.iter().zip(self.widgets.iter_mut()).zip(&heights) {
            y -= height;
            match (line, widget) {
                (SchikkenLine::Heading(_) | SchikkenLine::Text(_), Widget::Label(l)) => {
                    l.setFrame(rect(MARGIN, y, full, *height));
                    l.setTextColor(Some(&NSColor::labelColor()));
                }
                (SchikkenLine::Problem(_), Widget::Label(l)) => {
                    l.setFrame(rect(MARGIN, y, full, *height));
                    l.setTextColor(Some(&NSColor::systemRedColor()));
                }
                (
                    SchikkenLine::Field { caption, text, invalid, .. },
                    Widget::Field { caption: c, field, pushed, .. },
                ) => {
                    c.setFrame(rect(MARGIN, y + 2.0, CAPTION_WIDTH, 20.0));
                    field.setFrame(rect(MARGIN + CAPTION_WIDTH + 6.0, y, full - CAPTION_WIDTH - 6.0, 24.0));
                    set_text(c, caption);
                    let color = if *invalid { NSColor::systemRedColor() } else { NSColor::labelColor() };
                    c.setTextColor(Some(&color));
                    // What the user typed stays until the model changes; a field being edited is left alone.
                    if *pushed != *text && field.currentEditor().is_none() {
                        field.setStringValue(&NSString::from_str(text));
                        *pushed = text.clone();
                    }
                    field.setAccessibilityLabel(Some(&NSString::from_str(caption)));
                }
                (
                    SchikkenLine::Choice { caption, options, selected, .. },
                    Widget::Choice { caption: c, popup, pushed, .. },
                ) => {
                    c.setFrame(rect(MARGIN, y + 2.0, CAPTION_WIDTH, 20.0));
                    popup.setFrame(rect(MARGIN + CAPTION_WIDTH + 6.0, y, full - CAPTION_WIDTH - 6.0, 26.0));
                    set_text(c, caption);
                    for (i, o) in options.iter().enumerate() {
                        if let Some(item) = popup.itemAtIndex(i as isize) {
                            item.setTitle(&NSString::from_str(o));
                        }
                    }
                    if *pushed != *selected {
                        popup.selectItemAtIndex(*selected as isize);
                        *pushed = *selected;
                    }
                    popup.setAccessibilityLabel(Some(&NSString::from_str(caption)));
                }
                (SchikkenLine::Check { caption, on, .. }, Widget::Check { button, .. }) => {
                    button.setFrame(rect(MARGIN, y, full, 22.0));
                    set_button_title(button, caption);
                    button.setState(if *on { NSControlStateValueOn } else { NSControlStateValueOff });
                }
                (SchikkenLine::Button { title, enabled, .. }, Widget::Button { button, .. }) => {
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
    /// Shows, fills or hides the Schikken panel as the model says: a flow opens it by itself, the
    /// Schikkenpaneel menu item opens the options panel, and it only belongs to the active window.
    pub(crate) fn refresh_schikken(&mut self, vm: &AppViewModel, document: DocumentId) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let model = schikken_panel(vm, document, self.locale);
        let wanted = vm.active == Some(document) && model.as_ref().is_some_and(|m| m.flow || self.schikken.requested);
        let Some(model) = model.filter(|_| wanted) else {
            self.schikken.hide();
            return;
        };
        let shape: Vec<Shape> = model.lines.iter().map(shape_of).collect();
        if shape != self.schikken.shape {
            self.schikken.rebuild(mtm, &model.lines);
            self.schikken.shape = shape;
        }
        let height = self.schikken.fill(&model.lines);
        let was_shown = self.schikken.shown;
        self.schikken.shown = true;
        let panel = self.schikken.panel.clone();
        let window_frame = self.window.frame();
        if let Err(e) = crate::appkit::objc_try("showing the Schikken panel", || {
            panel.setContentSize(NSSize::new(PANEL_WIDTH, height));
            if !was_shown {
                panel.setFrameTopLeftPoint(NSPoint::new(
                    window_frame.origin.x + window_frame.size.width - PANEL_WIDTH - 24.0,
                    window_frame.origin.y + window_frame.size.height - 60.0,
                ));
                panel.orderFront(None);
            }
        }) {
            eprintln!("burnburn: {e}");
        }
    }

    /// Schikken ▸ Schikkenpaneel: asks for (or dismisses) the options panel.
    pub(crate) fn toggle_schikken(&mut self) -> bool {
        self.schikken.requested = !self.schikken.requested;
        self.schikken.requested
    }
}

#[allow(dead_code)]
fn _window_type(_: &NSWindow) {}
