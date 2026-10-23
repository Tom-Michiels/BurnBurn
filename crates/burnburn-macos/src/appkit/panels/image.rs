//! The bitmap import sheet (P03) and the Adjust Image sheet (T09), plus the one action target of
//! all image panels (these two and the tracer) and the file panels of the Beeld menu.
//!
//! The sheets only show what `panels_image` computed from the view model and forward what the user
//! typed as `NativeEvent`s. The command buttons (Importeer, Pas toe, ...) use the window
//! controller's `panelButton:` (tag = menu tag of the command); fields, pop-ups and checkboxes use
//! [`ImagePanelTarget`], which reads every widget of its panel and sends one event, so one edit is
//! one intent with all values. `unsafe` is limited to the Objective-C class definition, the
//! control wiring and the bitmap copy (SAFETY comments); no callback lets a panic unwind into AppKit.
//! The widgets of the two sheets come from forms (`files_form::image_import_form`,
//! `image_adjust_forms`, LF6b); the sheets copy the view model into them as before.
#![allow(unsafe_code)]

use std::path::PathBuf;
use std::ptr;
use std::rc::Weak;

use block2::RcBlock;
use burnburn_presentation::{AppViewModel, CommandKey};
use burnburn_types::{DocumentId, OperationId};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSBitmapImageRep, NSButton, NSColor, NSControlStateValueOn,
    NSDeviceRGBColorSpace, NSImage, NSImageFrameStyle, NSImageScaling, NSImageView, NSLineBreakMode, NSModalResponse,
    NSOpenPanel, NSPopUpButton, NSSavePanel, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSSize, NSString};

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::files_form::{image_adjust as adjust_id, image_adjust_forms};
use burnburn_host_models::panels::files_form::{image_import as import_id, image_import_form};

use super::form_surface::{FormSurface, form_sheet, point_at};
use super::tracer::{TracerSheet, tracer_sheet};
use super::widgets::{label, rect, set_text};
use crate::adapter::{MenuItem, NativeEvent, menu_tag};
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text};
use crate::panels_image::{BitmapImage, ImageAdjustPanel, ImageImportPanel, image_adjust_panel, image_import_panel};

/// Picture types the bitmap flow offers in file panels (W4-UI-gaten: BMP and TIFF decode now; a
/// damaged file of either type still gets the decoder's honest refusal).
pub(crate) const BITMAP_TYPES: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "tif", "tiff"];

/// Picture types the tracer reads.
pub(crate) const TRACE_TYPES: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "tif", "tiff"];

const IMPORT_WIDTH: f64 = 460.0;
/// The values form and the preview form of the Adjust Image sheet.
const ADJUST_WIDTHS: (f64, f64) = (372.0, 360.0);

/// A titled sheet window of `size` with an empty content view.
pub(crate) fn sheet_window(
    mtm: MainThreadMarker,
    size: (f64, f64),
    title: &str,
) -> (Retained<NSWindow>, Retained<NSView>) {
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let sheet = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, size.0, size.1),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with the caller's Retained.
    unsafe { sheet.setReleasedWhenClosed(false) };
    sheet.setTitle(&NSString::from_str(title));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, size.0, size.1));
    (sheet, content)
}

/// A label added to `content`.
pub(crate) fn caption(
    mtm: MainThreadMarker,
    content: &NSView,
    text: &str,
    frame: objc2_foundation::NSRect,
) -> Retained<NSTextField> {
    let l = label(mtm, text, frame);
    content.addSubview(&l);
    l
}

/// A word-wrapping label (issues, hints) in `color`.
pub(crate) fn note(
    mtm: MainThreadMarker,
    content: &NSView,
    frame: objc2_foundation::NSRect,
    color: &NSColor,
) -> Retained<NSTextField> {
    let l = caption(mtm, content, "", frame);
    l.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    l.setTextColor(Some(color));
    l
}

