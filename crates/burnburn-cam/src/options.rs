use burnburn_geometry::{Point, ProductionTolerance};

/// Where the head is when the job starts (bed view, mm). Used as the first nearest-neighbour
/// reference; it does not move the job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StartPoint {
    /// The controller's coordinate origin (machine `(0, 0)`).
    MachineOrigin,
    /// A known head position in bed-view coordinates.
    Bed(Point),
}

/// One of the nine anchor points of the job's bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobAnchor {
    BottomLeft,
    Bottom,
    BottomRight,
    Left,
    Center,
    Right,
    TopLeft,
    Top,
    TopRight,
}

/// How document coordinates become bed coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Document millimetres are bed-view millimetres (bottom-left bed corner = `(0, 0)`).
    Absolute,
    /// Translate the job so `anchor` of its burning bounds lands on `at` (bed view).
    Anchored { anchor: JobAnchor, at: Point },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompileOptions {
    pub tolerance: ProductionTolerance,
    pub placement: Placement,
    pub start: StartPoint,
}
