//! The Cut Settings sheet of one layer.

use crate::appkit::panels::widgets::{field, label, push_button, rect, set_text};
use crate::appkit::window::DocWindow;

use burnburn_presentation::{AppViewModel, CutExtras, CutField, CutValue};
use burnburn_types::{DocumentId, OperationId};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSLineBreakMode, NSPopUpButton, NSScrollView, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::NSString;

use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{
    OptionRow, RowControl, choice_items, cut_settings_panel, option_caption, option_rows, sheet_mode_of,
};

/// The control of one option field.
#[derive(Clone)]
pub(crate) enum OptionControl {
    Text(Retained<NSTextField>),
    Flag(Retained<NSButton>),
    Popup(Retained<NSPopUpButton>),
}

impl OptionControl {
    fn view(&self) -> &NSView {
        match self {
            OptionControl::Text(f) => f,
            OptionControl::Flag(b) => b,
            OptionControl::Popup(p) => p,
        }
    }

    /// Whether the control is on the sheet now (a field of another mode is hidden).
    pub(crate) fn shown(&self) -> bool {
        !self.view().isHidden()
    }

    /// The value as typed or chosen.
    pub(crate) fn read(&self) -> CutValue {
        match self {
            OptionControl::Text(f) => CutValue::Text(f.stringValue().to_string()),
            OptionControl::Flag(b) => CutValue::Flag(b.state() == NSControlStateValueOn),
            OptionControl::Popup(p) => CutValue::Choice(usize::try_from(p.indexOfSelectedItem()).unwrap_or(0)),
        }
    }

    fn set_enabled(&self, enabled: bool) {
        match self {
            OptionControl::Text(f) => f.setEnabled(enabled),
            OptionControl::Flag(b) => b.setEnabled(enabled),
            OptionControl::Popup(p) => p.setEnabled(enabled),
        }
    }

    /// Target and action for the controls whose change can grey other rows.
    #[allow(unsafe_code)]
    pub(crate) fn set_target(&self, target: Option<&objc2::runtime::AnyObject>) {
        let action = target.map(|_| objc2::sel!(cutSettingsOptionChanged:));
        match self {
            OptionControl::Text(_) => {}
            OptionControl::Flag(b) => {
                // SAFETY: a nil target cannot dangle; a controller target is weak and DocWindow keeps
                // the controller alive while the control exists (cleared in Drop).
                unsafe { b.setTarget(target) };
                // SAFETY: as above; the action is a controller method taking the sender.
                unsafe { b.setAction(action) };
            }
            OptionControl::Popup(p) => {
                // SAFETY: as above.
                unsafe { p.setTarget(target) };
                // SAFETY: as above.
                unsafe { p.setAction(action) };
            }
        }
    }
}

/// One option control of the Cut Settings sheet, borrowed for tests (`WindowParts::cut_options`).
pub struct CutOptionPart<'a> {
    pub field: CutField,
    pub control: &'a objc2_app_kit::NSControl,
    /// The note under the row (why it is greyed, or the advice).
    pub note: &'a NSTextField,
}

impl OptionWidgets {
    pub(crate) fn part(&self) -> CutOptionPart<'_> {
        let control: &objc2_app_kit::NSControl = match &self.control {
            OptionControl::Text(f) => f,
            OptionControl::Flag(b) => b,
            OptionControl::Popup(p) => p,
        };
        CutOptionPart { field: self.field, control, note: &self.note }
    }
}

/// Everything one option row consists of.
pub(crate) struct OptionWidgets {
    pub(crate) field: CutField,
    pub(crate) caption: Retained<NSTextField>,
    pub(crate) control: OptionControl,
    pub(crate) note: Retained<NSTextField>,
}

/// Size of the sheet and of its scrolling option area.
const SHEET_W: f64 = 580.0;
const SHEET_H: f64 = 700.0;
const OPTIONS_BOTTOM: f64 = 92.0;
const OPTION_ROW_H: f64 = 26.0;
const OPTION_NOTE_H: f64 = 28.0;

/// The Cut Settings sheet of one layer.
pub(crate) struct CutSettingsSheet {
    pub(crate) sheet: Retained<NSWindow>,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) mode: Retained<NSPopUpButton>,
    /// Speed, min power, max power, passes, kerf.
    pub(crate) fields: [Retained<NSTextField>; 5],
    pub(crate) output: Retained<NSButton>,
    pub(crate) visible: Retained<NSButton>,
    pub(crate) error: Retained<NSTextField>,
    pub(crate) ok: Retained<NSButton>,
    pub(crate) cancel: Retained<NSButton>,
    /// The operation whose values are in the fields while the sheet is up.
    pub(crate) shown: Option<OperationId>,
    /// One row per option field, in a scrolling area; rows of other modes are hidden.
    pub(crate) options: Vec<OptionWidgets>,
    options_view: Retained<NSView>,
    options_scroll: Retained<NSScrollView>,
}

