//! The status bar at the foot of the document window (LF7): pointer coordinate on the left, zoom
//! and unit on the right. The texts come from `burnburn_host_models::panels::status_bar`.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBox, NSBoxType, NSColor, NSFont, NSTextAlignment, NSTextField, NSView,
};
use objc2_foundation::NSString;

use super::panels::widgets::{rect, set_text};
use crate::l10n::Locale;
use crate::panels::{StatusBar, status_bar_labels};

pub(crate) const STATUS_HEIGHT: f64 = 24.0;
const COORDINATE_WIDTH: f64 = 260.0;
const ZOOM_WIDTH: f64 = 90.0;
const UNIT_WIDTH: f64 = 28.0;
const MARGIN: f64 = 12.0;

pub(crate) struct StatusBarWidgets {
    pub(crate) view: Retained<NSView>,
    pub(crate) coordinate: Retained<NSTextField>,
    pub(crate) zoom: Retained<NSTextField>,
    pub(crate) unit: Retained<NSTextField>,
}

impl StatusBarWidgets {
    /// Copy the texts into the labels (only changed values are written).
    pub(crate) fn show(&self, bar: &StatusBar) {
        set_text(&self.coordinate, &bar.coordinate);
        set_text(&self.zoom, &bar.zoom);
        set_text(&self.unit, &bar.unit);
    }
}

/// The bar, `width` wide, with its three labels (empty until the first refresh).
pub(crate) fn status_bar_widgets(mtm: MainThreadMarker, width: f64, locale: Locale) -> StatusBarWidgets {
    let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, STATUS_HEIGHT));
    view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    let (coordinate_label, zoom_label, unit_label) = status_bar_labels(locale);
    let rule = NSBox::initWithFrame(NSBox::alloc(mtm), rect(0.0, STATUS_HEIGHT - 1.0, width, 1.0));
    rule.setBoxType(NSBoxType::Separator);
    rule.setAutoresizingMask(NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewMinYMargin.0,
    ));
    view.addSubview(&rule);
    let label = |frame, accessibility: &str, alignment, mask| {
        let l = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        l.setFrame(frame);
        l.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(11.0, 0.0)));
        l.setTextColor(Some(&NSColor::secondaryLabelColor()));
        l.setAlignment(alignment);
        l.setAutoresizingMask(mask);
        l.setAccessibilityLabel(Some(&NSString::from_str(accessibility)));
        view.addSubview(&l);
        l
    };
    let line_y = 3.0;
    let line_h = STATUS_HEIGHT - 8.0;
    let coordinate = label(
        rect(MARGIN, line_y, COORDINATE_WIDTH, line_h),
        coordinate_label,
        NSTextAlignment::Left,
        NSAutoresizingMaskOptions::ViewMaxXMargin,
    );
    let unit = label(
        rect(width - MARGIN - UNIT_WIDTH, line_y, UNIT_WIDTH, line_h),
        unit_label,
        NSTextAlignment::Left,
        NSAutoresizingMaskOptions::ViewMinXMargin,
    );
    let zoom = label(
        rect(width - MARGIN - UNIT_WIDTH - ZOOM_WIDTH, line_y, ZOOM_WIDTH, line_h),
        zoom_label,
        NSTextAlignment::Right,
        NSAutoresizingMaskOptions::ViewMinXMargin,
    );
    StatusBarWidgets { view, coordinate, zoom, unit }
}
