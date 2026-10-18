use std::fmt;

use burnburn_geometry::Point;

/// Why a viewport or camera value was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewError {
    NotFinite,
    NotPositive,
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViewError::NotFinite => f.write_str("view value must be finite"),
            ViewError::NotPositive => f.write_str("view size and scale must be positive"),
        }
    }
}

impl std::error::Error for ViewError {}

fn positive(v: f64) -> Result<f64, ViewError> {
    if !v.is_finite() {
        Err(ViewError::NotFinite)
    } else if v > 0.0 {
        Ok(v)
    } else {
        Err(ViewError::NotPositive)
    }
}

/// The canvas view's size in logical points and its backing scale (2.0 on Retina).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    width_pt: f64,
    height_pt: f64,
    backing_scale: f64,
}

impl Viewport {
    pub fn new(width_pt: f64, height_pt: f64, backing_scale: f64) -> Result<Self, ViewError> {
        Ok(Viewport {
            width_pt: positive(width_pt)?,
            height_pt: positive(height_pt)?,
            backing_scale: positive(backing_scale)?,
        })
    }

    pub fn width_pt(&self) -> f64 {
        self.width_pt
    }

    pub fn height_pt(&self) -> f64 {
        self.height_pt
    }

    pub fn backing_scale(&self) -> f64 {
        self.backing_scale
    }

    /// The drawable size in whole device pixels (rounded, at least one).
    pub fn size_px(&self) -> [u32; 2] {
        let px = |pt: f64| (pt * self.backing_scale).round().clamp(1.0, f64::from(u32::MAX)) as u32;
        [px(self.width_pt), px(self.height_pt)]
    }

    /// Centre in device pixels of the drawable (whole-pixel size), shared by projection and
    /// pointer mapping so both agree exactly.
    fn center_px(&self) -> [f64; 2] {
        let [w, h] = self.size_px();
        [f64::from(w) / 2.0, f64::from(h) / 2.0]
    }
}

/// Which world point is at the viewport centre and how many logical points one millimetre spans.
/// World is y-up millimetres; screen points are y-down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    center_mm: Point,
    points_per_mm: f64,
}

impl Camera {
    pub fn new(center_mm: Point, points_per_mm: f64) -> Result<Self, ViewError> {
        if !center_mm.is_finite() {
            return Err(ViewError::NotFinite);
        }
        Ok(Camera { center_mm, points_per_mm: positive(points_per_mm)? })
    }

    pub fn center_mm(&self) -> Point {
        self.center_mm
    }

    pub fn points_per_mm(&self) -> f64 {
        self.points_per_mm
    }

    pub fn world_to_points(&self, vp: &Viewport, p: Point) -> [f64; 2] {
        let [x, y] = self.world_to_px(vp, p);
        [x / vp.backing_scale, y / vp.backing_scale]
    }

    pub fn points_to_world(&self, vp: &Viewport, pt: [f64; 2]) -> Point {
        let s = self.points_per_mm * vp.backing_scale;
        let [cx, cy] = vp.center_px();
        Point::new(
            self.center_mm.x + (pt[0] * vp.backing_scale - cx) / s,
            self.center_mm.y - (pt[1] * vp.backing_scale - cy) / s,
        )
    }

    /// Millimetres spanned by one logical point: converts screen tolerances to world tolerances.
    pub fn mm_per_point(&self) -> f64 {
        1.0 / self.points_per_mm
    }

    /// A world point in viewport-relative device pixels (`f64`; the renderer narrows later).
    pub fn world_to_px(&self, vp: &Viewport, p: Point) -> [f64; 2] {
        let s = self.points_per_mm * vp.backing_scale;
        let [cx, cy] = vp.center_px();
        [cx + (p.x - self.center_mm.x) * s, cy - (p.y - self.center_mm.y) * s]
    }

    /// Drags the content by `delta_pt` logical points (screen axes): content follows the pointer.
    /// A non-finite delta is ignored.
    pub fn pan_by_points(&mut self, delta_pt: [f64; 2]) {
        let c = Point::new(
            self.center_mm.x - delta_pt[0] / self.points_per_mm,
            self.center_mm.y + delta_pt[1] / self.points_per_mm,
        );
        if c.is_finite() {
            self.center_mm = c;
        }
    }

    /// Zooms by `factor` keeping the world point under `anchor_pt` in place. A rejected factor
    /// leaves the camera unchanged.
    pub fn zoom_at(&mut self, vp: &Viewport, anchor_pt: [f64; 2], factor: f64) -> Result<(), ViewError> {
        let points_per_mm = positive(positive(factor)? * self.points_per_mm)?;
        let anchor = self.points_to_world(vp, anchor_pt);
        let s = points_per_mm * vp.backing_scale;
        let [cx, cy] = vp.center_px();
        let center = Point::new(
            anchor.x - (anchor_pt[0] * vp.backing_scale - cx) / s,
            anchor.y + (anchor_pt[1] * vp.backing_scale - cy) / s,
        );
        if !center.is_finite() {
            return Err(ViewError::NotFinite);
        }
        self.points_per_mm = points_per_mm;
        self.center_mm = center;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Camera, Viewport) {
        (Camera::new(Point::new(0.0, 0.0), 2.0).unwrap(), Viewport::new(200.0, 100.0, 2.0).unwrap())
    }

