//! The toolbar's Uitlijnen control: six align and two distribute segments (momentary). It sits in
//! the window's native toolbar next to Schets and only forwards the clicked segment's command.

use burnburn_presentation::{AppViewModel, MessageKey};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSAccessibility, NSImage, NSSegmentSwitchTracking, NSSegmentedControl};
use objc2_foundation::NSString;

use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text, message_or_key};
use crate::panels::{ARRANGE_TOOLBAR, arrange_toolbar_enabled};

/// SF Symbols for the segments (same order as `ARRANGE_TOOLBAR`). A segment whose symbol the
/// system does not know shows the command's name instead, so it is never blank.
const SYMBOLS: [&str; 8] = [
    "align.horizontal.left",
    "align.horizontal.center",
    "align.horizontal.right",
    "align.vertical.top",
    "align.vertical.center",
    "align.vertical.bottom",
    "distribute.horizontal.center",
    "distribute.vertical.center",
];

pub(crate) fn arrange_toolbar(mtm: MainThreadMarker, locale: Locale) -> Retained<NSSegmentedControl> {
    let control = NSSegmentedControl::new(mtm);
    control.setSegmentCount(ARRANGE_TOOLBAR.len() as isize);
    control.setTrackingMode(NSSegmentSwitchTracking::Momentary);
    for (i, command) in ARRANGE_TOOLBAR.iter().enumerate() {
        let name = NSString::from_str(&message_or_key(MessageKey::Command(*command), locale));
        let segment = i as isize;
        match NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(SYMBOLS[i]), Some(&name))
        {
            Some(image) => control.setImage_forSegment(Some(&image), segment),
            None => control.setLabel_forSegment(&name, segment),
        }
        control.setToolTip_forSegment(Some(&name), segment);
    }
    control.sizeToFit();
    control.setAccessibilityLabel(Some(&NSString::from_str(host_text(HostText::MenuAlign, locale))));
    control
}

impl DocWindow {
    /// Enablement per segment from the same command table as the Schikken menu.
    pub(crate) fn refresh_arrange_toolbar(&self, vm: &AppViewModel) {
        for (i, on) in arrange_toolbar_enabled(vm).into_iter().enumerate() {
            self.arrange_tools.setEnabled_forSegment(on, i as isize);
        }
    }
}
