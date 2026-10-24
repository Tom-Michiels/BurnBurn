//! Rulers (LF9): bands above and left of the canvas with marks that scale with the zoom and
//! numbers in the unit of the preference. The layout is pure (no pixels), so that it can be tested
//! per zoom level; drawing is done in `scene.rs` from the marks given here.

use burnburn_editor::{Camera, Viewport};
use burnburn_geometry::Point;

/// Thickness of a ruler band, in logical points.
pub const RULER_THICKNESS_PT: f64 = 20.0;
/// Least distance between two numbered (major) marks, in logical points.
pub const RULER_MIN_MAJOR_GAP_PT: f64 = 64.0;

const MM_PER_INCH: f64 = 25.4;

/// The unit the ruler numbers are in (the display-unit preference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulerUnit {
    Millimetre,
    Inch,
}

impl RulerUnit {
    pub fn mm_per_unit(self) -> f64 {
        match self {
            RulerUnit::Millimetre => 1.0,
            RulerUnit::Inch => MM_PER_INCH,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            RulerUnit::Millimetre => "mm",
            RulerUnit::Inch => "in",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulerAxis {
    /// The ruler above the canvas (world x).
    Horizontal,
    /// The ruler left of the canvas (world y, up).
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// Long mark with a number.
    Major,
    Minor,
}

/// Distance between numbered marks and how many parts a step is divided in, in ruler units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulerStep {
    pub major: f64,
    pub subdivisions: u32,
}

/// One mark: where it is along the ruler (logical points from the view's top or left edge), its
/// value in ruler units and its kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulerMark {
    pub at_pt: f64,
    pub value: f64,
    pub kind: MarkKind,
}

/// What the host asks for: the unit, the cursor (world mm) to mark, and the decimal separator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rulers {
    pub unit: RulerUnit,
    pub cursor_mm: Option<Point>,
    pub decimal: char,
}

/// The finest 1, 2 or 5 x 10^n (in the ruler's unit) whose numbered marks are at least
/// [`RULER_MIN_MAJOR_GAP_PT`] apart at `points_per_unit`, with its subdivision into minor marks
/// (1 -> tenths, 2 -> halves of a unit, 5 -> units; every minor step is again 1, 2 or 5 x 10^n).
pub fn ruler_step(points_per_unit: f64) -> RulerStep {
    let fallback = RulerStep { major: 1.0, subdivisions: 10 };
    if !(points_per_unit.is_finite() && points_per_unit > 0.0) {
        return fallback;
    }
    let min_units = RULER_MIN_MAJOR_GAP_PT / points_per_unit;
    let mut decade = 10f64.powf(min_units.log10().floor() - 1.0);
    for _ in 0..6 {
        for (mantissa, subdivisions) in [(1.0, 10), (2.0, 4), (5.0, 5)] {
            let major = mantissa * decade;
            if major * points_per_unit >= RULER_MIN_MAJOR_GAP_PT * (1.0 - 1e-12) {
                return RulerStep { major, subdivisions };
            }
        }
        decade *= 10.0;
    }
    fallback
}

/// Most marks one ruler can have; a finer layout is not drawn (it would be a wash).
const MAX_MARKS: usize = 4000;

/// The marks of one ruler for this camera, within the view, ordered along the ruler.
pub fn ruler_marks(camera: &Camera, viewport: &Viewport, axis: RulerAxis, unit: RulerUnit) -> Vec<RulerMark> {
    let mm = unit.mm_per_unit();
    let points_per_unit = camera.points_per_mm() * mm;
    let step = ruler_step(points_per_unit);
    let minor = step.major / f64::from(step.subdivisions);
    // World span (in ruler units) covered by the view along this axis.
    let (from, to) = match axis {
        RulerAxis::Horizontal => {
            let width = viewport.width_pt();
            (camera.points_to_world(viewport, [0.0, 0.0]).x, camera.points_to_world(viewport, [width, 0.0]).x)
        }
        RulerAxis::Vertical => {
            let height = viewport.height_pt();
            (camera.points_to_world(viewport, [0.0, 0.0]).y, camera.points_to_world(viewport, [0.0, height]).y)
        }
    };
    let (lo, hi) = ((from / mm).min(to / mm), (from / mm).max(to / mm));
    let (first, last) = ((lo / minor).ceil(), (hi / minor).floor());
    let count = last - first + 1.0;
    if !(count.is_finite() && (0.0..=MAX_MARKS as f64).contains(&count)) {
        return Vec::new();
    }
    let mut marks: Vec<RulerMark> = (first as i64..=last as i64)
        .map(|k| {
            let value = k as f64 * minor;
            let (world, along) = match axis {
                RulerAxis::Horizontal => (Point::new(value * mm, 0.0), 0),
                RulerAxis::Vertical => (Point::new(0.0, value * mm), 1),
            };
            let at_pt = camera.world_to_points(viewport, world)[along];
            let kind = if k.rem_euclid(i64::from(step.subdivisions)) == 0 { MarkKind::Major } else { MarkKind::Minor };
            RulerMark { at_pt, value, kind }
        })
        .collect();
    // The vertical ruler runs against the world axis (screen y is down): order along the ruler.
    marks.sort_by(|a, b| a.at_pt.total_cmp(&b.at_pt));
    marks
}

/// The number written at a major mark: whole numbers where the step is, else as many decimals as
/// the step needs; `decimal` is the separator of the host's locale.
pub fn mark_text(value: f64, step: &RulerStep, decimal: char) -> String {
    let decimals = if step.major >= 1.0 { 0 } else { (-step.major.log10()).ceil().clamp(0.0, 6.0) as usize };
    let rounded = format!("{value:.decimals$}");
    // No "-0".
    let text = if rounded.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
        rounded.trim_start_matches('-').to_owned()
    } else {
        rounded
    };
    text.replace('.', &decimal.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_have_as_many_decimals_as_the_step_needs_and_the_locale_separator() {
        let whole = RulerStep { major: 20.0, subdivisions: 4 };
        assert_eq!(mark_text(-40.0, &whole, ','), "-40");
        let fine = RulerStep { major: 0.5, subdivisions: 5 };
        assert_eq!(mark_text(2.5, &fine, ','), "2,5");
        assert_eq!(mark_text(2.5, &fine, '.'), "2.5");
        let finer = RulerStep { major: 0.02, subdivisions: 4 };
        assert_eq!(mark_text(0.14, &finer, '.'), "0.14");
        // Float noise around zero never shows as "-0".
        assert_eq!(mark_text(-1e-12, &fine, '.'), "0.0");
    }
}