/// A square-ish image view that keeps the picture's aspect.
pub(crate) fn image_view(
    mtm: MainThreadMarker,
    content: &NSView,
    name: &str,
    frame: objc2_foundation::NSRect,
) -> Retained<NSImageView> {
    let v = NSImageView::initWithFrame(NSImageView::alloc(mtm), frame);
    v.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    v.setImageFrameStyle(NSImageFrameStyle::GrayBezel);
    v.setAccessibilityLabel(Some(&NSString::from_str(name)));
    content.addSubview(&v);
    v
}

/// Show `image` (or nothing) in `view` when it is not the picture already shown there.
pub(crate) fn show_bitmap(view: &NSImageView, shown: &mut Option<BitmapImage>, image: Option<&BitmapImage>) {
    if shown.as_ref() == image {
        return;
    }
    match image.and_then(ns_image) {
        Some(ns) => view.setImage(Some(&ns)),
        None => view.setImage(None),
    }
    *shown = image.cloned();
}

/// The bitmap as an `NSImage` of its pixel size in points. `None` for an empty or inconsistent one.
fn ns_image(bitmap: &BitmapImage) -> Option<Retained<NSImage>> {
    let (w, h) = (isize::try_from(bitmap.width).ok()?, isize::try_from(bitmap.height).ok()?);
    let len = usize::try_from(w).ok()?.checked_mul(usize::try_from(h).ok()?)?.checked_mul(4)?;
    if w == 0 || h == 0 || bitmap.rgba.len() != len {
        return None;
    }
    // SAFETY: null planes make the representation allocate its own buffer of `h` rows of
    // `w * 4` bytes; the colour space name is an immutable AppKit constant.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            w,
            h,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            w * 4,
            32,
        )
    }?;
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: the representation owns `len` bytes at `data` (chunky RGBA, no row padding because
    // bytesPerRow is exactly `w * 4`); nothing else reads or writes them during this copy. The
    // pixels are opaque, so premultiplied and straight alpha are the same bytes.
    let dst = unsafe { std::slice::from_raw_parts_mut(data, len) };
    dst.copy_from_slice(&bitmap.rgba);
    let size = NSSize::new(f64::from(bitmap.width), f64::from(bitmap.height));
    rep.setSize(size);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    image.addRepresentation(&rep);
    Some(image)
}

/// Mark `field` red while it is the one in error.
pub(crate) fn mark_field(field: &NSTextField, in_error: bool) {
    let color = if in_error { NSColor::systemRedColor() } else { NSColor::controlTextColor() };
    field.setTextColor(Some(&color));
}

/// Begin `sheet` on `window`; `true` when AppKit took it.
pub(crate) fn begin_sheet(window: &NSWindow, sheet: &NSWindow, what: &str) -> bool {
    let (window, sheet) = (window.retain_clone(), sheet.retain_clone());
    match crate::appkit::objc_try(what, || window.beginSheet_completionHandler(&sheet, None)) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("burnburn: {e}");
            false
        }
    }
}

/// End `sheet` on `window`.
pub(crate) fn end_sheet(window: &NSWindow, sheet: &NSWindow, what: &str) {
    let (window, sheet) = (window.retain_clone(), sheet.retain_clone());
    if let Err(e) = crate::appkit::objc_try(what, || window.endSheet(&sheet)) {
        eprintln!("burnburn: {e}");
    }
}

/// `retain` for the borrowed AppKit objects the sheet helpers receive.
trait RetainClone {
    fn retain_clone(&self) -> Retained<Self>
    where
        Self: Sized;
}

impl<T: objc2::Message> RetainClone for T {
    fn retain_clone(&self) -> Retained<Self> {
        self.retain()
    }
}

// ---------------------------------------------------------------------------------------------
// The action target of the image panels.
// ---------------------------------------------------------------------------------------------

/// The widgets the target reads when an action fires.
pub(crate) struct ImageTargetParts {
    pub(crate) app: Weak<HostApp>,
    pub(crate) document: DocumentId,
    pub(crate) dpi: Retained<NSTextField>,
    pub(crate) adjust_fields: [Retained<NSTextField>; 9],
    pub(crate) gray: Retained<NSPopUpButton>,
    pub(crate) trace_fields: [Retained<NSTextField>; 4],
    pub(crate) trace_mode: Retained<NSPopUpButton>,
    pub(crate) trace_threshold: Retained<NSPopUpButton>,
    pub(crate) trace_invert: Retained<NSButton>,
    pub(crate) trace_curves: Retained<NSButton>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnImagePanelTarget"]
    #[ivars = ImageTargetParts]
    pub(crate) struct ImagePanelTarget;

