//! Finger joints and tab/slot joints along one straight edge (edge-local coordinates: `0..length`
//! along the edge, depth measured into the panel).

use burnburn_geometry::{Path, Point, Rect};
use burnburn_types::Length;

use crate::error::{GenError, non_negative, positive};

/// Whether an edge starts (and, with an odd count, ends) with a finger or with a slot. Mating
/// edges use opposite starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeStart {
    Finger,
    Slot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    /// Material reaches the outer edge.
    Finger,
    /// Material is recessed by the joint depth (the mate's finger goes here).
    Slot,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointSegment {
    pub from: f64,
    pub to: f64,
    pub kind: SegmentKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeProfile {
    pub segments: Vec<JointSegment>,
    /// Slot depth = material thickness of the mating panel.
    pub depth: f64,
}

impl EdgeProfile {
    pub fn count(&self, kind: SegmentKind) -> usize {
        self.segments.iter().filter(|s| s.kind == kind).count()
    }
}

/// A finger joint along an edge of `length`. `finger_width` is nominal: the count is the largest
/// odd number of segments not narrower than it (at least 3), all segments equal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FingerJoint {
    pub length: f64,
    /// Thickness of the mating material = slot depth.
    pub thickness: f64,
    pub finger_width: f64,
    /// Design clearance added to every slot width (split over both sides). Not kerf.
    pub fit: f64,
}

impl FingerJoint {
    fn validate(&self) -> Result<(), GenError> {
        positive("length", self.length)?;
        positive("thickness", self.thickness)?;
        positive("finger_width", self.finger_width)?;
        non_negative("fit", self.fit)?;
        Ok(())
    }

    /// Number of segments (fingers + slots), odd and ≥ 3.
    pub fn segments(&self) -> Result<u32, GenError> {
        self.validate()?;
        let raw = (self.length / self.finger_width).floor();
        let n = if raw >= f64::from(u32::MAX) { u32::MAX - 1 } else { raw as u32 };
        let n = if n % 2 == 0 { n.saturating_sub(1) } else { n };
        if n < 3 {
            // Report the raw count so the inspector can say "only 2 fit".
            return Err(GenError::Count { param: "fingers", min: 3, max: u64::from(u32::MAX), value: raw as u64 });
        }
        Ok(n)
    }

    pub fn profile(&self, start: EdgeStart) -> Result<EdgeProfile, GenError> {
        let n = self.segments()?;
        let seg = self.length / f64::from(n);
        if self.fit >= seg {
            return Err(GenError::TooSmall { param: "fit", min_mm: 0.0, value_mm: self.fit });
        }
        let half = self.fit / 2.0;
        let mut segments = Vec::with_capacity(n as usize);
        for i in 0..n {
            let finger = (i % 2 == 0) == (start == EdgeStart::Finger);
            let kind = if finger { SegmentKind::Finger } else { SegmentKind::Slot };
            segments.push(JointSegment { from: f64::from(i) * seg, to: f64::from(i + 1) * seg, kind });
        }
        if let Some(last) = segments.last_mut() {
            last.to = self.length; // exact end, no accumulated rounding
        }
        // Widen every slot by `fit` (half on each side, clipped at the edge ends).
        let slots: Vec<usize> = (0..segments.len()).filter(|&i| segments[i].kind == SegmentKind::Slot).collect();
        for i in slots {
            if i > 0 {
                segments[i].from -= half;
                segments[i - 1].to -= half;
            } else {
                segments[i].to += half; // the widening goes inward only
            }
            if i + 1 < segments.len() {
                segments[i].to += half;
                segments[i + 1].from += half;
            } else {
                segments[i].from -= half;
            }
        }
        Ok(EdgeProfile { segments, depth: self.thickness })
    }
}

/// Tabs on one panel's edge and the matching slots (closed rectangles) in the other panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabSlot {
    pub length: f64,
    pub tabs: u32,
    pub tab_width: f64,
    /// Material thickness of the slotted panel = tab length.
    pub thickness: f64,
    /// Design clearance added to slot width and height. Not kerf.
    pub fit: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabSlotResult {
    /// Tab edge: `Finger` segments are the tabs (they protrude by `thickness`).
    pub tab_edge: EdgeProfile,
    /// Slot cut-outs, centred on the tab centres along the x-axis (y = 0 is the slot centre line).
    pub slots: Vec<Path>,
    pub centres: Vec<f64>,
}

pub fn tab_and_slot(p: &TabSlot) -> Result<TabSlotResult, GenError> {
    positive("length", p.length)?;
    positive("tab_width", p.tab_width)?;
    positive("thickness", p.thickness)?;
    non_negative("fit", p.fit)?;
    // At least one tab width of material between and around tabs.
    let max = (p.length / (2.0 * p.tab_width)).floor().max(0.0) as u64;
    if p.tabs == 0 || u64::from(p.tabs) > max {
        return Err(GenError::Count { param: "tabs", min: 1, max, value: u64::from(p.tabs) });
    }
    let n = f64::from(p.tabs);
    let centres: Vec<f64> = (0..p.tabs).map(|i| p.length * (f64::from(i) + 0.5) / n).collect();
    let mut segments = Vec::new();
    let mut at = 0.0;
    for &c in &centres {
        let (a, b) = (c - p.tab_width / 2.0, c + p.tab_width / 2.0);
        segments.push(JointSegment { from: at, to: a, kind: SegmentKind::Slot });
        segments.push(JointSegment { from: a, to: b, kind: SegmentKind::Finger });
        at = b;
    }
    segments.push(JointSegment { from: at, to: p.length, kind: SegmentKind::Slot });
    let (w, h) = (p.tab_width + p.fit, p.thickness + p.fit);
    let len = |v: f64| Length::from_mm(v).map_err(|_| GenError::NotFinite { param: "tab_width" });
    let slots = centres
        .iter()
        .map(|&c| Ok(Path::rectangle(Rect::new(Point::new(c - w / 2.0, -h / 2.0), len(w)?, len(h)?)?)))
        .collect::<Result<Vec<_>, GenError>>()?;
    // The tab edge's "depth" is how far the tabs protrude.
    Ok(TabSlotResult { tab_edge: EdgeProfile { segments, depth: p.thickness }, slots, centres })
}
