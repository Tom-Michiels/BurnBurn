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
