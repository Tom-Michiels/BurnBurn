//! The text toolbar and the text-on-path sheet (W3-UI-tekst): a floating utility panel that
//! follows the selected text object, and the sheet behind Tekst ▸ Tekst op pad…. Like every panel
//! it only copies the headless model (`crate::panels::text_panels`) into widgets and reports a
//! control as a slot with its value (`textform::TextTarget`).

use std::rc::Weak;

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSControl, NSPanel, NSPopUpButton, NSSegmentedControl, NSTextField, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSSize, NSString};

use super::image::sheet_window;
use super::textform::{
    Cells, Form, FormRow, TextTarget, caption, check, message_line, new_target, popup, push, segments, set_check,
    set_field, set_popup_items, text_field, unwire,
};
use super::widgets::{rect, set_text};
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{Slot, TextText, text_panels, text_text};

const PANEL_WIDTH: f64 = 340.0;
const SHEET_WIDTH: f64 = 420.0;

/// A floating utility panel that shows while its model is present and the window is active.
pub(crate) struct ToolPanel {
    pub(crate) panel: Retained<NSPanel>,
    pub(crate) shown: bool,
}

impl ToolPanel {
    pub(crate) fn new(mtm: objc2::MainThreadMarker, title: &str, form: &Form) -> ToolPanel {
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            rect(0.0, 0.0, form.width, 200.0),
            NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow,
            NSBackingStoreType::Buffered,
            false,
        );
        // SAFETY: ownership stays with this Retained.
        #[allow(unsafe_code)]
        unsafe {
            panel.setReleasedWhenClosed(false)
        };
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(true);
        panel.setTitle(&NSString::from_str(title));
        panel.setContentView(Some(&form.content));
        ToolPanel { panel, shown: false }
    }

    /// Shows the panel at `offset` from the window's top-left corner with room for `height`.
    pub(crate) fn show(&mut self, window: &NSWindow, offset: (f64, f64), height: f64, width: f64) {
        let was_shown = self.shown;
        self.shown = true;
        let frame = window.frame();
        let panel = self.panel.clone();
        if let Err(e) = crate::appkit::objc_try("showing a text panel", || {
            panel.setContentSize(NSSize::new(width, height));
            if !was_shown {
                panel.setFrameTopLeftPoint(NSPoint::new(
                    frame.origin.x + offset.0,
                    frame.origin.y + frame.size.height - offset.1,
                ));
                panel.orderFront(None);
            }
        }) {
            eprintln!("burnburn: {e}");
        }
    }

    pub(crate) fn hide(&mut self) {
        if self.shown {
            self.shown = false;
            let panel = self.panel.clone();
            if let Err(e) = crate::appkit::objc_try("hiding a text panel", || panel.orderOut(None)) {
                eprintln!("burnburn: {e}");
            }
        }
    }
}

/// A sheet shown on the document window while its model is present.
pub(crate) struct SheetHolder {
    pub(crate) sheet: Retained<NSWindow>,
    pub(crate) shown: bool,
}

impl SheetHolder {
    pub(crate) fn show(&mut self, window: &Retained<NSWindow>, size: (f64, f64)) {
        let sheet = self.sheet.clone();
        if let Err(e) =
            crate::appkit::objc_try("sizing a text sheet", || sheet.setContentSize(NSSize::new(size.0, size.1)))
        {
            eprintln!("burnburn: {e}");
        }
        if self.shown {
            return;
        }
        self.shown = true;
        let (window, sheet) = (window.clone(), self.sheet.clone());
        if let Err(e) =
            crate::appkit::objc_try("showing a text sheet", || window.beginSheet_completionHandler(&sheet, None))
        {
            eprintln!("burnburn: {e}");
            self.shown = false;
        }
    }

    pub(crate) fn hide(&mut self, window: &NSWindow) {
        if self.shown {
            self.shown = false;
            window.endSheet(&self.sheet);
        }
    }
}

/// A control as a plain view, for a form row.
macro_rules! view {
    ($v:expr) => {{
        let view: &objc2_app_kit::NSView = &$v;
        objc2::Message::retain(view)
    }};
}
pub(crate) use view;

