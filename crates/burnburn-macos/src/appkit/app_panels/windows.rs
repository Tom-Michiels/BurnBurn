//! The windows of the app-level panels (W3-UI-app): preferences, shortcut editor, undo history,
//! preview and RD inspector. Each builder lays out its widgets once; each `refresh` copies a plain
//! panel model (`crate::app_view`) into them and never decides anything itself.

use std::rc::Weak;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSFloatingWindowLevel, NSFont, NSLineBreakMode, NSPopUpButton, NSScrollView, NSSlider,
    NSTableColumn, NSTableView, NSTextField, NSTextView, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::NSString;

use super::target::{AppPanelTarget, HISTORY_TABLE, KeyRecorder, PreviewView, RD_TABLE, SHORTCUT_TABLE};
use crate::app_text::{AppText, app_text};
use crate::app_view::{PrefsPanel, PreviewPanel, RdPanel};
use crate::prefs_form::{RESET_ID, prefs_form};
use crate::appkit::app::HostApp;
use crate::appkit::panels::form::{FormView, Wiring, build_form};
use crate::appkit::panels::image::{caption, note, show_bitmap};
use crate::appkit::panels::widgets::{button, rect, set_text};
use crate::appkit::symbols::{self, Symbol};
use crate::l10n::Locale;
use crate::panels_image::BitmapImage;

/// Pinned to the bottom (left, or right): the flexible side is above and beside.
const BOTTOM_LEFT: NSAutoresizingMaskOptions = NSAutoresizingMaskOptions::ViewMaxYMargin;
const BOTTOM_RIGHT: NSAutoresizingMaskOptions = NSAutoresizingMaskOptions(
    NSAutoresizingMaskOptions::ViewMaxYMargin.0 | NSAutoresizingMaskOptions::ViewMinXMargin.0,
);

/// A titled, closable window that stays alive when closed (the host reopens it).
#[allow(unsafe_code)]
fn utility_window(
    mtm: MainThreadMarker,
    title: &str,
    size: (f64, f64),
    resizable: bool,
    float: bool,
) -> (Retained<NSWindow>, Retained<NSView>) {
    let mut style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    if resizable {
        style |= NSWindowStyleMask::Resizable;
    }
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, size.0, size.1),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with the caller's Retained.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(title));
    if float {
        // A test process cannot take focus from the user's frontmost app: float above it.
        window.setLevel(NSFloatingWindowLevel);
    }
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, size.0, size.1));
    window.setContentView(Some(&content));
    window.center();
    (window, content)
}

/// Brings a window forward (floating windows of a test process need `orderFrontRegardless`).
pub(crate) fn present(window: &NSWindow, float: bool) {
    if float {
        window.orderFrontRegardless();
    } else {
        window.makeKeyAndOrderFront(None);
    }
}

#[allow(unsafe_code)]
fn wire(control: &objc2_app_kit::NSControl, target: &AnyObject, action: objc2::runtime::Sel, tag: isize) {
    control.setTag(tag);
    // SAFETY: `action` is an `AppPanelTarget` method taking the sender (`v@:@`); the target is kept
    // by `AppPanels`, which outlives every control (cleared in its Drop).
    unsafe { control.setTarget(Some(target)) };
    // SAFETY: as above.
    unsafe { control.setAction(Some(action)) };
}

#[allow(unsafe_code)]
fn make_table(
    mtm: MainThreadMarker,
    frame: objc2_foundation::NSRect,
    tag: isize,
    name: &str,
    columns: &[(&str, &str, f64)],
    target: &AppPanelTarget,
) -> (Retained<NSScrollView>, Retained<NSTableView>) {
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), frame);
    scroll.setHasVerticalScroller(true);
    // The table takes what the window gains (the rows around it keep their place).
    scroll.setAutoresizingMask(NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
    ));
    let table =
        NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, frame.size.width, frame.size.height));
    for (id, title, width) in columns {
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str(id));
        column.setTitle(&NSString::from_str(title));
        column.setWidth(*width);
        column.setEditable(false);
        table.addTableColumn(&column);
    }
    table.setTag(tag);
    table.setAllowsMultipleSelection(false);
    table.setAllowsEmptySelection(true);
    table.setAccessibilityLabel(Some(&NSString::from_str(name)));
    // SAFETY: data source and delegate are weak; `AppPanels` clears them in Drop before the
    // target goes.
    unsafe { table.setDataSource(Some(ProtocolObject::from_ref(target))) };
    // SAFETY: as above.
    unsafe { table.setDelegate(Some(ProtocolObject::from_ref(target))) };
    scroll.setDocumentView(Some(&table));
    (scroll, table)
}

