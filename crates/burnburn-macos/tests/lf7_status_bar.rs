//! LF7: the status bar under the document window shows the pointer's coordinate in the display
//! unit, the zoom and the unit. Headless model; the native label wiring is tested in
//! `tests/native/macos/lf7_document_window.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_editor::{Camera, Viewport};
use burnburn_geometry::Point;
use burnburn_host_models::format::LengthPreference;
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::status_bar;

/// 72 points per inch: one point per 25.4/72 mm is true size (100 %).
const TRUE_SIZE: f64 = 72.0 / 25.4;

fn camera(centre: (f64, f64), points_per_mm: f64) -> (Camera, Viewport) {
    (Camera::new(Point::new(centre.0, centre.1), points_per_mm).unwrap(), Viewport::new(800.0, 600.0, 2.0).unwrap())
}

#[test]
fn the_pointer_at_the_middle_of_the_canvas_is_the_camera_centre() {
    let (cam, vp) = camera((120.0, 80.0), TRUE_SIZE);
    let bar = status_bar(Some([400.0, 300.0]), &cam, &vp, LengthPreference::Millimeters, Locale::Nl);
    assert_eq!(bar.coordinate, "X 120  Y 80");
    assert_eq!(bar.unit, "mm");
    assert_eq!(bar.zoom, "100 %");
}

#[test]
fn a_pointer_right_of_and_above_the_centre_has_larger_x_and_y() {
    // 10 points per mm: 50 points right is +5 mm, 20 points up (screen y is down) is +2 mm.
    let (cam, vp) = camera((0.0, 0.0), 10.0);
    let bar = status_bar(Some([450.0, 280.0]), &cam, &vp, LengthPreference::Millimeters, Locale::En);
    assert_eq!(bar.coordinate, "X 5  Y 2");
    assert_eq!(bar.zoom, "352.8 %");
}

#[test]
fn inches_and_the_dutch_decimal_comma_follow_the_preference_and_locale() {
    let (cam, vp) = camera((25.4, 50.8), TRUE_SIZE);
    let bar = status_bar(Some([400.0, 300.0]), &cam, &vp, LengthPreference::Inches, Locale::Nl);
    assert_eq!(bar.coordinate, "X 1  Y 2");
    assert_eq!(bar.unit, "in");
    let (cam, vp) = camera((12.5, 0.0), TRUE_SIZE);
    let bar = status_bar(Some([400.0, 300.0]), &cam, &vp, LengthPreference::Millimeters, Locale::Nl);
    assert_eq!(bar.coordinate, "X 12,5  Y 0");
}

#[test]
fn without_a_pointer_the_coordinate_is_empty_but_zoom_and_unit_stay() {
    let (cam, vp) = camera((0.0, 0.0), TRUE_SIZE);
    let bar = status_bar(None, &cam, &vp, LengthPreference::Millimeters, Locale::Nl);
    assert_eq!(bar.coordinate, "");
    assert_eq!(bar.zoom, "100 %");
    assert_eq!(bar.unit, "mm");
}
