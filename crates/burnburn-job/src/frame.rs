use burnburn_geometry::Point;

use crate::{MachinePoint, PlanError};

/// Which physical bed corner is the controller's coordinate origin. Machine coordinates grow
/// away from that corner, so every point on the bed has `0 <= x <= width`, `0 <= y <= height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OriginCorner {
    BottomLeft,
    TopLeft,
    TopRight,
    BottomRight,
}

/// The bed and its coordinate frame. The document's bed view is y-up with `(0, 0)` at the
/// bottom-left bed corner; [`MachineFrame::to_machine`] maps that view to controller coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MachineFrame {
    width_mm: f64,
    height_mm: f64,
    origin: OriginCorner,
}

impl MachineFrame {
    pub fn new(width_mm: f64, height_mm: f64, origin: OriginCorner) -> Result<Self, PlanError> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if ok(width_mm) && ok(height_mm) {
            Ok(MachineFrame { width_mm, height_mm, origin })
        } else {
            Err(PlanError::OutOfRange("bed size"))
        }
    }

    pub fn width_mm(&self) -> f64 {
        self.width_mm
    }

    pub fn height_mm(&self) -> f64 {
        self.height_mm
    }

    pub fn origin(&self) -> OriginCorner {
        self.origin
    }

    /// Bed view (y-up, bottom-left origin, mm) → machine frame. Exact: mirrors only, no scaling.
    pub fn to_machine(&self, p: Point) -> Result<MachinePoint, PlanError> {
        let (w, h) = (self.width_mm, self.height_mm);
        let (x, y) = match self.origin {
            OriginCorner::BottomLeft => (p.x, p.y),
            OriginCorner::TopLeft => (p.x, h - p.y),
            OriginCorner::TopRight => (w - p.x, h - p.y),
            OriginCorner::BottomRight => (w - p.x, p.y),
        };
        MachinePoint::new(x, y)
    }

    /// Machine frame → bed view; the inverse of [`MachineFrame::to_machine`].
    pub fn to_bed(&self, p: MachinePoint) -> Point {
        let (w, h) = (self.width_mm, self.height_mm);
        let (x, y) = (p.x_mm(), p.y_mm());
        match self.origin {
            OriginCorner::BottomLeft => Point::new(x, y),
            OriginCorner::TopLeft => Point::new(x, h - y),
            OriginCorner::TopRight => Point::new(w - x, h - y),
            OriginCorner::BottomRight => Point::new(w - x, y),
        }
    }

    /// True when `p` lies on the bed. Exact comparison, no tolerance and no rounding: a point
    /// 1 nm outside is outside (IM15 "geen afronding naar binnen").
    pub fn contains(&self, p: MachinePoint) -> bool {
        (0.0..=self.width_mm).contains(&p.x_mm()) && (0.0..=self.height_mm).contains(&p.y_mm())
    }
}