fn set_checked(b: &NSButton, on: bool) {
    b.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
}

// ---- preferences ------------------------------------------------------------------------------

/// The tag of the "restore defaults" button (the other controls carry the index of their
/// preference in `PrefKey::ALL`).
pub(crate) const PREFS_RESET_TAG: isize = 1000;

pub(crate) struct PrefsWindow {
    pub(crate) window: Retained<NSWindow>,
    view: FormView,
    locale: Locale,
}

/// The preferences window (LF6c): a form with a tab per group, built from
/// `crate::prefs_form::prefs_form`; every control reports through `prefControl:` with the tag of
/// its preference.
pub(crate) fn build_prefs(
    mtm: MainThreadMarker,
    target: &AppPanelTarget,
    locale: Locale,
    panel: &PrefsPanel,
    float: bool,
) -> PrefsWindow {
    let form = prefs_form(panel, locale);
    let t: &AnyObject = target;
    let view = build_form(mtm, &form.spec, 520.0, Some(Wiring { target: t, action: sel!(prefControl:) }));
    for (id, control) in view.controls() {
        let any: &AnyObject = control.as_ref();
        let Some(c) = any.downcast_ref::<objc2_app_kit::NSControl>() else { continue };
        if id == RESET_ID {
            c.setTag(PREFS_RESET_TAG);
        } else if let Some((_, key)) = form.keys.iter().find(|(i, _)| *i == id) {
            c.setTag(pref_tag(*key));
        }
    }
    let size = view.size();
    let (window, _) = utility_window(mtm, &form.spec.title, (size.width, size.height), false, float);
    window.setContentView(Some(view.root()));
    PrefsWindow { window, view, locale }
}

/// The tag of a pref control: its index in `PrefKey::ALL`.
fn pref_tag(key: burnburn_presentation::PrefKey) -> isize {
    burnburn_presentation::PrefKey::ALL.iter().position(|k| *k == key).map_or(-1, |i| i as isize)
}

pub(crate) fn refresh_prefs(w: &mut PrefsWindow, panel: &PrefsPanel) {
    let form = prefs_form(panel, w.locale);
    // The notice has a slot, so the shape stays; a refused value stays visible in its field.
    w.view.update(&form.spec);
}

/// Selects the tab called `title` (for tests that drive the window like a user).
pub(crate) fn select_prefs_tab(w: &PrefsWindow, title: &str) -> bool {
    let Some(tabs) = w.view.tab_view() else { return false };
    let index = tabs.tabViewItems().iter().position(|i| i.label().to_string() == title);
    match index {
        Some(i) => {
            tabs.selectTabViewItemAtIndex(i as isize);
            true
        }
        None => false,
    }
}

// ---- shortcut editor --------------------------------------------------------------------------

pub(crate) struct ShortcutsWindow {
    pub(crate) window: Retained<NSWindow>,
    pub(crate) table: Retained<NSTableView>,
    pub(crate) search: Retained<NSTextField>,
    /// Record…, Clear, Default, Reset all, Replace (tags 0…4).
    pub(crate) buttons: [Retained<NSButton>; 5],
    pub(crate) recorder: Retained<KeyRecorder>,
    pub(crate) prompt: Retained<NSTextField>,
    pub(crate) notice: Retained<NSTextField>,
}

