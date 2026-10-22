//! Plates (P06): sheets of stock as a document level, each with its own size and material.
//! Every object belongs to at most one plate.

use burnburn_types::{CanonicalEncode, CanonicalWriter, GeometryId, Length, OperationId};

/// One sheet of stock.
#[derive(Debug, Clone, PartialEq)]
pub struct Plate {
    pub name: String,
    pub width: Length,
    pub height: Length,
    pub material: String,
    pub thickness: Option<Length>,
}

/// Something a plate view could not include as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlateIssue {
    /// An operation of this plate also targets an object that is not on the plate; that target
    /// is left out of the plate view.
    TargetOffPlate { operation: OperationId, geometry: GeometryId },
    /// Derived geometry on the plate whose owner is not validly evaluated (stale / failed /
    /// missing source): production must be refused.
    Blocked(GeometryId),
}

impl CanonicalEncode for Plate {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.name);
        out.f64(self.width.mm());
        out.f64(self.height.mm());
        out.str(&self.material);
        match self.thickness {
            Some(t) => {
                out.tag("thickness");
                out.f64(t.mm());
            }
            None => out.tag("no-thickness"),
        }
    }
}

/// One plate as far as distributing is concerned: its size in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateRoom {
    pub id: super::PlateId,
    pub width: f64,
    pub height: f64,
}

/// The rectangle a part instance takes (its bounding box, mm) and the plate it already stands on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    pub id: super::PartInstanceId,
    pub width: f64,
    pub height: f64,
    pub plate: Option<super::PlateId>,
}

/// Which instance goes to which plate, and which found no room anywhere.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Distribution {
    pub assigned: Vec<(super::PartInstanceId, super::PlateId)>,
    pub unplaced: Vec<super::PartInstanceId>,
}

/// Rows of parts on one plate: the filled width of each row and its height.
#[derive(Debug, Default)]
struct Rows {
    rows: Vec<(f64, f64)>,
}

impl Rows {
    /// Puts a `w x h` rectangle on `plate`, in the first row it fits in or a new row; `gap`
    /// separates parts in a row and rows from each other. `false` when there is no room.
    fn place(&mut self, plate: &PlateRoom, w: f64, h: f64, gap: f64) -> bool {
        for (used, height) in &mut self.rows {
            let needed = if *used > 0.0 { *used + gap + w } else { w };
            if h <= *height + 1e-9 && needed <= plate.width + 1e-9 {
                *used = needed;
                return true;
            }
        }
        let top = self.rows.iter().map(|(_, height)| height + gap).sum::<f64>();
        if top + h <= plate.height + 1e-9 && w <= plate.width + 1e-9 {
            self.rows.push((w, h));
            return true;
        }
        false
    }
}

fn usable(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

fn tallest_first(a: &&Footprint, b: &&Footprint) -> std::cmp::Ordering {
    b.height.total_cmp(&a.height).then(b.width.total_cmp(&a.width)).then(a.id.cmp(&b.id))
}

/// Distributes the instances that stand on no plate over `plates` (G8-editor, P06): the parts
/// are laid in rows, tallest first, on the first plate that has room for them, also turned a
/// quarter. What is on a plate already stays there and takes its room first. Only the choice of
/// plate is made here: where on the plate a part goes is for the nest. A part with a size that is
/// no length stays unplaced, as does one without room.
pub fn distribute_over_plates(plates: &[PlateRoom], items: &[Footprint], gap: f64) -> Distribution {
    let gap = if gap.is_finite() && gap > 0.0 { gap } else { 0.0 };
    let mut rows: Vec<Rows> = plates.iter().map(|_| Rows::default()).collect();
    let mut placed: Vec<&Footprint> = items.iter().filter(|i| i.plate.is_some()).collect();
    placed.sort_by(tallest_first);
    for item in placed {
        if let Some(index) = plates.iter().position(|p| Some(p.id) == item.plate)
            && usable(item.width)
            && usable(item.height)
        {
            // A plate that is already overfull is not made worse: the part just stays.
            let _ = rows[index].place(&plates[index], item.width, item.height, gap);
        }
    }
    let mut candidates: Vec<&Footprint> = items.iter().filter(|i| i.plate.is_none()).collect();
    candidates.sort_by(tallest_first);
    let mut out = Distribution::default();
    for item in candidates {
        let mut done = false;
        if usable(item.width) && usable(item.height) {
            'plates: for (index, plate) in plates.iter().enumerate() {
                for (w, h) in [(item.width, item.height), (item.height, item.width)] {
                    if rows[index].place(plate, w, h, gap) {
                        out.assigned.push((item.id, plate.id));
                        done = true;
                        break 'plates;
                    }
                }
            }
        }
        if !done {
            out.unplaced.push(item.id);
        }
    }
    out
}
