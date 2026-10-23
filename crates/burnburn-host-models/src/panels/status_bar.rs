//! The status bar under the document window: pointer coordinate, zoom and unit (LF7).

use burnburn_editor::{Camera, Viewport};

use crate::format::{LengthPreference, format_number};
use crate::l10n::Locale;

/// The three texts of the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBar {
    /// `X 12,5  Y 30`; empty while the pointer has not been over the canvas.
    pub coordinate: String,
    /// `100 %` is true size (72 points per inch).
    pub zoom: String,
    /// The display unit of the coordinate (`mm` or `in`).
    pub unit: String,
}

const MM_PER_INCH: f64 = 25.4;
/// Points per millimetre at true size.
const TRUE_SIZE_POINTS_PER_MM: f64 = 72.0 / MM_PER_INCH;

/// The status bar for a pointer position (points in the canvas view, as the view reports them)
/// under `camera`. The coordinate is the world point under the pointer, in the display unit.
pub fn status_bar(
    pointer_pt: Option<[f64; 2]>,
    camera: &Camera,
    viewport: &Viewport,
    unit: LengthPreference,
    locale: Locale,
) -> StatusBar {
    let (factor, unit_name) = match unit {
        LengthPreference::Millimeters => (1.0, "mm"),
        LengthPreference::Inches => (1.0 / MM_PER_INCH, "in"),
    };
    let coordinate = pointer_pt
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .map(|p| {
            let world = camera.points_to_world(viewport, p);
            let number = |mm: f64| format_number(mm * factor, 3, locale);
            format!("X {}  Y {}", number(world.x), number(world.y))
        })
        .unwrap_or_default();
    let zoom = format!("{} %", format_number(camera.points_per_mm() / TRUE_SIZE_POINTS_PER_MM * 100.0, 1, locale));
    StatusBar { coordinate, zoom, unit: unit_name.to_owned() }
}