pub(crate) fn build_shortcuts(
    mtm: MainThreadMarker,
    target: &AppPanelTarget,
    app: Weak<HostApp>,
    locale: Locale,
    float: bool,
) -> ShortcutsWindow {
    let t: &AnyObject = target;
    let text = |x| app_text(x, locale);
    let (w, h) = (600.0, 560.0);
    let (window, content) = utility_window(mtm, text(AppText::ShortcutsTitle), (w, h), true, float);
    let search = crate::appkit::panels::widgets::field(mtm, rect(16.0, h - 40.0, w - 32.0, 24.0));
    search.setPlaceholderString(Some(&NSString::from_str(text(AppText::ShortcutSearch))));
    search.setAccessibilityLabel(Some(&NSString::from_str(text(AppText::ShortcutSearch))));
    wire(&search, t, sel!(shortcutFilter:), 0);
    content.addSubview(&search);
    let (scroll, table) = make_table(
        mtm,
        rect(16.0, 190.0, w - 32.0, h - 240.0),
        SHORTCUT_TABLE,
        text(AppText::ShortcutsTitle),
        &[
            ("name", text(AppText::ShortcutColCommand), 340.0),
            ("key", text(AppText::ShortcutColKey), 110.0),
            ("default", text(AppText::ShortcutColDefault), 110.0),
        ],
        target,
    );
    content.addSubview(&scroll);
    let mut buttons = Vec::new();
    for (i, (title, x, width)) in [
        (AppText::ShortcutRecord, 16.0, 120.0),
        (AppText::ShortcutClear, 144.0, 130.0),
        (AppText::ShortcutResetOne, 282.0, 100.0),
        (AppText::ShortcutResetAll, w - 150.0, 134.0),
    ]
    .into_iter()
    .enumerate()
    {
        let b = button(mtm, text(title), t, sel!(shortcutButton:), rect(x, 150.0, width, 28.0));
        // Under the table: they keep their distance from the bottom (the last one also from the right).
        b.setAutoresizingMask(if i == 3 { BOTTOM_RIGHT } else { BOTTOM_LEFT });
        b.setTag(i as isize);
        content.addSubview(&b);
        buttons.push(b);
    }
    let recorder = KeyRecorder::new(mtm, app, rect(16.0, 112.0, w - 32.0, 30.0));
    recorder.setWantsLayer(true);
    recorder.setAccessibilityLabel(Some(&NSString::from_str(text(AppText::ShortcutRecording))));
    content.addSubview(&recorder);
    let prompt = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    prompt.setFrame(rect(8.0, 4.0, w - 48.0, 20.0));
    prompt.setTextColor(Some(&NSColor::secondaryLabelColor()));
    recorder.addSubview(&prompt);
    let notice = note(mtm, &content, rect(16.0, 40.0, w - 180.0, 62.0), &NSColor::systemRedColor());
    notice.setAutoresizingMask(BOTTOM_LEFT);
    let replace =
        button(mtm, text(AppText::ShortcutReplace), t, sel!(shortcutButton:), rect(w - 150.0, 62.0, 134.0, 28.0));
    replace.setAutoresizingMask(BOTTOM_RIGHT);
    replace.setTag(4);
    replace.setHidden(true);
    content.addSubview(&replace);
    buttons.push(replace);
    let buttons: [Retained<NSButton>; 5] = buttons.try_into().unwrap_or_else(|v: Vec<Retained<NSButton>>| {
        // Five buttons were pushed above; keep the fixed-size array honest without a panic.
        let spare = || button(mtm, "", t, sel!(shortcutButton:), rect(0.0, 0.0, 0.0, 0.0));
        let mut it = v.into_iter();
        std::array::from_fn(|_| it.next().unwrap_or_else(spare))
    });
    ShortcutsWindow { window, table, search, buttons, recorder, prompt, notice }
}

// ---- history ----------------------------------------------------------------------------------

pub(crate) struct HistoryWindow {
    pub(crate) window: Retained<NSWindow>,
    pub(crate) table: Retained<NSTableView>,
}

#[allow(unsafe_code)]
pub(crate) fn build_history(
    mtm: MainThreadMarker,
    target: &AppPanelTarget,
    locale: Locale,
    float: bool,
) -> HistoryWindow {
    let t: &AnyObject = target;
    let text = |x| app_text(x, locale);
    let (w, h) = (320.0, 460.0);
    let (window, content) = utility_window(mtm, text(AppText::HistoryTitle), (w, h), true, float);
    let hint = note(mtm, &content, rect(12.0, h - 44.0, w - 24.0, 34.0), &NSColor::secondaryLabelColor());
    set_text(&hint, text(AppText::HistoryHint));
    let (scroll, table) = make_table(
        mtm,
        rect(12.0, 12.0, w - 24.0, h - 64.0),
        HISTORY_TABLE,
        text(AppText::HistoryTitle),
        &[("step", text(AppText::HistoryTitle), w - 44.0)],
        target,
    );
    scroll.setAutoresizingMask(objc2_app_kit::NSAutoresizingMaskOptions(
        objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable.0
            | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable.0,
    ));
    // A click on a row (also on the row already selected) is a jump.
    // SAFETY: `historyClicked:` takes the sender (`v@:@`); the target is kept by `AppPanels`.
    unsafe { table.setTarget(Some(t)) };
    // SAFETY: as above.
    unsafe { table.setAction(Some(sel!(historyClicked:))) };
    content.addSubview(&scroll);
    HistoryWindow { window, table }
}

