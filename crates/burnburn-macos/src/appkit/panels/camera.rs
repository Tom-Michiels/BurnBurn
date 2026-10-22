//! The bed-camera section of the production strip.

use crate::appkit::panels::widgets::{push_button, rect, set_text, strip_label};
use crate::appkit::window::DocWindow;

use burnburn_presentation::CameraViewModel;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSAccessibility, NSAutoresizingMaskOptions, NSButton, NSFont, NSPopUpButton, NSTextField, NSView};
use objc2_foundation::NSString;

use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{CameraControl, CameraText, camera_panel, camera_text};

use crate::appkit::panels::production::{CAMERA_WIDTH, LIFT};

/// The bed-camera section.
pub(crate) struct CameraSection {
    /// Status, frame, note.
    pub(crate) labels: [Retained<NSTextField>; 3],
    pub(crate) devices: Retained<NSPopUpButton>,
    /// Request access, start, stop (tags = [`CameraControl`] order).
    pub(crate) buttons: [Retained<NSButton>; 3],
    /// Opens the camera window (tag 3); always enabled.
    pub(crate) window_button: Retained<NSButton>,
}

/// Tag of a camera button.
pub(crate) fn camera_control(tag: isize) -> Option<CameraControl> {
    [CameraControl::RequestAccess, CameraControl::Start, CameraControl::Stop, CameraControl::Window]
        .get(usize::try_from(tag).ok()?)
        .copied()
}

/// The bed-camera part of the production strip.
pub(crate) fn build(mtm: MainThreadMarker, strip: &NSView, width: f64, locale: Locale) -> CameraSection {
    let t = |text: HostText| host_text(text, locale);
    let x = width - CAMERA_WIDTH;
    let cw = CAMERA_WIDTH - 12.0;
    let camera_title =
        strip_label(mtm, strip, t(HostText::CameraTitle), rect(x, 186.0 + LIFT, cw - 132.0, 18.0), false);
    camera_title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let window_button = push_button(mtm, camera_text(CameraText::OpenButton, locale));
    window_button.setFrame(rect(x + cw - 128.0, 182.0 + LIFT, 128.0, 24.0));
    window_button.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    window_button.setTag(3);
    window_button.setAccessibilityLabel(Some(&NSString::from_str(camera_text(CameraText::Accessibility, locale))));
    strip.addSubview(&window_button);
    let status = strip_label(mtm, strip, "", rect(x, 150.0 + LIFT, cw, 32.0), true);
    let devices =
        NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(x, 120.0 + LIFT, cw, 26.0), false);
    devices.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::CameraTitle))));
    devices.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&devices);
    // Two lines: the three titles do not fit side by side in the column.
    let slots = [(0.0, 86.0 + LIFT, 130.0), (134.0, 86.0 + LIFT, 104.0), (0.0, 54.0 + LIFT, 104.0)];
    let camera_button = |i: usize, text: HostText| {
        let b = push_button(mtm, t(text));
        let (dx, y, w) = slots[i.min(2)];
        b.setFrame(rect(x + dx, y, w, 28.0));
        b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        b.setTag(i as isize);
        b.setEnabled(false);
        strip.addSubview(&b);
        b
    };
    let buttons = [
        camera_button(0, HostText::CameraAccessButton),
        camera_button(1, HostText::CameraStartButton),
        camera_button(2, HostText::CameraStopButton),
    ];
    let frame = strip_label(mtm, strip, "", rect(x, 62.0, cw, 18.0), false);
    let note = strip_label(mtm, strip, "", rect(x, 8.0, cw, 50.0), true);
    note.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    // The camera column keeps its width at the right edge when the window grows (the layer table,
    // which takes the width, must not run into it).
    let right_top = NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewMinXMargin.0 | NSAutoresizingMaskOptions::ViewMinYMargin.0,
    );
    for v in [&*camera_title as &NSView, &window_button, &status, &devices, &frame, &note]
        .into_iter()
        .chain(buttons.iter().map(|b| -> &NSView { b }))
    {
        v.setAutoresizingMask(right_top);
    }
    CameraSection { labels: [status, frame, note], devices, buttons, window_button }
}

impl DocWindow {
    /// The bed-camera section (also refreshed by the host's camera tick while a camera runs).
    pub(crate) fn refresh_camera(&self, camera: Option<&CameraViewModel>) {
        let panel = camera_panel(camera, self.locale);
        for (label, text) in self.camera.labels.iter().zip([&panel.status, &panel.frame, &panel.note]) {
            set_text(label, text);
        }
        let popup = &self.camera.devices;
        let titles: Vec<String> = popup.itemTitles().iter().map(|t| t.to_string()).collect();
        if titles != panel.devices {
            popup.removeAllItems();
            for name in &panel.devices {
                popup.addItemWithTitle(&NSString::from_str(name));
            }
        }
        if let Some(i) = panel.selected
            && popup.indexOfSelectedItem() != i as isize
        {
            popup.selectItemAtIndex(i as isize);
        }
        popup.setEnabled(panel.start_enabled);
        for (b, on) in self.camera.buttons.iter().zip([panel.access_enabled, panel.start_enabled, panel.stop_enabled]) {
            b.setEnabled(on);
        }
    }
}