/// The toolbar panel, the text-on-path sheet and their widgets.
pub(crate) struct TextSection {
    target: Retained<TextTarget>,
    controls: Vec<Retained<NSControl>>,
    toolbar: ToolPanel,
    form: Form,
    family: Retained<NSPopUpButton>,
    bold: Retained<NSButton>,
    italic: Retained<NSButton>,
    uppercase: Retained<NSButton>,
    welded: Retained<NSButton>,
    single_line: Retained<NSButton>,
    align: Retained<NSSegmentedControl>,
    valign: Retained<NSPopUpButton>,
    h_space: Retained<NSTextField>,
    v_space: Retained<NSTextField>,
    height: Retained<NSTextField>,
    note: Retained<NSTextField>,
    convert: Retained<NSButton>,
    // --- text on a path
    path: SheetHolder,
    path_form: Form,
    path_distort: Retained<NSPopUpButton>,
    path_start: Retained<NSTextField>,
    path_offset: Retained<NSTextField>,
    path_reverse: Retained<NSButton>,
    path_live: Retained<NSButton>,
    path_note: Retained<NSTextField>,
    path_apply: Retained<NSButton>,
    path_cancel: Retained<NSButton>,
}

fn controls_of(list: &[&NSControl]) -> Vec<Retained<NSControl>> {
    list.iter().map(|c| objc2::Message::retain(*c)).collect()
}

/// A view's retained handle for a labelled row.
fn labelled_row(
    mtm: objc2::MainThreadMarker,
    locale: Locale,
    text: TextText,
    control: &objc2_app_kit::NSView,
    height: f64,
) -> FormRow {
    FormRow::new(Cells::Labeled(caption(mtm, text_text(text, locale)), objc2::Message::retain(control)), height)
}

