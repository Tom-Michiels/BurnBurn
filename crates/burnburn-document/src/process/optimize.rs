//! Optimization settings (W2-L3): how the compiler orders and trims the cut paths of a job.
//!
//! LightBurn's "Optimization Settings" is an *ordered* hierarchy (`Order by`) plus switches for
//! the planning heuristics. The order matters: Layer→Priority runs every layer completely, and
//! inside each layer the low priorities first; Priority→Layer runs priority 0 of every layer
//! before any priority 1. The defaults reproduce the planner as it was before these settings
//! existed (layer order, inner contours first, nearest neighbour), so existing projects keep
//! their plans. The compiler (`burnburn-cam`) reads them; nothing here touches a machine.

use burnburn_types::Length;

use super::ProcessError;

/// One level of the `Order by` hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OrderKey {
    /// The execution order of the operations (layers), sub-operations right after their parent.
    Layer,
    /// The outermost group of a shape; ungrouped shapes first, then groups by creation order.
    Group,
    /// The shape's [`CutPriority`], lowest first.
    Priority,
}

/// What the planner may not change about the way a shape is cut (C06): where it starts and
/// which way it runs. Both are free until locked; the document's own start and direction stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PathLock {
    /// The shape is entered at its own start point, whatever "best start" would pick.
    pub start: bool,
    /// The shape runs in the direction it was drawn, whatever "best direction" would pick.
    pub direction: bool,
}

impl PathLock {
    pub fn is_none(self) -> bool {
        !self.start && !self.direction
    }
}

/// Cut order priority of one shape; lower numbers are cut first. `0` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CutPriority(i32);

impl CutPriority {
    pub const MAX: i32 = 10_000;

    pub fn new(value: i32) -> Result<Self, ProcessError> {
        if (-Self::MAX..=Self::MAX).contains(&value) {
            Ok(CutPriority(value))
        } else {
            Err(ProcessError::OutOfRange { field: "priority" })
        }
    }

    pub fn get(self) -> i32 {
        self.0
    }
}

/// Planner options of a document. `Default` is the planner of before IM-W2: layer order, inner
/// shapes first, nearest-neighbour travel, best entry point and direction, no overlap removal.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizationSettings {
    order_by: Vec<OrderKey>,
    inner_first: bool,
    reduce_travel: bool,
    best_start: bool,
    best_direction: bool,
    hide_backlash: bool,
    remove_overlapping_mm: Option<f64>,
}

impl Default for OptimizationSettings {
    fn default() -> Self {
        OptimizationSettings {
            order_by: vec![OrderKey::Layer],
            inner_first: true,
            reduce_travel: true,
            best_start: true,
            best_direction: true,
            hide_backlash: false,
            remove_overlapping_mm: None,
        }
    }
}

impl OptimizationSettings {
    /// Largest overlap tolerance accepted (mm): a bigger one would merge neighbouring lines.
    pub const MAX_OVERLAP_TOLERANCE_MM: f64 = 5.0;

    /// The `Order by` hierarchy, most significant first. Levels that are not listed never
    /// reorder anything; they only break ties, in the order Layer, Group, Priority.
    pub fn order_by(&self) -> &[OrderKey] {
        &self.order_by
    }

    /// A contour inside another closed contour is cut before it (parts do not drop out early).
    pub fn inner_first(&self) -> bool {
        self.inner_first
    }

    /// Choose the next contour by the shortest laser-off move; off keeps the given order.
    pub fn reduce_travel(&self) -> bool {
        self.reduce_travel
    }

    /// Enter closed contours at the vertex nearest to the head; off starts at the shape's own
    /// start point (the first node of its path), which is how manual start points are respected.
    pub fn best_start(&self) -> bool {
        self.best_start
    }

    /// Open contours may be cut from their far end; off keeps the drawn direction.
    pub fn best_direction(&self) -> bool {
        self.best_direction
    }

    /// Prefer approaches that do not reverse the head at the start of a cut (backlash).
    pub fn hide_backlash(&self) -> bool {
        self.hide_backlash
    }

    /// Distance tolerance (mm) within which a later cut over an earlier one is dropped from the
    /// planned output; `None` keeps every line.
    pub fn remove_overlapping_mm(&self) -> Option<f64> {
        self.remove_overlapping_mm
    }

    pub fn is_default(&self) -> bool {
        *self == OptimizationSettings::default()
    }

    /// The same settings with another `Order by` hierarchy. Duplicates are rejected.
    pub fn with_order(mut self, order: Vec<OrderKey>) -> Result<Self, ProcessError> {
        if order.iter().enumerate().any(|(i, k)| order[..i].contains(k)) {
            return Err(ProcessError::OutOfRange { field: "order by" });
        }
        self.order_by = order;
        Ok(self)
    }

    pub fn with_inner_first(mut self, on: bool) -> Self {
        self.inner_first = on;
        self
    }

    pub fn with_reduce_travel(mut self, on: bool) -> Self {
        self.reduce_travel = on;
        self
    }

    pub fn with_best_start(mut self, on: bool) -> Self {
        self.best_start = on;
        self
    }

    pub fn with_best_direction(mut self, on: bool) -> Self {
        self.best_direction = on;
        self
    }

    pub fn with_hide_backlash(mut self, on: bool) -> Self {
        self.hide_backlash = on;
        self
    }

    /// Removes overlapping lines within `tolerance` (positive, at most
    /// [`Self::MAX_OVERLAP_TOLERANCE_MM`]); `None` switches it off.
    pub fn with_remove_overlapping(mut self, tolerance: Option<Length>) -> Result<Self, ProcessError> {
        self.remove_overlapping_mm = match tolerance {
            None => None,
            Some(t) if t.mm() > 0.0 && t.mm() <= Self::MAX_OVERLAP_TOLERANCE_MM => Some(t.mm()),
            Some(t) if t.mm() > 0.0 => return Err(ProcessError::OutOfRange { field: "overlap tolerance" }),
            Some(_) => return Err(ProcessError::NotPositive { field: "overlap tolerance" }),
        };
        Ok(self)
    }
}