// ---- preview ----------------------------------------------------------------------------------

/// Size of the preview bitmap in points; the image view shows it one to one.
pub(crate) const PREVIEW_SIZE: [u32; 2] = [760, 456];

pub(crate) struct PreviewWindow {
    pub(crate) window: Retained<NSWindow>,
    image: Retained<PreviewView>,
    last_bitmap: Option<BitmapImage>,
    plan: Retained<NSTextField>,
    stale: Retained<NSTextField>,
    time: Retained<NSTextField>,
    picked: Retained<NSTextField>,
    legend: Retained<NSTextField>,
    pub(crate) play: Retained<NSButton>,
    slider: Retained<NSSlider>,
    speed: Retained<NSPopUpButton>,
    travel: Retained<NSButton>,
    previous: Retained<NSButton>,
    next: Retained<NSButton>,
    /// Shown instead of the controls while the window has no plan.
    no_plan: Retained<NSTextField>,
}

#[allow(unsafe_code)]
pub(crate) fn build_preview(
    mtm: MainThreadMarker,
    target: &AppPanelTarget,
    app: Weak<HostApp>,
    locale: Locale,
    speed_titles: &[String],
    float: bool,
) -> PreviewWindow {
    let t: &AnyObject = target;
    let text = |x| app_text(x, locale);
    let (w, h) = (792.0, 680.0);
    let (window, content) = utility_window(mtm, text(AppText::PreviewTitle), (w, h), false, float);
    let plan = caption(mtm, &content, "", rect(16.0, h - 30.0, 380.0, 18.0));
    plan.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    let stale = note(mtm, &content, rect(16.0, h - 56.0, w - 32.0, 22.0), &NSColor::systemOrangeColor());
    let image = PreviewView::new(
        mtm,
        app,
        rect(16.0, h - 60.0 - f64::from(PREVIEW_SIZE[1]), f64::from(PREVIEW_SIZE[0]), f64::from(PREVIEW_SIZE[1])),
    );
    image.setImageScaling(objc2_app_kit::NSImageScaling::ScaleAxesIndependently);
    image.setImageFrameStyle(objc2_app_kit::NSImageFrameStyle::GrayBezel);
    image.setAccessibilityLabel(Some(&NSString::from_str(text(AppText::PreviewTitle))));
    content.addSubview(&image);
    let base = h - 60.0 - f64::from(PREVIEW_SIZE[1]);
    let play = button(mtm, text(AppText::PreviewPlay), t, sel!(previewButton:), rect(16.0, base - 38.0, 100.0, 28.0));
    play.setTag(0);
    content.addSubview(&play);
    // SAFETY: `previewSeek:` takes the sender (`v@:@`); the target is kept by `AppPanels`.
    let slider = unsafe {
        NSSlider::sliderWithValue_minValue_maxValue_target_action(0.0, 0.0, 1.0, Some(t), Some(sel!(previewSeek:)), mtm)
    };
    slider.setFrame(rect(126.0, base - 36.0, 440.0, 24.0));
    slider.setContinuous(true);
    slider.setAccessibilityLabel(Some(&NSString::from_str(text(AppText::PreviewTime))));
    content.addSubview(&slider);
    let time = caption(mtm, &content, "", rect(576.0, base - 34.0, w - 592.0, 20.0));
    let speed =
        NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(16.0, base - 76.0, 80.0, 26.0), false);
    for title in speed_titles {
        speed.addItemWithTitle(&NSString::from_str(title));
    }
    speed.setAccessibilityLabel(Some(&NSString::from_str(text(AppText::PreviewSpeed))));
    wire(&speed, t, sel!(previewSpeed:), 0);
    content.addSubview(&speed);
    // SAFETY: no target or action yet; `wire` sets both right below.
    let travel = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(text(AppText::PreviewShowTravel)),
            None,
            None,
            mtm,
        )
    };
    travel.setFrame(rect(110.0, base - 74.0, 200.0, 22.0));
    wire(&travel, t, sel!(previewButton:), 9);
    content.addSubview(&travel);
    let previous = button(mtm, "", t, sel!(previewButton:), rect(w - 106.0, base - 76.0, 40.0, 28.0));
    previous.setTag(-1);
    symbols::apply(&previous, Symbol::Previous, locale);
    let next = button(mtm, "", t, sel!(previewButton:), rect(w - 60.0, base - 76.0, 40.0, 28.0));
    next.setTag(1);
    symbols::apply(&next, Symbol::Next, locale);
    content.addSubview(&previous);
    content.addSubview(&next);
    let picked = note(mtm, &content, rect(16.0, base - 106.0, w - 32.0, 22.0), &NSColor::controlTextColor());
    let legend = caption(mtm, &content, "", rect(16.0, base - 130.0, w - 32.0, 18.0));
    legend.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    legend.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let no_plan = note(mtm, &content, rect(16.0, base + 200.0, w - 32.0, 40.0), &NSColor::secondaryLabelColor());
    set_text(&no_plan, text(AppText::PreviewNoPlan));
    no_plan.setHidden(true);
    PreviewWindow {
        window,
        image,
        last_bitmap: None,
        plan,
        stale,
        time,
        picked,
        legend,
        play,
        slider,
        speed,
        travel,
        previous,
        next,
        no_plan,
    }
}