pub(crate) fn text_section(
    mtm: objc2::MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> TextSection {
    let t = |text: TextText| text_text(text, locale);
    let target = new_target(mtm, app, document);
    let labelled = |text: TextText, control: &objc2_app_kit::NSView, height: f64| -> FormRow {
        labelled_row(mtm, locale, text, control, height)
    };
    let family = popup(mtm, &target, Slot::Family, t(TextText::Font));
    let bold = check(mtm, &target, Slot::Bold, t(TextText::Bold));
    let italic = check(mtm, &target, Slot::Italic, t(TextText::Italic));
    let uppercase = check(mtm, &target, Slot::Uppercase, t(TextText::Uppercase));
    let welded = check(mtm, &target, Slot::Welded, t(TextText::Welded));
    let single_line = check(mtm, &target, Slot::SingleLine, t(TextText::SingleLine));
    let align = segments(
        mtm,
        &target,
        Slot::Align,
        &[t(TextText::AlignLeft), t(TextText::AlignCenter), t(TextText::AlignRight), t(TextText::AlignJustify)],
        t(TextText::Alignment),
    );
    let valign = popup(mtm, &target, Slot::VAlign, t(TextText::Vertical));
    for title in [TextText::VBaseline, TextText::VTop, TextText::VMiddle, TextText::VBottom] {
        valign.addItemWithTitle(&NSString::from_str(t(title)));
    }
    let h_space = text_field(mtm, &target, Slot::HSpace, t(TextText::HSpace));
    let v_space = text_field(mtm, &target, Slot::VSpace, t(TextText::VSpace));
    let height = text_field(mtm, &target, Slot::Height, t(TextText::Height));
    let note = message_line(mtm, true);
    let convert = push(mtm, &target, Slot::Convert, t(TextText::Convert));
    let rows = vec![
        labelled(TextText::Font, &family, 26.0),
        FormRow::new(Cells::Side(vec![view!(bold), view!(italic), view!(uppercase)]), 22.0),
        FormRow::new(Cells::Side(vec![view!(welded), view!(single_line)]), 22.0),
        labelled(TextText::Alignment, &align, 24.0),
        labelled(TextText::Vertical, &valign, 26.0),
        labelled(TextText::HSpace, &h_space, 24.0),
        labelled(TextText::VSpace, &v_space, 24.0),
        labelled(TextText::Height, &height, 24.0),
        FormRow::new(Cells::Full(view!(note)), 32.0),
        FormRow::new(Cells::Full(view!(convert)), 28.0),
    ];
    let form = Form::new(mtm, PANEL_WIDTH, rows);
    let toolbar = ToolPanel::new(mtm, t(TextText::ToolbarTitle), &form);

    // --- the text-on-path sheet
    let path_distort = popup(mtm, &target, Slot::PathDistort, t(TextText::Distort));
    path_distort.addItemWithTitle(&NSString::from_str(t(TextText::DistortRigid)));
    path_distort.addItemWithTitle(&NSString::from_str(t(TextText::DistortWarp)));
    let path_start = text_field(mtm, &target, Slot::PathStart, t(TextText::PathStart));
    let path_offset = text_field(mtm, &target, Slot::PathOffset, t(TextText::PathOffset));
    let path_reverse = check(mtm, &target, Slot::PathReverse, t(TextText::PathReverse));
    let path_live = check(mtm, &target, Slot::PathLive, t(TextText::PathLive));
    let path_note = message_line(mtm, true);
    let path_cancel = push(mtm, &target, Slot::PathCancel, t(TextText::Cancel));
    let path_apply = push(mtm, &target, Slot::PathApply, t(TextText::Apply));
    path_apply.setKeyEquivalent(&NSString::from_str("\r"));
    path_cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let path_rows = vec![
        labelled(TextText::Distort, &path_distort, 26.0),
        labelled(TextText::PathStart, &path_start, 24.0),
        labelled(TextText::PathOffset, &path_offset, 24.0),
        FormRow::new(Cells::Full(view!(path_reverse)), 22.0),
        FormRow::new(Cells::Full(view!(path_live)), 22.0),
        FormRow::new(Cells::Full(view!(path_note)), 32.0),
        FormRow::new(Cells::Side(vec![view!(path_cancel), view!(path_apply)]), 28.0),
    ];
    let path_form = Form::new(mtm, SHEET_WIDTH, path_rows);
    let (sheet, _) = sheet_window(mtm, (SHEET_WIDTH, 240.0), t(TextText::PathTitle));
    sheet.setContentView(Some(&path_form.content));

    let controls = controls_of(&[
        &family,
        &bold,
        &italic,
        &uppercase,
        &welded,
        &single_line,
        &align,
        &valign,
        &h_space,
        &v_space,
        &height,
        &convert,
        &path_distort,
        &path_start,
        &path_offset,
        &path_reverse,
        &path_live,
        &path_cancel,
        &path_apply,
    ]);
    TextSection {
        target,
        controls,
        toolbar,
        form,
        family,
        bold,
        italic,
        uppercase,
        welded,
        single_line,
        align,
        valign,
        h_space,
        v_space,
        height,
        note,
        convert,
        path: SheetHolder { sheet, shown: false },
        path_form,
        path_distort,
        path_start,
        path_offset,
        path_reverse,
        path_live,
        path_note,
        path_apply,
        path_cancel,
    }
}

impl TextSection {
    /// Nil the targets (weak references) before the section goes: nothing can call a dead object.
    pub(crate) fn clear_targets(&self) {
        unwire(&self.controls);
        let _ = &self.target;
    }

    /// Take the panel and the sheet away (the window is going).
    pub(crate) fn hide(&mut self, window: &NSWindow) {
        self.path.hide(window);
        self.toolbar.hide();
    }

    pub(crate) fn toolbar_visible(&self) -> bool {
        self.toolbar.shown
    }

    pub(crate) fn path_visible(&self) -> bool {
        self.path.shown
    }

    /// The widgets, for tests that drive them like a user.
    pub(crate) fn widgets(&self) -> TextWidgets<'_> {
        TextWidgets {
            panel: &self.toolbar.panel,
            family: &self.family,
            bold: &self.bold,
            italic: &self.italic,
            uppercase: &self.uppercase,
            welded: &self.welded,
            single_line: &self.single_line,
            align: &self.align,
            valign: &self.valign,
            h_space: &self.h_space,
            v_space: &self.v_space,
            height: &self.height,
            note: &self.note,
            convert: &self.convert,
            path_sheet: &self.path.sheet,
            path_distort: &self.path_distort,
            path_start: &self.path_start,
            path_offset: &self.path_offset,
            path_reverse: &self.path_reverse,
            path_live: &self.path_live,
            path_note: &self.path_note,
            path_apply: &self.path_apply,
            path_cancel: &self.path_cancel,
        }
    }
}