    impl ImagePanelTarget {
        /// The DPI field of the bitmap import sheet.
        #[unsafe(method(imageDpi:))]
        fn image_dpi(&self, _sender: Option<&AnyObject>) {
            ffi_guard("imageDpi:", (), || {
                let p = self.ivars();
                self.send(NativeEvent::ImageDpiCommitted { document: p.document, text: p.dpi.stringValue().to_string() });
            });
        }

        /// Any field or the gray pop-up of the Adjust Image sheet: all values go together.
        #[unsafe(method(imageAdjust:))]
        fn image_adjust(&self, _sender: Option<&AnyObject>) {
            ffi_guard("imageAdjust:", (), || {
                let p = self.ivars();
                let fields = p.adjust_fields.each_ref().map(|f| f.stringValue().to_string());
                self.send(NativeEvent::ImageAdjustEdited {
                    document: p.document,
                    fields,
                    gray_index: p.gray.indexOfSelectedItem(),
                });
            });
        }

        /// Any field, pop-up or checkbox of the tracer panel (except the recipe): all values go together.
        #[unsafe(method(traceForm:))]
        fn trace_form(&self, _sender: Option<&AnyObject>) {
            ffi_guard("traceForm:", (), || {
                let p = self.ivars();
                let fields = p.trace_fields.each_ref().map(|f| f.stringValue().to_string());
                self.send(NativeEvent::TraceFormEdited {
                    document: p.document,
                    fields,
                    mode_index: p.trace_mode.indexOfSelectedItem(),
                    threshold_index: p.trace_threshold.indexOfSelectedItem(),
                    invert: p.trace_invert.state() == NSControlStateValueOn,
                    curves: p.trace_curves.state() == NSControlStateValueOn,
                });
            });
        }

        /// The recipe pop-up of the tracer panel.
        #[unsafe(method(tracePreset:))]
        fn trace_preset(&self, sender: Option<&AnyObject>) {
            ffi_guard("tracePreset:", (), || {
                let Some(popup) = sender.and_then(|s| s.downcast_ref::<NSPopUpButton>()) else { return };
                self.send(NativeEvent::TracePresetChosen {
                    document: self.ivars().document,
                    index: popup.indexOfSelectedItem(),
                });
            });
        }

        /// The tracer's "Gebruik voorstel" button.
        #[unsafe(method(traceSuggestion:))]
        fn trace_suggestion(&self, _sender: Option<&AnyObject>) {
            ffi_guard("traceSuggestion:", (), || {
                self.send(NativeEvent::TraceSuggestionClicked { document: self.ivars().document });
            });
        }
    }

    unsafe impl NSObjectProtocol for ImagePanelTarget {}
);

impl ImagePanelTarget {
    fn new(mtm: MainThreadMarker, parts: ImageTargetParts) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(parts);
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    fn send(&self, event: NativeEvent) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.native(event);
        }
    }
}

/// Point a control at the image target.
pub(crate) fn wire(control: &objc2_app_kit::NSControl, target: &ImagePanelTarget, action: objc2::runtime::Sel) {
    // SAFETY: the target is weak; `ImageSheets` keeps it alive while the control exists and clears
    // the target in `detach`. The action is one of the target's methods taking the sender (`v@:@`).
    unsafe { control.setTarget(Some(target.as_ref())) };
    // SAFETY: as above.
    unsafe { control.setAction(Some(action)) };
}

// ---------------------------------------------------------------------------------------------
// The sheets.
// ---------------------------------------------------------------------------------------------