/// What the preview window shows: `None` for a window without a plan.
pub(crate) fn refresh_preview(w: &mut PreviewWindow, panel: Option<&PreviewPanel>, bitmap: Option<&BitmapImage>) {
    let has = panel.is_some();
    for control in [&w.play as &NSView, &w.slider, &w.speed, &w.travel, &w.previous, &w.next, &w.image] {
        control.setHidden(!has);
    }
    w.no_plan.setHidden(has);
    let Some(p) = panel else {
        for l in [&w.plan, &w.stale, &w.time, &w.picked, &w.legend] {
            set_text(l, "");
        }
        return;
    };
    set_text(&w.plan, &p.plan_text);
    set_text(&w.stale, p.stale.as_deref().unwrap_or(""));
    set_text(&w.time, &p.time_text);
    set_text(&w.picked, p.picked.as_deref().unwrap_or(&p.picked_hint));
    set_text(&w.legend, p.legend);
    w.play.setTitle(&NSString::from_str(p.play_title));
    w.slider.setMaxValue(p.slider_max.max(0.001));
    if !w.slider_is_dragging() {
        w.slider.setDoubleValue(p.slider_value);
    }
    if w.speed.indexOfSelectedItem() != p.speed_index as isize {
        w.speed.selectItemAtIndex(p.speed_index as isize);
    }
    set_checked(&w.travel, p.show_travel);
    show_bitmap(&w.image, &mut w.last_bitmap, bitmap);
}

impl PreviewWindow {
    pub(crate) fn image(&self) -> Retained<PreviewView> {
        self.image.clone()
    }

    /// The slider is being dragged (its value is the user's, not the playback's).
    fn slider_is_dragging(&self) -> bool {
        self.window.currentEvent().is_some_and(|e| {
            e.r#type() == objc2_app_kit::NSEventType::LeftMouseDragged
                && self.window.firstResponder().is_some_and(|r| {
                    let slider: &AnyObject = &self.slider;
                    let responder: &AnyObject = &r;
                    std::ptr::eq(slider, responder)
                })
        })
    }
}

// ---- RD inspector -----------------------------------------------------------------------------

pub(crate) struct RdWindow {
    pub(crate) window: Retained<NSWindow>,
    pub(crate) table: Retained<NSTableView>,
    header: Retained<NSTextField>,
    banner: Retained<NSTextField>,
}