    #[test]
    fn world_maps_to_y_down_points_around_viewport_centre() {
        let (cam, vp) = setup();
        // Centre of the view is world origin; +x right, +y up (so smaller y_pt).
        assert_eq!(cam.world_to_points(&vp, Point::new(0.0, 0.0)), [100.0, 50.0]);
        assert_eq!(cam.world_to_points(&vp, Point::new(10.0, 5.0)), [120.0, 40.0]);
    }

    #[test]
    fn points_to_world_inverts_world_to_points() {
        let cam = Camera::new(Point::new(37.5, -12.25), 3.7).unwrap();
        let vp = Viewport::new(640.0, 480.0, 1.0).unwrap();
        for p in [Point::new(0.0, 0.0), Point::new(150.0, 80.0), Point::new(-1e4, 2e4)] {
            let back = cam.points_to_world(&vp, cam.world_to_points(&vp, p));
            assert!((back.x - p.x).abs() <= 1e-9 * (1.0 + p.x.abs()), "{p:?} -> {back:?}");
            assert!((back.y - p.y).abs() <= 1e-9 * (1.0 + p.y.abs()), "{p:?} -> {back:?}");
        }
    }

    /// Ported from `burnburn-render` (DR-02). 800 × 600 pt at scale 2 is 1600 × 1200 px; the
    /// camera centres (60, 40) mm at 2 pt/mm, so 1 mm = 4 px and the origin is at 800 − 240,
    /// 600 + 160 px.
    #[test]
    fn world_maps_to_device_pixels_at_retina_scale() {
        let vp = Viewport::new(800.0, 600.0, 2.0).unwrap();
        let cam = Camera::new(Point::new(60.0, 40.0), 2.0).unwrap();
        assert_eq!(vp.size_px(), [1600, 1200]);
        assert_eq!(cam.world_to_px(&vp, Point::new(0.0, 0.0)), [560.0, 760.0]);
        assert_eq!(cam.world_to_px(&vp, Point::new(120.0, 80.0)), [1040.0, 440.0]);
        assert_eq!(Viewport::new(0.2, 0.2, 1.0).unwrap().size_px(), [1, 1], "at least one pixel");
    }

    /// Panning by (+10, +5) pt moves content with the pointer: +20, +10 px at scale 2.
    #[test]
    fn pan_moves_content_with_pointer() {
        let vp = Viewport::new(800.0, 600.0, 2.0).unwrap();
        let mut cam = Camera::new(Point::new(60.0, 40.0), 2.0).unwrap();
        let before = cam.world_to_px(&vp, Point::new(0.0, 0.0));
        cam.pan_by_points([10.0, 5.0]);
        let after = cam.world_to_px(&vp, Point::new(0.0, 0.0));
        assert_eq!([after[0] - before[0], after[1] - before[1]], [20.0, 10.0]);
    }

    /// Zoom keeps the world point under the anchor fixed; a rejected factor changes nothing.
    #[test]
    fn zoom_keeps_anchor_world_point_fixed() {
        let vp = Viewport::new(800.0, 600.0, 2.0).unwrap();
        let mut cam = Camera::new(Point::new(60.0, 40.0), 2.0).unwrap();
        let anchor_pt = [100.0, 50.0];
        // Centre (400, 300) pt; anchor 300 pt left (150 mm) and 250 pt up (125 mm).
        let under = cam.points_to_world(&vp, anchor_pt);
        assert_eq!(under, Point::new(-90.0, 165.0));
        cam.zoom_at(&vp, anchor_pt, 4.0).unwrap();
        assert_eq!(cam.points_per_mm(), 8.0);
        assert_eq!(cam.points_to_world(&vp, anchor_pt), under);
        assert_eq!(cam.world_to_points(&vp, under), anchor_pt);
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(cam.zoom_at(&vp, anchor_pt, bad).is_err(), "{bad}");
        }
        assert_eq!(cam.points_per_mm(), 8.0, "a rejected zoom leaves the camera unchanged");
    }

    #[test]
    fn rejects_non_finite_or_non_positive_view_values() {
        assert_eq!(Viewport::new(0.0, 10.0, 1.0), Err(ViewError::NotPositive));
        assert_eq!(Viewport::new(f64::NAN, 10.0, 1.0), Err(ViewError::NotFinite));
        assert_eq!(Camera::new(Point::new(0.0, 0.0), -1.0), Err(ViewError::NotPositive));
        assert_eq!(Camera::new(Point::new(f64::INFINITY, 0.0), 1.0), Err(ViewError::NotFinite));
    }
}
