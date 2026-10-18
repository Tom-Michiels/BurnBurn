//! The production strip under the canvas: operations panel and bed camera side by side.

use crate::appkit::panels::widgets::rect;
use crate::appkit::panels::{
    camera::{self, CameraSection},
    operations::{self, OperationsSection},
};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::NSView;

use crate::l10n::Locale;

/// Height of the production strip under the canvas (palette, layers, bed camera).
pub(crate) const STRIP_HEIGHT: f64 = 250.0;
pub(crate) const CAMERA_WIDTH: f64 = 250.0;

/// The production strip's widgets (targets/actions are set once the controller exists).
pub(crate) fn production_strip(
    mtm: MainThreadMarker,
    width: f64,
    locale: Locale,
) -> (Retained<NSView>, OperationsSection, CameraSection) {
    let strip = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, STRIP_HEIGHT));
    let ops_width = width - CAMERA_WIDTH - 12.0;
    let operations = operations::build(mtm, &strip, ops_width, locale);
    let camera = camera::build(mtm, &strip, width, locale);
    (strip, operations, camera)
}