pub(crate) fn build_rd(mtm: MainThreadMarker, target: &AppPanelTarget, locale: Locale, float: bool) -> RdWindow {
    let t: &AnyObject = target;
    let text = |x| app_text(x, locale);
    let (w, h) = (940.0, 600.0);
    let (window, content) = utility_window(mtm, text(AppText::RdTitle), (w, h), true, float);
    let header = caption(mtm, &content, "", rect(16.0, h - 34.0, w - 250.0, 20.0));
    header.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    // The header keeps its width: the refresh button stays at the right edge beside it.
    header.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    let refresh = button(mtm, text(AppText::InspectRd), t, sel!(rdRefresh:), rect(w - 226.0, h - 38.0, 210.0, 28.0));
    refresh.setAutoresizingMask(NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewMinXMargin.0 | NSAutoresizingMaskOptions::ViewMinYMargin.0,
    ));
    content.addSubview(&refresh);
    let banner = note(mtm, &content, rect(16.0, h - 76.0, w - 32.0, 34.0), &NSColor::systemOrangeColor());
    let (scroll, table) = make_table(
        mtm,
        rect(16.0, 56.0, w - 32.0, h - 140.0),
        RD_TABLE,
        text(AppText::RdTitle),
        &[
            ("offset", text(AppText::RdColOffset), 80.0),
            ("bytes", text(AppText::RdColBytes), 250.0),
            ("name", text(AppText::RdColName), 210.0),
            ("decoded", text(AppText::RdColDecoded), 340.0),
        ],
        target,
    );
    scroll.setAutoresizingMask(objc2_app_kit::NSAutoresizingMaskOptions(
        objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable.0
            | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable.0,
    ));
    content.addSubview(&scroll);
    let notes = note(mtm, &content, rect(16.0, 8.0, w - 32.0, 40.0), &NSColor::secondaryLabelColor());
    notes.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    notes.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    notes.setAutoresizingMask(NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewMaxYMargin.0,
    ));
    set_text(&notes, &format!("{} {}", text(AppText::RdNoUpload), text(AppText::RdNameIsLabel)));
    RdWindow { window, table, header, banner }
}

pub(crate) fn refresh_rd(w: &RdWindow, panel: Option<&RdPanel>, locale: Locale) {
    set_text(&w.header, panel.map_or_else(|| app_text(AppText::RdNoPlan, locale), |p| p.header.as_str()));
    set_text(&w.banner, panel.and_then(|p| p.banner.as_deref()).unwrap_or(""));
}

// ---- small updates the host makes -------------------------------------------------------------

pub(crate) fn set_notice(w: &ShortcutsWindow, text: &str, show_replace: bool) {
    set_text(&w.notice, text);
    w.buttons[4].setHidden(!show_replace);
}

pub(crate) fn notice_text(w: &ShortcutsWindow) -> String {
    w.notice.stringValue().to_string()
}

pub(crate) fn preview_texts(w: &PreviewWindow) -> (String, String) {
    (w.time.stringValue().to_string(), w.picked.stringValue().to_string())
}

pub(crate) fn set_prompt(w: &ShortcutsWindow, text: &str) {
    set_text(&w.prompt, text);
}

/// The accessory of the support save panel: two privacy checkboxes (off) and the exact report in a
/// read-only text view. The checkboxes call `supportOption:` so the report follows them.
#[allow(unsafe_code)]
pub(crate) fn support_accessory(
    mtm: MainThreadMarker,
    target: &AppPanelTarget,
    locale: Locale,
) -> (Retained<NSButton>, Retained<NSButton>, Retained<NSTextView>, Retained<NSTextField>, Retained<NSView>) {
    let t: &AnyObject = target;
    let (w, h) = (560.0, 330.0);
    let accessory = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, w, h));
    let check = |text: AppText, y: f64| {
        // SAFETY: no target or action yet; `wire` sets both right below.
        let b = unsafe {
            NSButton::checkboxWithTitle_target_action(&NSString::from_str(app_text(text, locale)), None, None, mtm)
        };
        b.setFrame(rect(12.0, y, w - 24.0, 22.0));
        b.setState(NSControlStateValueOff);
        wire(&b, t, sel!(supportOption:), 0);
        accessory.addSubview(&b);
        b
    };
    let paths = check(AppText::SupportPaths, h - 28.0);
    let ids = check(AppText::SupportIds, h - 54.0);
    let scroll = NSTextView::scrollableTextView(mtm);
    scroll.setFrame(rect(12.0, 8.0, w - 24.0, h - 72.0));
    scroll.setHasVerticalScroller(true);
    let report = scroll
        .documentView()
        .and_then(|v| v.downcast::<NSTextView>().ok())
        .unwrap_or_else(|| NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, w - 24.0, h - 72.0)));
    report.setEditable(false);
    report.setSelectable(true);
    report.setFont(Some(&NSFont::userFixedPitchFontOfSize(11.0).unwrap_or_else(|| NSFont::systemFontOfSize(11.0))));
    report.setAccessibilityLabel(Some(&NSString::from_str(app_text(AppText::SupportTitle, locale))));
    accessory.addSubview(&scroll);
    let note = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    (paths, ids, report, note, accessory)
}
