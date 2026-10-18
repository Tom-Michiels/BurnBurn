//! Snapping (E04): named targets near the pointer, a capture radius in screen points and a
//! temporary modifier that suppresses it. The targets themselves (endpoint, intersection,
//! midpoint, centre, nearest) come from the domain (`burnburn_geometry::snap_candidates`, Kern
//! IM09); the editor adds the grid, the screen-point radius, exclusion and suppression.

use burnburn_document::DocumentSnapshot;
use burnburn_geometry::{HitTestTolerance, Path, Point, SnapFilter};
use burnburn_types::{GeometryId, Length};

/// What the pointer snapped to. The order is the tie-break priority (domain order, grid last).
/// `Tangent` is not offered by the domain yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SnapKind {
    Endpoint,
    Intersection,
    Midpoint,
    Center,
    /// Closest point on an outline.
    Nearest,
    Grid,
}

impl From<burnburn_geometry::SnapKind> for SnapKind {
    fn from(k: burnburn_geometry::SnapKind) -> Self {
        match k {
            burnburn_geometry::SnapKind::Endpoint => SnapKind::Endpoint,
            burnburn_geometry::SnapKind::Intersection => SnapKind::Intersection,
            burnburn_geometry::SnapKind::Midpoint => SnapKind::Midpoint,
            burnburn_geometry::SnapKind::Center => SnapKind::Center,
            burnburn_geometry::SnapKind::Nearest => SnapKind::Nearest,
        }
    }
}

/// One named snap target (shown next to the cursor and as a scene overlay).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapCandidate {
    pub at: Point,
    pub kind: SnapKind,
    /// The object that offers the target; `None` for the grid.
    pub source: Option<GeometryId>,
}

/// Per-window snap preferences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapSettings {
    /// Object targets (endpoint, intersection, midpoint, centre, nearest).
    pub objects: bool,
    /// Grid spacing; `None` = grid snapping off.
    pub grid_mm: Option<f64>,
    /// Capture radius in logical screen points (independent of zoom).
    pub radius_pt: f64,
}

impl Default for SnapSettings {
    fn default() -> Self {
        SnapSettings { objects: true, grid_mm: None, radius_pt: 8.0 }
    }
}

/// Object targets within the capture radius of `at`, best first (kind priority, then distance).
/// Objects in `exclude` (for example the ones being dragged) offer nothing.
pub fn snap_candidates(
    snapshot: &DocumentSnapshot,
    settings: &SnapSettings,
    mm_per_point: f64,
    at: Point,
    exclude: &[GeometryId],
) -> Vec<SnapCandidate> {
    if !settings.objects {
        return Vec::new();
    }
    let Some(tol) = Length::from_mm(settings.radius_pt * mm_per_point).ok().and_then(|l| HitTestTolerance::new(l).ok())
    else {
        return Vec::new();
    };
    let shapes: Vec<(GeometryId, &Path)> =
        snapshot.objects().filter(|(id, _)| !exclude.contains(id)).map(|(id, obj)| (id, obj.path().as_ref())).collect();
    burnburn_geometry::snap_candidates(&shapes, at, &tol, &SnapFilter::all())
        .into_iter()
        .map(|c| SnapCandidate { at: c.point, kind: c.kind.into(), source: Some(c.source) })
        .collect()
}