/// The bitmap import sheet: file, pixel size, DPI field with the proposal, physical size, issue.
pub(crate) struct ImageImportSheet {
    pub(crate) sheet: Retained<NSWindow>,
    surface: FormSurface,
    /// File, pixels, DPI caption, proposal, size, issue.
    pub(crate) labels: [Retained<NSTextField>; 6],
    pub(crate) dpi: Retained<NSTextField>,
    pub(crate) commit: Retained<NSButton>,
    pub(crate) cancel: Retained<NSButton>,
    /// `false` while the sheet is down.
    pub(crate) shown: bool,
    /// The DPI text as the view model last had it (the field is written when it changed).
    last_dpi: Option<String>,
}

/// The Adjust Image sheet.
pub(crate) struct ImageAdjustSheet {
    pub(crate) sheet: Retained<NSWindow>,
    surface: FormSurface,
    /// Operation title, source size, issue, "berekenen…".
    pub(crate) labels: [Retained<NSTextField>; 4],
    /// Brightness, contrast, gamma, sharpen amount, sharpen radius, crop x, y, width, height.
    pub(crate) fields: [Retained<NSTextField>; 9],
    pub(crate) gray: Retained<NSPopUpButton>,
    pub(crate) preview: Retained<NSImageView>,
    pub(crate) apply: Retained<NSButton>,
    pub(crate) cancel: Retained<NSButton>,
    /// The operation whose values are in the fields while the sheet is up.
    pub(crate) shown: Option<OperationId>,
    /// The field texts as the view model last had them (a field is written when its text changed).
    pub(crate) last_fields: Option<[String; 9]>,
    pub(crate) last_gray: Option<usize>,
    pub(crate) last_preview: Option<BitmapImage>,
}

/// All image sheets of one window and their action target.
pub(crate) struct ImageSheets {
    pub(crate) import: ImageImportSheet,
    pub(crate) adjust: ImageAdjustSheet,
    pub(crate) tracer: TracerSheet,
    target: Retained<ImagePanelTarget>,
}

