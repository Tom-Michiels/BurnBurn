//! The camera window (W3-UI-camera): one floating utility panel for the whole app with the camera
//! picture on the bed (switch, opacity slider, update, material height, edge margin), the cameras in
//! the bed system, the stored calibrations (with the invalid ones badged), the lens-calibration
//! wizard and the workspace-alignment wizard with its photo, in which the marks are clicked.
//!
//! The window only copies the headless model (`crate::panels::camera_window_panel`) into widgets
//! and turns user actions into `UiIntent`s via the headless builders. Nothing here opens a camera:
//! the privacy prompt belongs to the access button of the main window's camera section, and the
//! camera starts on this window's "use camera" button.

#![allow(unsafe_code)] // defining Objective-C subclasses and sending to `super` has no safe API (SAFETY below)

use std::cell::RefCell;
use std::rc::Weak;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSBitmapImageRep, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSDeviceRGBColorSpace, NSEvent, NSFont, NSImage, NSImageScaling, NSImageView, NSPanel,
    NSPopUpButton, NSScrollView, NSSlider, NSTextField, NSView, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSSize, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{field, label, push_button, rect, set_error_text, set_text};
use crate::l10n::Locale;
use crate::panels::{CameraCommand, CameraInputs, CameraText, CameraWindowPanel, camera_text};

pub(crate) const WIDTH: f64 = 560.0;
const HEIGHT: f64 = 1140.0;
const VIEW_HEIGHT: f64 = 640.0;
const MARGIN: f64 = 12.0;
const PHOTO_HEIGHT: f64 = 300.0;

/// Tags of the text fields (the commit routes by tag).
const FIELD_HEIGHT: isize = 1;
const FIELD_MARGIN: isize = 2;

// ---------------------------------------------------------------------------------------------
// The photo: a click is a mark position.
// ---------------------------------------------------------------------------------------------

pub(crate) struct PhotoIvars {
    app: Weak<HostApp>,
}