/// The target a tool uses for `at`: the best object target, else the nearest grid point, else
/// nothing. `suppress` (primary modifier held) turns snapping off for this event.
pub fn snap_point(
    snapshot: &DocumentSnapshot,
    settings: &SnapSettings,
    mm_per_point: f64,
    at: Point,
    suppress: bool,
    exclude: &[GeometryId],
) -> Option<SnapCandidate> {
    if suppress {
        return None;
    }
    if let Some(best) = snap_candidates(snapshot, settings, mm_per_point, at, exclude).into_iter().next() {
        return Some(best);
    }
    let g = settings.grid_mm.filter(|g| g.is_finite() && *g > 0.0)?;
    let p = Point::new((at.x / g).round() * g, (at.y / g).round() * g);
    p.is_finite().then_some(SnapCandidate { at: p, kind: SnapKind::Grid, source: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_document::{GeometryObject, GeometrySource, ObjectKind};
    use burnburn_geometry::{EllipticalArc, PathBuilder, Rect};
    use burnburn_types::{Angle, StableId};
    use std::f64::consts::TAU;

    fn rect(raw: u64, x: f64, y: f64, w: f64, h: f64) -> GeometryObject {
        let r = Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap();
        GeometryObject::new(
            GeometryId::from_raw(raw),
            "r".into(),
            ObjectKind::Rectangle,
            GeometrySource::Drawn,
            Path::rectangle(r),
        )
    }

    fn circle(raw: u64, cx: f64, cy: f64, r: f64) -> GeometryObject {
        let arc = EllipticalArc::circle(
            Point::new(cx, cy),
            r,
            Angle::from_radians(0.0).unwrap(),
            Angle::from_radians(TAU).unwrap(),
        )
        .unwrap();
        let path = PathBuilder::new().move_to(arc.start_point()).arc(arc).close().build().unwrap();
        GeometryObject::new(GeometryId::from_raw(raw), "c".into(), ObjectKind::Path, GeometrySource::Drawn, path)
    }

    fn doc() -> DocumentSnapshot {
        DocumentSnapshot::from_parts(vec![rect(1, 0.0, 0.0, 40.0, 20.0), circle(2, 100.0, 0.0, 10.0)], 3).unwrap()
    }

    /// 0.5 mm per point: the default 8 pt radius is 4 mm.
    const MM_PER_PT: f64 = 0.5;

    fn snap(at: (f64, f64)) -> Option<SnapCandidate> {
        snap_point(&doc(), &SnapSettings::default(), MM_PER_PT, Point::new(at.0, at.1), false, &[])
    }

    #[test]
    fn snap_names_endpoint_midpoint_and_centre() {
        let id1 = Some(GeometryId::from_raw(1));
        assert_eq!(
            snap((39.5, 0.3)),
            Some(SnapCandidate { at: Point::new(40.0, 0.0), kind: SnapKind::Endpoint, source: id1 })
        );
        assert_eq!(
            snap((20.4, 0.2)),
            Some(SnapCandidate { at: Point::new(20.0, 0.0), kind: SnapKind::Midpoint, source: id1 })
        );
        assert_eq!(
            snap((0.3, 9.0)).map(|c| (c.at, c.kind)),
            Some((Point::new(0.0, 10.0), SnapKind::Midpoint)),
            "closing edge"
        );
        assert_eq!(
            snap((19.0, 11.0)).map(|c| (c.at, c.kind)),
            Some((Point::new(20.0, 10.0), SnapKind::Center)),
            "closed shape centre"
        );
        let c = snap((100.5, 0.5)).unwrap();
        assert_eq!((c.at, c.kind, c.source), (Point::new(100.0, 0.0), SnapKind::Center, Some(GeometryId::from_raw(2))));
        assert_eq!(snap((60.0, 60.0)), None, "nothing within the radius and no grid");
    }

    /// Domain order: kind priority first (a named point beats the nearer outline point), then distance.
    #[test]
    fn named_targets_beat_the_nearest_outline_point() {
        assert_eq!(snap((37.5, 19.0)).map(|c| c.kind), Some(SnapKind::Endpoint));
        let all = snap_candidates(&doc(), &SnapSettings::default(), MM_PER_PT, Point::new(21.0, 19.0), &[]);
        let kinds: Vec<_> = all.iter().map(|c| (c.at, c.kind)).collect();
        assert_eq!(kinds.first(), Some(&(Point::new(20.0, 20.0), SnapKind::Midpoint)), "{kinds:?}");
        assert!(kinds.contains(&(Point::new(21.0, 20.0), SnapKind::Nearest)), "{kinds:?}");
        assert!(all.windows(2).all(|w| w[0].kind <= w[1].kind));
    }

    #[test]
    fn primary_modifier_suppresses_snapping() {
        let s = SnapSettings { grid_mm: Some(5.0), ..SnapSettings::default() };
        assert_eq!(snap_point(&doc(), &s, MM_PER_PT, Point::new(39.5, 0.3), true, &[]), None);
    }

    #[test]
    fn snap_radius_is_in_screen_points_not_millimetres() {
        // 3 mm from the corner: inside 8 pt at 0.5 mm/pt (4 mm), outside at 0.25 mm/pt (2 mm).
        let at = Point::new(43.0, 0.0);
        let s = SnapSettings::default();
        assert!(snap_point(&doc(), &s, 0.5, at, false, &[]).is_some());
        assert!(snap_point(&doc(), &s, 0.25, at, false, &[]).is_none());
    }

    #[test]
    fn grid_applies_only_when_no_object_target_is_near() {
        let s = SnapSettings { grid_mm: Some(5.0), ..SnapSettings::default() };
        let g = snap_point(&doc(), &s, MM_PER_PT, Point::new(62.4, 57.6), false, &[]).unwrap();
        assert_eq!((g.at, g.kind, g.source), (Point::new(60.0, 60.0), SnapKind::Grid, None));
        let e = snap_point(&doc(), &s, MM_PER_PT, Point::new(39.0, 1.0), false, &[]).unwrap();
        assert_eq!(e.kind, SnapKind::Endpoint, "object target beats grid");
        let off = SnapSettings { objects: false, ..s };
        assert_eq!(
            snap_point(&doc(), &off, MM_PER_PT, Point::new(39.0, 1.0), false, &[]).map(|c| c.kind),
            Some(SnapKind::Grid)
        );
    }

    #[test]
    fn excluded_objects_offer_no_targets() {
        let s = SnapSettings::default();
        let at = Point::new(39.5, 0.3);
        assert_eq!(snap_point(&doc(), &s, MM_PER_PT, at, false, &[GeometryId::from_raw(1)]), None);
    }
}
