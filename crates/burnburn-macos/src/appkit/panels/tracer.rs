//! The tracer panel (T01, T02, T04, T10), a sheet on the document window while presentation has a
//! tracer open: recipe, kind, resolution, threshold, invert, "negeer kleiner dan", smoothness and
//! curves on the left; the traced outline and its summary on the right.
//!
//! The canvas does not draw the tracer's preview path, so the outline is rasterised by
//! `panels_image::trace_preview_bitmap` into a small image view of this panel. All edits go
//! through `ImagePanelTarget` (`traceForm:`, `tracePreset:`, `traceSuggestion:`); the buttons run
//! `ApplyTrace` / `CloseTracer` through the window controller's `panelButton:`.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSImageView, NSPopUpButton, NSTextField,
    NSWindow,
};
use objc2_foundation::NSString;

use burnburn_presentation::{AppViewModel, CommandKey, TracePreset};
use burnburn_types::DocumentId;
use objc2_app_kit::NSView;

use super::image::{
    CommandButton, begin_sheet, caption, end_sheet, heading, image_view, input, mark_field, note, popup, sheet_window,
    show_bitmap,
};
use super::widgets::{push_button, rect, set_text};
use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text};
use crate::panels_image::{BitmapImage, THRESHOLD_CHOICES, TRACE_MODES, TracerPanel, trace_preset_name, tracer_panel};

const SIZE: (f64, f64) = (700.0, 520.0);

/// The tracer sheet and what it last showed.
pub(crate) struct TracerSheet {
    pub(crate) sheet: Retained<NSWindow>,
    /// Source size, summary, "berekenen…", scale hint, issue.
    pub(crate) labels: [Retained<NSTextField>; 5],
    pub(crate) recipe: Retained<NSPopUpButton>,
    pub(crate) mode: Retained<NSPopUpButton>,
    pub(crate) threshold: Retained<NSPopUpButton>,
    /// Resolution, threshold value, ignore below, smoothness.
    pub(crate) fields: [Retained<NSTextField>; 4],
    pub(crate) invert: Retained<NSButton>,
    pub(crate) curves: Retained<NSButton>,
    pub(crate) suggestion: Retained<NSButton>,
    pub(crate) preview: Retained<NSImageView>,
    pub(crate) apply: Retained<NSButton>,
    pub(crate) close: Retained<NSButton>,
    pub(crate) shown: bool,
    /// What the view model's form had at the last refresh; a widget is written when its value changed.
    last: Option<TracerLast>,
    last_preview: Option<BitmapImage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TracerLast {
    fields: [String; 4],
    mode: usize,
    threshold: usize,
    invert: bool,
    curves: bool,
}

#[allow(unsafe_code)]
fn checkbox(
    mtm: MainThreadMarker,
    content: &NSView,
    title: &str,
    frame: objc2_foundation::NSRect,
) -> Retained<NSButton> {
    // SAFETY: no target and no action yet; `image_sheets` wires both once the target exists.
    let b = unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(title), None, None, mtm) };
    b.setFrame(frame);
    content.addSubview(&b);
    b
}

/// The tracer sheet's window and widgets (not shown until presentation has a tracer).
pub(crate) fn tracer_sheet(mtm: MainThreadMarker, locale: Locale, command_button: &CommandButton<'_>) -> TracerSheet {
    let t = |text: HostText| host_text(text, locale);
    let (w, h) = SIZE;
    let left = 330.0;
    let (sheet, content) = sheet_window(mtm, SIZE, t(HostText::TracerTitle));
    let mut y = h - 16.0;
    let source = heading(mtm, &content, rect(16.0, y - 20.0, left, 20.0));
    y -= 32.0;
    let row = |y: f64, name: HostText| caption(mtm, &content, t(name), rect(16.0, y - 22.0, 196.0, 22.0));
    row(y, HostText::TracerRecipe);
    let preset_titles: Vec<String> = TracePreset::ALL.iter().map(|p| trace_preset_name(*p, locale)).collect();
    let recipe = popup(mtm, &content, t(HostText::TracerRecipe), &preset_titles, rect(212.0, y - 26.0, 150.0, 26.0));
    y -= 34.0;
    row(y, HostText::TracerMode);
    let mode_titles = [t(HostText::TracerModeContour), t(HostText::TracerModeCenterline)].map(str::to_owned);
    debug_assert_eq!(mode_titles.len(), TRACE_MODES.len());
    let mode = popup(mtm, &content, t(HostText::TracerMode), &mode_titles, rect(212.0, y - 26.0, 150.0, 26.0));
    y -= 34.0;
    row(y, HostText::TracerDpi);
    let dpi = input(mtm, &content, t(HostText::TracerDpi), rect(216.0, y - 24.0, 114.0, 24.0));
    y -= 30.0;
    let suggestion = push_button(mtm, "");
    suggestion.setFrame(rect(16.0, y - 24.0, left, 24.0));
    suggestion.setHidden(true);
    content.addSubview(&suggestion);
    y -= 30.0;
    row(y, HostText::TracerThreshold);
    let threshold_titles =
        [t(HostText::TracerThresholdOtsu), t(HostText::TracerThresholdFixed), t(HostText::TracerThresholdAdaptive)]
            .map(str::to_owned);
    debug_assert_eq!(threshold_titles.len(), THRESHOLD_CHOICES.len());
    let threshold =
        popup(mtm, &content, t(HostText::TracerThreshold), &threshold_titles, rect(212.0, y - 26.0, 150.0, 26.0));
    y -= 34.0;
    row(y, HostText::TracerThresholdValue);
    let value = input(mtm, &content, t(HostText::TracerThresholdValue), rect(216.0, y - 24.0, 114.0, 24.0));
    y -= 30.0;
    let invert = checkbox(mtm, &content, t(HostText::TracerInvert), rect(16.0, y - 22.0, left, 22.0));
    y -= 28.0;
    row(y, HostText::TracerIgnore);
    let ignore = input(mtm, &content, t(HostText::TracerIgnore), rect(216.0, y - 24.0, 114.0, 24.0));
    y -= 30.0;
    row(y, HostText::TracerSmoothness);
    let smoothness = input(mtm, &content, t(HostText::TracerSmoothness), rect(216.0, y - 24.0, 114.0, 24.0));
    y -= 30.0;
    let curves = checkbox(mtm, &content, t(HostText::TracerCurves), rect(16.0, y - 22.0, left, 22.0));
    let issue = note(mtm, &content, rect(16.0, 56.0, left, 40.0), &NSColor::systemRedColor());

    let x = 364.0;
    let preview = image_view(mtm, &content, t(HostText::TracerTitle), rect(x, h - 16.0 - 322.0, 320.0, 322.0));
    let summary = note(mtm, &content, rect(x, h - 16.0 - 322.0 - 40.0, 320.0, 36.0), &NSColor::controlTextColor());
    summary.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    let computing = caption(mtm, &content, "", rect(x, h - 16.0 - 322.0 - 62.0, 320.0, 18.0));
    let scale_hint = note(mtm, &content, rect(x, h - 16.0 - 322.0 - 104.0, 320.0, 38.0), &NSColor::systemOrangeColor());
    let close =
        command_button(&content, t(HostText::TracerClose), CommandKey::CloseTracer, rect(w - 232.0, 12.0, 104.0, 28.0));
    close.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let apply =
        command_button(&content, t(HostText::TracerApply), CommandKey::ApplyTrace, rect(w - 124.0, 12.0, 112.0, 28.0));
    sheet.setContentView(Some(&content));
    TracerSheet {
        sheet,
        labels: [source, summary, computing, scale_hint, issue],
        recipe,
        mode,
        threshold,
        fields: [dpi, value, ignore, smoothness],
        invert,
        curves,
        suggestion,
        preview,
        apply,
        close,
        shown: false,
        last: None,
        last_preview: None,
    }
}