define_class!(
    #[unsafe(super(NSImageView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnCameraPhoto"]
    #[ivars = PhotoIvars]
    pub(crate) struct PhotoView;

    impl PhotoView {
        /// The first click on an inactive panel already places a mark; it is not only activation.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            ffi_guard("camera photo mouseDown:", (), || {
                let at = self.convertPoint_fromView(event.locationInWindow(), None);
                let size = self.bounds().size;
                if let Some(app) = self.ivars().app.upgrade() {
                    app.camera_photo_clicked((size.width, size.height), (at.x, at.y));
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for PhotoView {}
);

impl PhotoView {
    fn new(mtm: MainThreadMarker, app: Weak<HostApp>, frame: objc2_foundation::NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PhotoIvars { app });
        // SAFETY: NSImageView's designated initialiser on a freshly allocated instance with ivars set.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        view
    }
}

// ---------------------------------------------------------------------------------------------
// Target of every control.
// ---------------------------------------------------------------------------------------------

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnCameraTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct CameraTarget;

    impl CameraTarget {
        /// A button: the tag is its index in `CameraCommand::ALL`.
        #[unsafe(method(cameraCommand:))]
        fn camera_command(&self, sender: Option<&AnyObject>) {
            ffi_guard("cameraCommand:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some(command) = CameraCommand::from_tag(button.tag()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.camera_command_pressed(command);
                }
            });
        }

        #[unsafe(method(cameraVisible:))]
        fn camera_visible(&self, sender: Option<&AnyObject>) {
            ffi_guard("cameraVisible:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.dispatch(crate::panels::overlay_visible_intent(button.state() == NSControlStateValueOn));
                }
            });
        }

        #[unsafe(method(cameraOpacity:))]
        fn camera_opacity(&self, sender: Option<&AnyObject>) {
            ffi_guard("cameraOpacity:", (), || {
                let Some(slider) = sender.and_then(|s| s.downcast_ref::<NSSlider>()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.dispatch(crate::panels::opacity_intent(slider.doubleValue()));
                }
            });
        }

        /// Material height and edge margin commit (Return, Tab): the text goes on as typed.
        #[unsafe(method(cameraText:))]
        fn camera_text(&self, sender: Option<&AnyObject>) {
            ffi_guard("cameraText:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let text = field.stringValue().to_string();
                if let Some(app) = self.ivars().app.upgrade() {
                    app.camera_text_committed(field.tag(), &text);
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for CameraTarget {}
);

// ---------------------------------------------------------------------------------------------
// The window
// ---------------------------------------------------------------------------------------------

pub(crate) struct CameraWindow {
    pub(crate) panel: Retained<NSPanel>,
    /// Kept alive for the controls' (weak) target.
    _target: Retained<CameraTarget>,
    commands: Vec<(CameraCommand, Retained<NSButton>)>,
    visible: Retained<NSButton>,
    opacity: Retained<NSSlider>,
    height: Retained<NSTextField>,
    margin: Retained<NSTextField>,
    overlay_status: Retained<NSTextField>,
    note: Retained<NSTextField>,
    devices: Retained<NSPopUpButton>,
    lens_profile: Retained<NSTextField>,
    running: Retained<NSPopUpButton>,
    calibrations: Retained<NSPopUpButton>,
    cols: Retained<NSTextField>,
    rows: Retained<NSTextField>,
    pitch: Retained<NSTextField>,
    lens_status: Retained<NSTextField>,
    align_height: Retained<NSTextField>,
    marks: Retained<NSPopUpButton>,
    mark_x: Retained<NSTextField>,
    mark_y: Retained<NSTextField>,
    power: Retained<NSTextField>,
    speed: Retained<NSTextField>,
    marks_status: Retained<NSTextField>,
    photo: Retained<PhotoView>,
    alignment_status: Retained<NSTextField>,
    refusal: Retained<NSTextField>,
    /// The picture last put in the photo view: (frame sequence, clicked pixels), to redraw only on change.
    shown_photo: RefCell<Option<(u64, Vec<Option<[i64; 2]>>)>>,
    locale: Locale,
    /// The user asked for the window; it is on screen while this is true.
    pub(crate) shown: bool,
}

/// What a native test needs to drive the window like a user.
pub struct CameraParts<'a> {
    pub panel: &'a NSPanel,
    pub shown: bool,
    pub commands: &'a [(CameraCommand, Retained<NSButton>)],
    pub visible: &'a NSButton,
    pub opacity: &'a NSSlider,
    pub height: &'a NSTextField,
    pub overlay_status: &'a NSTextField,
    pub devices: &'a NSPopUpButton,
    pub running: &'a NSPopUpButton,
    pub calibrations: &'a NSPopUpButton,
    pub marks: &'a NSPopUpButton,
    pub lens_status: &'a NSTextField,
    pub marks_status: &'a NSTextField,
    pub alignment_status: &'a NSTextField,
    pub refusal: &'a NSTextField,
    pub photo: &'a NSImageView,
    pub fields: [&'a NSTextField; 11],
}

#[allow(unsafe_code)]
fn wire(b: &NSButton, target: &AnyObject, action: objc2::runtime::Sel, tag: isize) {
    b.setTag(tag);
    // SAFETY: `action` is a `CameraTarget` method taking the sender (`v@:@`); the target is weak and
    // the window keeps it alive while the control exists (cleared in `CameraWindow::clear`).
    unsafe { b.setTarget(Some(target)) };
    // SAFETY: as above.
    unsafe { b.setAction(Some(action)) };
}

pub(crate) fn camera_window(mtm: MainThreadMarker, app: Weak<HostApp>, locale: Locale) -> CameraWindow {
    let t = |text: CameraText| camera_text(text, locale);
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow | NSWindowStyleMask::Closable;
    // SAFETY: NSPanel's designated initialiser on a freshly allocated instance.
    let panel: Retained<NSPanel> = unsafe {
        msg_send![
            NSPanel::alloc(mtm),
            initWithContentRect: rect(0.0, 0.0, WIDTH, VIEW_HEIGHT),
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(false);
    panel.setTitle(&NSString::from_str(t(CameraText::WindowTitle)));
    panel.setAccessibilityLabel(Some(&NSString::from_str(t(CameraText::Accessibility))));

    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT));
    let inner = WIDTH - 2.0 * MARGIN - 16.0;
    let y = std::cell::Cell::new(HEIGHT);
    let row = |height: f64, gap: f64| {
        y.set(y.get() - height);
        let at = y.get();
        y.set(at - gap);
        at
    };
    let heading = |text: &str| {
        let at = row(18.0, 2.0);
        let l = label(mtm, text, rect(MARGIN, at, inner, 18.0));
        l.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
        content.addSubview(&l);
    };
    let small = |text: &str, at: f64, x: f64, w: f64| {
        let l = label(mtm, text, rect(x, at, w, 18.0));
        l.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        content.addSubview(&l);
    };
    let text_field = |at: f64, x: f64, w: f64, value: &str, accessible: &str| {
        let f = field(mtm, rect(x, at + 2.0, w, 24.0));
        f.setStringValue(&NSString::from_str(value));
        f.setAccessibilityLabel(Some(&NSString::from_str(accessible)));
        content.addSubview(&f);
        f
    };
    let wrapped = |height: f64, size: f64| {
        let at = row(height, 4.0);
        let l = label(mtm, "", rect(MARGIN, at, inner, height));
        l.setFont(Some(&NSFont::systemFontOfSize(size)));
        l.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
        content.addSubview(&l);
        l
    };
    let mut commands: Vec<(CameraCommand, Retained<NSButton>)> = Vec::new();
    let mut command_button = |command: CameraCommand, frame: objc2_foundation::NSRect| {
        let b = push_button(mtm, command.title(locale));
        b.setFrame(frame);
        content.addSubview(&b);
        commands.push((command, b));
    };
    let popup = |at: f64, x: f64, w: f64, accessible: &str| {
        let p = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(x, at, w, 26.0), false);
        p.setAccessibilityLabel(Some(&NSString::from_str(accessible)));
        content.addSubview(&p);
        p
    };

    // --- the picture on the bed ---
    heading(t(CameraText::SectionOverlay));
    let at = row(22.0, 4.0);
    // SAFETY: no target and no action yet; wired below once the target exists.
    let visible = unsafe {
        NSButton::checkboxWithTitle_target_action(&NSString::from_str(t(CameraText::ShowOverlay)), None, None, mtm)
    };
    visible.setFrame(rect(MARGIN, at, 220.0, 22.0));
    content.addSubview(&visible);
    small(t(CameraText::Opacity), at + 2.0, MARGIN + 230.0, 60.0);
    // SAFETY: no target and no action yet; wired below once the target exists.
    let opacity =
        unsafe { NSSlider::sliderWithValue_minValue_maxValue_target_action(50.0, 0.0, 100.0, None, None, mtm) };
    opacity.setFrame(rect(MARGIN + 292.0, at, inner - 292.0, 22.0));
    opacity.setAccessibilityLabel(Some(&NSString::from_str(t(CameraText::Opacity))));
    content.addSubview(&opacity);
    let at = row(28.0, 4.0);
    command_button(CameraCommand::UpdateOverlay, rect(MARGIN, at, 170.0, 28.0));
    small(t(CameraText::MaterialHeight), at + 6.0, MARGIN + 180.0, 130.0);
    let height = text_field(at, MARGIN + 312.0, 56.0, "0", t(CameraText::MaterialHeight));
    small(t(CameraText::EdgeMargin), at + 6.0, MARGIN + 376.0, 80.0);
    let margin = text_field(at, MARGIN + 458.0, 56.0, "0", t(CameraText::EdgeMargin));
    let overlay_status = wrapped(54.0, 11.0);
    let note = wrapped(30.0, 11.0);
    note.setTextColor(Some(&NSColor::secondaryLabelColor()));

    // --- cameras ---
    heading(t(CameraText::SectionCameras));
    let at = row(28.0, 4.0);
    let devices = popup(at, MARGIN, 190.0, t(CameraText::UseCamera));
    small(t(CameraText::LensProfile), at + 6.0, MARGIN + 198.0, 80.0);
    let lens_profile = text_field(at, MARGIN + 280.0, 100.0, "standard", t(CameraText::LensProfile));
    command_button(CameraCommand::UseCamera, rect(MARGIN + 388.0, at, 126.0, 28.0));
    let at = row(28.0, 4.0);
    let running = popup(at, MARGIN, 380.0, t(CameraText::SectionCameras));
    command_button(CameraCommand::DropCamera, rect(MARGIN + 388.0, at, 126.0, 28.0));

    // --- calibrations ---
    heading(t(CameraText::SectionCalibrations));
    let at = row(28.0, 4.0);
    let calibrations = popup(at, MARGIN, inner, t(CameraText::SectionCalibrations));
    let at = row(28.0, 4.0);
    command_button(CameraCommand::DeleteCalibration, rect(MARGIN, at, 140.0, 28.0));
    command_button(CameraCommand::Recalibrate, rect(MARGIN + 148.0, at, 170.0, 28.0));

    // --- lens wizard ---
    heading(t(CameraText::SectionLens));
    let at = row(28.0, 4.0);
    small(t(CameraText::GridColumns), at + 6.0, MARGIN, 100.0);
    let cols = text_field(at, MARGIN + 102.0, 40.0, "9", t(CameraText::GridColumns));
    small(t(CameraText::GridRows), at + 6.0, MARGIN + 150.0, 110.0);
    let rows = text_field(at, MARGIN + 262.0, 40.0, "6", t(CameraText::GridRows));
    small(t(CameraText::GridPitch), at + 6.0, MARGIN + 312.0, 90.0);
    let pitch = text_field(at, MARGIN + 404.0, 60.0, "25", t(CameraText::GridPitch));
    let at = row(28.0, 4.0);
    command_button(CameraCommand::StartLens, rect(MARGIN, at, 110.0, 28.0));
    command_button(CameraCommand::AddPhoto, rect(MARGIN + 116.0, at, 150.0, 28.0));
    command_button(CameraCommand::FitLens, rect(MARGIN + 272.0, at, 100.0, 28.0));
    command_button(CameraCommand::AcceptLens, rect(MARGIN + 378.0, at, 136.0, 28.0));
    let at = row(28.0, 4.0);
    command_button(CameraCommand::CancelLens, rect(MARGIN, at, 110.0, 28.0));
    let lens_status = wrapped(66.0, 11.0);

    // --- alignment wizard ---
    heading(t(CameraText::SectionAlignment));
    let at = row(28.0, 4.0);
    small(t(CameraText::AlignHeight), at + 6.0, MARGIN, 190.0);
    let align_height = text_field(at, MARGIN + 192.0, 56.0, "0", t(CameraText::AlignHeight));
    command_button(CameraCommand::StartAlignment, rect(MARGIN + 258.0, at, 150.0, 28.0));
    command_button(CameraCommand::CancelAlignment, rect(MARGIN + 414.0, at, 100.0, 28.0));
    let at = row(28.0, 4.0);
    let marks = popup(at, MARGIN, inner, t(CameraText::Marks));
    let at = row(28.0, 4.0);
    small(t(CameraText::MarkX), at + 6.0, MARGIN, 50.0);
    let mark_x = text_field(at, MARGIN + 52.0, 70.0, "", t(CameraText::MarkX));
    small(t(CameraText::MarkY), at + 6.0, MARGIN + 130.0, 50.0);
    let mark_y = text_field(at, MARGIN + 182.0, 70.0, "", t(CameraText::MarkY));
    command_button(CameraCommand::SetMark, rect(MARGIN + 262.0, at, 130.0, 28.0));
    let at = row(28.0, 4.0);
    small(t(CameraText::Power), at + 6.0, MARGIN, 90.0);
    let power = text_field(at, MARGIN + 92.0, 46.0, "10", t(CameraText::Power));
    small(t(CameraText::Speed), at + 6.0, MARGIN + 146.0, 110.0);
    let speed = text_field(at, MARGIN + 258.0, 56.0, "100", t(CameraText::Speed));
    command_button(CameraCommand::ProposeMarks, rect(MARGIN + 322.0, at, 192.0, 28.0));
    let at = row(28.0, 4.0);
    command_button(CameraCommand::SendMarks, rect(MARGIN, at, 300.0, 28.0));
    let marks_status = wrapped(44.0, 11.0);
    let at = row(28.0, 4.0);
    command_button(CameraCommand::TakePhoto, rect(MARGIN, at, 220.0, 28.0));
    let at = row(PHOTO_HEIGHT, 4.0);
    let photo = PhotoView::new(mtm, app.clone(), rect(MARGIN, at, inner, PHOTO_HEIGHT));
    photo.setAccessibilityLabel(Some(&NSString::from_str(t(CameraText::PhotoHint))));
    content.addSubview(&photo);
    let alignment_status = wrapped(70.0, 11.0);
    let at = row(28.0, 4.0);
    command_button(CameraCommand::FitAlignment, rect(MARGIN, at, 190.0, 28.0));
    command_button(CameraCommand::SaveAlignment, rect(MARGIN + 196.0, at, 190.0, 28.0));
    let refusal = wrapped(40.0, 11.0);
    refusal.setTextColor(Some(&NSColor::systemRedColor()));

    // The scrolled window around the long content.
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, WIDTH, VIEW_HEIGHT));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    scroll.setDocumentView(Some(&content));
    panel.setContentView(Some(&scroll));
    content.scrollPoint(NSPoint::new(0.0, HEIGHT));

    let target = CameraTarget::alloc(mtm).set_ivars(TargetIvars { app });
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<CameraTarget> = unsafe { msg_send![super(target), init] };
    let t_ref: &AnyObject = &target;
    for (command, b) in &commands {
        wire(b, t_ref, sel!(cameraCommand:), command.tag());
    }
    wire(&visible, t_ref, sel!(cameraVisible:), 0);
    for (f, tag) in [(&height, FIELD_HEIGHT), (&margin, FIELD_MARGIN)] {
        f.setTag(tag);
        // SAFETY: as in `wire`.
        unsafe { f.setTarget(Some(t_ref)) };
        // SAFETY: as in `wire`.
        unsafe { f.setAction(Some(sel!(cameraText:))) };
    }
    // SAFETY: as in `wire`.
    unsafe { opacity.setTarget(Some(t_ref)) };
    // SAFETY: as in `wire`.
    unsafe { opacity.setAction(Some(sel!(cameraOpacity:))) };
    opacity.setContinuous(false);

    CameraWindow {
        panel,
        _target: target,
        commands,
        visible,
        opacity,
        height,
        margin,
        overlay_status,
        note,
        devices,
        lens_profile,
        running,
        calibrations,
        cols,
        rows,
        pitch,
        lens_status,
        align_height,
        marks,
        mark_x,
        mark_y,
        power,
        speed,
        marks_status,
        photo,
        alignment_status,
        refusal,
        shown_photo: RefCell::new(None),
        locale,
        shown: false,
    }
}

