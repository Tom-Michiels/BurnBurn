//! The offset tool: pick a contour, then click on the side and at the distance wanted. The new
//! outline is a new object; the source stays (so derived sketch output can be offset too).

use std::sync::Arc;

use burnburn_geometry::{HitKind, HitTestTolerance, OffsetError, OffsetJoin, Path, Point, ProductionTolerance, offset};
use burnburn_types::{CancellationToken, Length};

use super::modify::{Action, Rejection, outline_hit, path_preview, rejected};
use super::{EditCommit, ToolContext, ToolPreview, ToolStep};
use crate::input::PointerEvent;

/// Chord tolerance of the offset outline (mm), the same as the boolean tools use.
const OFFSET_TOLERANCE_MM: f64 = 0.01;
/// Offsets smaller than this are no offset (float noise, not a user tolerance).
const MIN_DISTANCE_MM: f64 = 1e-9;

#[derive(Debug, Clone)]
pub(super) struct OffsetAction {
    source: Option<Arc<Path>>,
    /// +1 while the pointer is outside the contour, -1 inside.
    side: f64,
    preview: Option<ToolPreview>,
}

impl Default for OffsetAction {
    fn default() -> Self {
        OffsetAction { source: None, side: 1.0, preview: None }
    }
}

/// Signed distance from the outline to `q`: positive outside, negative inside a closed contour.
fn signed_distance(path: &Path, q: Point) -> Option<f64> {
    let near = path.nearest(q)?;
    let tol = HitTestTolerance::new(Length::from_mm(1e-9).ok()?).ok()?;
    let inside = matches!(path.hit(q, &tol), Some(HitKind::Interior));
    Some(if inside { -near.distance } else { near.distance })
}

fn offset_path(path: &Path, distance: f64) -> Result<Path, OffsetError> {
    let tolerance = Length::from_mm(OFFSET_TOLERANCE_MM)
        .ok()
        .and_then(|l| ProductionTolerance::new(l).ok())
        .ok_or(OffsetError::NotFinite)?;
    let out = offset(path, distance, OffsetJoin::Miter, &tolerance, &CancellationToken::never())?;
    if out.is_empty() { Err(OffsetError::Collapsed) } else { Ok(out) }
}

impl OffsetAction {
    fn finish(&mut self, distance: f64) -> ToolStep {
        let source = self.source.take();
        self.reset();
        let result = match source {
            Some(path) if distance.abs() > MIN_DISTANCE_MM => offset_path(&path, distance).map_err(Rejection::Offset),
            _ => Err(Rejection::InvalidValue),
        };
        match result {
            Ok(path) => ToolStep::Commit(EditCommit::CreatePath { path }),
            Err(r) => rejected(r),
        }
    }
}

impl Action for OffsetAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, ev: &PointerEvent) -> ToolStep {
        let Some(path) = self.source.clone() else {
            return match outline_hit(ctx, ev.position).and_then(|id| ctx.document.object(id)) {
                Some(obj) => {
                    self.source = Some(obj.path().clone());
                    ToolStep::PreviewChanged
                }
                None => rejected(Rejection::NothingPicked),
            };
        };
        let distance = signed_distance(&path, at).unwrap_or(0.0);
        self.finish(distance)
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        let Some(path) = &self.source else { return };
        let distance = signed_distance(path, at).filter(|d| d.abs() > MIN_DISTANCE_MM);
        if let Some(d) = distance {
            self.side = d.signum();
        }
        self.preview = distance.and_then(|d| offset_path(path, d).ok()).map(path_preview);
    }

    fn reset(&mut self) -> bool {
        self.preview = None;
        self.side = 1.0;
        self.source.take().is_some()
    }

    fn preview(&self) -> Option<ToolPreview> {
        self.preview.clone()
    }

    /// The typed distance replaces the pointer's distance; the pointer still chooses the side.
    fn exact_size(&mut self, width: Length, _height: Length) -> ToolStep {
        if self.source.is_none() {
            return ToolStep::Nothing;
        }
        let distance = self.side * width.mm().abs();
        self.finish(distance)
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::OffsetError;
    use burnburn_types::Length;

    use super::super::fixture::*;
    use super::super::modify::Rejection;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind, ToolPreview, ToolStep};
    use crate::input::CanvasKey;

    fn square() -> Fixture {
        with_objects(vec![rect(1, 0.0, 0.0, 10.0, 10.0)])
    }

    fn bounds(steps: &[ToolStep]) -> (f64, f64, f64, f64) {
        let EditCommit::CreatePath { path } = only(steps) else { panic!("create") };
        let b = path.bounds().unwrap();
        (b.min().x, b.min().y, b.max().x, b.max().y)
    }

    fn close(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
        [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)].iter().all(|(x, y)| (x - y).abs() < 0.02)
    }

    #[test]
    fn outside_grows_the_contour_by_the_distance_to_the_outline() {
        let (f, mut ed) = (square(), editor(ToolKind::Offset));
        let steps = clicks(&mut ed, &f, &[(10.0, 5.0), (12.0, 5.0)]);
        let b = bounds(&steps);
        assert!(close(b, (-2.0, -2.0, 12.0, 12.0)), "{b:?}");
    }

    #[test]
    fn inside_shrinks_it() {
        let (f, mut ed) = (square(), editor(ToolKind::Offset));
        let steps = clicks(&mut ed, &f, &[(10.0, 5.0), (8.0, 5.0)]);
        let b = bounds(&steps);
        assert!(close(b, (2.0, 2.0, 8.0, 8.0)), "{b:?}");
    }

    #[test]
    fn moving_previews_the_offset_and_a_typed_distance_wins_over_the_pointer() {
        let (f, mut ed) = (square(), editor(ToolKind::Offset));
        click(&mut ed, &f, 10.0, 5.0);
        hover_to(&mut ed, &f, 13.0, 5.0);
        assert!(matches!(ed.preview(), Some(ToolPreview::Shape { .. })));
        let three = Length::from_mm(3.0).unwrap();
        let step = ed.exact_size(three, three);
        let b = bounds(&[step]);
        assert!(close(b, (-3.0, -3.0, 13.0, 13.0)), "side from the pointer, size typed: {b:?}");
    }

    #[test]
    fn escape_after_the_pick_changes_nothing() {
        let (f, mut ed) = (square(), editor(ToolKind::Offset));
        click(&mut ed, &f, 10.0, 5.0);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Cancelled);
        assert!(ed.preview().is_none());
        let steps = click(&mut ed, &f, 12.0, 5.0);
        assert!(commits(&steps).is_empty(), "the second click is a new pick, not a commit");
    }

    #[test]
    fn open_contours_and_empty_picks_are_refused() {
        let f = with_objects(vec![polyline(1, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false)]);
        let mut ed = editor(ToolKind::Offset);
        assert_eq!(rejection(&click(&mut ed, &f, 50.0, 50.0)), Some(Rejection::NothingPicked));
        let steps = clicks(&mut ed, &f, &[(5.0, 0.0), (5.0, -3.0)]);
        assert!(commits(&steps).is_empty());
        assert!(matches!(rejection(&steps), Some(Rejection::Offset(OffsetError::OpenContour { .. }))), "{steps:?}");
    }
}
