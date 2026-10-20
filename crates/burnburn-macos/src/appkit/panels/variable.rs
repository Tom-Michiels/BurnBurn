//! The variable-text panel (W3-UI-tekst, E11): a floating utility panel beside the text toolbar
//! with Normal/Variable, the serial counter, Auto-Advance, Previous/Next/Reset, the CSV rows, the
//! live preview, Test and Bake. It copies the headless model (`crate::panels::text_panels`) into
//! widgets; the CSV button opens a file panel on the window (`HostApp::show_csv_panel`).

use std::rc::Weak;

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSColor, NSControl, NSPopUpButton, NSSegmentedControl, NSTextField};
use objc2_foundation::NSString;

use super::text::{ToolPanel, view};
use super::textform::{
    Cells, Form, FormRow, TextTarget, caption, check, message_line, new_target, popup, push, segments, set_check,
    set_field, set_popup_items, text_field, unwire,
};
use super::widgets::set_text;
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{Slot, TextText, text_panels, text_text};

const PANEL_WIDTH: f64 = 380.0;

pub(crate) struct VariableSection {
    target: Retained<TextTarget>,
    controls: Vec<Retained<NSControl>>,
    panel: ToolPanel,
    form: Form,
    kind: Retained<NSSegmentedControl>,
    offset: Retained<NSTextField>,
    current: Retained<NSTextField>,
    start: Retained<NSTextField>,
    end: Retained<NSTextField>,
    advance_by: Retained<NSTextField>,
    digits: Retained<NSTextField>,
    wrap: Retained<NSButton>,
    policy: Retained<NSPopUpButton>,
    csv_line: Retained<NSTextField>,
    clear_csv: Retained<NSButton>,
    row: Retained<NSTextField>,
    preview: Retained<NSTextField>,
    test_text: Retained<NSTextField>,
    problem: Retained<NSTextField>,
    bound: Retained<NSTextField>,
    bake: Retained<NSButton>,
}

pub(crate) fn variable_section(
    mtm: objc2::MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> VariableSection {
    let t = |text: TextText| text_text(text, locale);
    let target = new_target(mtm, app, document);
    let labelled = |text: TextText, control: &objc2_app_kit::NSView, height: f64| {
        FormRow::new(Cells::Labeled(caption(mtm, t(text)), objc2::Message::retain(control)), height)
    };
    let kind =
        segments(mtm, &target, Slot::VarKind, &[t(TextText::KindNormal), t(TextText::KindVariable)], t(TextText::Kind));
    let offset = text_field(mtm, &target, Slot::VarOffset, t(TextText::VarOffset));
    let current = text_field(mtm, &target, Slot::SerialCurrent, t(TextText::SerialCurrent));
    let start = text_field(mtm, &target, Slot::SerialStart, t(TextText::SerialStart));
    let end = text_field(mtm, &target, Slot::SerialEnd, t(TextText::SerialEnd));
    let advance_by = text_field(mtm, &target, Slot::SerialBy, t(TextText::SerialBy));
    let digits = text_field(mtm, &target, Slot::SerialDigits, t(TextText::SerialDigits));
    let wrap = check(mtm, &target, Slot::SerialWrap, t(TextText::SerialWrap));
    let policy = popup(mtm, &target, Slot::VarPolicy, t(TextText::AutoAdvance));
    for title in [TextText::PolicyManual, TextText::PolicyUpload, TextText::PolicyStart] {
        policy.addItemWithTitle(&NSString::from_str(t(title)));
    }
    let previous = push(mtm, &target, Slot::VarPrevious, t(TextText::Previous));
    let next = push(mtm, &target, Slot::VarNext, t(TextText::Next));
    let reset = push(mtm, &target, Slot::VarReset, t(TextText::Reset));
    let csv_line = message_line(mtm, false);
    let choose_csv = push(mtm, &target, Slot::VarCsv, t(TextText::ChooseCsv));
    let clear_csv = push(mtm, &target, Slot::VarClearCsv, t(TextText::ClearCsv));
    let row = text_field(mtm, &target, Slot::VarRow, t(TextText::Row));
    let preview = message_line(mtm, false);
    let test = push(mtm, &target, Slot::VarTest, t(TextText::Test));
    let bake = push(mtm, &target, Slot::VarBake, t(TextText::Bake));
    let test_text = message_line(mtm, false);
    let problem = message_line(mtm, true);
    let bound = message_line(mtm, false);
    let rows = vec![
        labelled(TextText::Kind, &kind, 24.0),
        labelled(TextText::VarOffset, &offset, 24.0),
        labelled(TextText::SerialCurrent, &current, 24.0),
        labelled(TextText::SerialStart, &start, 24.0),
        labelled(TextText::SerialEnd, &end, 24.0),
        labelled(TextText::SerialBy, &advance_by, 24.0),
        labelled(TextText::SerialDigits, &digits, 24.0),
        FormRow::new(Cells::Full(view!(wrap)), 22.0),
        labelled(TextText::AutoAdvance, &policy, 26.0),
        FormRow::new(Cells::Side(vec![view!(previous), view!(next), view!(reset)]), 28.0),
        FormRow::new(Cells::Full(view!(csv_line)), 20.0),
        FormRow::new(Cells::Side(vec![view!(choose_csv), view!(clear_csv)]), 28.0),
        labelled(TextText::Row, &row, 24.0),
        FormRow::new(Cells::Full(view!(preview)), 34.0),
        FormRow::new(Cells::Side(vec![view!(test), view!(bake)]), 28.0),
        FormRow::new(Cells::Full(view!(test_text)), 60.0),
        FormRow::new(Cells::Full(view!(problem)), 34.0),
        FormRow::new(Cells::Full(view!(bound)), 20.0),
    ];
    let form = Form::new(mtm, PANEL_WIDTH, rows);
    let panel = ToolPanel::new(mtm, t(TextText::VariableTitle), &form);
    let all: Vec<&NSControl> = vec![
        &kind,
        &offset,
        &current,
        &start,
        &end,
        &advance_by,
        &digits,
        &wrap,
        &policy,
        &previous,
        &next,
        &reset,
        &choose_csv,
        &clear_csv,
        &row,
        &test,
        &bake,
    ];
    let controls = all.iter().map(|c| objc2::Message::retain(*c)).collect();
    VariableSection {
        target,
        controls,
        panel,
        form,
        kind,
        offset,
        current,
        start,
        end,
        advance_by,
        digits,
        wrap,
        policy,
        csv_line,
        clear_csv,
        row,
        preview,
        test_text,
        problem,
        bound,
        bake,
    }
}

impl VariableSection {
    pub(crate) fn clear_targets(&self) {
        unwire(&self.controls);
        let _ = &self.target;
    }

    pub(crate) fn hide(&mut self) {
        self.panel.hide();
    }

    pub(crate) fn visible(&self) -> bool {
        self.panel.shown
    }

    /// The widgets, for tests that drive them like a user.
    pub(crate) fn widgets(&self) -> VariableWidgets<'_> {
        VariableWidgets {
            panel: &self.panel.panel,
            kind: &self.kind,
            start: &self.start,
            end: &self.end,
            advance_by: &self.advance_by,
            current: &self.current,
            row: &self.row,
            policy: &self.policy,
            preview: &self.preview,
            problem: &self.problem,
            test_text: &self.test_text,
            bake: &self.bake,
            clear_csv: &self.clear_csv,
        }
    }
}