fn set_items(popup: &NSPopUpButton, items: &[String], placeholder: &str) {
    let titles: Vec<String> = popup.itemTitles().iter().map(|t| t.to_string()).collect();
    let wanted: Vec<String> = if items.is_empty() { vec![placeholder.to_owned()] } else { items.to_vec() };
    if titles != wanted {
        let keep = popup.indexOfSelectedItem();
        popup.removeAllItems();
        for item in &wanted {
            popup.addItemWithTitle(&NSString::from_str(item));
        }
        if keep >= 0 && (keep as usize) < wanted.len() {
            popup.selectItemAtIndex(keep);
        }
    }
    popup.setEnabled(!items.is_empty());
}

/// Set a model-driven field unless the user is typing in it.
fn set_field(f: &NSTextField, text: &str) {
    if f.currentEditor().is_none() {
        set_text(f, text);
    }
}

impl CameraWindow {
    pub(crate) fn parts(&self) -> CameraParts<'_> {
        CameraParts {
            panel: &self.panel,
            shown: self.shown,
            commands: &self.commands,
            visible: &self.visible,
            opacity: &self.opacity,
            height: &self.height,
            overlay_status: &self.overlay_status,
            devices: &self.devices,
            running: &self.running,
            calibrations: &self.calibrations,
            marks: &self.marks,
            lens_status: &self.lens_status,
            marks_status: &self.marks_status,
            alignment_status: &self.alignment_status,
            refusal: &self.refusal,
            photo: &self.photo,
            fields: [
                &self.height,
                &self.margin,
                &self.lens_profile,
                &self.cols,
                &self.rows,
                &self.pitch,
                &self.align_height,
                &self.mark_x,
                &self.mark_y,
                &self.power,
                &self.speed,
            ],
        }
    }

    /// What the user typed and chose, as the headless builders take it.
    pub(crate) fn inputs(&self) -> CameraInputs {
        let text = |f: &NSTextField| f.stringValue().to_string();
        CameraInputs {
            device: self.devices.indexOfSelectedItem(),
            running: self.running.indexOfSelectedItem(),
            calibration: self.calibrations.indexOfSelectedItem(),
            mark: self.marks.indexOfSelectedItem(),
            lens_profile: text(&self.lens_profile),
            cols: text(&self.cols),
            rows: text(&self.rows),
            pitch: text(&self.pitch),
            align_height: text(&self.align_height),
            power: text(&self.power),
            speed: text(&self.speed),
            mark_x: text(&self.mark_x),
            mark_y: text(&self.mark_y),
        }
    }

    pub(crate) fn select_mark(&self, index: usize) {
        if let Ok(i) = isize::try_from(index) {
            self.marks.selectItemAtIndex(i);
        }
    }

    /// Copies the model into the widgets.
    pub(crate) fn refresh(&self, model: &CameraWindowPanel) {
        let t = |text: CameraText| camera_text(text, self.locale);
        self.visible.setState(if model.overlay_visible { NSControlStateValueOn } else { NSControlStateValueOff });
        if (self.opacity.doubleValue() - model.opacity_percent).abs() > 0.5 {
            self.opacity.setDoubleValue(model.opacity_percent);
        }
        set_field(&self.height, &model.height_text);
        set_field(&self.margin, &model.margin_text);
        set_text(&self.overlay_status, &model.overlay_status);
        set_text(&self.note, &model.note);
        set_items(&self.devices, &model.devices, t(CameraText::NoCamerasRunning));
        set_items(&self.running, &model.running, t(CameraText::NoCamerasRunning));
        set_items(&self.calibrations, &model.calibrations, t(CameraText::NoCalibrations));
        set_items(&self.marks, &model.marks, "—");
        set_text(&self.lens_status, &model.lens_status);
        set_text(&self.alignment_status, &model.alignment_status);
        set_text(&self.marks_status, &model.marks_status);
        set_error_text(&self.refusal, &model.refusal);
        for (command, button) in &self.commands {
            button.setEnabled(model.is_enabled(*command));
        }
    }

    /// Shows `rgba` (with the clicked marks drawn on it) in the photo view when it changed.
    pub(crate) fn show_photo(
        &self,
        key: Option<(u64, Vec<Option<[i64; 2]>>)>,
        image: impl FnOnce() -> Option<(u32, u32, Vec<u8>)>,
    ) {
        if *self.shown_photo.borrow() == key {
            return;
        }
        match key.as_ref().and_then(|_| image()).and_then(|(w, h, rgba)| ns_image(w, h, &rgba)) {
            Some(ns) => self.photo.setImage(Some(&ns)),
            None => self.photo.setImage(None),
        }
        // A photo that just appeared is scrolled into view: the marks are clicked in it.
        let appeared = self.shown_photo.borrow().is_none() && key.is_some();
        *self.shown_photo.borrow_mut() = key;
        if appeared {
            self.photo.scrollRectToVisible(self.photo.bounds());
        }
    }

    /// Clears every target before the target goes (a nil one cannot dangle).
    fn clear(&self) {
        for (_, b) in &self.commands {
            // SAFETY: a nil target cannot dangle.
            unsafe { b.setTarget(None) };
        }
        // SAFETY: as above.
        unsafe { self.visible.setTarget(None) };
        // SAFETY: as above.
        unsafe { self.opacity.setTarget(None) };
        for f in [&self.height, &self.margin] {
            // SAFETY: as above.
            unsafe { f.setTarget(None) };
        }
    }
}

/// An `NSImage` of the photo at its pixel size. `None` for an empty or inconsistent buffer.
fn ns_image(width: u32, height: u32, rgba: &[u8]) -> Option<Retained<NSImage>> {
    let (w, h) = (isize::try_from(width).ok()?, isize::try_from(height).ok()?);
    let len = usize::try_from(w).ok()?.checked_mul(usize::try_from(h).ok()?)?.checked_mul(4)?;
    if w == 0 || h == 0 || rgba.len() != len {
        return None;
    }
    // SAFETY: null planes make the representation allocate its own buffer of `h` rows of `w * 4`
    // bytes; the colour space name is an immutable AppKit constant.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
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
    // bytesPerRow is exactly `w * 4`); nothing else reads or writes them during this copy. Camera
    // frames are opaque, so premultiplied and straight alpha are the same bytes.
    let dst = unsafe { std::slice::from_raw_parts_mut(data, len) };
    dst.copy_from_slice(rgba);
    let size = NSSize::new(f64::from(width), f64::from(height));
    rep.setSize(size);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    image.addRepresentation(&rep);
    Some(image)
}

impl Drop for CameraWindow {
    fn drop(&mut self) {
        self.clear();
    }
}