/// The Cut Settings sheet (targets are set once the controller exists).
#[allow(unsafe_code)]
pub(crate) fn cut_settings_sheet(mtm: MainThreadMarker, locale: Locale) -> CutSettingsSheet {
    let t = |text: HostText| host_text(text, locale);
    let (w, h) = (SHEET_W, SHEET_H);
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let sheet = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, w, h),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with this Retained.
    unsafe { sheet.setReleasedWhenClosed(false) };
    sheet.setTitle(&NSString::from_str(t(HostText::CutSettingsTitle)));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, w, h));
    let title = label(mtm, "", rect(16.0, h - 36.0, w - 32.0, 20.0));
    title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    content.addSubview(&title);
    let row_y = |i: usize| h - 72.0 - i as f64 * 32.0;
    let caption = |i: usize, text: HostText| {
        let l = label(mtm, t(text), rect(16.0, row_y(i), 190.0, 22.0));
        content.addSubview(&l);
    };
    caption(0, HostText::CutSettingsMode);
    let mode = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        rect(210.0, row_y(0) - 2.0, 194.0, 26.0),
        false,
    );
    // Blocked modes stay listed but disabled (C05): enablement is ours, not AppKit's.
    mode.setAutoenablesItems(false);
    mode.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::CutSettingsMode))));
    content.addSubview(&mode);
    let captions = [
        HostText::CutSettingsSpeed,
        HostText::CutSettingsPowerMin,
        HostText::CutSettingsPowerMax,
        HostText::CutSettingsPasses,
        HostText::CutSettingsKerf,
    ];
    let fields = captions.map(|text| {
        let i = 1 + captions.iter().position(|c| *c == text).unwrap_or(0);
        caption(i, text);
        let f = field(mtm, rect(210.0, row_y(i), 194.0, 24.0));
        f.setAccessibilityLabel(Some(&NSString::from_str(t(text))));
        content.addSubview(&f);
        f
    });
    let checkbox = |i: usize, text: HostText| {
        // SAFETY: no target and no action yet; set once the controller exists (not needed: read on OK).
        let b = unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(t(text)), None, None, mtm) };
        b.setFrame(rect(16.0, row_y(i), w - 32.0, 22.0));
        content.addSubview(&b);
        b
    };
    let output = checkbox(6, HostText::CutSettingsOutput);
    let visible = checkbox(7, HostText::CutSettingsVisible);
    let error = label(mtm, "", rect(16.0, 50.0, w - 32.0, 34.0));
    error.setTextColor(Some(&NSColor::systemRedColor()));
    error.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    content.addSubview(&error);
    let cancel = push_button(mtm, t(HostText::CutSettingsCancel));
    cancel.setFrame(rect(w - 232.0, 12.0, 104.0, 28.0));
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let ok = push_button(mtm, t(HostText::CutSettingsOk));
    ok.setFrame(rect(w - 116.0, 12.0, 104.0, 28.0));
    ok.setKeyEquivalent(&NSString::from_str("\r"));
    content.addSubview(&cancel);
    content.addSubview(&ok);
    let scroll_h = row_y(7) - 8.0 - OPTIONS_BOTTOM;
    let options_scroll =
        NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(16.0, OPTIONS_BOTTOM, w - 32.0, scroll_h));
    options_scroll.setHasVerticalScroller(true);
    options_scroll.setDrawsBackground(false);
    options_scroll.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::CutSettingsOptions))));
    let options_view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, w - 48.0, scroll_h));
    let options = CutField::ALL.map(|field| option_widgets(mtm, &options_view, field, locale)).into_iter().collect();
    options_scroll.setDocumentView(Some(&options_view));
    content.addSubview(&options_scroll);
    sheet.setContentView(Some(&content));
    CutSettingsSheet {
        sheet,
        title,
        mode,
        fields,
        output,
        visible,
        error,
        ok,
        cancel,
        shown: None,
        options,
        options_view,
        options_scroll,
    }
}

