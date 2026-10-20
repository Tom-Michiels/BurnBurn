//! The bitmap import sheet (P03) and the Adjust Image sheet (T09), plus the one action target of
//! all image panels (these two and the tracer) and the file panels of the Beeld menu.
//!
//! The sheets only show what `panels_image` computed from the view model and forward what the user
//! typed as `NativeEvent`s. The command buttons (Importeer, Pas toe, ...) use the window
//! controller's `panelButton:` (tag = menu tag of the command); fields, pop-ups and checkboxes use
//! [`ImagePanelTarget`], which reads every widget of its panel and sends one event, so one edit is
//! one intent with all values. `unsafe` is limited to the Objective-C class definition, the
//! control wiring and the bitmap copy (SAFETY comments); no callback lets a panic unwind into AppKit.
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
    NSDeviceRGBColorSpace, NSFont, NSImage, NSImageFrameStyle, NSImageScaling, NSImageView, NSLineBreakMode,
    NSModalResponse, NSOpenPanel, NSPopUpButton, NSSavePanel, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSSize, NSString};

use super::tracer::{TracerSheet, tracer_sheet};
use super::widgets::{button, field, label, rect, set_text};
use crate::adapter::{MenuItem, NativeEvent, menu_tag};
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text};
use crate::panels_image::{
    BitmapImage, GRAY_METHODS, ImageAdjustPanel, ImageImportPanel, gray_method_name, image_adjust_panel,
    image_import_panel,
};

/// Picture types the bitmap flow offers in file panels. BMP and TIFF are listed so the decoder's
/// honest refusal reaches the user, instead of the file silently not being selectable.
pub(crate) const BITMAP_TYPES: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "tif", "tiff"];

/// Picture types the tracer reads.
pub(crate) const TRACE_TYPES: [&str; 3] = ["png", "jpg", "jpeg"];

const IMPORT_SIZE: (f64, f64) = (460.0, 290.0);
const ADJUST_SIZE: (f64, f64) = (700.0, 490.0);

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

/// A bold label added to `content`.
pub(crate) fn heading(
    mtm: MainThreadMarker,
    content: &NSView,
    frame: objc2_foundation::NSRect,
) -> Retained<NSTextField> {
    let l = caption(mtm, content, "", frame);
    l.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    l
}

/// A text field with an accessibility label, added to `content`.
pub(crate) fn input(
    mtm: MainThreadMarker,
    content: &NSView,
    name: &str,
    frame: objc2_foundation::NSRect,
) -> Retained<NSTextField> {
    let f = field(mtm, frame);
    f.setAccessibilityLabel(Some(&NSString::from_str(name)));
    content.addSubview(&f);
    f
}

