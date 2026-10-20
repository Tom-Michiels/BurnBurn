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

use crate::appkit::panels::production::CAMERA_WIDTH;

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
    let camera_title = strip_label(mtm, strip, t(HostText::CameraTitle), rect(x, 186.0, cw - 120.0, 18.0), false);
    camera_title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let window_button = push_button(mtm, camera_text(CameraText::OpenButton, locale));
    window_button.setFrame(rect(x + cw - 116.0, 182.0, 116.0, 24.0));
    window_button.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    window_button.setTag(3);
    window_button.setAccessibilityLabel(Some(&NSString::from_str(camera_text(CameraText::Accessibility, locale))));
    strip.addSubview(&window_button);
    let status = strip_label(mtm, strip, "", rect(x, 150.0, cw, 32.0), true);
    let devices = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(x, 120.0, cw, 26.0), false);
    devices.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::CameraTitle))));
    devices.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&devices);
    let bw = (cw - 8.0) / 3.0;
    let camera_button = |i: usize, text: HostText| {
        let b = push_button(mtm, t(text));
        b.setFrame(rect(x + i as f64 * (bw + 4.0), 86.0, bw, 28.0));
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