/// The widgets of one option field (hidden until a refresh places them).
fn option_widgets(mtm: MainThreadMarker, parent: &NSView, field: CutField, locale: Locale) -> OptionWidgets {
    let caption_text = option_caption(field, locale);
    let caption = label(mtm, "", rect(0.0, 0.0, 240.0, 20.0));
    let note = label(mtm, "", rect(0.0, 0.0, SHEET_W - 80.0, OPTION_NOTE_H));
    note.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    note.setTextColor(Some(&NSColor::secondaryLabelColor()));
    note.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    let control = match field.control() {
        burnburn_presentation::CutControl::Text => {
            caption.setStringValue(&NSString::from_str(caption_text));
            let f = field_widget(mtm);
            f.setAccessibilityLabel(Some(&NSString::from_str(caption_text)));
            OptionControl::Text(f)
        }
        burnburn_presentation::CutControl::Flag => {
            // SAFETY: no target and no action yet; set once the controller exists.
            #[allow(unsafe_code)]
            let b = unsafe {
                NSButton::checkboxWithTitle_target_action(&NSString::from_str(caption_text), None, None, mtm)
            };
            OptionControl::Flag(b)
        }
        burnburn_presentation::CutControl::Choice => {
            caption.setStringValue(&NSString::from_str(caption_text));
            let p =
                NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(0.0, 0.0, 280.0, 26.0), false);
            p.setAutoenablesItems(false);
            p.setAccessibilityLabel(Some(&NSString::from_str(caption_text)));
            OptionControl::Popup(p)
        }
    };
    for v in [&*caption as &NSView, control.view(), &*note] {
        v.setHidden(true);
        parent.addSubview(v);
    }
    OptionWidgets { field, caption, control, note }
}

fn field_widget(mtm: MainThreadMarker) -> Retained<NSTextField> {
    field(mtm, rect(0.0, 0.0, 150.0, 24.0))
}

/// The values on the option widgets that are on the sheet now.
pub(crate) fn read_options(options: &[(CutField, OptionControl)]) -> Vec<(CutField, CutValue)> {
    options.iter().filter(|(_, c)| c.shown()).map(|(f, c)| (*f, c.read())).collect()
}