/// The variable-text panel's widgets as a test sees them.
pub struct VariableWidgets<'a> {
    pub panel: &'a objc2_app_kit::NSPanel,
    pub kind: &'a NSSegmentedControl,
    pub start: &'a NSTextField,
    pub end: &'a NSTextField,
    pub advance_by: &'a NSTextField,
    pub current: &'a NSTextField,
    pub row: &'a NSTextField,
    pub policy: &'a NSPopUpButton,
    pub preview: &'a NSTextField,
    pub problem: &'a NSTextField,
    pub test_text: &'a NSTextField,
    pub bake: &'a NSButton,
    pub clear_csv: &'a NSButton,
}

impl DocWindow {
    /// Show, fill or hide the variable-text panel as presentation says.
    pub(crate) fn refresh_variable(&mut self, vm: &AppViewModel, document: DocumentId) {
        let panels = text_panels(vm, document, self.locale);
        let window = self.window.clone();
        let s = &mut self.variable;
        let Some(p) = panels.variable.as_ref().filter(|_| vm.active == Some(document)) else {
            s.hide();
            return;
        };
        let wanted = isize::from(p.variable);
        if s.kind.selectedSegment() != wanted {
            s.kind.setSelectedSegment(wanted);
        }
        // Fields in error keep what the user typed, so it can be corrected.
        let keep = !p.problem.is_empty();
        for (field, text) in [
            (&s.offset, &p.offset),
            (&s.current, &p.current),
            (&s.start, &p.start),
            (&s.end, &p.end),
            (&s.advance_by, &p.advance_by),
            (&s.digits, &p.digits),
            (&s.row, &p.row),
        ] {
            set_field(field, text, keep);
        }
        s.offset.setEnabled(p.variable);
        set_check(&s.wrap, p.wrap, true, "");
        let policies: Vec<(String, bool, String)> = (0..3)
            .map(|i| {
                let item = [TextText::PolicyManual, TextText::PolicyUpload, TextText::PolicyStart][i];
                (text_text(item, self.locale).to_owned(), true, String::new())
            })
            .collect();
        set_popup_items(&s.policy, &policies, p.policy);
        set_text(&s.csv_line, &p.csv_line);
        s.clear_csv.setEnabled(p.csv_loaded);
        s.row.setEnabled(p.csv_loaded);
        set_text(&s.preview, &p.preview);
        let color = if p.preview_is_error { NSColor::systemRedColor() } else { NSColor::labelColor() };
        s.preview.setTextColor(Some(&color));
        set_text(&s.test_text, &p.test);
        set_text(&s.problem, &p.problem);
        set_text(&s.bound, &p.bound);
        s.bake.setEnabled(p.variable);
        let height = s.form.lay_out();
        // Beside the text toolbar.
        s.panel.show(&window, (12.0 + 350.0, 150.0), height, PANEL_WIDTH);
    }
}