fn state(on: bool) -> isize {
    if on { NSControlStateValueOn } else { NSControlStateValueOff }
}

impl DocWindow {
    /// Show, fill or end the tracer sheet as presentation opens and closes the tracer.
    pub(crate) fn refresh_tracer(&mut self, vm: &AppViewModel, document: DocumentId) {
        let model = vm.document(document).and_then(|d| d.tracer.as_ref());
        let s = &mut self.image.tracer;
        let Some(panel) = model.map(|m| tracer_panel(m, self.locale)) else {
            if s.shown {
                s.shown = false;
                s.last = None;
                s.last_preview = None;
                end_sheet(&self.window, &s.sheet, "ending the tracer sheet");
            }
            return;
        };
        let TracerPanel {
            source,
            mode_index,
            fields,
            threshold_index,
            threshold_enabled,
            ignore_enabled,
            invert,
            curves,
            preset_index,
            suggestion_button,
            summary,
            computing,
            scale_hint,
            issue,
            issue_field,
            preview,
            ..
        } = panel;
        for (l, text) in s.labels.iter().zip([&source, &summary, &computing, &scale_hint, &issue]) {
            set_text(l, text);
        }
        // A widget is written when the view model's value changed since the last refresh (a preset,
        // the resolution proposal) and once per opening; a refused input keeps what the user typed.
        let now = TracerLast { fields, mode: mode_index, threshold: threshold_index, invert, curves };
        let last = s.last.as_ref();
        for (i, (f, text)) in s.fields.iter().zip(&now.fields).enumerate() {
            if last.is_none_or(|l| l.fields[i] != *text) {
                f.setStringValue(&NSString::from_str(text));
            }
            mark_field(f, issue_field == Some(i));
        }
        if last.is_none_or(|l| l.mode != now.mode) {
            s.mode.selectItemAtIndex(now.mode as isize);
        }
        if last.is_none_or(|l| l.threshold != now.threshold) {
            s.threshold.selectItemAtIndex(now.threshold as isize);
        }
        if last.is_none_or(|l| l.invert != now.invert) {
            s.invert.setState(state(now.invert));
        }
        if last.is_none_or(|l| l.curves != now.curves) {
            s.curves.setState(state(now.curves));
        }
        s.last = Some(now);
        s.recipe.selectItemAtIndex(preset_index.map_or(-1, |i| i as isize));
        // The threshold value applies to the fixed threshold, "negeer kleiner dan" to contours.
        s.fields[1].setEnabled(threshold_enabled);
        s.fields[2].setEnabled(ignore_enabled);
        match &suggestion_button {
            Some(title) => {
                if s.suggestion.title().to_string() != *title {
                    s.suggestion.setTitle(&NSString::from_str(title));
                }
                s.suggestion.setHidden(false);
            }
            None => s.suggestion.setHidden(true),
        }
        show_bitmap(&s.preview, &mut s.last_preview, preview.as_ref());
        s.apply.setEnabled(
            vm.active == Some(document) && vm.commands.get(&CommandKey::ApplyTrace).is_some_and(|c| c.enabled),
        );
        if !s.shown {
            s.shown = begin_sheet(&self.window, &s.sheet, "showing the tracer sheet");
        }
    }
}