/// Build the image sheets of one window. `controller` is the window controller whose
/// `panelButton:` runs the command buttons.
pub(crate) fn image_sheets(
    mtm: MainThreadMarker,
    controller: &AnyObject,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> ImageSheets {
    let command_button = |b: &NSButton, key: CommandKey| {
        point_at(b, controller, sel!(panelButton:));
        b.setTag(menu_tag(MenuItem::Command(key)));
    };
    let import = import_sheet(mtm, locale, &command_button);
    let adjust = adjust_sheet(mtm, locale, &command_button);
    let tracer = tracer_sheet(mtm, locale, &command_button);
    let target = ImagePanelTarget::new(
        mtm,
        ImageTargetParts {
            app,
            document,
            dpi: import.dpi.clone(),
            adjust_fields: adjust.fields.clone(),
            gray: adjust.gray.clone(),
            trace_fields: tracer.fields.clone(),
            trace_mode: tracer.mode.clone(),
            trace_threshold: tracer.threshold.clone(),
            trace_invert: tracer.invert.clone(),
            trace_curves: tracer.curves.clone(),
        },
    );
    wire(&import.dpi, &target, sel!(imageDpi:));
    for f in &adjust.fields {
        wire(f, &target, sel!(imageAdjust:));
    }
    wire(&adjust.gray, &target, sel!(imageAdjust:));
    for f in &tracer.fields {
        wire(f, &target, sel!(traceForm:));
    }
    for c in [&tracer.mode, &tracer.threshold] {
        wire(c, &target, sel!(traceForm:));
    }
    for c in [&tracer.invert, &tracer.curves] {
        wire(c, &target, sel!(traceForm:));
    }
    wire(&tracer.recipe, &target, sel!(tracePreset:));
    wire(&tracer.suggestion, &target, sel!(traceSuggestion:));
    ImageSheets { import, adjust, tracer, target }
}

impl ImageSheets {
    /// Clear every weak target (before the window controller and the target go).
    pub(crate) fn clear_targets(&self) {
        let controls: Vec<&objc2_app_kit::NSControl> = [
            &*self.import.dpi as &objc2_app_kit::NSControl,
            &self.import.commit,
            &self.import.cancel,
            &self.adjust.gray,
            &self.adjust.apply,
            &self.adjust.cancel,
            &self.tracer.mode,
            &self.tracer.threshold,
            &self.tracer.recipe,
            &self.tracer.suggestion,
            &self.tracer.invert,
            &self.tracer.curves,
            &self.tracer.apply,
            &self.tracer.close,
        ]
        .into_iter()
        .chain(self.adjust.fields.iter().map(|f| &**f as &objc2_app_kit::NSControl))
        .chain(self.tracer.fields.iter().map(|f| &**f as &objc2_app_kit::NSControl))
        .collect();
        for c in controls {
            // SAFETY: a nil target cannot dangle.
            unsafe { c.setTarget(None) };
        }
        let _ = &self.target;
    }

    /// End every sheet that is still up (window teardown).
    pub(crate) fn end_all(&self, window: &NSWindow) {
        if self.import.shown {
            window.endSheet(&self.import.sheet);
        }
        if self.adjust.shown.is_some() {
            window.endSheet(&self.adjust.sheet);
        }
        if self.tracer.shown {
            window.endSheet(&self.tracer.sheet);
        }
    }
}

/// Makes a button of a form run `CommandKey` through the window controller's `panelButton:`.
pub(crate) type CommandButton<'a> = dyn Fn(&NSButton, CommandKey) + 'a;

fn import_sheet(mtm: MainThreadMarker, locale: Locale, command_button: &CommandButton<'_>) -> ImageImportSheet {
    let spec = image_import_form(locale);
    let surface = FormSurface::new(mtm, &[(&spec, IMPORT_WIDTH)]);
    let sheet = form_sheet(mtm, &surface, host_text(HostText::ImageImportTitle, locale));
    let cancel = surface.dialog_button(ButtonRole::Cancel);
    command_button(&cancel, CommandKey::CancelImport);
    let commit = surface.dialog_button(ButtonRole::Primary);
    command_button(&commit, CommandKey::CommitImport);
    ImageImportSheet {
        sheet,
        labels: [
            surface.text(import_id::FILE),
            surface.text(import_id::PIXELS),
            surface.caption(import_id::DPI),
            surface.text(import_id::SUGGESTION),
            surface.text(import_id::SIZE),
            surface.text(import_id::ISSUE),
        ],
        dpi: surface.text(import_id::DPI),
        commit,
        cancel,
        shown: false,
        last_dpi: None,
        surface,
    }
}

fn adjust_sheet(mtm: MainThreadMarker, locale: Locale, command_button: &CommandButton<'_>) -> ImageAdjustSheet {
    let (left, right) = image_adjust_forms(locale);
    let surface = FormSurface::new(mtm, &[(&left, ADJUST_WIDTHS.0), (&right, ADJUST_WIDTHS.1)]);
    let sheet = form_sheet(mtm, &surface, host_text(HostText::ImageAdjustTitle, locale));
    let cancel = surface.dialog_button(ButtonRole::Cancel);
    command_button(&cancel, CommandKey::CancelImageAdjust);
    let apply = surface.dialog_button(ButtonRole::Primary);
    command_button(&apply, CommandKey::ApplyImageAdjust);
    // Brightness, contrast, gamma, sharpen amount and radius, then the crop x, y, width and height.
    let (v, c) = (adjust_id::VALUE, adjust_id::CROP);
    let fields = [v, v + 1, v + 2, v + 3, v + 4, c, c + 1, c + 2, c + 3].map(|id| surface.text(id));
    ImageAdjustSheet {
        sheet,
        labels: [
            surface.text(adjust_id::TITLE),
            surface.text(adjust_id::SOURCE),
            surface.text(adjust_id::ISSUE),
            surface.text(adjust_id::COMPUTING),
        ],
        fields,
        gray: surface.popup(adjust_id::GRAY),
        preview: surface.image(adjust_id::PREVIEW),
        apply,
        cancel,
        shown: None,
        last_fields: None,
        last_gray: None,
        last_preview: None,
        surface,
    }
}

impl DocWindow {
    /// Show, fill or end the image sheets as presentation opens and closes them.
    pub(crate) fn refresh_image(&mut self, vm: &AppViewModel, document: DocumentId) {
        self.refresh_image_import(vm, document);
        self.refresh_image_adjust(vm, document);
        self.refresh_tracer(vm, document);
    }

    fn refresh_image_import(&mut self, vm: &AppViewModel, document: DocumentId) {
        let model = vm.document(document).and_then(|d| d.image.import.as_ref());
        let Some(panel) = model.map(|m| image_import_panel(m, self.locale)) else {
            if self.image.import.shown {
                self.image.import.shown = false;
                self.image.import.last_dpi = None;
                end_sheet(&self.window, &self.image.import.sheet, "ending the bitmap import sheet");
            }
            return;
        };
        let s = &mut self.image.import;
        let ImageImportPanel {
            file,
            pixels,
            dpi_caption,
            dpi_text,
            suggestion,
            size,
            issue,
            dpi_invalid,
            commit_enabled,
        } = panel;
        // The caption of the resolution row (index 2) is part of the form, with the unit as its
        // suffix, so the model's `dpi_caption` is not copied over it.
        let _ = dpi_caption;
        for (i, (l, text)) in
            s.labels.iter().zip([&file, &pixels, &dpi_caption, &suggestion, &size, &issue]).enumerate()
        {
            if i != 2 {
                set_text(l, text);
            }
        }
        // The DPI field is written when the view model's text changed since the last refresh (the
        // proposal, a corrected value); a refused text stays what the user typed.
        if s.last_dpi.as_deref() != Some(dpi_text.as_str()) {
            s.dpi.setStringValue(&NSString::from_str(&dpi_text));
            s.last_dpi = Some(dpi_text);
        }
        mark_field(&s.dpi, dpi_invalid && !issue.is_empty());
        s.commit.setEnabled(commit_enabled);
        s.surface.fit_window(&s.sheet);
        if !s.shown {
            s.shown = begin_sheet(&self.window, &s.sheet, "showing the bitmap import sheet");
        }
    }

    fn refresh_image_adjust(&mut self, vm: &AppViewModel, document: DocumentId) {
        let locale = self.locale;
        let model = vm.document(document).and_then(|d| d.image.adjust.as_ref());
        let Some(panel) = model.map(|m| image_adjust_panel(m, locale)) else {
            let s = &mut self.image.adjust;
            if s.shown.take().is_some() {
                s.last_fields = None;
                s.last_gray = None;
                s.last_preview = None;
                end_sheet(&self.window, &s.sheet, "ending the Adjust Image sheet");
            }
            return;
        };
        let s = &mut self.image.adjust;
        let ImageAdjustPanel {
            operation,
            title,
            source,
            fields,
            gray_index,
            issue,
            issue_field,
            computing,
            preview,
            apply_enabled,
        } = panel;
        for (l, text) in s.labels.iter().zip([&title, &source, &issue, &computing]) {
            set_text(l, text);
        }
        // A field is written when the view model's text for it changed since the last refresh (and
        // once per opening): a refused input keeps what the user typed, as the model keeps the old text.
        let reopened = s.shown != Some(operation);
        for (i, (f, text)) in s.fields.iter().zip(&fields).enumerate() {
            if reopened || s.last_fields.as_ref().is_none_or(|last| last[i] != *text) {
                f.setStringValue(&NSString::from_str(text));
            }
            mark_field(f, issue_field == Some(i));
        }
        if reopened || s.last_gray != Some(gray_index) {
            s.gray.selectItemAtIndex(gray_index as isize);
        }
        s.last_fields = Some(fields);
        s.last_gray = Some(gray_index);
        show_bitmap(&s.preview, &mut s.last_preview, preview.as_ref());
        s.apply.setEnabled(apply_enabled);
        s.surface.fit_window(&s.sheet);
        if s.shown.is_none() {
            s.shown = Some(operation);
            if !begin_sheet(&self.window, &s.sheet, "showing the Adjust Image sheet") {
                s.shown = None;
            }
        } else {
            s.shown = Some(operation);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// File panels of the Beeld menu.
// ---------------------------------------------------------------------------------------------

/// The open panel for pictures of these types: files only, one or (`multiple`, G8-trace T10) several.
pub(crate) fn image_open_panel(mtm: MainThreadMarker, types: &[&str], multiple: bool) -> Retained<NSOpenPanel> {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(multiple);
    let names: Vec<Retained<NSString>> = types.iter().map(|t| NSString::from_str(t)).collect();
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&names)));
    panel
}

/// The panel that asks for the files of a batch of traces: several pictures of the tracer's types.
pub fn trace_batch_open_panel(mtm: MainThreadMarker) -> Retained<NSOpenPanel> {
    image_open_panel(mtm, &TRACE_TYPES, true)
}

/// An open sheet for several pictures (the files of a batch); `event` turns the chosen paths, in the
/// order the panel lists them (empty on cancel), into the event.
pub(crate) fn show_image_open_panel_multiple(
    mtm: MainThreadMarker,
    window: &NSWindow,
    app: Weak<HostApp>,
    types: &[&str],
    what: &'static str,
    event: impl Fn(Vec<PathBuf>) -> NativeEvent + 'static,
) -> Option<Retained<NSOpenPanel>> {
    let panel = image_open_panel(mtm, types, true);
    let reader = panel.clone();
    let handler = RcBlock::new(move |response: NSModalResponse| {
        ffi_guard(what, (), || {
            let paths: Vec<PathBuf> = if response == crate::adapter::response::OK {
                reader.URLs().iter().filter_map(|u| u.to_file_path()).collect()
            } else {
                Vec::new()
            };
            if let Some(app) = app.upgrade() {
                app.native(event(paths));
            }
        });
    });
    if let Err(e) = crate::appkit::objc_try(what, || panel.beginSheetModalForWindow_completionHandler(window, &handler))
    {
        eprintln!("burnburn: {e}");
        return None;
    }
    Some(panel)
}

/// An open sheet for one picture; `event` turns the chosen path (`None` on cancel) into the event.
pub(crate) fn show_image_open_panel(
    mtm: MainThreadMarker,
    window: &NSWindow,
    app: Weak<HostApp>,
    types: &[&str],
    what: &'static str,
    event: impl Fn(Option<PathBuf>) -> NativeEvent + 'static,
) {
    let panel = image_open_panel(mtm, types, false);
    let reader = panel.clone();
    let handler = RcBlock::new(move |response: NSModalResponse| {
        ffi_guard(what, (), || {
            let path: Option<PathBuf> = (response == crate::adapter::response::OK)
                .then(|| reader.URLs().firstObject())
                .flatten()
                .and_then(|u| u.to_file_path());
            if let Some(app) = app.upgrade() {
                app.native(event(path));
            }
        });
    });
    if let Err(e) = crate::appkit::objc_try(what, || panel.beginSheetModalForWindow_completionHandler(window, &handler))
    {
        eprintln!("burnburn: {e}");
    }
}

/// A save sheet for a PNG; `event` turns the chosen path (`None` on cancel) into the event.
pub(crate) fn show_bitmap_save_panel(
    mtm: MainThreadMarker,
    window: &NSWindow,
    app: Weak<HostApp>,
    suggested_name: &str,
    event: impl Fn(Option<PathBuf>) -> NativeEvent + 'static,
) {
    let panel = NSSavePanel::savePanel(mtm);
    panel.setNameFieldStringValue(&NSString::from_str(suggested_name));
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[NSString::from_str("png")])));
    let reader = panel.clone();
    let handler = RcBlock::new(move |response: NSModalResponse| {
        ffi_guard("bitmap save panel completion", (), || {
            let path = (response == crate::adapter::response::OK)
                .then(|| reader.URL())
                .flatten()
                .and_then(|u| u.to_file_path());
            if let Some(app) = app.upgrade() {
                app.native(event(path));
            }
        });
    });
    if let Err(e) = crate::appkit::objc_try("showing the bitmap save sheet", || {
        panel.beginSheetModalForWindow_completionHandler(window, &handler)
    }) {
        eprintln!("burnburn: {e}");
    }
}