impl DocWindow {
    /// Show, fill or end the Cut Settings sheet as presentation opens and closes it. The fields
    /// are filled once per opening, so a refused OK keeps what the user typed; the option rows
    /// are laid out again on every refresh from the values on the sheet (a mode, a switch or a
    /// pop-up changes which rows exist and which are greyed).
    pub(crate) fn refresh_cut_settings(&mut self, vm: &AppViewModel, document: DocumentId) {
        let locale = self.locale;
        let c = &mut self.cut_settings;
        let Some(panel) = cut_settings_panel(vm, document, locale) else {
            if c.shown.take().is_some() {
                let (window, sheet) = (self.window.clone(), c.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("ending the Cut Settings sheet", || window.endSheet(&sheet)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        let Some(sheet_vm) = vm.document(document).and_then(|d| d.cut_settings.as_ref()) else { return };
        let titles: Vec<String> = c.mode.itemTitles().iter().map(|t| t.to_string()).collect();
        let wanted: Vec<String> = panel.modes.iter().map(|m| m.title.clone()).collect();
        if titles != wanted {
            c.mode.removeAllItems();
            for title in &wanted {
                c.mode.addItemWithTitle(&NSString::from_str(title));
            }
        }
        for (i, m) in panel.modes.iter().enumerate() {
            if let Some(item) = c.mode.itemAtIndex(i as isize) {
                item.setEnabled(m.enabled);
            }
        }
        set_text(&c.error, &panel.error);
        let opening = c.shown != Some(panel.operation);
        if opening {
            set_text(&c.title, &panel.title);
            c.mode.selectItemAtIndex(panel.mode_index.map_or(-1, |i| i as isize));
            for (f, text) in
                c.fields.iter().zip([&panel.speed, &panel.power_min, &panel.power_max, &panel.passes, &panel.kerf])
            {
                f.setStringValue(&NSString::from_str(text));
            }
            for (b, on) in [(&c.output, panel.output), (&c.visible, panel.visible)] {
                b.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
            }
            fill_options(c, sheet_vm, locale);
        }
        let kind = sheet_mode_of(sheet_vm, c.mode.indexOfSelectedItem());
        c.fields[4].setEnabled(kind == burnburn_presentation::OperationKind::Line);
        layout_options(c, sheet_vm, kind, locale, opening);
        if opening {
            let attached = c.shown.is_some();
            c.shown = Some(panel.operation);
            if !attached {
                let (window, sheet) = (self.window.clone(), c.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("showing the Cut Settings sheet", || {
                    window.beginSheet_completionHandler(&sheet, None)
                }) {
                    eprintln!("burnburn: {e}");
                    c.shown = None;
                }
            }
        }
    }
}

/// Put the stored values of the operation on the option widgets (once per opening).
fn fill_options(c: &CutSettingsSheet, sheet: &burnburn_presentation::CutSettingsViewModel, locale: Locale) {
    for w in &c.options {
        let Some(value) = sheet.extras.get(w.field) else { continue };
        match (&w.control, value) {
            (OptionControl::Text(f), CutValue::Text(t)) => f.setStringValue(&NSString::from_str(t)),
            (OptionControl::Flag(b), CutValue::Flag(on)) => {
                b.setState(if *on { NSControlStateValueOn } else { NSControlStateValueOff })
            }
            (OptionControl::Popup(p), CutValue::Choice(i)) => {
                set_popup_items(p, &choice_items(sheet, w.field, locale));
                p.selectItemAtIndex(*i as isize);
            }
            _ => {}
        }
    }
}

fn set_popup_items(p: &NSPopUpButton, items: &[crate::panels::ModeItem]) {
    let titles: Vec<String> = p.itemTitles().iter().map(|t| t.to_string()).collect();
    let wanted: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
    if titles.iter().map(String::as_str).collect::<Vec<_>>() != wanted {
        let selected = p.indexOfSelectedItem();
        p.removeAllItems();
        for title in &wanted {
            p.addItemWithTitle(&NSString::from_str(title));
        }
        p.selectItemAtIndex(selected);
    }
    for (i, item) in items.iter().enumerate() {
        if let Some(entry) = p.itemAtIndex(i as isize) {
            entry.setEnabled(item.enabled);
        }
    }
}

/// The values the option widgets hold now, for the fields this sheet has.
fn live_options(c: &CutSettingsSheet, sheet: &burnburn_presentation::CutSettingsViewModel) -> CutExtras {
    let mut live = CutExtras::new();
    for w in &c.options {
        if sheet.extras.get(w.field).is_some() {
            live.set(w.field, w.control.read());
        }
    }
    live
}

/// Show the rows the chosen mode has, with their notes, and stack them from the top.
fn layout_options(
    c: &CutSettingsSheet,
    sheet: &burnburn_presentation::CutSettingsViewModel,
    kind: burnburn_presentation::OperationKind,
    locale: Locale,
    to_top: bool,
) {
    let rows: Vec<OptionRow> = option_rows(sheet, kind, &live_options(c, sheet), locale);
    let height_of = |r: &OptionRow| OPTION_ROW_H + if r.note.is_empty() { 0.0 } else { OPTION_NOTE_H };
    let content_h = rows.iter().map(height_of).sum::<f64>() + 8.0;
    let clip_h = c.options_scroll.frame().size.height;
    let total_h = content_h.max(clip_h);
    let width = c.options_view.frame().size.width;
    c.options_view.setFrame(rect(0.0, 0.0, width, total_h));
    let mut y = total_h - 4.0;
    for w in &c.options {
        let Some(row) = rows.iter().find(|r| r.field == w.field) else {
            for v in [&*w.caption as &NSView, w.control.view(), &*w.note] {
                v.setHidden(true);
            }
            continue;
        };
        y -= OPTION_ROW_H;
        let has_caption = matches!(row.control, RowControl::Text(_) | RowControl::Popup { .. });
        w.caption.setHidden(!has_caption);
        w.caption.setFrame(rect(0.0, y + 2.0, 244.0, 20.0));
        w.control.view().setHidden(false);
        match (&w.control, &row.control) {
            (OptionControl::Text(f), RowControl::Text(_)) => f.setFrame(rect(250.0, y, 150.0, 24.0)),
            (OptionControl::Flag(b), RowControl::Check(_)) => b.setFrame(rect(0.0, y + 2.0, width, 22.0)),
            (OptionControl::Popup(p), RowControl::Popup { items, .. }) => {
                set_popup_items(p, items);
                p.setFrame(rect(250.0, y - 1.0, width - 250.0, 26.0));
            }
            _ => {}
        }
        w.control.set_enabled(row.enabled);
        set_text(&w.note, &row.note);
        let tip = NSString::from_str(&row.note);
        w.control.view().setToolTip(if row.note.is_empty() { None } else { Some(&tip) });
        w.note.setHidden(row.note.is_empty());
        if !row.note.is_empty() {
            y -= OPTION_NOTE_H;
            w.note.setFrame(rect(20.0, y + 4.0, width - 20.0, OPTION_NOTE_H));
        }
    }
    if to_top {
        c.options_view.scrollPoint(objc2_foundation::NSPoint::new(0.0, total_h));
    }
}
