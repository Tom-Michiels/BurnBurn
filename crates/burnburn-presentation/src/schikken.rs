//! Schikken (W3-UI-schikken): the plain-data types of Dock, Move Together, Warp/Deform, Resize
//! Slots, Tangent Circle, Print and Cut, Center Finder and Nesting. One intent
//! ([`SchikkenIntent`]), one view-model section ([`SchikkenViewModel`]) and one message family
//! ([`SchikkenMessage`]), so the shared files only get one additive line each.

use burnburn_geometry::DockContact;
use burnburn_types::GeometryId;

/// Which selected object a dock slides against (it stays; the others move).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DockReference {
    /// The first selected object (LightBurn's rule, shown as the align reference).
    #[default]
    FirstSelected,
    LastSelected,
    /// A chosen member of the selection.
    Object(GeometryId),
}

/// Which side Move Together closes towards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TogetherAnchorChoice {
    /// The lowest / leftmost object stays.
    #[default]
    Start,
    /// The highest / rightmost object stays.
    End,
    Object(GeometryId),
}

/// The options of Dock and Move Together, as the panel shows them. The gap is raw text (DR-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOptionsInput {
    pub reference: DockReference,
    pub contact: DockContactChoice,
    /// Clearance left between the objects, mm as typed (`0`, `2`, `1,5 mm`).
    pub gap: String,
    /// Objects of one group move as one body.
    pub by_group: bool,
    pub anchor: TogetherAnchorChoice,
}

impl Default for PlaceOptionsInput {
    fn default() -> Self {
        PlaceOptionsInput {
            reference: DockReference::default(),
            contact: DockContactChoice::default(),
            gap: "0".to_owned(),
            by_group: false,
            anchor: TogetherAnchorChoice::default(),
        }
    }
}

/// What counts as the boundary of the objects when docking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DockContactChoice {
    /// Bounding boxes touch.
    #[default]
    Bounds,
    /// The outlines touch: a notch can be entered.
    Outline,
}

impl DockContactChoice {
    pub(crate) fn contact(self) -> DockContact {
        match self {
            DockContactChoice::Bounds => DockContact::Bounds,
            DockContactChoice::Outline => DockContact::Geometry,
        }
    }
}

/// Why Dock or Move Together did nothing. The document is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceFailure {
    /// Nothing of the reference lies ahead in that direction.
    NoContact,
    /// A moving object already overlaps the reference.
    Overlapping,
    /// The reference chosen is not part of the selection.
    ReferenceNotSelected,
    /// Fewer objects than the command needs.
    TooFewObjects,
    /// An object has no geometry.
    EmptyGeometry,
    /// Everything stays where it is already.
    NothingToMove,
    /// Any other refusal of the command.
    Refused,
}

/// Messages of this module's flows (localised by the host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SchikkenMessage {
    /// Dock refused; says why.
    Dock(PlaceFailure),
    /// Move Together refused.
    MoveTogether(PlaceFailure),
    /// The gap field holds no valid, non-negative length.
    InvalidGap,
}

/// The state of the Dock / Move Together options for the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOptionsViewModel {
    pub input: PlaceOptionsInput,
    /// Why the last options input was refused (the previous options stay in force).
    pub error: Option<SchikkenMessage>,
}

/// Why the handles of Warp or Deform cannot be applied as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReshapeProblem {
    /// The quadrilateral is not strictly convex.
    NotConvex,
    /// The corners enclose no area.
    Degenerate,
    /// The control net folds the plane over itself.
    Folded,
    /// Any other refusal of the mapping.
    Other,
}

/// The Warp / Deform session of a window, while one of those tools is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReshapeViewModel {
    /// Warp only: bilinear instead of projective interpolation.
    pub bilinear: bool,
    /// Some handle is off its start position: Apply and Reset are available.
    pub modified: bool,
    pub problem: Option<ReshapeProblem>,
    /// The selection is too large for a live preview: only the frame is drawn.
    pub simplified: bool,
}

/// Everything this module adds to a document window's view model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchikkenViewModel {
    pub place: PlaceOptionsViewModel,
    /// `Some` while the Warp or Deform tool is active on a usable selection.
    pub reshape: Option<ReshapeViewModel>,
}

/// A user action of this module (everything but the command keys, which carry no payload).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SchikkenIntent {
    /// The Dock / Move Together options panel. Invalid text keeps the old options and says why.
    SetPlaceOptions { document: burnburn_types::DocumentId, options: PlaceOptionsInput },
    /// Warp: bilinear (`true`) or projective interpolation of the four corners.
    SetWarpKind { document: burnburn_types::DocumentId, bilinear: bool },
}