/// A pop-up with `titles` and an accessibility label, added to `content`.
pub(crate) fn popup(
    mtm: MainThreadMarker,
    content: &NSView,
    name: &str,
    titles: &[String],
    frame: objc2_foundation::NSRect,
) -> Retained<NSPopUpButton> {
    let p = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), frame, false);
    for title in titles {
        p.addItemWithTitle(&NSString::from_str(title));
    }
    p.setAccessibilityLabel(Some(&NSString::from_str(name)));
    content.addSubview(&p);
    p
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
    let command_button = |content: &NSView, title: &str, key: CommandKey, frame| {
        let b = button(mtm, title, controller, sel!(panelButton:), frame);
        b.setTag(menu_tag(MenuItem::Command(key)));
        content.addSubview(&b);
        b
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

pub(crate) type CommandButton<'a> =
    dyn Fn(&NSView, &str, CommandKey, objc2_foundation::NSRect) -> Retained<NSButton> + 'a;

fn import_sheet(mtm: MainThreadMarker, locale: Locale, command_button: &CommandButton<'_>) -> ImageImportSheet {
    let t = |text: HostText| host_text(text, locale);
    let (w, h) = IMPORT_SIZE;
    let (sheet, content) = sheet_window(mtm, IMPORT_SIZE, t(HostText::ImageImportTitle));
    let file = heading(mtm, &content, rect(16.0, h - 36.0, w - 32.0, 20.0));
    let pixels = caption(mtm, &content, "", rect(16.0, h - 60.0, w - 32.0, 18.0));
    let dpi_caption = caption(mtm, &content, t(HostText::ImageImportDpi), rect(16.0, h - 96.0, 190.0, 22.0));
    let dpi = input(mtm, &content, t(HostText::ImageImportDpi), rect(212.0, h - 98.0, 120.0, 24.0));
    let suggestion = note(mtm, &content, rect(16.0, h - 144.0, w - 32.0, 40.0), &NSColor::secondaryLabelColor());
    let size = caption(mtm, &content, "", rect(16.0, h - 172.0, w - 32.0, 18.0));
    size.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    let issue = note(mtm, &content, rect(16.0, 56.0, w - 32.0, 40.0), &NSColor::systemRedColor());
    let cancel = command_button(
        &content,
        t(HostText::ImageImportCancel),
        CommandKey::CancelImport,
        rect(w - 232.0, 12.0, 104.0, 28.0),
    );
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let commit = command_button(
        &content,
        t(HostText::ImageImportCommit),
        CommandKey::CommitImport,
        rect(w - 116.0, 12.0, 104.0, 28.0),
    );
    commit.setKeyEquivalent(&NSString::from_str("\r"));
    sheet.setContentView(Some(&content));
    ImageImportSheet {
        sheet,
        labels: [file, pixels, dpi_caption, suggestion, size, issue],
        dpi,
        commit,
        cancel,
        shown: false,
        last_dpi: None,
    }
}

fn adjust_sheet(mtm: MainThreadMarker, locale: Locale, command_button: &CommandButton<'_>) -> ImageAdjustSheet {
    let t = |text: HostText| host_text(text, locale);
    let (w, h) = ADJUST_SIZE;
    let left = 330.0;
    let (sheet, content) = sheet_window(mtm, ADJUST_SIZE, t(HostText::ImageAdjustTitle));
    let mut y = h - 16.0;
    let title = heading(mtm, &content, rect(16.0, y - 20.0, left, 20.0));
    y -= 24.0;
    let source = caption(mtm, &content, "", rect(16.0, y - 18.0, left, 18.0));
    y -= 28.0;
    // Five value rows, the gray pop-up, the crop heading and two crop rows of two fields.
    let y_values = y;
    let y_crop = y_values - 5.0 * 30.0 - 36.0 - 38.0;
    let rows: [(HostText, f64, f64, f64, f64, f64); 9] = [
        (HostText::ImageAdjustBrightness, 16.0, 196.0, 216.0, 114.0, y_values),
        (HostText::ImageAdjustContrast, 16.0, 196.0, 216.0, 114.0, y_values - 30.0),
        (HostText::ImageAdjustGamma, 16.0, 196.0, 216.0, 114.0, y_values - 60.0),
        (HostText::ImageAdjustSharpenAmount, 16.0, 196.0, 216.0, 114.0, y_values - 90.0),
        (HostText::ImageAdjustSharpenRadius, 16.0, 196.0, 216.0, 114.0, y_values - 120.0),
        (HostText::ImageAdjustCropX, 16.0, 60.0, 80.0, 80.0, y_crop),
        (HostText::ImageAdjustCropY, 176.0, 60.0, 240.0, 90.0, y_crop),
        (HostText::ImageAdjustCropWidth, 16.0, 60.0, 80.0, 80.0, y_crop - 30.0),
        (HostText::ImageAdjustCropHeight, 176.0, 60.0, 240.0, 90.0, y_crop - 30.0),
    ];
    let fields = rows.map(|(name, cx, cw, fx, fw, ry)| {
        caption(mtm, &content, t(name), rect(cx, ry - 22.0, cw, 22.0));
        input(mtm, &content, t(name), rect(fx, ry - 24.0, fw, 24.0))
    });
    y = y_values - 150.0;
    caption(mtm, &content, t(HostText::ImageAdjustGray), rect(16.0, y - 22.0, 196.0, 22.0));
    let gray_titles: Vec<String> = GRAY_METHODS.iter().map(|g| gray_method_name(*g, locale).to_owned()).collect();
    let gray = popup(mtm, &content, t(HostText::ImageAdjustGray), &gray_titles, rect(212.0, y - 26.0, 150.0, 26.0));
    y -= 36.0;
    let crop = caption(mtm, &content, t(HostText::ImageAdjustCrop), rect(16.0, y - 34.0, left, 34.0));
    crop.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    crop.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    let issue = note(mtm, &content, rect(16.0, 56.0, left, 40.0), &NSColor::systemRedColor());
    let x = 364.0;
    let preview_caption = caption(mtm, &content, t(HostText::ImageAdjustPreview), rect(x, h - 36.0, 320.0, 18.0));
    preview_caption.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    let preview = image_view(mtm, &content, t(HostText::ImageAdjustPreview), rect(x, h - 366.0, 320.0, 322.0));
    let computing = caption(mtm, &content, "", rect(x, h - 392.0, 320.0, 18.0));
    let cancel = command_button(
        &content,
        t(HostText::ImageAdjustCancel),
        CommandKey::CancelImageAdjust,
        rect(w - 232.0, 12.0, 104.0, 28.0),
    );
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let apply = command_button(
        &content,
        t(HostText::ImageAdjustApply),
        CommandKey::ApplyImageAdjust,
        rect(w - 116.0, 12.0, 104.0, 28.0),
    );
    sheet.setContentView(Some(&content));
    ImageAdjustSheet {
        sheet,
        labels: [title, source, issue, computing],
        fields,
        gray,
        preview,
        apply,
        cancel,
        shown: None,
        last_fields: None,
        last_gray: None,
        last_preview: None,
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
        for (l, text) in s.labels.iter().zip([&file, &pixels, &dpi_caption, &suggestion, &size, &issue]) {
            set_text(l, text);
        }
        // The DPI field is written when the view model's text changed since the last refresh (the
        // proposal, a corrected value); a refused text stays what the user typed.
        if s.last_dpi.as_deref() != Some(dpi_text.as_str()) {
            s.dpi.setStringValue(&NSString::from_str(&dpi_text));
            s.last_dpi = Some(dpi_text);
        }
        mark_field(&s.dpi, dpi_invalid && !issue.is_empty());
        s.commit.setEnabled(commit_enabled);
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

/// An open sheet for one picture; `event` turns the chosen path (`None` on cancel) into the event.
pub(crate) fn show_image_open_panel(
    mtm: MainThreadMarker,
    window: &NSWindow,
    app: Weak<HostApp>,
    types: &[&str],
    what: &'static str,
    event: impl Fn(Option<PathBuf>) -> NativeEvent + 'static,
) {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    let names: Vec<Retained<NSString>> = types.iter().map(|t| NSString::from_str(t)).collect();
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&names)));
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
