//! LF-C follow-up: the canvas chrome follows the window's settings — the rulers use the length
//! unit of the preferences and the locale's decimal sign and mark the pointer, and the snap labels
//! are in the window language (until now: always millimetres, a comma and Dutch labels).

#![cfg(target_os = "macos")]

use burnburn_geometry::Point;
use burnburn_host_models::format::LengthPreference;
use burnburn_macos::appkit::canvas_chrome_for;
use burnburn_macos::l10n::Locale;
use burnburn_render::{RulerUnit, SnapLabels};

#[test]
fn rulers_follow_the_unit_preference_and_the_locale() {
    let nl = canvas_chrome_for(LengthPreference::Millimeters, Locale::Nl, None);
    let rulers = nl.rulers.expect("rulers");
    assert_eq!((rulers.unit, rulers.decimal), (RulerUnit::Millimetre, ','));
    let en = canvas_chrome_for(LengthPreference::Inches, Locale::En, None);
    let rulers = en.rulers.expect("rulers");
    assert_eq!((rulers.unit, rulers.decimal), (RulerUnit::Inch, '.'));
}

#[test]
fn the_pointer_is_marked_on_the_rulers_when_it_is_over_the_canvas() {
    let at = Point::new(12.5, 40.0);
    let chrome = canvas_chrome_for(LengthPreference::Millimeters, Locale::Nl, Some(at));
    assert_eq!(chrome.rulers.and_then(|r| r.cursor_mm), Some(at));
    let away = canvas_chrome_for(LengthPreference::Millimeters, Locale::Nl, None);
    assert_eq!(away.rulers.and_then(|r| r.cursor_mm), None);
}

#[test]
fn snap_labels_are_in_the_window_language() {
    assert_eq!(canvas_chrome_for(LengthPreference::Millimeters, Locale::Nl, None).snap_labels, Some(SnapLabels::DUTCH));
    assert_eq!(
        canvas_chrome_for(LengthPreference::Millimeters, Locale::En, None).snap_labels,
        Some(SnapLabels::ENGLISH)
    );
}