/// The text panels' widgets as a test sees them.
pub struct TextWidgets<'a> {
    pub panel: &'a NSPanel,
    pub family: &'a NSPopUpButton,
    pub bold: &'a NSButton,
    pub italic: &'a NSButton,
    pub uppercase: &'a NSButton,
    pub welded: &'a NSButton,
    pub single_line: &'a NSButton,
    pub align: &'a NSSegmentedControl,
    pub valign: &'a NSPopUpButton,
    pub h_space: &'a NSTextField,
    pub v_space: &'a NSTextField,
    pub height: &'a NSTextField,
    pub note: &'a NSTextField,
    pub convert: &'a NSButton,
    pub path_sheet: &'a NSWindow,
    pub path_distort: &'a NSPopUpButton,
    pub path_start: &'a NSTextField,
    pub path_offset: &'a NSTextField,
    pub path_reverse: &'a NSButton,
    pub path_live: &'a NSButton,
    pub path_note: &'a NSTextField,
    pub path_apply: &'a NSButton,
    pub path_cancel: &'a NSButton,
}

impl DocWindow {
    /// Show, fill or hide the text toolbar and the text-on-path sheet as presentation says.
    pub(crate) fn refresh_text(&mut self, vm: &AppViewModel, document: DocumentId) {
        let panels = text_panels(vm, document, self.locale);
        let window = self.window.clone();
        let s = &mut self.text;
        match panels.toolbar.as_ref().filter(|_| vm.active == Some(document)) {
            Some(p) => {
                let items: Vec<(String, bool, String)> =
                    p.families.iter().map(|f| (f.clone(), true, String::new())).collect();
                set_popup_items(&s.family, &items, p.family);
                set_check(&s.bold, p.bold.on, p.bold.enabled, &p.bold.tip);
                set_check(&s.italic, p.italic.on, p.italic.enabled, &p.italic.tip);
                set_check(&s.uppercase, p.uppercase, true, "");
                set_check(&s.welded, p.welded.on, p.welded.enabled, &p.welded.tip);
                set_check(&s.single_line, p.single_line, true, "");
                if s.align.selectedSegment() != p.align as isize {
                    s.align.setSelectedSegment(p.align as isize);
                }
                if s.valign.indexOfSelectedItem() != p.valign as isize {
                    s.valign.selectItemAtIndex(p.valign as isize);
                }
                // A field in error keeps what the user typed, so it can be corrected.
                let keep = !p.note.is_empty();
                set_field(&s.h_space, &p.h_space, keep);
                set_field(&s.v_space, &p.v_space, keep);
                set_field(&s.height, &p.height, keep);
                set_text(&s.note, &p.note);
                s.convert.setEnabled(p.convert_enabled);
                let height = s.form.lay_out();
                s.toolbar.show(&window, (12.0, 150.0), height, PANEL_WIDTH);
            }
            None => s.toolbar.hide(),
        }
        match panels.path.as_ref().filter(|_| vm.active == Some(document)) {
            Some(p) => {
                if s.path_distort.indexOfSelectedItem() != p.distort as isize {
                    s.path_distort.selectItemAtIndex(p.distort as isize);
                }
                let keep = !p.note.is_empty() && p.apply_enabled;
                set_field(&s.path_start, &p.start, keep);
                set_field(&s.path_offset, &p.offset, keep);
                set_check(&s.path_reverse, p.reverse, true, "");
                set_check(&s.path_live, p.live, true, "");
                set_text(&s.path_note, &p.note);
                s.path_apply.setEnabled(p.apply_enabled);
                let height = s.path_form.lay_out();
                s.path.show(&window, (SHEET_WIDTH, height));
            }
            None => s.path.hide(&window),
        }
    }
}
