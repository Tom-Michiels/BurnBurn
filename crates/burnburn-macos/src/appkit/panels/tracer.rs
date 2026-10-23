//! The tracer panel (T01, T02, T04, T10), a sheet on the document window while presentation has a
//! tracer open: recipe, kind, resolution, threshold, invert, "negeer kleiner dan", smoothness and
//! curves on the left; the traced outline and its summary on the right.
//!
//! The canvas does not draw the tracer's preview path, so the outline is rasterised by
//! `panels_image::trace_preview_bitmap` into a small image view of this panel. All edits go
//! through `ImagePanelTarget` (`traceForm:`, `tracePreset:`, `traceSuggestion:`); the buttons run
//! `ApplyTrace` / `CloseTracer` through the window controller's `panelButton:`.

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::files_form::{tracer as id, tracer_forms};
use burnburn_presentation::{AppViewModel, CommandKey};
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSButton, NSControlStateValueOff, NSControlStateValueOn, NSImageView, NSPopUpButton, NSTextField, NSWindow,
};
use objc2_foundation::NSString;

use super::form_surface::{FormSurface, form_sheet};
use super::image::{CommandButton, begin_sheet, end_sheet, mark_field, show_bitmap};
use super::widgets::set_text;
use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text};
use crate::panels_image::{BitmapImage, TracerPanel, tracer_panel};

/// The settings form and the preview form of the tracer sheet.
const WIDTHS: (f64, f64) = (372.0, 360.0);

/// The tracer sheet and what it last showed.
pub(crate) struct TracerSheet {
    pub(crate) sheet: Retained<NSWindow>,
    surface: FormSurface,
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
    /// "Pas toe als trace" (T02): the result stays a re-editable trace of the project.
    pub(crate) apply_linked: Retained<NSButton>,
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

/// The tracer sheet's window and widgets (not shown until presentation has a tracer).
pub(crate) fn tracer_sheet(mtm: MainThreadMarker, locale: Locale, command_button: &CommandButton<'_>) -> TracerSheet {
    let (left, right) = tracer_forms(locale);
    let mut surface = FormSurface::new(mtm, &[(&left, WIDTHS.0), (&right, WIDTHS.1)]);
    let sheet = form_sheet(mtm, &surface, host_text(HostText::TracerTitle, locale));
    let close = surface.dialog_button(ButtonRole::Cancel);
    command_button(&close, CommandKey::CloseTracer);
    let apply = surface.dialog_button(ButtonRole::Primary);
    command_button(&apply, CommandKey::ApplyTrace);
    let apply_linked = surface.button(id::APPLY_LINKED);
    command_button(&apply_linked, CommandKey::ApplyTraceLinked);
    // The proposal button shows only while there is a proposal.
    surface.set_hidden(id::SUGGESTION, true);
    surface.relayout();
    TracerSheet {
        sheet,
        labels: [
            surface.text(id::SOURCE),
            surface.text(id::SUMMARY),
            surface.text(id::COMPUTING),
            surface.text(id::SCALE_HINT),
            surface.text(id::ISSUE),
        ],
        recipe: surface.popup(id::RECIPE),
        mode: surface.popup(id::MODE),
        threshold: surface.popup(id::THRESHOLD),
        fields: [
            surface.text(id::DPI),
            surface.text(id::THRESHOLD_VALUE),
            surface.text(id::IGNORE),
            surface.text(id::SMOOTHNESS),
        ],
        invert: surface.button(id::INVERT),
        curves: surface.button(id::CURVES),
        suggestion: surface.button(id::SUGGESTION),
        preview: surface.image(id::PREVIEW),
        apply,
        apply_linked,
        close,
        shown: false,
        last: None,
        last_preview: None,
        surface,
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
                s.surface.set_hidden(id::SUGGESTION, false);
            }
            None => s.surface.set_hidden(id::SUGGESTION, true),
        }
        show_bitmap(&s.preview, &mut s.last_preview, preview.as_ref());
        s.apply.setEnabled(
            vm.active == Some(document) && vm.commands.get(&CommandKey::ApplyTrace).is_some_and(|c| c.enabled),
        );
        s.apply_linked.setEnabled(
            vm.active == Some(document) && vm.commands.get(&CommandKey::ApplyTraceLinked).is_some_and(|c| c.enabled),
        );
        s.surface.fit_window(&s.sheet);
        if !s.shown {
            s.shown = begin_sheet(&self.window, &s.sheet, "showing the tracer sheet");
        }
    }
}
